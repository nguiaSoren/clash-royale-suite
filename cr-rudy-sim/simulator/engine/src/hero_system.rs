//! Hero ability handlers — manual activation, costs elixir
//!
//! Data from evo_hero_abilities.json → GameData.heroes
//!
//! Heroes:
//!   giant       — Mighty Slam: knockback + stun in radius
//!   ice-golem   — Ice Nova: slow + area damage + freeze
//!   knight      — Triumphant Taunt: taunt all enemies in range + shield
//!   mini-pekka  — Breakfast Boost: heal 30% + level increase
//!   musketeer   — Trusty Turret: spawn a turret building
//!   wizard      — Fiery Flight: fly + tornado projectiles
//!
//! Activation flow:
//!   1. Python calls activate_hero(entity_id)
//!   2. We find the entity, verify it's a hero, check elixir
//!   3. Spend elixir, apply effects immediately
//!   4. Some effects are ongoing (taunt, flight) — tracked via HeroState
//!   5. tick_hero_state() decrements ability durations each tick
//!   6. enforce_taunts() overrides targeting for taunted enemies

use crate::data_types::GameData;
use crate::entities::*;
use crate::game_state::*;
use crate::combat;

// =========================================================================
// Constants
// =========================================================================

const TPS: i32 = 20;

fn ms_to_ticks(ms: i32) -> i32 {
    (ms * TPS + 999) / 1000
}

// =========================================================================
// Public API — called from lib.rs (Python binding)
// =========================================================================

/// Activate a hero's ability. Called via Python `match.activate_hero(entity_id)`.
/// Returns Ok(()) on success, Err(message) on failure.
pub fn activate_hero_ability(
    state: &mut GameState,
    data: &GameData,
    entity_id: EntityId,
) -> Result<(), String> {
    // Find the entity
    let entity_idx = state.entities.iter().position(|e| e.id == entity_id && e.alive)
        .ok_or_else(|| format!("Entity {} not found or dead", entity_id.0))?;

    // Verify it's a hero
    let entity = &state.entities[entity_idx];
    let hero_state = entity.hero_state.as_ref()
        .ok_or("Entity is not a hero")?;

    if !hero_state.is_hero {
        return Err("Entity is not a hero".to_string());
    }

    if hero_state.ability_active {
        return Err("Hero ability already active".to_string());
    }

    let hero_key = hero_state.hero_key.clone();

    // Look up hero ability data — try multiple key formats since
    // evo_hero_abilities.json may use different casing/formatting than
    // the character key used in spawn_troop.
    let hero_def_opt = data.heroes.get(&hero_key)
        .or_else(|| {
            // Try with hyphens: "goldenknight" → "golden-knight"
            let hyphenated = hero_key.chars().enumerate().fold(String::new(), |mut s, (i, c)| {
                if i > 0 && c.is_uppercase() {
                    s.push('-');
                }
                s.push(c.to_ascii_lowercase());
                s
            });
            data.heroes.get(&hyphenated)
        })
        .or_else(|| {
            // Try lowercase exact match against all hero keys
            let lower = hero_key.to_lowercase();
            data.heroes.iter()
                .find(|(k, _)| k.to_lowercase() == lower)
                .map(|(_, v)| v)
        });

    // If not found in heroes, try champion system (SK, GK, AQ, Monk, MM)
    let hero_def = match hero_def_opt {
        Some(def) => def,
        None => {
            // Check if this entity has a champion ability field
            let has_champion_ability = data.characters.get(&hero_key)
                .or_else(|| data.characters.get(&state.entities[entity_idx].card_key))
                .and_then(|s| s.ability.as_ref())
                .map(|a| crate::champion_system::is_champion(a))
                .unwrap_or(false);

            if has_champion_ability {
                // Delegate to champion system — it handles its own elixir cost
                return crate::champion_system::activate_champion(state, data, entity_idx);
            }

            return Err(format!("Hero def not found for key: {} (heroes available: {:?})",
                hero_key, data.heroes.keys().collect::<Vec<_>>()));
        }
    };

    let elixir_cost = hero_def.ability.elixir_cost;

    // Determine which player owns this hero
    let team = state.entities[entity_idx].team;

    // Check and spend elixir
    let player = state.player_mut(team);
    if !player.spend_elixir(elixir_cost) {
        return Err(format!(
            "Not enough elixir: need {}, have {}",
            elixir_cost,
            player.elixir_whole()
        ));
    }

    // Apply effects based on hero type
    let entity = &state.entities[entity_idx];
    let ex = entity.x;
    let ey = entity.y;
    let eteam = entity.team;
    let ehp = entity.hp;
    let emhp = entity.max_hp;

    for effect in &hero_def.ability.effects {
        apply_hero_effect(state, data, entity_idx, effect, ex, ey, eteam, ehp, emhp);
    }

    // Mark ability as active
    if let Some(ref mut hs) = state.entities[entity_idx].hero_state {
        hs.ability_active = true;
        // Set duration based on the longest effect duration
        let max_duration = hero_def.ability.effects.iter()
            .filter_map(|e| e.duration_ms)
            .map(ms_to_ticks)
            .max()
            .unwrap_or(100); // Default 5 seconds
        hs.ability_remaining = max_duration;
    }

    Ok(())
}

// =========================================================================
// Per-tick processing — called from engine.rs
// =========================================================================

/// Tick hero ability durations, expire effects.
pub fn tick_hero_state(state: &mut GameState) {
    for entity in state.entities.iter_mut() {
        if !entity.alive {
            continue;
        }
        let hero_state = match &mut entity.hero_state {
            Some(hs) if hs.ability_active => hs,
            _ => continue,
        };

        hero_state.ability_remaining -= 1;
        if hero_state.ability_remaining <= 0 {
            hero_state.ability_active = false;

            // Remove flight override
            if hero_state.is_flying_override {
                hero_state.is_flying_override = false;
                entity.z = 0; // Return to ground
            }
        }
    }
}

/// Enforce taunt targeting: any entity with a taunt buff must target the taunt source.
pub fn enforce_taunts(state: &mut GameState) {
    let entity_count = state.entities.len();
    for i in 0..entity_count {
        let entity = &state.entities[i];
        if !entity.alive || entity.is_immobilized() {
            continue;
        }
        if let Some(taunt_id) = entity.taunt_override() {
            // Verify the taunt source is still alive
            let taunt_alive = state.entities.iter().any(|e| e.id == taunt_id && e.alive);
            if taunt_alive {
                state.entities[i].target = Some(taunt_id);
            }
        }
    }
}

// =========================================================================
// Setup — called when spawning a hero entity
// =========================================================================

/// Initialize hero state on a freshly deployed entity.
/// Call after Entity::new_troop when the card is a hero card.
pub fn setup_hero_state(entity: &mut Entity, hero_key: &str) {
    entity.hero_state = Some(HeroState {
        is_hero: true,
        hero_key: hero_key.to_string(),
        ability_active: false,
        ability_remaining: 0,
        is_flying_override: false,
    });
}

/// Check if a card key is a hero card.
pub fn is_hero_card(data: &GameData, card_key: &str) -> bool {
    if data.heroes.contains_key(card_key) {
        return true;
    }
    // Try lowercase
    let lower = card_key.to_lowercase();
    data.heroes.keys().any(|k| k.to_lowercase() == lower)
}

// =========================================================================
// Internal: Apply individual hero effects
// =========================================================================

fn apply_hero_effect(
    state: &mut GameState,
    data: &GameData,
    entity_idx: usize,
    effect: &crate::data_types::HeroEffect,
    ex: i32,
    ey: i32,
    eteam: Team,
    _ehp: i32,
    emhp: i32,
) {
    let effect_type = effect.effect_type.as_str();
    let radius = effect.radius.unwrap_or(0);
    let duration_ticks = effect.duration_ms.map(ms_to_ticks).unwrap_or(100);
    let damage = effect.damage.unwrap_or(0);
    let value = effect.value.unwrap_or(0.0) as i32;

    match effect_type {
        "taunt" => {
            // Knight hero: force all enemies in radius to target this hero
            let hero_id = state.entities[entity_idx].id;
            let taunt_radius = effect.taunt_radius.unwrap_or(radius);
            let taunt_duration = effect.taunt_duration_ms.map(ms_to_ticks).unwrap_or(duration_ticks);
            let radius_sq = (taunt_radius as i64) * (taunt_radius as i64);

            // Apply taunt buff to all enemies in range
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == eteam {
                    continue;
                }
                if !entity.is_troop() {
                    continue;
                }
                if entity.dist_sq_to(ex, ey) <= radius_sq {
                    entity.add_buff(ActiveBuff {
                        key: format!("hero_taunt_{}", hero_id.0),
                        remaining_ticks: taunt_duration,
                        speed_percent: 0,
                        hitspeed_percent: 0,
                        damage_percent: 0,
                        damage_reduction: 0,
                        heal_per_tick: 0,
                        damage_per_tick: 0,
                        damage_hit_interval: 0,
                        damage_hit_timer: 0,
                        building_damage_percent: 0,
                        crown_tower_damage_percent: 0,
                        stun: false,
                        freeze: false,
                        invisible: false,
                        taunt_target: Some(hero_id),
                        death_spawn: None,
                        death_spawn_count: 0,
                        death_spawn_is_enemy: false,
                        remove_on_attack: false,
                        spawn_speed_multiplier: 0,
                        enable_stacking: false,
                        allowed_over_heal_perc: 0,
                        hp_multiplier: 0,
                    });
                }
            }

            // Grant shield to the hero
            if let Some(shield_hp) = effect.shield_hitpoints {
                state.entities[entity_idx].shield_hp += shield_hp;
            }
        }

        "knockback" => {
            // Giant hero: knockback + brief stun
            let radius_sq = (radius as i64) * (radius as i64);
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == eteam {
                    continue;
                }
                let can_affect = match (entity.is_flying(), effect.affects_air, effect.affects_ground) {
                    (true, Some(true), _) | (true, None, _) => true,
                    (false, _, Some(true)) | (false, _, None) => true,
                    _ => false,
                };
                if !can_affect { continue; }

                if entity.dist_sq_to(ex, ey) <= radius_sq {
                    // Knockback = brief stun
                    entity.add_buff(ActiveBuff {
                        key: "hero_kb".to_string(),
                        remaining_ticks: 6, // 0.3s
                        speed_percent: 0,
                        hitspeed_percent: 0,
                        damage_percent: 0,
                        damage_reduction: 0,
                        heal_per_tick: 0,
                        damage_per_tick: 0,
                damage_hit_interval: 0,
                damage_hit_timer: 0,
                        building_damage_percent: 0,
                        crown_tower_damage_percent: 0,
                        stun: true,
                        freeze: false,
                        invisible: false,
                    taunt_target: None,
                    death_spawn: None,
                    death_spawn_count: 0,
                    death_spawn_is_enemy: false,
                    remove_on_attack: false,
                    spawn_speed_multiplier: 0,
                    enable_stacking: false,
                    allowed_over_heal_perc: 0,
                    hp_multiplier: 0,
                    });

                    // Push enemy away from hero position
                    let dx = entity.x - ex;
                    let dy = entity.y - ey;
                    let dist = ((dx as i64 * dx as i64 + dy as i64 * dy as i64) as f64).sqrt() as i32;
                    if dist > 0 {
                        let push = 1200; // Push distance
                        entity.x += dx * push / dist;
                        entity.y += dy * push / dist;
                        // Clamp to arena
                        entity.x = entity.x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
                        entity.y = entity.y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
                    }
                }
            }
        }

        "stun" => {
            // Giant hero: stun enemies in radius
            let radius_sq = (radius as i64) * (radius as i64);
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == eteam {
                    continue;
                }
                if entity.dist_sq_to(ex, ey) <= radius_sq {
                    entity.add_buff(ActiveBuff {
                        key: "hero_stun".to_string(),
                        remaining_ticks: duration_ticks,
                        speed_percent: 0,
                        hitspeed_percent: 0,
                        damage_percent: 0,
                        damage_reduction: 0,
                        heal_per_tick: 0,
                        damage_per_tick: 0,
                damage_hit_interval: 0,
                damage_hit_timer: 0,
                        building_damage_percent: 0,
                        crown_tower_damage_percent: 0,
                        stun: true,
                        freeze: false,
                        invisible: false,
                    taunt_target: None,
                    death_spawn: None,
                    death_spawn_count: 0,
                    death_spawn_is_enemy: false,
                    remove_on_attack: false,
                    spawn_speed_multiplier: 0,
                    enable_stacking: false,
                    allowed_over_heal_perc: 0,
                    hp_multiplier: 0,
                    });
                }
            }
        }

        "slow" => {
            // Ice Golem hero: slow enemies
            let radius_sq = (radius as i64) * (radius as i64);
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == eteam {
                    continue;
                }
                if entity.dist_sq_to(ex, ey) <= radius_sq {
                    entity.add_buff(ActiveBuff {
                        key: "hero_slow".to_string(),
                        remaining_ticks: duration_ticks,
                        speed_percent: -value,
                        hitspeed_percent: 0,
                        damage_percent: 0,
                        damage_reduction: 0,
                        heal_per_tick: 0,
                        damage_per_tick: 0,
                damage_hit_interval: 0,
                damage_hit_timer: 0,
                        building_damage_percent: 0,
                        crown_tower_damage_percent: 0,
                        stun: false,
                        freeze: false,
                        invisible: false,
                    taunt_target: None,
                    death_spawn: None,
                    death_spawn_count: 0,
                    death_spawn_is_enemy: false,
                    remove_on_attack: false,
                    spawn_speed_multiplier: 0,
                    enable_stacking: false,
                    allowed_over_heal_perc: 0,
                    hp_multiplier: 0,
                    });
                }
            }
        }

        "freeze" => {
            // Ice Golem hero: freeze enemies
            let radius_sq = (radius as i64) * (radius as i64);
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == eteam {
                    continue;
                }
                if entity.dist_sq_to(ex, ey) <= radius_sq {
                    entity.add_buff(ActiveBuff {
                        key: "hero_freeze".to_string(),
                        remaining_ticks: duration_ticks,
                        speed_percent: 0,
                        hitspeed_percent: 0,
                        damage_percent: 0,
                        damage_reduction: 0,
                        heal_per_tick: 0,
                        damage_per_tick: 0,
                damage_hit_interval: 0,
                damage_hit_timer: 0,
                        building_damage_percent: 0,
                        crown_tower_damage_percent: 0,
                        stun: false,
                        freeze: true,
                        invisible: false,
                    taunt_target: None,
                    death_spawn: None,
                    death_spawn_count: 0,
                    death_spawn_is_enemy: false,
                    remove_on_attack: false,
                    spawn_speed_multiplier: 0,
                    enable_stacking: false,
                    allowed_over_heal_perc: 0,
                    hp_multiplier: 0,
                    });
                }
            }
        }

        "area_damage" => {
            // Ice Golem hero / Giant hero: deal damage in radius
            let radius_sq = (radius as i64) * (radius as i64);
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == eteam {
                    continue;
                }
                if entity.dist_sq_to(ex, ey) <= radius_sq {
                    combat::apply_damage_to_entity(entity, damage);
                }
            }
            // Hit towers too
            let enemy_towers = combat::enemy_tower_ids(eteam);
            for tid in &enemy_towers {
                if let Some(tpos) = combat::tower_pos(state, *tid) {
                    let dx = (ex - tpos.0) as i64;
                    let dy = (ey - tpos.1) as i64;
                    if dx * dx + dy * dy <= (radius as i64) * (radius as i64) {
                        combat::apply_damage_to_tower(state, *tid, damage);
                    }
                }
            }
        }

        "heal" => {
            // Mini PEKKA hero: heal percentage of max HP
            if value > 0 {
                let heal_amount = emhp * value / 100;
                let e = &mut state.entities[entity_idx];
                e.hp = (e.hp + heal_amount).min(e.max_hp);
            }
        }

        "buff" => {
            // Mini PEKKA hero: level increase → we approximate as damage + HP boost
            let buff_ref = effect.buff_reference.as_deref().unwrap_or("");
            if buff_ref == "level_increase" {
                // Approximate 1-5 level increase as +15% damage and HP per level, avg 3 levels
                let e = &mut state.entities[entity_idx];
                let boost = 45; // 3 levels × 15% ≈ 45% boost
                e.add_buff(ActiveBuff {
                    key: "hero_level_boost".to_string(),
                    remaining_ticks: i32::MAX, // Permanent for this deployment
                    speed_percent: 0,
                    hitspeed_percent: 0,
                    damage_percent: boost,
                    damage_reduction: 0,
                    heal_per_tick: 0,
                    damage_per_tick: 0,
                damage_hit_interval: 0,
                damage_hit_timer: 0,
                    building_damage_percent: 0,
                    crown_tower_damage_percent: 0,
                    stun: false,
                    freeze: false,
                    invisible: false,
                    taunt_target: None,
                    death_spawn: None,
                    death_spawn_count: 0,
                    death_spawn_is_enemy: false,
                    remove_on_attack: false,
                    spawn_speed_multiplier: 0,
                    enable_stacking: false,
                    allowed_over_heal_perc: 0,
                    hp_multiplier: 0,
                });
                // Also boost max HP
                e.max_hp = e.max_hp * (100 + boost) / 100;
                e.hp = e.hp * (100 + boost) / 100;
            }
        }

        "building_spawn" => {
            // Musketeer hero: spawn a turret building
            if let Some(ref spawn_key) = effect.spawn_character {
                let offset_y = effect.value.unwrap_or(3000.0) as i32;
                let spawn_y = ey + eteam.forward_y() * offset_y;
                let duration_ms = effect.duration_ms.unwrap_or(10000);
                let duration_ticks = ms_to_ticks(duration_ms);

                // Try to find building stats; if not available, create a simple building
                if let Some(stats) = data.buildings.get(spawn_key)
                    .or_else(|| data.characters.get(spawn_key))
                {
                    let id = state.alloc_id();
                    let mut bld = Entity::new_building(id, eteam, stats, ex, spawn_y, 11);
                    // Override lifetime with ability duration
                    if let EntityKind::Building(ref mut bd) = bld.kind {
                        bd.lifetime = duration_ticks;
                        bd.lifetime_remaining = duration_ticks;
                    }
                    bld.deploy_timer = 0;
                    state.entities.push(bld);
                }
                // If the building key isn't in data, we just skip (graceful degradation)
            }
        }

        "flight" => {
            // Wizard hero: gain flight
            state.entities[entity_idx].z = 2000; // Flying height
            if let Some(ref mut hs) = state.entities[entity_idx].hero_state {
                hs.is_flying_override = true;
            }
        }

        "projectile_tornado" => {
            // Wizard hero: projectiles gain tornado effect
            // Simplified: deal area damage around the hero's position each tick while active
            // The actual per-tick damage is handled in tick_hero_state or via a buff
            let radius_sq = (radius as i64) * (radius as i64);
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == eteam {
                    continue;
                }
                if entity.dist_sq_to(ex, ey) <= radius_sq {
                    combat::apply_damage_to_entity(entity, damage);
                    // Also pull towards hero
                    let dx = ex - entity.x;
                    let dy = ey - entity.y;
                    let dist = entity.dist_sq_to(ex, ey);
                    if dist > 0 {
                        let d = (dist as f64).sqrt() as i32;
                        if d > 0 {
                            entity.x += dx * 200 / d;
                            entity.y += dy * 200 / d;
                        }
                    }
                }
            }
        }

        _ => {} // Unknown effect type — skip gracefully
    }
}
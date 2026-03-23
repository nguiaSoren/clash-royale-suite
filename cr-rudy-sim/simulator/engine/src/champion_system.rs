//! Champion ability system — Skeleton King, Golden Knight, Archer Queen, Monk, Mighty Miner
//!
//! Champions are troops with an `ability` field in their CharacterStats.
//! They differ from Hero Cards (knight hero, giant hero, etc.) which are
//! defined in evo_hero_abilities.json. Champions use data from:
//!   - cards_stats_spell.json (SkeletonKingGraveyard, Deflect, GoldenKnightShield)
//!   - cards_stats_character_buff.json (ArcherQueenRapid, GoldenKnightCharge, ShieldBoostMonk)
//!   - cards_stats_characters.json (dash_damage, dash_count for GK)
//!   - cards_stats_building.json (MightyMinerBomb death_damage/radius)
//!
//! Activation flow:
//!   1. Python calls activate_hero(entity_id)
//!   2. hero_system.rs checks data.heroes first (Hero Cards)
//!   3. If not found, delegates to champion_system::activate_champion()
//!   4. We match on the entity's CharacterStats.ability field
//!   5. Apply effects using existing buff/spell/dash systems
//!
//! Champion abilities:
//!   SkeletonKing         — Summon: spawn SkeletonKingGraveyard zone (skeletons)
//!   ArcherQueenRapid     — Buff: invisibility + 2.8× attack speed - 25% move speed
//!   Deflect (Monk)       — Buff: 80% damage reduction for 4 seconds
//!   GoldenKnightChain    — Dash: chain-dash through up to 10 enemies
//!   MightyMinerLaneSwitch — Teleport to opposite lane + drop bomb

use crate::data_types::GameData;
use crate::entities::*;
use crate::game_state::*;
use crate::combat;

/// Ticks per second.
const TPS: i32 = 20;

fn ms_to_ticks(ms: i32) -> i32 {
    (ms * TPS + 999) / 1000
}

// =========================================================================
// Public API
// =========================================================================

/// Check if an entity's ability field matches a known champion ability.
pub fn is_champion(ability: &str) -> bool {
    matches!(ability,
        "SkeletonKing" | "ArcherQueenRapid" | "Deflect" | "MegaDeflect" |
        "GoldenKnightChain" | "MightyMinerLaneSwitch" | "SuperHogJump"
    )
}

/// Activate a champion ability. Called from hero_system when the hero
/// def lookup fails but the entity has an ability field.
///
/// Returns Ok(()) on success, Err(message) on failure.
pub fn activate_champion(
    state: &mut GameState,
    data: &GameData,
    entity_idx: usize,
) -> Result<(), String> {
    let entity = &state.entities[entity_idx];
    if !entity.alive {
        return Err("Entity is dead".to_string());
    }

    // Read the ability name from CharacterStats via the card_key
    let card_key = entity.card_key.clone();
    let ability = data.characters.get(&card_key)
        .and_then(|s| s.ability.as_ref())
        .cloned()
        .ok_or_else(|| format!("No ability field for card_key: {}", card_key))?;

    // Read entity state before mutable operations
    let ex = entity.x;
    let ey = entity.y;
    let eteam = entity.team;
    let eid = entity.id;

    match ability.as_str() {
        "SkeletonKing" => activate_skeleton_king(state, data, entity_idx, ex, ey, eteam),
        "ArcherQueenRapid" => activate_archer_queen(state, entity_idx),
        "Deflect" | "MegaDeflect" => activate_monk_deflect(state, data, entity_idx),
        "GoldenKnightChain" => activate_golden_knight(state, data, entity_idx, ex, ey, eteam, eid),
        "MightyMinerLaneSwitch" => activate_mighty_miner(state, data, entity_idx, ex, ey, eteam),
        "SuperHogJump" => Ok(()), // Super Hog Rider — not a standard champion, skip
        _ => Err(format!("Unknown champion ability: {}", ability)),
    }
}

/// Tick champion-specific per-frame logic (e.g., Golden Knight chain dash progression).
/// Called from engine.rs each tick.
pub fn tick_champions(state: &mut GameState, _data: &GameData) {
    // Golden Knight chain dash: if a GK is in chain-dash state, process next hop
    let entity_count = state.entities.len();
    let mut dash_events: Vec<(usize, i32, i32, i32, EntityId)> = Vec::new(); // (idx, tx, ty, damage, target_id)

    for i in 0..entity_count {
        let entity = &state.entities[i];
        if !entity.alive {
            continue;
        }
        if let Some(ref hs) = entity.hero_state {
            if !hs.ability_active {
                continue;
            }
        } else {
            continue;
        }

        if let EntityKind::Troop(ref t) = entity.kind {
            // Check if this is a GK in chain dash (is_dashing with dash_count > 0)
            if t.is_dashing && t.dash_timer > 0 {
                // Dash is being handled by combat.rs tick_combat dash system
                continue;
            }
            // If GK just finished a dash hop (is_dashing=false, ability still active),
            // check if there are more enemies to chain to
            if !t.is_dashing && t.dash_cooldown <= 0 {
                if let Some(ref hs) = entity.hero_state {
                    if hs.ability_active && hs.ability_remaining > 0 {
                        let card_key = &entity.card_key;
                        let is_gk = card_key == "goldenknight"
                            || card_key.contains("golden")
                            || card_key.contains("GoldenKnight");
                        if is_gk && t.dash_damage > 0 {
                            // Find nearest enemy within dash_secondary_range that hasn't been hit
                            let my_x = entity.x;
                            let my_y = entity.y;
                            let my_team = entity.team;
                            let chain_range = 5500i64; // dash_secondary_range from data
                            let chain_range_sq = chain_range * chain_range;

                            let mut best_id: Option<EntityId> = None;
                            let mut best_dist = i64::MAX;

                            for j in 0..entity_count {
                                if i == j { continue; }
                                let target = &state.entities[j];
                                if !target.alive || target.team == my_team {
                                    continue;
                                }
                                if !target.is_troop() && !target.is_building() {
                                    continue;
                                }
                                if target.deploy_timer > 0 {
                                    continue;
                                }
                                let dx = (my_x - target.x) as i64;
                                let dy = (my_y - target.y) as i64;
                                let dist = dx * dx + dy * dy;
                                if dist <= chain_range_sq && dist < best_dist {
                                    best_dist = dist;
                                    best_id = Some(target.id);
                                }
                            }

                            if let Some(tid) = best_id {
                                // Find target position
                                if let Some(te) = state.entities.iter().find(|e| e.id == tid) {
                                    dash_events.push((i, te.x, te.y, t.dash_damage, tid));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Apply chain dash hops
    for (idx, tx, ty, damage, target_id) in dash_events {
        if let EntityKind::Troop(ref mut t) = state.entities[idx].kind {
            t.is_dashing = true;
            t.dash_timer = 4; // Quick hop (~200ms)
            t.dash_target_x = tx;
            t.dash_target_y = ty;
            t.dash_impact_damage = damage;
        }
        // Set target to the dash target
        state.entities[idx].target = Some(target_id);
        // Decrement ability remaining (used as chain count)
        if let Some(ref mut hs) = state.entities[idx].hero_state {
            hs.ability_remaining -= 1;
            if hs.ability_remaining <= 0 {
                hs.ability_active = false;
            }
        }
    }
}

// =========================================================================
// Individual champion ability implementations
// =========================================================================

/// Skeleton King: spawn a SkeletonKingGraveyard spell zone centered on the SK.
/// Data: life_duration=10000ms, radius=4000, spawn_character=SkeletonKingSkeleton,
///       spawn_interval=250ms, spawn_initial_delay=250ms
fn activate_skeleton_king(
    state: &mut GameState,
    data: &GameData,
    entity_idx: usize,
    ex: i32,
    ey: i32,
    eteam: Team,
) -> Result<(), String> {
    // Read spell data for SkeletonKingGraveyard
    let spell = data.spells.get("SkeletonKingGraveyard");
    let (duration_ms, radius, spawn_key, spawn_interval_ms, spawn_delay_ms) = if let Some(s) = spell {
        (
            s.life_duration,
            s.radius,
            s.spawn_character.clone().unwrap_or_else(|| "SkeletonKingSkeleton".to_string()),
            s.spawn_interval,
            if s.spawn_initial_delay > 0 { s.spawn_initial_delay } else { s.spawn_interval },
        )
    } else {
        // Fallback to hardcoded values from the data
        (10000, 4000, "SkeletonKingSkeleton".to_string(), 250, 250)
    };

    let duration_ticks = ms_to_ticks(duration_ms);
    let spawn_interval_ticks = ms_to_ticks(spawn_interval_ms);
    let spawn_delay_ticks = ms_to_ticks(spawn_delay_ms);

    // Determine level from entity
    let level = if let EntityKind::Troop(ref t) = state.entities[entity_idx].kind {
        t.level
    } else {
        1
    };

    // Create the graveyard zone centered on SK's position
    let id = state.alloc_id();
    let zone = Entity::new_spell_zone(
        id, eteam, "SkeletonKingGraveyard",
        ex, ey, radius, duration_ticks,
        0,    // no direct damage
        0,    // no hit interval (spawner only)
        true, // affects air
        true, // affects ground
        None, // no buff
        0,    // no buff duration
        false, false, 0, 0, // only_enemies, only_own, ct_pct, attract
        Some(spawn_key), spawn_interval_ticks, spawn_delay_ticks, level,
        false, 0, 0, 0, // hit_biggest_targets, max_hit_targets, projectile_damage, projectile_ct_pct
        None, // no spell_projectile_key
        3000, // spawn_min_radius (SkeletonKingGraveyard=3000, same ring as regular Graveyard)
        0, // heal_per_hit
        0, false, // no pushback
        0, 0,     // no distance-scaled pushback
        false, // no_effect_to_crown_towers (SK graveyard has no crown tower interaction)
        false, // affects_hidden (spawner zone, not relevant)
        1, 1,  // level_scale: spawner zone, no buff DOT
    );
    state.entities.push(zone);

    // Mark ability as active
    if let Some(ref mut hs) = state.entities[entity_idx].hero_state {
        hs.ability_active = true;
        hs.ability_remaining = duration_ticks;
    }

    Ok(())
}

/// Archer Queen: apply ArcherQueenRapid buff (invisibility + 2.8× attack speed).
/// Data: hit_speed_multiplier=280 (+180%), speed_multiplier=-25, invisible=true
fn activate_archer_queen(
    state: &mut GameState,
    entity_idx: usize,
) -> Result<(), String> {
    // Buff duration: ~5 seconds (100 ticks) — standard AQ ability duration
    let duration_ticks = 100;

    let buff = ActiveBuff {
        key: "ArcherQueenRapid".to_string(),
        remaining_ticks: duration_ticks,
        speed_percent: -25,       // -25% movement speed (from data: speed_multiplier=-25)
        hitspeed_percent: 180,    // +180% attack speed (from data: hit_speed_multiplier=280 → 280-100=180)
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
        invisible: true,  // AQ is invisible during ability (from data: ArcherQueenRapid.invisible=True)
        taunt_target: None,
        death_spawn: None,
        death_spawn_count: 0,
        death_spawn_is_enemy: false,
        remove_on_attack: false,
        spawn_speed_multiplier: 0,
        enable_stacking: false,
        allowed_over_heal_perc: 0,
        hp_multiplier: 0,
    };

    state.entities[entity_idx].add_buff(buff);

    // Mark ability as active
    if let Some(ref mut hs) = state.entities[entity_idx].hero_state {
        hs.ability_active = true;
        hs.ability_remaining = duration_ticks;
    }

    // AQ invisibility is now handled via the invisible=true field on the ActiveBuff.
    // Entity::is_invisible() checks active buffs for invisible=true.

    Ok(())
}

/// Monk: Deflect — 80% damage reduction for 4 seconds.
/// Data: ShieldBoostMonk buff: damage_reduction=80, Deflect spell: life_duration=4000ms
fn activate_monk_deflect(
    state: &mut GameState,
    data: &GameData,
    entity_idx: usize,
) -> Result<(), String> {
    // Read duration from Deflect spell data
    let duration_ms = data.spells.get("Deflect")
        .map(|s| s.life_duration)
        .unwrap_or(4000);
    let duration_ticks = ms_to_ticks(duration_ms);

    // Read damage reduction from ShieldBoostMonk buff data
    let damage_red = data.buffs.get("ShieldBoostMonk")
        .map(|b| b.damage_reduction)
        .unwrap_or(80);

    let buff = ActiveBuff {
        key: "ShieldBoostMonk".to_string(),
        remaining_ticks: duration_ticks,
        speed_percent: 0,
        hitspeed_percent: 0,
        damage_percent: 0,
        damage_reduction: damage_red,
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
    };

    state.entities[entity_idx].add_buff(buff);

    if let Some(ref mut hs) = state.entities[entity_idx].hero_state {
        hs.ability_active = true;
        hs.ability_remaining = duration_ticks;
    }

    Ok(())
}

/// Golden Knight: chain dash through up to 10 enemies.
/// Data: dash_damage=310 (level-scaled), dash_count=10, dash_secondary_range=5500,
///       dash_push_back=2000, dash_immune_to_damage_time=100ms
fn activate_golden_knight(
    state: &mut GameState,
    _data: &GameData,
    entity_idx: usize,
    ex: i32,
    ey: i32,
    eteam: Team,
    _eid: EntityId,
) -> Result<(), String> {
    // Read dash parameters from the entity's TroopData (already level-scaled)
    let (dash_damage, dash_count) = if let EntityKind::Troop(ref t) = state.entities[entity_idx].kind {
        (t.dash_damage, if t.dash_count > 0 { t.dash_count } else { 10 }) // fallback 10 if data missing
    } else {
        return Err("Golden Knight is not a troop".to_string());
    };

    if dash_damage <= 0 {
        return Err("Golden Knight has no dash_damage".to_string());
    }

    // Find the nearest enemy to start the chain
    let entity_count = state.entities.len();
    let chain_range_sq = 5500i64 * 5500;
    let mut best_id: Option<EntityId> = None;
    let mut best_dist = i64::MAX;

    for i in 0..entity_count {
        if i == entity_idx { continue; }
        let target = &state.entities[i];
        if !target.alive || target.team == eteam || target.deploy_timer > 0 {
            continue;
        }
        if !target.is_troop() && !target.is_building() {
            continue;
        }
        let dx = (ex - target.x) as i64;
        let dy = (ey - target.y) as i64;
        let dist = dx * dx + dy * dy;
        if dist <= chain_range_sq && dist < best_dist {
            best_dist = dist;
            best_id = Some(target.id);
        }
    }

    // Start the first dash hop
    if let Some(tid) = best_id {
        let target_pos = state.entities.iter()
            .find(|e| e.id == tid)
            .map(|e| (e.x, e.y));

        if let Some((tx, ty)) = target_pos {
            if let EntityKind::Troop(ref mut t) = state.entities[entity_idx].kind {
                t.is_dashing = true;
                t.dash_timer = 4; // Quick hop (~200ms)
                t.dash_target_x = tx;
                t.dash_target_y = ty;
                t.dash_impact_damage = dash_damage;
            }
            state.entities[entity_idx].target = Some(tid);
        }
    }

    // Mark ability active with remaining = dash_count (decremented per hop in tick_champions)
    if let Some(ref mut hs) = state.entities[entity_idx].hero_state {
        hs.ability_active = true;
        hs.ability_remaining = dash_count;
    }

    // Apply speed buff during chain dash
    state.entities[entity_idx].add_buff(ActiveBuff {
        key: "GoldenKnightCharge".to_string(),
        remaining_ticks: dash_count * 10, // ~5 seconds max
        speed_percent: 100, // 2× speed (from data: speed_multiplier=200 → delta=+100)
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

    Ok(())
}

/// Mighty Miner: teleport to opposite lane + drop bomb at old position.
/// Data: MightyMinerBomb: death_damage=334, death_damage_radius=3000
fn activate_mighty_miner(
    state: &mut GameState,
    data: &GameData,
    entity_idx: usize,
    ex: i32,
    ey: i32,
    eteam: Team,
) -> Result<(), String> {
    // Lane switch: flip X position to opposite side of arena
    // If on left side (X < 0), move to right side, and vice versa
    let new_x = -ex;
    state.entities[entity_idx].x = new_x;
    // Y stays the same (same vertical position, different lane)

    // Drop bomb at old position
    // MightyMinerBomb: death_damage=334, death_damage_radius=3000
    let (bomb_damage, bomb_radius) = data.buildings.get("MightyMinerBomb")
        .map(|b| (b.death_damage, b.death_damage_radius))
        .unwrap_or((334, 3000));

    // Level-scale bomb damage
    let bomb_damage_scaled = if let EntityKind::Troop(ref t) = state.entities[entity_idx].kind {
        let base_char_dmg = data.characters.get(&state.entities[entity_idx].card_key)
            .map(|c| c.damage)
            .unwrap_or(40);
        if base_char_dmg > 0 {
            (bomb_damage as i64 * state.entities[entity_idx].damage as i64 / base_char_dmg as i64) as i32
        } else {
            bomb_damage
        }
    } else {
        bomb_damage
    };

    // Apply AoE damage at old position (bomb explosion)
    let radius_sq = (bomb_radius as i64) * (bomb_radius as i64);
    for entity in state.entities.iter_mut() {
        if !entity.alive || entity.team == eteam {
            continue;
        }
        if entity.deploy_timer > 0 {
            continue;
        }
        if matches!(entity.kind, EntityKind::Projectile(_) | EntityKind::SpellZone(_)) {
            continue;
        }
        let dx = (entity.x - ex) as i64;
        let dy = (entity.y - ey) as i64;
        let dist = dx * dx + dy * dy;
        if dist <= radius_sq {
            combat::apply_damage_to_entity(entity, bomb_damage_scaled);
        }
    }

    // Also damage towers in bomb radius
    let enemy_towers = combat::enemy_tower_ids(eteam);
    for tid in &enemy_towers {
        if let Some(tpos) = combat::tower_pos(state, *tid) {
            let dx = (ex - tpos.0) as i64;
            let dy = (ey - tpos.1) as i64;
            if dx * dx + dy * dy <= radius_sq {
                combat::apply_damage_to_tower(state, *tid, bomb_damage_scaled);
            }
        }
    }

    // Mark ability as active briefly (for animation)
    if let Some(ref mut hs) = state.entities[entity_idx].hero_state {
        hs.ability_active = true;
        hs.ability_remaining = 10; // 0.5s animation
    }

    // Reset targeting and damage ramp after lane switch.
    // The ability is a combat reset — even though the ramp would usually
    // reset naturally when a new target is acquired, explicitly zeroing it
    // handles the edge case where the same entity is in range from both lanes.
    state.entities[entity_idx].target = None;
    if let EntityKind::Troop(ref mut t) = state.entities[entity_idx].kind {
        t.ramp_ticks = 0;
        t.ramp_target = None;
    }

    Ok(())
}
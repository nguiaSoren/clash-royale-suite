//! Evolution ability handlers — one per effect_type enum variant
//!
//! Data from evo_hero_abilities.json → GameData.evolutions
//!
//! Trigger conditions (from data):
//!   always_active           — continuous aura each tick
//!   on_deploy               — fires once when deploy_timer hits 0
//!   on_each_attack          — fires after every attack lands
//!   on_kill                 — fires when this entity kills something
//!   on_death                — fires when this entity dies
//!   after_first_hit         — fires once after first time damaged
//!   when_enemies_in_range   — conditional per-tick (Archers power shot)
//!   while_charging          — while entity is in charge state (Battle Ram)
//!   while_not_attacking     — while entity has no target / not attacking
//!   when_below_half_hp      — fires once when HP drops below 50%
//!   when_below_75_percent_hp — fires once when HP drops below 75%
//!   when_shield_destroyed   — fires once when shield_hp hits 0
//!   when_shield_destroyed_and_traveled_distance — compound condition
//!   on_skeleton_death       — fires when a nearby skeleton dies (Witch evo)
//!   on_attack_or_damage_taken — fires on attack OR when taking damage
//!
//! Effect types (from data):
//!   damage_buff, speed_buff, hitspeed_buff, damage_reduction
//!   slow, freeze, stun, knockback
//!   heal, area_damage, area_pull
//!   spawn_unit, projectile_bounce, projectile_chain
//!   respawn, custom

use crate::data_types::{GameData, EvoEffect};
use crate::entities::*;
use crate::game_state::*;

// =========================================================================
// Constants
// =========================================================================

/// Ticks per second (for ms → tick conversion).
const TPS: i32 = 20;

/// Convert milliseconds to ticks, rounding up.
fn ms_to_ticks(ms: i32) -> i32 {
    (ms * TPS + 999) / 1000
}

// =========================================================================
// Public API — called from engine.rs each tick
// =========================================================================

/// Main evo ability tick. Processes all evolved entities.
pub fn tick_evo_abilities(state: &mut GameState, data: &GameData) {
    // NOTE: tick_buffs is called from engine.rs step 9d (not here) to avoid
    // double-ticking buff timers. Previously this was called here too, causing
    // buffs to expire at 2× speed.

    // Phase 2: Process evo abilities for evolved entities
    // We need to collect actions to avoid borrow issues.
    let mut actions: Vec<EvoAction> = Vec::new();

    let entity_count = state.entities.len();
    for i in 0..entity_count {
        let entity = &state.entities[i];
        if !entity.alive {
            continue;
        }

        // Only process troops with evo_state
        let evo_state = match &entity.evo_state {
            Some(es) => es.clone(),
            None => continue,
        };

        let is_evolved = match &entity.kind {
            EntityKind::Troop(t) => t.is_evolved,
            EntityKind::Building(_) => entity.evo_state.is_some(),
            _ => continue,
        };
        if !is_evolved {
            continue;
        }

        // Look up evo ability data
        let evo_def = match data.evolutions.get(&entity.card_key) {
            Some(e) => e,
            None => continue,
        };

        let trigger = &evo_def.ability.trigger.condition;
        let entity_id = entity.id;
        let entity_team = entity.team;
        let entity_x = entity.x;
        let entity_y = entity.y;
        let entity_hp = entity.hp;
        let entity_max_hp = entity.max_hp;

        // Check trigger condition
        let should_fire = match trigger.as_str() {
            "always_active" => true,

            "on_deploy" => {
                entity.deploy_timer <= 0 && !evo_state.deploy_fired
            }

            // on_each_attack, on_kill, on_death handled via notify_ callbacks
            "on_each_attack" | "on_kill" | "on_death" | "on_skeleton_death" => false,

            "after_first_hit" => {
                evo_state.been_hit && evo_state.attack_count == 0
            }

            "when_enemies_in_range" => true,

            "while_charging" => entity.target.is_some(),

            "while_not_attacking" => entity.target.is_none(),

            "when_below_half_hp" => {
                entity_hp * 2 <= entity_max_hp && evo_state.cooldown <= 0
            }

            "when_below_75_percent_hp" => {
                entity_hp * 4 <= entity_max_hp * 3 && evo_state.cooldown <= 0
            }

            "when_shield_destroyed" => {
                evo_state.shield_destroyed && evo_state.cooldown <= 0
            }

            "when_shield_destroyed_and_traveled_distance" => {
                evo_state.shield_destroyed && evo_state.distance_traveled > 3000
                    && evo_state.cooldown <= 0
            }

            "on_attack_or_damage_taken" => false,

            _ => false,
        };

        if should_fire {
            for effect in &evo_def.ability.effects {
                if let Some(action) = build_evo_action(
                    effect, entity_id, entity_team, entity_x, entity_y,
                    entity_hp, entity_max_hp, i,
                ) {
                    actions.push(action);
                }
            }

            // Mark deploy fired
            if trigger == "on_deploy" {
                if let Some(ref mut es) = state.entities[i].evo_state {
                    es.deploy_fired = true;
                }
            }

            // Set cooldown for one-shot conditional triggers
            if matches!(trigger.as_str(),
                "when_below_half_hp" | "when_below_75_percent_hp" |
                "when_shield_destroyed" | "when_shield_destroyed_and_traveled_distance"
            ) {
                if let Some(ref mut es) = state.entities[i].evo_state {
                    es.cooldown = i32::MAX; // One-shot: never fires again
                }
            }
        }

        // Tick cooldown
        if let Some(ref mut es) = state.entities[i].evo_state {
            if es.cooldown > 0 && es.cooldown < i32::MAX {
                es.cooldown -= 1;
            }
            // Update distance tracking
            let dx = (entity_x - es.prev_x).abs();
            let dy = (entity_y - es.prev_y).abs();
            es.distance_traveled += dx + dy;
            es.prev_x = entity_x;
            es.prev_y = entity_y;
        }
    }

    // Phase 3: Execute collected actions
    execute_evo_actions(state, data, &actions);
}

// =========================================================================
// Notification hooks — called from combat.rs
// =========================================================================

/// Called when an evolved entity attacks. Fires on_each_attack effects.
/// Works for both troops and buildings that have evo_state (e.g., Furnace evo).
pub fn notify_evo_attack(state: &mut GameState, data: &GameData, attacker_idx: usize) {
    if attacker_idx >= state.entities.len() {
        return;
    }
    let entity = &state.entities[attacker_idx];
    if !entity.alive || entity.evo_state.is_none() {
        return;
    }

    // Check if entity is evolved — troops have is_evolved flag,
    // buildings have evo_state set at deploy time
    let has_evo = match &entity.kind {
        EntityKind::Troop(t) => t.is_evolved,
        EntityKind::Building(_) => entity.evo_state.is_some(),
        _ => false,
    };
    if !has_evo {
        return;
    }

    let evo_def = match data.evolutions.get(&entity.card_key) {
        Some(e) => e,
        None => return,
    };

    let trigger = &evo_def.ability.trigger.condition;
    if trigger != "on_each_attack" && trigger != "on_attack_or_damage_taken" {
        return;
    }

    // Increment attack count
    if let Some(ref mut es) = state.entities[attacker_idx].evo_state {
        es.attack_count += 1;
    }

    let entity = &state.entities[attacker_idx];
    let eid = entity.id;
    let eteam = entity.team;
    let ex = entity.x;
    let ey = entity.y;
    let ehp = entity.hp;
    let emhp = entity.max_hp;

    let mut actions: Vec<EvoAction> = Vec::new();
    for effect in &evo_def.ability.effects {
        if let Some(action) = build_evo_action(effect, eid, eteam, ex, ey, ehp, emhp, attacker_idx) {
            actions.push(action);
        }
    }
    execute_evo_actions(state, data, &actions);
}

/// Called when an evolved building spawns units (e.g., Furnace evo).
/// Treats the spawn event as an "attack" for on_each_attack triggers.
pub fn tick_evo_building_spawn(state: &mut GameState, data: &GameData, building_idx: usize) {
    // Delegate to notify_evo_attack — it now handles buildings too
    notify_evo_attack(state, data, building_idx);
}

/// Called when an evolved entity takes damage.
pub fn notify_evo_damaged(state: &mut GameState, data: &GameData, entity_idx: usize) {
    if entity_idx >= state.entities.len() {
        return;
    }
    let entity = &state.entities[entity_idx];
    if !entity.alive || entity.evo_state.is_none() {
        return;
    }

    // Mark been_hit
    if let Some(ref mut es) = state.entities[entity_idx].evo_state {
        es.been_hit = true;
    }

    // Check shield destroyed
    if state.entities[entity_idx].shield_hp <= 0 {
        if let Some(ref mut es) = state.entities[entity_idx].evo_state {
            es.shield_destroyed = true;
        }
    }

    let entity = &state.entities[entity_idx];
    let has_evo = match &entity.kind {
        EntityKind::Troop(t) => t.is_evolved,
        EntityKind::Building(_) => entity.evo_state.is_some(),
        _ => return,
    };
    if !has_evo {
        return;
    }

    let evo_def = match data.evolutions.get(&entity.card_key) {
        Some(e) => e,
        None => return,
    };

    let trigger = &evo_def.ability.trigger.condition;
    let should_fire = match trigger.as_str() {
        "on_attack_or_damage_taken" => true,
        "after_first_hit" => {
            state.entities[entity_idx].evo_state.as_ref()
                .map_or(false, |es| es.attack_count == 0)
        }
        _ => false,
    };

    if !should_fire {
        return;
    }

    let entity = &state.entities[entity_idx];
    let eid = entity.id;
    let eteam = entity.team;
    let ex = entity.x;
    let ey = entity.y;
    let ehp = entity.hp;
    let emhp = entity.max_hp;

    let mut actions: Vec<EvoAction> = Vec::new();
    for effect in &evo_def.ability.effects {
        if let Some(action) = build_evo_action(effect, eid, eteam, ex, ey, ehp, emhp, entity_idx) {
            actions.push(action);
        }
    }

    if trigger == "after_first_hit" {
        if let Some(ref mut es) = state.entities[entity_idx].evo_state {
            es.attack_count = 1; // Prevent re-triggering
        }
    }

    execute_evo_actions(state, data, &actions);
}

/// Called when an evolved entity kills something.
pub fn notify_evo_kill(state: &mut GameState, data: &GameData, killer_idx: usize) {
    if killer_idx >= state.entities.len() {
        return;
    }

    // Check alive and has evo_state first (no long-lived borrow)
    if !state.entities[killer_idx].alive || state.entities[killer_idx].evo_state.is_none() {
        return;
    }

    // Mutable update first, before any immutable borrows
    if let Some(ref mut es) = state.entities[killer_idx].evo_state {
        es.kill_count += 1;
    }

    // Now take immutable borrow for the rest
    let entity = &state.entities[killer_idx];
    let has_evo = match &entity.kind {
        EntityKind::Troop(t) => t.is_evolved,
        EntityKind::Building(_) => entity.evo_state.is_some(),
        _ => return,
    };
    if !has_evo {
        return;
    }

    let evo_def = match data.evolutions.get(&entity.card_key) {
        Some(e) => e,
        None => return,
    };

    if evo_def.ability.trigger.condition != "on_kill" {
        return;
    }

    let entity = &state.entities[killer_idx];
    let eid = entity.id;
    let eteam = entity.team;
    let ex = entity.x;
    let ey = entity.y;
    let ehp = entity.hp;
    let emhp = entity.max_hp;

    let mut actions: Vec<EvoAction> = Vec::new();
    for effect in &evo_def.ability.effects {
        if let Some(action) = build_evo_action(effect, eid, eteam, ex, ey, ehp, emhp, killer_idx) {
            actions.push(action);
        }
    }
    execute_evo_actions(state, data, &actions);
}

/// Called when an evolved entity dies.
pub fn notify_evo_death(state: &mut GameState, data: &GameData, entity_idx: usize) {
    if entity_idx >= state.entities.len() {
        return;
    }
    let entity = &state.entities[entity_idx];
    if entity.evo_state.is_none() {
        return;
    }

    let has_evo = match &entity.kind {
        EntityKind::Troop(t) => t.is_evolved,
        EntityKind::Building(_) => entity.evo_state.is_some(),
        _ => return,
    };
    if !has_evo {
        return;
    }

    let evo_def = match data.evolutions.get(&entity.card_key) {
        Some(e) => e,
        None => return,
    };

    if evo_def.ability.trigger.condition != "on_death" {
        return;
    }

    let entity = &state.entities[entity_idx];
    let eid = entity.id;
    let eteam = entity.team;
    let ex = entity.x;
    let ey = entity.y;
    let ehp = entity.hp;
    let emhp = entity.max_hp;

    let mut actions: Vec<EvoAction> = Vec::new();
    for effect in &evo_def.ability.effects {
        if let Some(action) = build_evo_action(effect, eid, eteam, ex, ey, ehp, emhp, entity_idx) {
            actions.push(action);
        }
    }
    execute_evo_actions(state, data, &actions);
}

// =========================================================================
// Stat modifier application — called when spawning an evolved troop
// =========================================================================

/// Apply evo stat modifiers to a freshly spawned entity.
pub fn apply_evo_stat_modifiers(entity: &mut Entity, data: &GameData) {
    let evo_def = match data.evolutions.get(&entity.card_key) {
        Some(e) => e,
        None => return,
    };

    let mods = &evo_def.stat_modifiers;

    if let Some(hp_mult) = mods.hitpoints_multiplier {
        entity.max_hp = (entity.max_hp as f64 * hp_mult) as i32;
        entity.hp = entity.max_hp;
    }

    if let Some(dmg_mult) = mods.damage_multiplier {
        entity.damage = (entity.damage as f64 * dmg_mult) as i32;
    }

    if let Some(shield_hp) = mods.shield_hitpoints {
        entity.shield_hp = shield_hp;
    }

    if let Some(spd) = mods.speed_override {
        if let EntityKind::Troop(ref mut t) = entity.kind {
            t.speed = speed_to_units_per_tick(spd);
        }
    }

    if let Some(hs_mult) = mods.hit_speed_multiplier {
        if let EntityKind::Troop(ref mut t) = entity.kind {
            t.hit_speed = (t.hit_speed as f64 / hs_mult) as i32;
        }
    }

    if let Some(range) = mods.range_override {
        if let EntityKind::Troop(ref mut t) = entity.kind {
            t.range_sq = range_squared(range);
        }
    }
}

// =========================================================================
// Internal: Action types and execution
// =========================================================================

#[derive(Debug, Clone)]
enum EvoAction {
    BuffSelf {
        source_idx: usize,
        buff: ActiveBuff,
    },
    BuffEnemiesInRadius {
        team: Team,
        x: i32,
        y: i32,
        radius: i32,
        buff: ActiveBuff,
        affects_air: bool,
        affects_ground: bool,
    },
    BuffAlliesInRadius {
        team: Team,
        x: i32,
        y: i32,
        radius: i32,
        buff: ActiveBuff,
        affects_air: bool,
        affects_ground: bool,
    },
    AreaDamage {
        team: Team,
        x: i32,
        y: i32,
        radius: i32,
        damage: i32,
        affects_air: bool,
        affects_ground: bool,
    },
    SpawnUnit {
        team: Team,
        x: i32,
        y: i32,
        character_key: String,
        count: i32,
        level: usize,
    },
    HealSelf {
        source_idx: usize,
        amount: i32,
        allow_overheal: bool,
    },
    AreaPull {
        team: Team,
        x: i32,
        y: i32,
        radius: i32,
        strength: i32,
        affects_air: bool,
        affects_ground: bool,
    },
}

/// Build an EvoAction from an EvoEffect definition.
fn build_evo_action(
    effect: &EvoEffect,
    entity_id: EntityId,
    entity_team: Team,
    entity_x: i32,
    entity_y: i32,
    _entity_hp: i32,
    entity_max_hp: i32,
    entity_idx: usize,
) -> Option<EvoAction> {
    let effect_type = effect.effect_type.as_str();
    let target = effect.target.as_str();
    let radius = effect.radius.unwrap_or(0);
    let value = effect.value.unwrap_or(0.0) as i32;
    let duration_ticks = effect.duration_ms.map(ms_to_ticks).unwrap_or(i32::MAX);
    let damage = effect.damage.unwrap_or(0);

    match effect_type {
        "damage_buff" => {
            let buff = make_buff(
                &format!("evo_dmg_{}", entity_id.0), duration_ticks,
                0, 0, value, 0, 0, false, false,
            );
            match target {
                "self" => Some(EvoAction::BuffSelf { source_idx: entity_idx, buff }),
                _ => Some(EvoAction::BuffEnemiesInRadius {
                    team: entity_team, x: entity_x, y: entity_y, radius, buff,
                    affects_air: effect.affects_air.unwrap_or(true),
                    affects_ground: effect.affects_ground.unwrap_or(true),
                }),
            }
        }

        "speed_buff" => {
            let buff = make_buff(
                &format!("evo_spd_{}", entity_id.0), duration_ticks,
                value, 0, 0, 0, 0, false, false,
            );
            match target {
                "self" => Some(EvoAction::BuffSelf { source_idx: entity_idx, buff }),
                "allies_in_radius" => Some(EvoAction::BuffAlliesInRadius {
                    team: entity_team, x: entity_x, y: entity_y, radius, buff,
                    affects_air: effect.affects_air.unwrap_or(true),
                    affects_ground: effect.affects_ground.unwrap_or(true),
                }),
                _ => Some(EvoAction::BuffSelf { source_idx: entity_idx, buff }),
            }
        }

        "hitspeed_buff" => {
            let buff = make_buff(
                &format!("evo_hs_{}", entity_id.0), duration_ticks,
                0, value, 0, 0, 0, false, false,
            );
            Some(EvoAction::BuffSelf { source_idx: entity_idx, buff })
        }

        "damage_reduction" => {
            let buff = make_buff(
                &format!("evo_dr_{}", entity_id.0), duration_ticks,
                0, 0, 0, value, 0, false, false,
            );
            Some(EvoAction::BuffSelf { source_idx: entity_idx, buff })
        }

        "slow" => {
            let buff = make_buff(
                &format!("evo_slow_{}", entity_id.0), duration_ticks,
                -value, 0, 0, 0, 0, false, false,
            );
            Some(EvoAction::BuffEnemiesInRadius {
                team: entity_team, x: entity_x, y: entity_y, radius, buff,
                affects_air: effect.affects_air.unwrap_or(true),
                affects_ground: effect.affects_ground.unwrap_or(true),
            })
        }

        "freeze" => {
            let buff = make_buff(
                &format!("evo_frz_{}", entity_id.0), duration_ticks,
                0, 0, 0, 0, 0, false, true,
            );
            Some(EvoAction::BuffEnemiesInRadius {
                team: entity_team, x: entity_x, y: entity_y, radius, buff,
                affects_air: effect.affects_air.unwrap_or(true),
                affects_ground: effect.affects_ground.unwrap_or(true),
            })
        }

        "stun" => {
            let buff = make_buff(
                &format!("evo_stun_{}", entity_id.0), duration_ticks,
                0, 0, 0, 0, 0, true, false,
            );
            Some(EvoAction::BuffEnemiesInRadius {
                team: entity_team, x: entity_x, y: entity_y, radius, buff,
                affects_air: effect.affects_air.unwrap_or(true),
                affects_ground: effect.affects_ground.unwrap_or(true),
            })
        }

        "area_damage" => {
            Some(EvoAction::AreaDamage {
                team: entity_team, x: entity_x, y: entity_y,
                radius, damage,
                affects_air: effect.affects_air.unwrap_or(true),
                affects_ground: effect.affects_ground.unwrap_or(true),
            })
        }

        "spawn_unit" => {
            let char_key = effect.spawn_character.as_ref()?.clone();
            let count = effect.spawn_count.unwrap_or(1);
            Some(EvoAction::SpawnUnit {
                team: entity_team, x: entity_x, y: entity_y,
                character_key: char_key, count, level: 11,
            })
        }

        "heal" => {
            let heal_amount = if value > 0 {
                entity_max_hp * value / 100
            } else {
                entity_max_hp / 10
            };
            let count = effect.spawn_count.unwrap_or(1);
            Some(EvoAction::HealSelf {
                source_idx: entity_idx,
                amount: heal_amount * count,
                allow_overheal: true,
            })
        }

        "area_pull" => {
            let strength = effect.pull_strength.unwrap_or(300);
            Some(EvoAction::AreaPull {
                team: entity_team, x: entity_x, y: entity_y,
                radius, strength,
                affects_air: effect.affects_air.unwrap_or(true),
                affects_ground: effect.affects_ground.unwrap_or(true),
            })
        }

        "knockback" => {
            let buff = make_buff(
                &format!("evo_kb_{}", entity_id.0), 4,
                0, 0, 0, 0, 0, true, false,
            );
            Some(EvoAction::BuffEnemiesInRadius {
                team: entity_team, x: entity_x, y: entity_y,
                radius: radius.max(1500), buff,
                affects_air: effect.affects_air.unwrap_or(false),
                affects_ground: effect.affects_ground.unwrap_or(true),
            })
        }

        "projectile_bounce" | "projectile_chain" => {
            // Approximate as extra area damage hits
            if damage > 0 {
                Some(EvoAction::AreaDamage {
                    team: entity_team, x: entity_x, y: entity_y,
                    radius: 2500, damage,
                    affects_air: effect.affects_air.unwrap_or(true),
                    affects_ground: effect.affects_ground.unwrap_or(true),
                })
            } else {
                None
            }
        }

        "respawn" => {
            // Simplified: handled as spawn_unit with original card_key
            None
        }

        "custom" => {
            if damage > 0 {
                Some(EvoAction::AreaDamage {
                    team: entity_team,
                    x: entity_x,
                    y: entity_y + entity_team.forward_y() * 3000,
                    radius: radius.max(2000), damage,
                    affects_air: effect.affects_air.unwrap_or(true),
                    affects_ground: effect.affects_ground.unwrap_or(true),
                })
            } else {
                None
            }
        }

        // Hero-specific effects handled in hero_system.rs
        "flight" | "projectile_tornado" | "taunt" | "building_spawn" | "buff" => None,

        _ => None,
    }
}

/// Helper to construct an ActiveBuff.
fn make_buff(
    key: &str,
    remaining_ticks: i32,
    speed_percent: i32,
    hitspeed_percent: i32,
    damage_percent: i32,
    damage_reduction: i32,
    heal_per_tick: i32,
    stun: bool,
    freeze: bool,
) -> ActiveBuff {
    ActiveBuff {
        key: key.to_string(),
        remaining_ticks,
        speed_percent,
        hitspeed_percent,
        damage_percent,
        damage_reduction,
        heal_per_tick,
        damage_per_tick: 0,
                damage_hit_interval: 0,
                damage_hit_timer: 0,
        building_damage_percent: 0,
        crown_tower_damage_percent: 0,
        stun,
        freeze,
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
    }
}

/// Execute a batch of collected evo actions.
fn execute_evo_actions(state: &mut GameState, data: &GameData, actions: &[EvoAction]) {
    for action in actions {
        match action {
            EvoAction::BuffSelf { source_idx, buff } => {
                if *source_idx < state.entities.len() && state.entities[*source_idx].alive {
                    state.entities[*source_idx].add_buff(buff.clone());
                }
            }

            EvoAction::BuffEnemiesInRadius { team, x, y, radius, buff, affects_air, affects_ground } => {
                let radius_sq = (*radius as i64) * (*radius as i64);
                for entity in state.entities.iter_mut() {
                    if !entity.alive || entity.team == *team {
                        continue;
                    }
                    if entity.is_flying() && !affects_air { continue; }
                    if !entity.is_flying() && !affects_ground { continue; }
                    if entity.dist_sq_to(*x, *y) <= radius_sq {
                        entity.add_buff(buff.clone());
                    }
                }
            }

            EvoAction::BuffAlliesInRadius { team, x, y, radius, buff, affects_air, affects_ground } => {
                let radius_sq = (*radius as i64) * (*radius as i64);
                for entity in state.entities.iter_mut() {
                    if !entity.alive || entity.team != *team {
                        continue;
                    }
                    if entity.is_flying() && !affects_air { continue; }
                    if !entity.is_flying() && !affects_ground { continue; }
                    if entity.dist_sq_to(*x, *y) <= radius_sq {
                        entity.add_buff(buff.clone());
                    }
                }
            }

            EvoAction::AreaDamage { team, x, y, radius, damage, affects_air, affects_ground } => {
                let radius_sq = (*radius as i64) * (*radius as i64);
                for entity in state.entities.iter_mut() {
                    if !entity.alive || entity.team == *team {
                        continue;
                    }
                    if entity.is_flying() && !affects_air { continue; }
                    if !entity.is_flying() && !affects_ground { continue; }
                    if entity.dist_sq_to(*x, *y) <= radius_sq {
                        crate::combat::apply_damage_to_entity(entity, *damage);
                    }
                }
                // Also hit enemy towers
                let enemy_towers = crate::combat::enemy_tower_ids(*team);
                for tid in &enemy_towers {
                    if let Some(tpos) = crate::combat::tower_pos(state, *tid) {
                        let dx = (*x - tpos.0) as i64;
                        let dy = (*y - tpos.1) as i64;
                        if dx * dx + dy * dy <= radius_sq {
                            crate::combat::apply_damage_to_tower(state, *tid, *damage);
                        }
                    }
                }
            }

            EvoAction::SpawnUnit { team, x, y, character_key, count, level } => {
                if let Some(stats) = data.characters.get(character_key) {
                    let spread = stats.collision_radius.max(200);
                    for i in 0..*count {
                        let id = state.alloc_id();
                        let ox = ((i % 3) as i32 - 1) * spread;
                        let oy = ((i / 3) as i32) * spread;
                        let mut troop = Entity::new_troop(
                            id, *team, stats, x + ox, y + oy, *level, false,
                        );
                        troop.deploy_timer = 0;
                        state.entities.push(troop);
                    }
                }
            }

            EvoAction::HealSelf { source_idx, amount, allow_overheal } => {
                if *source_idx < state.entities.len() && state.entities[*source_idx].alive {
                    let e = &mut state.entities[*source_idx];
                    let max = if *allow_overheal { e.max_hp * 2 } else { e.max_hp };
                    e.hp = (e.hp + amount).min(max);
                }
            }

            EvoAction::AreaPull { team, x, y, radius, strength, affects_air, affects_ground } => {
                let radius_sq = (*radius as i64) * (*radius as i64);
                for entity in state.entities.iter_mut() {
                    if !entity.alive || entity.team == *team {
                        continue;
                    }
                    if entity.is_flying() && !affects_air { continue; }
                    if !entity.is_flying() && !affects_ground { continue; }
                    let dist = entity.dist_sq_to(*x, *y);
                    if dist <= radius_sq && dist > 0 {
                        let dx = *x - entity.x;
                        let dy = *y - entity.y;
                        let d = (dist as f64).sqrt() as i32;
                        if d > 0 {
                            entity.x += dx * *strength / d;
                            entity.y += dy * *strength / d;
                        }
                    }
                }
            }
        }
    }
}
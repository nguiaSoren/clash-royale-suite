//! Main simulation engine — the tick loop.
//!
//! Each tick (50 ms game time) processes in this order:
//!   1. Phase & elixir update
//!   2. Deploy timers countdown
//!   3. Building lifetime decay + spawner ticks
//!   4. Spell zone ticks
//!   5. Targeting (find/validate targets for troops & buildings)
//!   6. Movement (troops move towards targets or default lane path)
//!   7. Combat (melee attacks, ranged → spawn projectile)
//!   8. Projectile movement & impact
//!   9. Tower attacks
//!  10. Death processing (death spawns, death damage)
//!  11. Cleanup dead entities
//!  12. Match-end check

//!
//! Phase 2: combat logic now lives in combat.rs. Engine.rs is the orchestrator.

use crate::combat;
use crate::data_types::GameData;
use crate::entities::*;
use crate::game_state::*;
use crate::evo_system;
use crate::hero_system;
use crate::champion_system;

// =========================================================================
// Public API
// =========================================================================

/// Advance the game state by one tick.
pub fn tick(state: &mut GameState, data: &GameData) {
    if !state.is_running() {
        return;
    }

    // 0. Advance tick counter first so that update_phase() and all
    //    sub-systems see the correct tick number for this frame.
    //    Python callers reading .tick and .phase after step() will
    //    see consistent values (e.g., tick 3600 → Overtime, not DoubleElixir).
    state.tick += 1;

    // 1. Phase & elixir
    state.update_phase();
    let mult = state.elixir_multiplier();
    state.player1.tick_elixir(mult);
    state.player2.tick_elixir(mult);

    // 2. Deploy timers
    tick_deploy_timers(state, data);

    // 2b. Burrow movement (Miner, Goblin Drill dig phase)
    // Burrowing troops travel underground toward their target. They are
    // untargetable (deploy_timer=MAX) and invisible during travel. On arrival,
    // their deploy_timer is restored so the normal emerge animation plays.
    tick_burrow(state, data);

    // 3. Building lifetime & spawners
    tick_buildings(state, data);

    // 3b. Troop spawners (Witch, Night Witch — troops that spawn other troops)
    tick_troop_spawners(state, data);

    // 4. Spell zones (now tower-aware via combat.rs)
    combat::tick_spell_zones(state, data);

    // 5. Targeting (now uses tower pseudo-IDs via combat.rs)
    combat::tick_targeting(state);

    // 6. Movement (now bridge-aware via combat.rs)
    combat::tick_movement(state);

    // 6a. Attached troop sync — riders follow parent position (GoblinGiant SpearGoblins)
    tick_attached_troops(state);

    // 6b. Collision resolution — entity separation + building blocking.
    // Runs after movement so troops don't overlap each other or buildings.
    combat::tick_collisions(state);

    // 7. Combat — melee + ranged (tower damage + CT reduction via combat.rs)
    combat::tick_combat(state, data);

    // 7b. Idle buff system — unified, data-driven (buff_when_not_attacking).
    // Handles ALL buff_when_not_attacking mechanics: Royal Ghost invisibility,
    // BattleHealer self-heal, and any future idle-triggered buffs. What the buff
    // does (invisible, heal, speed boost, etc.) is determined entirely by the
    // BuffStats entry — no per-card branching. Replaces the old separate
    // tick_invisibility + tick_buff_when_not_attacking functions.
    combat::tick_idle_buff(state, data);

    // 7c. Fisherman hook special attack
    combat::tick_fisherman_hook(state, data);

    // 8. Projectile movement & impact (tower hits via combat.rs)
    combat::tick_projectiles(state, data);

    // 8b. Fix #4: Morph system — Cannon Cart shield break → stationary cannon.
    // Checks for troops whose shield just broke and morph_character is set.
    // Must run after combat (shield can break this tick) and before death processing.
    // Data-driven from CharacterStats.morph_character / heal_on_morph / morph_time.
    combat::tick_morphs(state, data);

    // 9. Tower attacks
    // 9a. Recompute tower buff state (Rage on towers: +35% attack speed).
    // Data-driven from spell zone buff hitspeed_multiplier. Must run before
    // tick_towers so the cooldown scaling uses the current-tick buff state.
    combat::tick_tower_buffs(state, data);
    combat::tick_towers(state);

    // In tick(), after combat::tick_towers(state):
    // 9b. Evo abilities (continuous + on_deploy triggers)
    evo_system::tick_evo_abilities(state, data);

    // 9c. Hero taunt enforcement + cooldown ticks
    hero_system::tick_hero_state(state);
    hero_system::enforce_taunts(state);

    // 9d. Champion ability tick (GK chain dash progression)
    champion_system::tick_champions(state, data);

    // 9e. HP-threshold buffs: buff_on50_hp and buff_on_xhp (data-driven).
    // Check all troops for HP thresholds and apply the corresponding buff
    // if they've crossed below the threshold for the first time.
    tick_hp_threshold_buffs(state, data);

    // 9d. Tick all active buffs: decrement timers, apply heal-over-time and
    //     damage-over-time (Poison DOT, etc.), remove expired buffs.
    for entity in state.entities.iter_mut() {
        if entity.alive {
            entity.tick_buffs();
        }
    }

    // 10. Death processing (death spawns + death damage via combat.rs)
    combat::tick_deaths(state, data);

    // 10b. FIX 5: Update crown counters every tick.
    // Previously crowns were only recounted inside tick_deaths, so if a tower
    // was destroyed by a projectile (tick_projectiles) or spell (tick_spell_zones)
    // but no entity died that tick, the crown count would be stale until the
    // next entity death. Now we always keep it current.
    state.player1.crowns = state.player2.recount_opponent_crowns();
    state.player2.crowns = state.player1.recount_opponent_crowns();

    // 11. Cleanup
    state.entities.retain(|e| e.alive);

    // 12. Match end check
    state.check_match_end();
}

/// Run a complete match. Returns the result.
pub fn run_match(state: &mut GameState, data: &GameData) -> MatchResult {
    while state.is_running() && state.tick < MAX_MATCH_TICKS {
        tick(state, data);
    }
    if state.is_running() {
        state.check_match_end();
    }
    state.result
}

// =========================================================================
// Subsystems that stay in engine.rs (not combat-related)
// =========================================================================

fn tick_deploy_timers(state: &mut GameState, data: &GameData) {
    // Collect spawn splash events: (x, y, radius, damage, pushback, team)
    let mut spawn_splashes: Vec<(i32, i32, i32, i32, i32, Team)> = Vec::new();
    // FIX: spawn_area_object zones to create on deploy completion
    // (team, spell_key, x, y, level)
    let mut area_object_spawns: Vec<(Team, String, i32, i32, usize)> = Vec::new();
    // starting_buff applications: (entity_index, buff_key, duration_ticks)
    let mut starting_buff_events: Vec<(usize, String, i32)> = Vec::new();

    for (ei, entity) in state.entities.iter_mut().enumerate() {
        if entity.deploy_timer > 0 {
            // Skip deploy timer countdown for burrowing troops — they are traveling
            // underground and their deploy_timer is a sentinel (i32::MAX). The real
            // deploy timer is restored by tick_burrow when the troop arrives.
            if entity.is_burrowing() {
                continue;
            }
            entity.deploy_timer -= 1;
            // Check if deploy just completed (timer hit 0 this tick)
            if entity.deploy_timer == 0 {
                if let EntityKind::Troop(ref mut t) = entity.kind {
                    if t.spawn_splash_damage > 0 && t.spawn_splash_radius > 0 && !t.spawn_splash_fired {
                        t.spawn_splash_fired = true;
                        spawn_splashes.push((
                            entity.x,
                            entity.y,
                            t.spawn_splash_radius,
                            t.spawn_splash_damage,
                            t.spawn_splash_pushback,
                            entity.team,
                        ));
                    }
                }

                // FIX: spawn_area_object — create a spell zone on deploy
                // Used by: IceWizard (IceWizardCold), BattleHealer (BattleHealerSpawnHeal)
                let card_key = entity.card_key.clone();
                let team = entity.team;
                let x = entity.x;
                let y = entity.y;
                let level = match &entity.kind {
                    EntityKind::Troop(t) => t.level,
                    _ => 11,
                };
                if let Some(stats) = data.characters.get(&card_key) {
                    if let Some(ref area_key) = stats.spawn_area_object {
                        if !area_key.is_empty() {
                            // FIX 5: Use spawn_area_object_level_index if set (1-indexed).
                            // This overrides the troop's own level for the deploy zone.
                            // When 0, inherit the troop's level (default behavior).
                            let zone_level = if stats.spawn_area_object_level_index > 0 {
                                stats.spawn_area_object_level_index as usize
                            } else {
                                level
                            };
                            area_object_spawns.push((team, area_key.clone(), x, y, zone_level));
                        }
                    }
                }
                // FIX: Also check building stats for spawn_area_object.
                // Buildings like GoblinDrill have spawn_area_object=GoblinDrillDamage
                // that should fire on deploy completion (arrival splash).
                if let Some(stats) = data.buildings.get(&card_key) {
                    if let Some(ref area_key) = stats.spawn_area_object {
                        if !area_key.is_empty() {
                            let zone_level = if stats.spawn_area_object_level_index > 0 {
                                stats.spawn_area_object_level_index as usize
                            } else {
                                level
                            };
                            area_object_spawns.push((team, area_key.clone(), x, y, zone_level));
                        }
                    }
                }

                // starting_buff: apply an innate buff when this troop finishes deploying.
                // Data-driven from CharacterStats.starting_buff + starting_buff_time.
                // Examples: troops with innate effects at spawn.
                if let Some(stats) = data.characters.get(&card_key) {
                    if let Some(ref buff_key) = stats.starting_buff {
                        if !buff_key.is_empty() && stats.starting_buff_time > 0 {
                            let duration = crate::entities::ms_to_ticks(stats.starting_buff_time);
                            starting_buff_events.push((ei, buff_key.clone(), duration));
                        }
                    }
                }
            }
        }
    }

    // Apply spawn splash damage + pushback to nearby enemies
    // Data-driven: spawn_pushback_radius = AoE radius, spawn_pushback = knockback distance.
    // MK: spawn_pushback=1800 pushes enemies radially away from landing point.
    for (sx, sy, radius, damage, pushback, team) in spawn_splashes {
        let radius_sq = (radius as i64) * (radius as i64);
        for entity in state.entities.iter_mut() {
            if !entity.alive || entity.team == team {
                continue;
            }
            // Only hit deployed troops and buildings (not other deploying entities)
            if entity.deploy_timer > 0 {
                continue;
            }
            if matches!(entity.kind, EntityKind::Projectile(_) | EntityKind::SpellZone(_)) {
                continue;
            }
            let dx = (entity.x - sx) as i64;
            let dy = (entity.y - sy) as i64;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq <= radius_sq {
                combat::apply_damage_to_entity(entity, damage);

                // Apply spawn_pushback: push enemy radially away from the landing point.
                // Data-driven from CharacterStats.spawn_pushback. Respects ignore_pushback.
                if pushback > 0 && entity.alive {
                    let is_immune = match &entity.kind {
                        EntityKind::Troop(t) => t.ignore_pushback,
                        _ => entity.is_building(), // Buildings are immovable
                    };
                    if !is_immune {
                        let dist = (dist_sq as f64).sqrt();
                        if dist > 1.0 {
                            let push_x = (dx as f64 / dist * pushback as f64) as i32;
                            let push_y = (dy as f64 / dist * pushback as f64) as i32;
                            entity.x += push_x;
                            entity.y += push_y;
                            entity.x = entity.x.clamp(-crate::game_state::ARENA_HALF_W, crate::game_state::ARENA_HALF_W);
                            entity.y = entity.y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);
                        }
                    }
                }
            }
        }
    }

    // Apply starting_buff to newly deployed entities.
    // Data-driven from CharacterStats.starting_buff + starting_buff_time.
    // Uses ActiveBuff::from_buff_stats() to wire ALL BuffStats fields automatically
    // (building_damage_percent, death_spawn, invisible, stun/freeze, etc.).
    for (idx, buff_key, duration) in starting_buff_events {
        if idx < state.entities.len() && state.entities[idx].alive {
            if let Some(bs) = data.buffs.get(&buff_key) {
                state.entities[idx].add_buff(
                    crate::entities::ActiveBuff::from_buff_stats(buff_key, duration, bs)
                );
            }
        }
    }

    // FIX: Spawn spell zones from spawn_area_object
    // Look up spell data for the area object key and create a zone entity.
    for (team, area_key, x, y, level) in area_object_spawns {
        if let Some(spell) = data.spells.get(&area_key) {
            let radius = spell.radius;
            let duration_ticks = if spell.life_duration > 0 {
                (spell.life_duration * 20 + 999) / 1000
            } else {
                1
            };
            let damage = if !spell.damage_per_level.is_empty() && level > 0 {
                let idx = (level - 1).min(spell.damage_per_level.len() - 1);
                spell.damage_per_level[idx]
            } else {
                spell.damage
            };
            let hit_interval = if spell.hit_speed > 0 {
                (spell.hit_speed * 20 + 999) / 1000
            } else {
                duration_ticks
            };
            let affects_air = spell.aoe_to_air || spell.hits_air;
            let affects_ground = spell.aoe_to_ground || spell.hits_ground;
            let buff_key = spell.buff.clone();
            let buff_time = if spell.buff_time > 0 {
                (spell.buff_time * 20 + 999) / 1000
            } else {
                duration_ticks
            };
            let ct_pct = spell.crown_tower_damage_percent;

            let id = state.alloc_id();
            let zone = Entity::new_spell_zone(
                id, team, &area_key, x, y, radius, duration_ticks,
                damage, hit_interval, affects_air, affects_ground,
                buff_key, buff_time,
                spell.only_enemies, spell.only_own_troops,
                ct_pct, 0, // attract_strength=0 for deploy zones
                None, 0, 0, level, // no spawner
                false, 0, 0, 0, // no hit_biggest
                None, // no spell_projectile_key
                0, // spawn_min_radius
                0, // heal_per_hit
                0, false, // no pushback for deploy zones
                0, 0,     // no distance-scaled pushback
                // Combine SpellStats + BuffStats no_effect_to_crown_towers.
                spell.no_effect_to_crown_towers
                    || spell.buff.as_ref()
                        .and_then(|bk| data.buffs.get(bk))
                        .map(|bs| bs.no_effect_to_crown_towers)
                        .unwrap_or(false),
                spell.affects_hidden,
                1, 1, // level_scale: deploy zone, DOT not primary damage
            );
            state.entities.push(zone);
        }
    }
}

/// Tick burrow movement for underground-traveling troops (Miner, Goblin Drill dig).
///
/// Each tick, burrowing entities move toward their target at `burrow_speed`.
/// They are untargetable during travel (deploy_timer == i32::MAX).
/// When the entity arrives within one tick's movement of the target:
///   - **Miner** (no morph): position set to target, is_burrowing cleared,
///     deploy_timer restored → emerges as a normal troop.
///   - **GoblinDrillDig** (has spawn_pathfind_morph): the dig entity is killed
///     and replaced with the morph target building (GoblinDrill) at the arrival
///     position. The building then spawns goblins, has lifetime, death spawns, etc.
///
/// Small random offset (~±300 units) is applied on arrival, matching real CR.
fn tick_burrow(state: &mut GameState, data: &GameData) {
    // Collect morph events: (entity_index, morph_building_key, arrival_x, arrival_y, team, level)
    // We can't mutate + push while iterating, so collect and apply after.
    struct MorphEvent {
        entity_idx: usize,
        building_key: String,
        x: i32,
        y: i32,
        team: Team,
        level: usize,
    }
    let mut morphs: Vec<MorphEvent> = Vec::new();

    for (idx, entity) in state.entities.iter_mut().enumerate() {
        if !entity.alive {
            continue;
        }
        let (target_x, target_y, speed, deploy_ticks) = match &entity.kind {
            EntityKind::Troop(t) if t.is_burrowing => {
                (t.burrow_target_x, t.burrow_target_y, t.burrow_speed, t.burrow_deploy_ticks)
            }
            _ => continue,
        };

        let dx = target_x - entity.x;
        let dy = target_y - entity.y;
        let dist_sq = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
        let speed_sq = (speed as i64) * (speed as i64);

        if dist_sq <= speed_sq {
            // Arrived — apply small random offset for positional variance
            let hash = (entity.id.0 as i64).wrapping_mul(2654435761) ^ (state.tick as i64);
            let offset_x = ((hash % 601) - 300) as i32;          // -300..+300
            let offset_y = (((hash >> 16) % 601) - 300) as i32;  // -300..+300

            let arrival_x = (target_x + offset_x).clamp(-ARENA_HALF_W, ARENA_HALF_W);
            let arrival_y = (target_y + offset_y).clamp(-ARENA_HALF_H, ARENA_HALF_H);

            // Check if this dig entity should morph into a building
            // Data-driven: look up the dig character's spawn_pathfind_morph field
            let morph_key = {
                let card_key = &entity.card_key;
                // The entity's card_key is the original building key (e.g., "goblin-drill")
                // Look up the building's summon_character to find the dig character
                data.buildings.get(card_key.as_str())
                    .and_then(|bld| bld.summon_character.as_ref())
                    .and_then(|sc| data.characters.get(sc.as_str()))
                    .and_then(|dig| dig.spawn_pathfind_morph.clone())
            };

            if let Some(ref building_name) = morph_key {
                // Goblin Drill pattern: morph into a building
                let level = match &entity.kind {
                    EntityKind::Troop(t) => t.level,
                    _ => 11,
                };
                morphs.push(MorphEvent {
                    entity_idx: idx,
                    building_key: entity.card_key.clone(), // "goblin-drill"
                    x: arrival_x,
                    y: arrival_y,
                    team: entity.team,
                    level,
                });
                // Mark dig entity for removal (will be replaced by building)
                entity.alive = false;
                entity.hp = 0;
            } else {
                // Miner pattern: emerge as a normal troop
                entity.x = arrival_x;
                entity.y = arrival_y;
                if let EntityKind::Troop(ref mut t) = entity.kind {
                    t.is_burrowing = false;
                }
                entity.deploy_timer = deploy_ticks;
            }
        } else {
            // Still traveling — move toward target at burrow_speed
            let dist = (dist_sq as f64).sqrt();
            let step_x = ((dx as f64 / dist) * speed as f64) as i32;
            let step_y = ((dy as f64 / dist) * speed as f64) as i32;
            entity.x += step_x;
            entity.y += step_y;
        }
    }

    // Apply morph events: spawn buildings to replace dig entities
    for morph in morphs {
        // Look up building stats from the original card key (e.g., "goblin-drill")
        if let Some(bld_stats) = data.buildings.get(&morph.building_key) {
            let id = state.alloc_id();
            let entity = Entity::new_building(
                id, morph.team, bld_stats, morph.x, morph.y, morph.level,
            );
            state.entities.push(entity);
        }
    }
}

fn tick_buildings(state: &mut GameState, data: &GameData) {
    // Building spawns: (team, key, x, y, level, spawn_index, spawn_count)
    // spawn_index/spawn_count enable angular placement matching troop spawners.
    // When a building spawns multiple units per wave, each unit gets a unique
    // index so it can be offset using data-driven spawn_angle_shift.
    let mut spawns: Vec<(Team, String, i32, i32, usize, i32, i32)> = Vec::new();
    // Phase 3: Track which buildings just spawned (for evo on_each_attack notification)
    let mut evo_spawn_building_indices: Vec<usize> = Vec::new();
    // Elixir Collector: deferred elixir grants (team, amount in whole elixir)
    let mut elixir_grants: Vec<(Team, i32)> = Vec::new();

    // ─── Pre-compute alive spawned unit counts per (team, card_key) for spawn_limit ───
    // This avoids borrowing state.entities immutably inside the mutable loop below.
    let mut spawn_alive_counts: std::collections::HashMap<(Team, String), i32> = std::collections::HashMap::new();
    for e in state.entities.iter() {
        if e.alive && !e.card_key.is_empty() {
            *spawn_alive_counts.entry((e.team, e.card_key.clone())).or_insert(0) += 1;
            // Also count by lowercase key for fuzzy matching
            let lower = e.card_key.to_lowercase().replace(' ', "-");
            if lower != e.card_key {
                *spawn_alive_counts.entry((e.team, lower)).or_insert(0) += 1;
            }
        }
    }

    for (idx, entity) in state.entities.iter_mut().enumerate() {
        if !entity.alive || entity.deploy_timer > 0 {
            continue;
        }
        if let EntityKind::Building(ref mut bld) = entity.kind {
            // Decay lifetime
            bld.lifetime_remaining -= 1;
            if bld.lifetime_remaining <= 0 {
                entity.alive = false;
                // Set hp to 0 so tick_deaths recognizes this as a fresh death
                // and fires death_damage (GiantSkeletonBomb, BalloonBomb, etc.).
                entity.hp = 0;
                continue;
            }

            // ─── Tesla hide mechanic (data-driven) ───
            // Buildings with hides_when_not_attacking pop up to attack, then hide.
            // While hidden: untargetable (handled in is_targetable()).
            // Transition: hidden → (has target) → pop up (up_time) → attack → hide_timer → hidden
            if bld.hides_when_not_attacking {
                if bld.is_hidden {
                    // Hidden: check if we have a target to pop up for
                    if entity.target.is_some() {
                        bld.is_hidden = false;
                        bld.hide_timer = bld.up_time_ticks; // Stay visible for up_time
                    }
                } else {
                    // Visible: count down hide timer
                    bld.hide_timer -= 1;
                    if bld.hide_timer <= 0 && entity.target.is_none() {
                        // No target and up_time expired → start hiding
                        bld.hide_timer = bld.hide_time_ticks;
                        // After hide_time_ticks, actually hide
                        if bld.hide_timer <= 0 {
                            bld.is_hidden = true;
                        }
                    } else if bld.hide_timer <= 0 && entity.target.is_some() {
                        // Still has target — refresh up_time
                        bld.hide_timer = bld.up_time_ticks;
                    }
                    // If hide_timer was counting down with no target, check completion
                    if bld.hide_timer <= 0 {
                        bld.is_hidden = true;
                    }
                }
            }

            // Spawner tick — two-level timing:
            //   1. Wave timer: counts down to next wave (spawn_pause_time cadence)
            //   2. Stagger timer: within a wave, delays between individual units
            if let Some(ref spawn_key) = bld.spawn_character {
                if bld.spawn_interval > 0 {
                    // ─── spawn_limit enforcement (data-driven) ───
                    // If spawn_limit > 0, count alive spawned units of this type
                    // owned by this team. Skip spawning if at capacity.
                    let at_limit = if bld.spawn_limit > 0 {
                        let spawn_key_lower = spawn_key.to_lowercase().replace(' ', "-");
                        let alive_count = spawn_alive_counts
                            .get(&(entity.team, spawn_key_lower))
                            .copied()
                            .unwrap_or(0);
                        alive_count >= bld.spawn_limit
                    } else {
                        false
                    };

                    if at_limit {
                        // Still tick the timer so it doesn't freeze, just don't spawn
                        bld.spawn_timer -= 1;
                        if bld.spawn_timer <= 0 {
                            bld.spawn_timer = bld.spawn_interval;
                        }
                    } else {
                    // --- Stagger: spawn pending units from an active wave ---
                    // Staggered units get sequential indices starting from 1
                    // (index 0 was the first unit spawned immediately with the wave).
                    if bld.spawn_stagger_remaining > 0 {
                        bld.spawn_stagger_timer -= 1;
                        if bld.spawn_stagger_timer <= 0 {
                            let stagger_idx = bld.spawn_count - bld.spawn_stagger_remaining;
                            spawns.push((
                                entity.team,
                                spawn_key.clone(),
                                entity.x,
                                entity.y,
                                bld.level,
                                stagger_idx, // spawn index within wave
                                bld.spawn_count, // total units in wave
                            ));
                            bld.spawn_stagger_remaining -= 1;
                            if bld.spawn_stagger_remaining > 0 {
                                bld.spawn_stagger_timer = bld.spawn_stagger;
                            }
                        }
                    }

                    // --- Wave timer: start a new wave when it fires ---
                    // ─── Rage spawn_speed_multiplier (data-driven) ───
                    // If the building has an active buff with spawn_speed_multiplier > 0
                    // (e.g., Rage=135 means 135% speed), the spawn interval is shortened.
                    // Effective interval = base_interval * 100 / spawn_speed_multiplier.
                    let spawn_speed_mult = entity.buffs.iter()
                        .filter(|b| b.remaining_ticks > 0 && b.spawn_speed_multiplier > 0)
                        .map(|b| b.spawn_speed_multiplier)
                        .max()
                        .unwrap_or(100);
                    bld.spawn_timer -= 1;
                    if bld.spawn_timer <= 0 {
                        // Apply spawn_speed_multiplier to the reset interval.
                        // 135% speed → interval * 100 / 135 ≈ 74% of base interval.
                        let effective_interval = if spawn_speed_mult > 100 {
                            (bld.spawn_interval as i64 * 100 / spawn_speed_mult as i64) as i32
                        } else {
                            bld.spawn_interval
                        };
                        bld.spawn_timer = effective_interval.max(1);

                        // Spawn the first unit of the wave immediately (index 0)
                        spawns.push((
                            entity.team,
                            spawn_key.clone(),
                            entity.x,
                            entity.y,
                            bld.level,
                            0,               // spawn index 0 = first in wave
                            bld.spawn_count, // total units in wave
                        ));

                        // Queue remaining units for staggered delivery
                        if bld.spawn_count > 1 && bld.spawn_stagger > 0 {
                            bld.spawn_stagger_remaining = bld.spawn_count - 1;
                            bld.spawn_stagger_timer = bld.spawn_stagger;
                        } else {
                            // No stagger — spawn all remaining units now
                            for i in 1..bld.spawn_count {
                                spawns.push((
                                    entity.team,
                                    spawn_key.clone(),
                                    entity.x,
                                    entity.y,
                                    bld.level,
                                    i,               // spawn index within wave
                                    bld.spawn_count, // total units in wave
                                ));
                            }
                        }

                        // Phase 3: If this building has evo_state, treat spawn as "attack"
                        if entity.evo_state.is_some() {
                            evo_spawn_building_indices.push(idx);
                        }
                    }
                } // end else (not at_limit)
                }
            }

            // Elixir Collector: generate elixir periodically.
            // mana_collect_amount=1 per cycle, mana_generate_time_ms=9000ms (180 ticks).
            if bld.elixir_per_collect > 0 && bld.elixir_generate_interval > 0 {
                bld.elixir_generate_timer -= 1;
                if bld.elixir_generate_timer <= 0 {
                    bld.elixir_generate_timer = bld.elixir_generate_interval;
                    // Grant elixir to the building's owner
                    elixir_grants.push((entity.team, bld.elixir_per_collect));
                }
            }
        }
    }

    // Spawn building-spawned units with angular placement.
    // Uses data-driven spawn_angle_shift from the spawned character's stats,
    // matching the troop spawner logic (Witch bats, Night Witch bats).
    // When spawn_angle_shift > 0 (Bat=45°, DarkWitch=90°), units are placed
    // at fixed angular intervals around the building. When 0, fall back to
    // equal-division circle (360° / count). Single-unit waves spawn at center.
    for (team, key, x, y, level, idx, count) in spawns {
        let lookup_key = key.to_lowercase().replace(' ', "-");
        // FIX: Use find_character() fuzzy lookup instead of direct get().
        // Building spawn_character values like "Goblin" are internal character
        // names, not card keys. find_character() checks key, lowercase-hyphenated,
        // normalized, and name-based lookups — resolving "Goblin" → goblin stats.
        let stats_opt = data.characters.get(&lookup_key)
            .or_else(|| data.find_character(&key));
        if let Some(stats) = stats_opt {
            let id = state.alloc_id();
            // Place units in a circle around (x, y) using collision_radius.
            let radius = stats.collision_radius.max(200);
            let (ox, oy) = if count <= 1 {
                (0, 0)
            } else {
                // Data-driven angular placement from spawn_angle_shift.
                // spawn_angle_shift is in degrees: Bat=45, DarkWitch=90.
                // When > 0, each unit is offset by this fixed angle step.
                // When 0, fall back to equal-division circle (360° / count).
                let angle_step_deg = if stats.spawn_angle_shift > 0 {
                    stats.spawn_angle_shift as f64
                } else {
                    360.0 / count as f64
                };
                let angle = (idx as f64) * angle_step_deg * std::f64::consts::PI / 180.0;
                ((angle.cos() * radius as f64) as i32, (angle.sin() * radius as f64) as i32)
            };
            let mut troop = Entity::new_troop(id, team, stats, x + ox, y + oy, level, false);
            troop.deploy_timer = 0;
            state.entities.push(troop);
        }
    }

    // Phase 3: Fire evo on_each_attack for buildings that just spawned
    for idx in evo_spawn_building_indices {
        evo_system::tick_evo_building_spawn(state, data, idx);
    }

    // Apply elixir from Elixir Collectors
    for (team, amount) in elixir_grants {
        let player = state.player_mut(team);
        player.elixir += amount * 10_000; // Convert whole elixir to fixed-point
        if player.elixir > crate::game_state::MAX_ELIXIR {
            player.elixir = crate::game_state::MAX_ELIXIR;
        }
    }
}

/// Tick troop-based spawners: Witch (Skeleton), Night Witch (Bat), etc.
/// These are troops with `spawn_character` + `spawn_pause_time` in their data.
/// Different from building spawners — these are mobile troops that periodically
/// spawn units at their current position.
fn tick_troop_spawners(state: &mut GameState, data: &GameData) {
    // Regular spawns: (team, key, x, y, level, spawn_index, spawn_total, spawn_radius)
    let mut spawns: Vec<(Team, String, i32, i32, usize, i32, i32, i32)> = Vec::new();
    // Attached spawns: (team, key, x, y, level, parent_id)
    let mut attach_spawns: Vec<(Team, String, i32, i32, usize, EntityId)> = Vec::new();

    for entity in state.entities.iter_mut() {
        if !entity.alive || entity.deploy_timer > 0 {
            continue;
        }
        if let EntityKind::Troop(ref mut t) = entity.kind {
            // Regular troop spawner (Witch, Night Witch)
            if let Some(ref spawn_key) = t.troop_spawn_character {
                if t.troop_spawn_interval > 0 {
                    t.troop_spawn_timer -= 1;
                    if t.troop_spawn_timer <= 0 {
                        // ─── Rage spawn_speed_multiplier (data-driven) ───
                        // If this troop has an active buff with spawn_speed_multiplier > 0,
                        // reduce the reset interval proportionally.
                        let spawn_speed_mult = entity.buffs.iter()
                            .filter(|b| b.remaining_ticks > 0 && b.spawn_speed_multiplier > 0)
                            .map(|b| b.spawn_speed_multiplier)
                            .max()
                            .unwrap_or(100);
                        let effective_interval = if spawn_speed_mult > 100 {
                            (t.troop_spawn_interval as i64 * 100 / spawn_speed_mult as i64) as i32
                        } else {
                            t.troop_spawn_interval
                        };
                        t.troop_spawn_timer = effective_interval.max(1);
                        let count = t.troop_spawn_number;
                        let radius = t.troop_spawn_radius;
                        for i in 0..count {
                            spawns.push((
                                entity.team,
                                spawn_key.clone(),
                                entity.x,
                                entity.y,
                                t.level,
                                i,       // spawn index within this wave
                                count,   // total in this wave
                                radius,  // spawn_radius from data
                            ));
                        }
                    }
                }
            }

            // Spawn-attach riders (GoblinGiant SpearGoblins)
            // take() ensures this fires only once
            if let Some(attach_key) = t.spawn_attach_character.take() {
                let count = t.spawn_attach_count;
                for _ in 0..count {
                    attach_spawns.push((
                        entity.team,
                        attach_key.clone(),
                        entity.x,
                        entity.y,
                        t.level,
                        entity.id, // parent ID for attached_to
                    ));
                }
            }
        }
    }

    // Spawn regular units with circular offset from parent position.
    // Uses data-driven spawn_angle_shift from the spawned character's stats.
    // When spawn_angle_shift > 0 (Bat=45°, DarkWitch=90°), units are placed
    // at fixed angular intervals. When 0, fall back to equal-division circle.
    for (team, key, x, y, level, idx, count, spawn_radius) in spawns {
        let lookup_key = key.to_lowercase().replace(' ', "-");
        let stats_opt = data.characters.get(&lookup_key)
            .or_else(|| data.characters.get(&key));
        if let Some(stats) = stats_opt {
            let id = state.alloc_id();
            // Place units in a circle around (x, y) using spawn_radius.
            // If spawn_radius is 0, fall back to collision_radius.
            let radius = if spawn_radius > 0 { spawn_radius } else { stats.collision_radius.max(200) };
            let (ox, oy) = if count <= 1 {
                (0, 0)
            } else {
                // Data-driven angular placement from spawn_angle_shift.
                // spawn_angle_shift is in degrees: Bat=45, DarkWitch=90.
                // When > 0, each unit is offset by this fixed angle step.
                // When 0, fall back to equal-division circle (360° / count).
                let angle_step_deg = if stats.spawn_angle_shift > 0 {
                    stats.spawn_angle_shift as f64
                } else {
                    360.0 / count as f64
                };
                let angle = (idx as f64) * angle_step_deg * std::f64::consts::PI / 180.0;
                ((angle.cos() * radius as f64) as i32, (angle.sin() * radius as f64) as i32)
            };
            let mut troop = Entity::new_troop(id, team, stats, x + ox, y + oy, level, false);
            troop.deploy_timer = 0;
            state.entities.push(troop);
        }
    }

    // Spawn attached riders (with parent tracking)
    for (team, key, x, y, level, parent_id) in attach_spawns {
        let lookup_key = key.to_lowercase().replace(' ', "-");
        let stats_opt = data.characters.get(&lookup_key)
            .or_else(|| data.characters.get(&key));
        if let Some(stats) = stats_opt {
            let id = state.alloc_id();
            let mut troop = Entity::new_troop(id, team, stats, x, y, level, false);
            troop.deploy_timer = 0;
            troop.attached_to = Some(parent_id); // Track parent for position sync
            state.entities.push(troop);
        }
    }
}
/// Sync attached troop positions to their parent (rider mechanic).
/// In real CR, SpearGoblins ride on GoblinGiant — they move with the Giant,
/// can attack independently from range, and detach (become normal troops)
/// only when the Giant dies.
fn tick_attached_troops(state: &mut GameState) {
    // Collect parent positions first (avoid borrow conflict)
    let mut parent_positions: Vec<(EntityId, i32, i32, bool)> = Vec::new();
    for entity in state.entities.iter() {
        if entity.is_troop() || entity.is_building() {
            parent_positions.push((entity.id, entity.x, entity.y, entity.alive));
        }
    }

    for entity in state.entities.iter_mut() {
        if let Some(parent_id) = entity.attached_to {
            if let Some(&(_, px, py, palive)) = parent_positions.iter()
                .find(|(id, _, _, _)| *id == parent_id)
            {
                if palive {
                    // Snap to parent position (riders stay on the carrier)
                    entity.x = px;
                    entity.y = py;
                } else {
                    // Parent died — detach rider (becomes independent troop)
                    entity.attached_to = None;
                }
            } else {
                // Parent not found (already cleaned up) — detach
                entity.attached_to = None;
            }
        }
    }
}
/// Check HP-threshold buffs for all troops: buff_on50_hp and buff_on_xhp.
///
/// In real CR, certain troops gain buffs when their HP drops below a threshold:
///   - buff_on50_hp: triggers at 50% HP (e.g., Battle Healer self-heal boost)
///   - buff_on_xhp: triggers at buff_on_xhp_percent% HP (generic threshold)
///
/// Each buff fires at most once per entity lifetime (tracked by checking if
/// the buff is already active — since these are one-shot triggers, not periodic).
fn tick_hp_threshold_buffs(state: &mut GameState, data: &GameData) {
    // Collect events: (entity_index, buff_key, duration_ticks)
    let mut buff_events: Vec<(usize, String, i32)> = Vec::new();

    for (idx, entity) in state.entities.iter().enumerate() {
        if !entity.alive || entity.deploy_timer > 0 {
            continue;
        }
        if !entity.is_troop() {
            continue;
        }
        let card_key = &entity.card_key;
        let stats = match data.characters.get(card_key.as_str()) {
            Some(s) => s,
            None => continue,
        };

        let hp_pct = if entity.max_hp > 0 {
            (entity.hp as i64 * 100 / entity.max_hp as i64) as i32
        } else {
            100
        };

        // buff_on50_hp: triggers when HP drops to or below 50%
        if let Some(ref buff_key) = stats.buff_on50_hp {
            if !buff_key.is_empty() && stats.buff_on50_hp_time > 0 && hp_pct <= 50 {
                // Only apply once — skip if already has this buff
                if !entity.has_buff(buff_key) {
                    let duration = crate::entities::ms_to_ticks(stats.buff_on50_hp_time);
                    buff_events.push((idx, buff_key.clone(), duration));
                }
            }
        }

        // buff_on_xhp: triggers when HP drops to or below buff_on_xhp_percent%
        if let Some(ref buff_key) = stats.buff_on_xhp {
            if !buff_key.is_empty() && stats.buff_on_xhp_time > 0 && stats.buff_on_xhp_percent > 0 {
                if hp_pct <= stats.buff_on_xhp_percent {
                    if !entity.has_buff(buff_key) {
                        let duration = crate::entities::ms_to_ticks(stats.buff_on_xhp_time);
                        buff_events.push((idx, buff_key.clone(), duration));
                    }
                }
            }
        }
    }

    // Apply collected buff events — data-driven via from_buff_stats().
    for (idx, buff_key, duration) in buff_events {
        if idx < state.entities.len() && state.entities[idx].alive {
            if let Some(bs) = data.buffs.get(&buff_key) {
                state.entities[idx].add_buff(
                    crate::entities::ActiveBuff::from_buff_stats(buff_key, duration, bs)
                );
            }
        }
    }
}
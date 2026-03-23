//! Combat system — targeting, damage, buff application
//!
//! Pure functions on &mut GameState + &GameData.

// TODO: Phase 2
//! Combat system — targeting, damage application, death mechanics.
//!
//! Phase 2: Troops can target + attack towers, projectiles damage towers,
//! crown tower damage reduction, death spawns, death damage, retarget timing,
//! melee splash, bridge-aware movement.

use crate::data_types::GameData;
use crate::entities::*;
use crate::game_state::*;

// =========================================================================
// Tower pseudo-entity IDs
// =========================================================================

/// Towers don't live in the entity Vec, so we use sentinel IDs.
/// Ranges chosen to never collide with real entity IDs (which start at 1).
pub const P1_KING_TOWER_ID: EntityId = EntityId(0xFFFF_FF01);
pub const P1_PRINCESS_LEFT_ID: EntityId = EntityId(0xFFFF_FF02);
pub const P1_PRINCESS_RIGHT_ID: EntityId = EntityId(0xFFFF_FF03);
pub const P2_KING_TOWER_ID: EntityId = EntityId(0xFFFF_FF04);
pub const P2_PRINCESS_LEFT_ID: EntityId = EntityId(0xFFFF_FF05);
pub const P2_PRINCESS_RIGHT_ID: EntityId = EntityId(0xFFFF_FF06);

/// Check if an EntityId is a tower sentinel.
pub fn is_tower_id(id: EntityId) -> bool {
    id.0 >= 0xFFFF_FF01 && id.0 <= 0xFFFF_FF06
}

/// Get the tower team from a sentinel ID.
pub fn tower_team(id: EntityId) -> Team {
    if id.0 <= P1_PRINCESS_RIGHT_ID.0 {
        Team::Player1
    } else {
        Team::Player2
    }
}

/// Resolve a tower sentinel ID to a position from game state.
pub fn tower_pos(state: &GameState, id: EntityId) -> Option<(i32, i32)> {
    let tower = tower_ref(state, id)?;
    if tower.alive {
        Some(tower.pos)
    } else {
        None
    }
}

/// Get HP of a tower by sentinel ID. Returns None if tower is dead or invalid.
pub fn tower_hp(state: &GameState, id: EntityId) -> Option<i32> {
    let tower = tower_ref(state, id)?;
    if tower.alive {
        Some(tower.hp)
    } else {
        None
    }
}

/// Get immutable reference to a tower by sentinel ID.
pub fn tower_ref(state: &GameState, id: EntityId) -> Option<&TowerState> {
    match id {
        _ if id == P1_KING_TOWER_ID => Some(&state.player1.king),
        _ if id == P1_PRINCESS_LEFT_ID => Some(&state.player1.princess_left),
        _ if id == P1_PRINCESS_RIGHT_ID => Some(&state.player1.princess_right),
        _ if id == P2_KING_TOWER_ID => Some(&state.player2.king),
        _ if id == P2_PRINCESS_LEFT_ID => Some(&state.player2.princess_left),
        _ if id == P2_PRINCESS_RIGHT_ID => Some(&state.player2.princess_right),
        _ => None,
    }
}

/// Get mutable reference to a tower by sentinel ID.
pub fn tower_mut(state: &mut GameState, id: EntityId) -> Option<&mut TowerState> {
    match id {
        _ if id == P1_KING_TOWER_ID => Some(&mut state.player1.king),
        _ if id == P1_PRINCESS_LEFT_ID => Some(&mut state.player1.princess_left),
        _ if id == P1_PRINCESS_RIGHT_ID => Some(&mut state.player1.princess_right),
        _ if id == P2_KING_TOWER_ID => Some(&mut state.player2.king),
        _ if id == P2_PRINCESS_LEFT_ID => Some(&mut state.player2.princess_left),
        _ if id == P2_PRINCESS_RIGHT_ID => Some(&mut state.player2.princess_right),
        _ => None,
    }
}

/// Get the enemy tower sentinel IDs for a team, ordered by targeting priority:
/// princess_left, princess_right, king.
pub fn enemy_tower_ids(team: Team) -> [EntityId; 3] {
    match team {
        Team::Player1 => [P2_PRINCESS_LEFT_ID, P2_PRINCESS_RIGHT_ID, P2_KING_TOWER_ID],
        Team::Player2 => [P1_PRINCESS_LEFT_ID, P1_PRINCESS_RIGHT_ID, P1_KING_TOWER_ID],
    }
}

// =========================================================================
// Snapshot types for borrow-safe targeting
// =========================================================================

#[derive(Clone)]
pub struct TargetSnapshot {
    pub id: EntityId,
    pub team: Team,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub is_flying: bool,
    pub is_building: bool,
    pub is_troop: bool,
    pub targetable: bool,
    pub is_invisible: bool,
}

/// Build snapshots of all entities + towers as a unified target list.
pub fn build_target_snapshots(state: &GameState) -> Vec<TargetSnapshot> {
    let mut snaps: Vec<TargetSnapshot> = state
        .entities
        .iter()
        .map(|e| TargetSnapshot {
            id: e.id,
            team: e.team,
            x: e.x,
            y: e.y,
            hp: e.hp,
            is_flying: e.is_flying(),
            is_building: e.is_building(),
            is_troop: e.is_troop(),
            targetable: e.is_targetable(),
            is_invisible: e.is_invisible(),
        })
        .collect();

    // Add towers as pseudo-entities (buildings, ground, targetable if alive)
    let tower_entries = [
        (P1_KING_TOWER_ID, Team::Player1, &state.player1.king),
        (P1_PRINCESS_LEFT_ID, Team::Player1, &state.player1.princess_left),
        (P1_PRINCESS_RIGHT_ID, Team::Player1, &state.player1.princess_right),
        (P2_KING_TOWER_ID, Team::Player2, &state.player2.king),
        (P2_PRINCESS_LEFT_ID, Team::Player2, &state.player2.princess_left),
        (P2_PRINCESS_RIGHT_ID, Team::Player2, &state.player2.princess_right),
    ];

    for (tid, team, tower) in &tower_entries {
        if !tower.alive {
            continue;
        }
        // King tower only targetable if activated or both princess towers down
        let king_blocked = match *tid {
            _ if *tid == P1_KING_TOWER_ID => {
                !state.player1.king.activated
                    && (state.player1.princess_left.alive || state.player1.princess_right.alive)
            }
            _ if *tid == P2_KING_TOWER_ID => {
                !state.player2.king.activated
                    && (state.player2.princess_left.alive || state.player2.princess_right.alive)
            }
            _ => false,
        };
        // King tower is always targetable as a building; the "blocked" check is for
        // whether troops *choose* to target it. We include it but mark accordingly.
        snaps.push(TargetSnapshot {
            id: *tid,
            team: *team,
            x: tower.pos.0,
            y: tower.pos.1,
            hp: tower.hp,
            is_flying: false,
            is_building: true,
            is_troop: false,
            targetable: !king_blocked,
            is_invisible: false,
        });
    }

    snaps
}

// =========================================================================
// Targeting
// =========================================================================

/// Check if a snapshot matches targeting preferences.
pub fn can_target(
    snap: &TargetSnapshot,
    attacks_ground: bool,
    attacks_air: bool,
    only_buildings: bool,
    only_troops: bool,
    only_towers: bool,
    only_king_tower: bool,
) -> bool {
    if snap.is_flying && !attacks_air {
        return false;
    }
    if !snap.is_flying && !attacks_ground {
        return false;
    }
    if only_buildings && !snap.is_building {
        return false;
    }
    if only_troops && !snap.is_troop {
        return false;
    }
    if only_towers && !is_tower_id(snap.id) {
        return false;
    }
    // Fix #12: target_only_king_tower — skip everything except the king tower.
    // Data-driven from CharacterStats.target_only_king_tower.
    if only_king_tower {
        let is_king = snap.id == P1_KING_TOWER_ID || snap.id == P2_KING_TOWER_ID;
        if !is_king {
            return false;
        }
    }
    true
}

/// Find the best target for an entity. Returns target ID or None.
pub fn find_target(
    my_id: EntityId,
    my_team: Team,
    my_x: i32,
    my_y: i32,
    sight_sq: i64,
    min_range_sq: i64,
    attacks_ground: bool,
    attacks_air: bool,
    only_buildings: bool,
    only_troops: bool,
    only_towers: bool,
    only_king_tower: bool,
    target_lowest_hp: bool,
    // Fix #13: deprioritize/ignore targets with specific buff (Ram Rider bola).
    // When deprioritize_buff is Some, enemies with that buff active are only
    // selected as a last resort (after all non-buffed targets are considered).
    deprioritize_buff: Option<&str>,
    snapshots: &[TargetSnapshot],
    // Fix #13: to check if a target has the buff, we need access to entities.
    // We pass a closure that checks entity buff state by EntityId.
    has_buff_fn: &dyn Fn(EntityId, &str) -> bool,
) -> Option<EntityId> {
    let mut best_id: Option<EntityId> = None;
    let mut best_dist: i64 = i64::MAX;
    let mut best_hp: i32 = i32::MAX;
    // Fix #13: fallback target for deprioritized enemies (only used if no better target)
    let mut deprio_best_id: Option<EntityId> = None;
    let mut deprio_best_dist: i64 = i64::MAX;

    for snap in snapshots {
        if !snap.targetable || snap.team == my_team || snap.id == my_id {
            continue;
        }
        // Invisible entities cannot be targeted by enemies (Royal Ghost mechanic)
        if snap.is_invisible {
            continue;
        }
        if !can_target(snap, attacks_ground, attacks_air, only_buildings, only_troops, only_towers, only_king_tower) {
            continue;
        }
        let dx = (my_x - snap.x) as i64;
        let dy = (my_y - snap.y) as i64;
        let dist = dx * dx + dy * dy;

        // Skip targets inside minimum range (Mortar dead zone)
        if dist < min_range_sq {
            continue;
        }

        if dist <= sight_sq {
            // Fix #13: deprioritize targets with buff (Ram Rider BolaSnare).
            // If the target has the deprioritized buff, put it in the fallback pool.
            let is_deprioritized = deprioritize_buff
                .map(|bk| has_buff_fn(snap.id, bk))
                .unwrap_or(false);

            if is_deprioritized {
                if dist < deprio_best_dist {
                    deprio_best_dist = dist;
                    deprio_best_id = Some(snap.id);
                }
                continue; // Skip primary selection — goes to fallback pool
            }

            if target_lowest_hp {
                // Prioritize lowest HP, break ties by distance
                if snap.hp < best_hp || (snap.hp == best_hp && dist < best_dist) {
                    best_hp = snap.hp;
                    best_dist = dist;
                    best_id = Some(snap.id);
                }
            } else {
                // Standard: nearest target
                if dist < best_dist {
                    best_dist = dist;
                    best_id = Some(snap.id);
                }
            }
        }
    }

    // Fix #13: fall back to deprioritized target if no primary target found
    best_id.or(deprio_best_id)
}

/// Run targeting for all troops and attacking buildings.
pub fn tick_targeting(state: &mut GameState) {
    let snapshots = build_target_snapshots(state);
    let len = state.entities.len();

    // Fix #13: Build a closure that checks if an entity has a specific buff.
    // This is passed to find_target for deprioritize_targets_with_buff logic.
    // We snapshot the buff state to avoid borrow conflicts during targeting.
    struct BuffCheck {
        id: EntityId,
        buff_keys: Vec<String>,
    }
    let buff_checks: Vec<BuffCheck> = state.entities.iter()
        .filter(|e| e.alive && !e.buffs.is_empty())
        .map(|e| BuffCheck {
            id: e.id,
            buff_keys: e.buffs.iter()
                .filter(|b| !b.is_expired())
                .map(|b| b.key.clone())
                .collect(),
        })
        .collect();
    let has_buff_fn = |eid: EntityId, bk: &str| -> bool {
        buff_checks.iter().any(|bc| bc.id == eid && bc.buff_keys.iter().any(|k| k == bk))
    };

    for i in 0..len {
        let entity = &state.entities[i];
        if !entity.is_targetable() {
            continue;
        }

        // Fix #12+13: Extract targeting params including king_tower and deprioritize buff.
        let (sight_sq, min_range_sq, atk_ground, atk_air, only_buildings, only_troops, only_towers,
             only_king_tower, lowest_hp, retarget_every_tick, deprio_buff) =
            match &entity.kind {
                EntityKind::Troop(t) => (
                    t.sight_range_sq,
                    0i64, // Troops have no minimum range
                    t.attacks_ground,
                    t.attacks_air,
                    t.target_only_buildings,
                    t.target_only_troops,
                    t.target_only_towers,
                    t.target_only_king_tower,
                    t.target_lowest_hp,
                    t.retarget_each_tick,
                    // Fix #13: deprioritize buff key (Ram Rider "BolaSnare").
                    // Only active when both deprioritize flag AND buff key are set.
                    if t.deprioritize_targets_with_buff {
                        t.ignore_targets_with_buff.clone()
                    } else {
                        None
                    },
                ),
                EntityKind::Building(b) if b.hit_speed > 0 => (
                    b.range_sq,
                    b.min_range_sq, // Mortar dead zone
                    b.attacks_ground,
                    b.attacks_air,
                    false,
                    false,
                    false,
                    false, // Buildings don't target only king tower
                    false,
                    false,
                    None, // Buildings don't deprioritize by buff
                ),
                _ => continue,
            };

        let my_id = entity.id;
        let my_team = entity.team;
        let my_x = entity.x;
        let my_y = entity.y;
        let old_target = entity.target;

        // Check if current target is still valid (alive, targetable, in leash range)
        let current_valid = old_target.map_or(false, |tid| {
            snapshots.iter().any(|s| {
                if s.id != tid || !s.targetable || s.team == my_team {
                    return false;
                }
                if !can_target(s, atk_ground, atk_air, only_buildings, only_troops, only_towers, only_king_tower) {
                    return false;
                }
                // Drop target if it moved way out of sight range (leash)
                let dx = (my_x - s.x) as i64;
                let dy = (my_y - s.y) as i64;
                let dist_sq = dx * dx + dy * dy;
                dist_sq <= sight_sq * 4  // 2x sight range squared
            })
        });

        // ── Building pull: building-only troops always retarget to nearest ──
        let force_retarget = (only_buildings && current_valid && old_target.is_some())
            || retarget_every_tick;

        if current_valid && !force_retarget {
            continue;
        }

        // Find the best valid target
        let deprio_ref = deprio_buff.as_deref();
        let new_target = find_target(
            my_id, my_team, my_x, my_y, sight_sq, min_range_sq, atk_ground, atk_air, only_buildings,
            only_troops, only_towers, only_king_tower, lowest_hp, deprio_ref, &snapshots, &has_buff_fn,
        );

        // For building-only troops with force_retarget: only actually switch
        // if the new target is genuinely different. If find_target returns the
        // same target, keep it (no pointless retarget penalty).
        if force_retarget {
            if new_target == old_target || new_target.is_none() {
                continue;
            }
        }

        // Apply retarget load time if switching to a new target
        if new_target != old_target && new_target.is_some() {
            if let EntityKind::Troop(ref mut t) = state.entities[i].kind {
                t.attack_cooldown = t.load_after_retarget.max(t.attack_cooldown);
                // Reset inferno-style ramp on troop retarget (Inferno Dragon)
                if t.ramp_damage3 > 0 {
                    t.ramp_ticks = 0;
                    t.ramp_target = new_target;
                }
                // Cancel windup if target changed during it (retarget reset exploit)
                if t.attack_phase == crate::entities::AttackPhase::Windup {
                    t.attack_phase = crate::entities::AttackPhase::Idle;
                    t.phase_timer = 0;
                    t.windup_target = None;
                }
            }
            // Reset inferno-style ramp on building retarget (Inferno Tower)
            if let EntityKind::Building(ref mut bld) = state.entities[i].kind {
                if bld.ramp_damage3 > 0 {
                    bld.ramp_ticks = 0;
                    bld.ramp_target = new_target;
                }
            }
        }

        state.entities[i].target = new_target;
    }
}

// =========================================================================
// Movement — bridge-aware pathing
// =========================================================================

/// Check if a position is on a bridge.
fn is_on_bridge(x: i32) -> bool {
    (x - BRIDGE_LEFT_X).abs() <= BRIDGE_HALF_W || (x - BRIDGE_RIGHT_X).abs() <= BRIDGE_HALF_W
}

/// Check if a troop needs to cross the river (non-flying).
/// Returns true if the troop is on its own side (or inside the river) and the
/// target is on the opposite side. This keeps bridge routing active even after
/// the troop enters the river zone, preventing straight-line river crossing.
fn needs_river_crossing(from_y: i32, to_y: i32, team: Team) -> bool {
    match team {
        // P1 troops go positive-Y; need to cross if not yet past river and target is past it
        Team::Player1 => from_y < RIVER_Y_MAX && to_y > RIVER_Y_MIN,
        // P2 troops go negative-Y; need to cross if not yet past river and target is past it
        Team::Player2 => from_y > RIVER_Y_MIN && to_y < RIVER_Y_MAX,
    }
}

// =========================================================================
// Waypoint lane graph — navigation for river crossing
// =========================================================================
//
// Instead of choosing between two raw bridge points, troops navigate a
// small graph of waypoints. This gives:
//   - Stable bridge commitment (troops don't jitter between bridges)
//   - Correct handling of center pulls and cross-lane retargeting
//   - Realistic lane transitions when targets change
//   - Natural diagonal movement (not horizontal-then-forward)
//
// The graph has lane waypoints on each side of the river, connected
// by bridge crossings. All node positions are derived from arena
// constants in game_state.rs — no hardcoded heuristics.
//
// Node layout:
//
//   P2 side (y > RIVER_Y_MAX):
//     [P2_LEFT]-----[P2_CENTER]-----[P2_RIGHT]
//         |                             |
//   River ====== LEFT_BRIDGE ====== RIGHT_BRIDGE ======
//         |                             |
//   P1 side (y < RIVER_Y_MIN):
//     [P1_LEFT]-----[P1_CENTER]-----[P1_RIGHT]
//
// Within each side, all nodes are connected (open ground).
// Cross-river connections ONLY via bridge node pairs.
// =========================================================================

/// Waypoint node index.
const NODE_P1_LEFT: usize = 0;
const NODE_P1_CENTER: usize = 1;
const NODE_P1_RIGHT: usize = 2;
const NODE_BRIDGE_LEFT_P1: usize = 3;
const NODE_BRIDGE_LEFT_P2: usize = 4;
const NODE_BRIDGE_RIGHT_P1: usize = 5;
const NODE_BRIDGE_RIGHT_P2: usize = 6;
const NODE_P2_LEFT: usize = 7;
const NODE_P2_CENTER: usize = 8;
const NODE_P2_RIGHT: usize = 9;
const NAV_NODE_COUNT: usize = 10;

/// Get the position of a navigation node. All positions derived from
/// arena constants (BRIDGE_LEFT_X, BRIDGE_RIGHT_X, RIVER_Y_MIN/MAX,
/// ARENA_HALF_W). No magic numbers.
fn nav_node_pos(node: usize) -> (i32, i32) {
    match node {
        // P1 side lane waypoints — positioned midway between river and typical deploy zone
        NODE_P1_LEFT    => (BRIDGE_LEFT_X,  (RIVER_Y_MIN + (-ARENA_HALF_H)) / 2),  // (-5100, -8300)
        NODE_P1_CENTER  => (0,              (RIVER_Y_MIN + (-ARENA_HALF_H)) / 2),  // (0, -8300)
        NODE_P1_RIGHT   => (BRIDGE_RIGHT_X, (RIVER_Y_MIN + (-ARENA_HALF_H)) / 2), // (5100, -8300)
        // Bridge entries/exits — at river edges, at bridge X positions
        NODE_BRIDGE_LEFT_P1  => (BRIDGE_LEFT_X,  RIVER_Y_MIN),  // (-5100, -1200)
        NODE_BRIDGE_LEFT_P2  => (BRIDGE_LEFT_X,  RIVER_Y_MAX),  // (-5100, 1200)
        NODE_BRIDGE_RIGHT_P1 => (BRIDGE_RIGHT_X, RIVER_Y_MIN),  // (5100, -1200)
        NODE_BRIDGE_RIGHT_P2 => (BRIDGE_RIGHT_X, RIVER_Y_MAX),  // (5100, 1200)
        // P2 side lane waypoints
        NODE_P2_LEFT    => (BRIDGE_LEFT_X,  (RIVER_Y_MAX + ARENA_HALF_H) / 2),    // (-5100, 8300)
        NODE_P2_CENTER  => (0,              (RIVER_Y_MAX + ARENA_HALF_H) / 2),    // (0, 8300)
        NODE_P2_RIGHT   => (BRIDGE_RIGHT_X, (RIVER_Y_MAX + ARENA_HALF_H) / 2),   // (5100, 8300)
        _ => (0, 0),
    }
}

/// Euclidean distance between two points.
fn point_dist(x1: i32, y1: i32, x2: i32, y2: i32) -> i64 {
    let dx = (x2 - x1) as i64;
    let dy = (y2 - y1) as i64;
    ((dx * dx + dy * dy) as f64).sqrt() as i64
}

/// Find the next waypoint for a troop that needs to cross the river.
///
/// Runs shortest-path on the waypoint graph from troop position to
/// target position. Returns the first waypoint the troop should walk
/// toward (the bridge entry on its side of the river).
///
/// Algorithm: Since the graph is tiny (10 nodes) and has a specific
/// structure (within-side fully connected, cross-river only via 2 bridges),
/// we can enumerate the 2 possible paths (left bridge vs right bridge)
/// and pick the shorter one. This is equivalent to Dijkstra on this graph
/// but simpler and branchless.
fn next_waypoint_for_crossing(
    from_x: i32, from_y: i32,
    to_x: i32, to_y: i32,
    team: Team,
) -> (i32, i32) {
    // Bridge entry/exit nodes depend on which side the troop is on
    let (entry_left, exit_left, entry_right, exit_right) = match team {
        Team::Player1 => (
            nav_node_pos(NODE_BRIDGE_LEFT_P1),
            nav_node_pos(NODE_BRIDGE_LEFT_P2),
            nav_node_pos(NODE_BRIDGE_RIGHT_P1),
            nav_node_pos(NODE_BRIDGE_RIGHT_P2),
        ),
        Team::Player2 => (
            nav_node_pos(NODE_BRIDGE_LEFT_P2),
            nav_node_pos(NODE_BRIDGE_LEFT_P1),
            nav_node_pos(NODE_BRIDGE_RIGHT_P2),
            nav_node_pos(NODE_BRIDGE_RIGHT_P1),
        ),
    };

    // Total path distance via left bridge:
    //   troop → left_entry → left_exit → target
    let dist_left = point_dist(from_x, from_y, entry_left.0, entry_left.1)
        + point_dist(entry_left.0, entry_left.1, exit_left.0, exit_left.1)
        + point_dist(exit_left.0, exit_left.1, to_x, to_y);

    // Total path distance via right bridge:
    //   troop → right_entry → right_exit → target
    let dist_right = point_dist(from_x, from_y, entry_right.0, entry_right.1)
        + point_dist(entry_right.0, entry_right.1, exit_right.0, exit_right.1)
        + point_dist(exit_right.0, exit_right.1, to_x, to_y);

    // Return the entry node of the shorter path
    if dist_left <= dist_right {
        entry_left
    } else {
        entry_right
    }
}

/// Default movement target — nearest alive enemy princess tower, or king.
pub fn default_target_pos(state: &GameState, team: Team) -> (i32, i32) {
    let enemy = state.opponent(team);

    // Prefer the princess tower on the side the troop is closer to
    // (we don't have the troop's X here, so just pick first alive princess, then king)
    if enemy.princess_left.alive {
        return enemy.princess_left.pos;
    }
    if enemy.princess_right.alive {
        return enemy.princess_right.pos;
    }
    enemy.king.pos
}

/// Smarter default target that considers troop position (left lane vs right lane).
pub fn default_target_for_troop(state: &GameState, team: Team, troop_x: i32) -> (i32, i32) {
    let enemy = state.opponent(team);

    let left_alive = enemy.princess_left.alive;
    let right_alive = enemy.princess_right.alive;

    match (left_alive, right_alive) {
        (true, true) => {
            // Pick the princess tower on the troop's side
            if troop_x <= 0 {
                enemy.princess_left.pos
            } else {
                enemy.princess_right.pos
            }
        }
        (true, false) => enemy.princess_left.pos,
        (false, true) => enemy.princess_right.pos,
        (false, false) => enemy.king.pos,
    }
}

/// Move a troop for one tick with waypoint-graph pathing.
///
/// Troops that need to cross the river use the lane graph to find the
/// shortest valid path through a bridge. The waypoint they walk toward
/// is the bridge entry node on the shorter path. Once on the bridge,
/// they proceed to the exit node, then directly to their target.
///
/// NOTE: Standard troop movement now uses inline ally-avoidance steering in
/// tick_movement() instead of calling this function. move_troop remains as a
/// utility for non-standard movement (e.g., future use by other systems).
///
/// River jumpers (Hog Rider, Royal Hogs, Ram Rider, Battle Ram) and
/// flying troops skip bridge routing entirely.
#[allow(dead_code)]
pub fn move_troop(
    entity: &mut Entity,
    target_x: i32,
    target_y: i32,
    speed: i32,
    range_sq: i64,
) {
    // Already in attack range?
    let dx = (target_x - entity.x) as i64;
    let dy = (target_y - entity.y) as i64;
    let dist_sq = dx * dx + dy * dy;
    if dist_sq <= range_sq {
        return;
    }

    // Determine effective movement target (waypoint or final target)
    // River jumpers and flying troops skip bridge routing.
    let can_skip_river = entity.is_flying() || match &entity.kind {
        EntityKind::Troop(t) => t.can_jump_river,
        _ => false,
    };

    let (eff_x, eff_y) = if !can_skip_river
        && needs_river_crossing(entity.y, target_y, entity.team)
        && !is_on_bridge(entity.x)
    {
        // Use waypoint graph to find the bridge entry on the shortest path.
        // next_waypoint_for_crossing computes total distance through each bridge
        // (troop→entry→exit→target) and returns the entry of the shorter route.
        next_waypoint_for_crossing(entity.x, entity.y, target_x, target_y, entity.team)
    } else {
        (target_x, target_y)
    };

    let dx = (eff_x - entity.x) as i64;
    let dy = (eff_y - entity.y) as i64;
    let dist_sq = dx * dx + dy * dy;
    let dist = (dist_sq as f64).sqrt() as i32;

    if dist > 0 {
        let move_x = ((dx * speed as i64) / dist as i64) as i32;
        let move_y = ((dy * speed as i64) / dist as i64) as i32;
        entity.x += move_x;
        entity.y += move_y;

        entity.x = entity.x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
        entity.y = entity.y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
    }
}

/// Run movement for all troops.
pub fn tick_movement(state: &mut GameState) {
    let len = state.entities.len();

    // Snapshot positions for target lookup (entities + towers)
    let snapshots = build_target_snapshots(state);

    for i in 0..len {
        let entity = &state.entities[i];
        // FIX: Dashing troops with dash immunity are "untargetable" (is_targetable()
        // returns false when is_dashing && dash_immune_remaining > 0). But they STILL
        // need their dash interpolation movement to run — the dash movement code is
        // inside this loop. Without this exception, Bandit/Golden Knight trigger a dash,
        // become untargetable due to immunity, get skipped by this gate, and are
        // permanently frozen at their pre-dash position (dash_timer never decrements
        // either because tick_combat also gates on is_targetable).
        let is_dashing_immune = match &entity.kind {
            EntityKind::Troop(ref t) => t.is_dashing && t.dash_immune_remaining > 0,
            _ => false,
        };
        if !entity.is_targetable() && !is_dashing_immune {
            continue;
        }

        let (speed, range_sq) = match &entity.kind {
            EntityKind::Troop(t) => (t.speed, t.range_sq),
            _ => continue,
        };
        if speed == 0 {
            continue;
        }

        // Phase 3: Skip movement if immobilized (stunned/frozen)
        if entity.is_immobilized() {
            continue;
        }

        // Fix #15: Skip movement during post-special-attack recovery.
        // Data-driven from CharacterStats.stop_time_after_special_attack.
        // While special_recovery_timer > 0, the troop is paused after firing a hook.
        if let EntityKind::Troop(ref t) = entity.kind {
            if t.special_recovery_timer > 0 {
                continue;
            }
        }

        // Skip movement during attack animation (windup + backswing).
        // Troops are rooted while winding up or recovering from an attack.
        // EXCEPTION: dashing troops must NOT be skipped — their dash movement
        // code (below) handles position interpolation during the dash. Without
        // this exception, a Bandit that dash-lands (→ Backswing) gets blocked
        // here, preventing all further dash movement and causing an infinite
        // dash→backswing→re-dash stall at zero displacement.
        let skip_attack_anim = if let EntityKind::Troop(ref t) = entity.kind {
            t.attack_phase != crate::entities::AttackPhase::Idle && !t.is_dashing
        } else {
            false
        };
        if skip_attack_anim {
            continue;
        }

        // Phase 3: Apply speed buff/debuff multiplier
        let mut effective_speed = (speed as i64 * entity.speed_multiplier() as i64 / 100) as i32;
        if effective_speed <= 0 {
            continue;
        }

        // Read charge config from immutable borrow before we need mutable access
        let (has_charge, charge_mult, is_dashing) = if let EntityKind::Troop(ref t) = entity.kind {
            (
                t.charge_range > 0 && t.charge_speed_multiplier > 0,
                t.charge_speed_multiplier,
                t.is_dashing,
            )
        } else {
            (false, 0, false)
        };

        // Read remaining immutable fields we need
        let entity_x = entity.x;
        let entity_y = entity.y;
        let entity_target = entity.target;
        let team = entity.team;

        // Drop the immutable borrow — everything below uses state.entities[i] directly

        // ─── Charge: update distance from LAST tick's movement ───
        // In real CR, charge tracks TOTAL distance moved, not distance toward
        // a specific target. Prince charges while running down the lane even
        // before seeing an enemy. Charge only resets when:
        //   - Stunned/frozen (handled in tick_combat)
        //   - Charge hit connects (handled in tick_combat hit frame)
        //   - Troop is standing still (not moving at all)
        if has_charge {
            if let EntityKind::Troop(ref mut t) = state.entities[i].kind {
                let dx = (entity_x - t.charge_prev_x) as i64;
                let dy = (entity_y - t.charge_prev_y) as i64;
                let moved = ((dx * dx + dy * dy) as f64).sqrt() as i32;
                t.charge_prev_x = entity_x;
                t.charge_prev_y = entity_y;

                if moved > 0 {
                    t.charge_distance_remaining -= moved;
                    if t.charge_distance_remaining <= 0 && !t.is_charging {
                        t.is_charging = true;
                        t.charge_hit_ready = true;
                    }
                }

                // Apply charge speed boost (now using up-to-date is_charging)
                if t.is_charging {
                    effective_speed = (effective_speed as i64 * charge_mult as i64 / 100) as i32;
                }
            }
        }

        let troop_x = entity_x;

        // Determine target position
        let (target_x, target_y) = if let Some(tid) = entity_target {
            if let Some(snap) = snapshots.iter().find(|s| s.id == tid) {
                (snap.x, snap.y)
            } else {
                default_target_for_troop(state, team, troop_x)
            }
        } else {
            default_target_for_troop(state, team, troop_x)
        };

        // Dash movement: interpolate position toward dash target each tick.
        // This replaces the old teleport-on-landing with smooth movement so the
        // replay shows the troop traveling across the arena during the dash.
        // Skips all normal movement logic (ally avoidance, bridge routing, etc.)
        // since the dashing troop is committed to its trajectory.
        if is_dashing {
            let (dash_tx, dash_ty, dash_spd) = if let EntityKind::Troop(ref t) = state.entities[i].kind {
                (t.dash_target_x, t.dash_target_y, t.dash_jump_speed)
            } else {
                continue;
            };
            let dx = (dash_tx - entity_x) as i64;
            let dy = (dash_ty - entity_y) as i64;
            let dist_sq_d = dx * dx + dy * dy;
            // Compute step speed: for fixed-time dashes (MK), derive speed from
            // distance / remaining ticks. For jump_speed dashes (Bandit), use jump_speed.
            let step_speed = if dash_spd > 0 {
                dash_spd
            } else {
                // Fixed-time: MK — try to cover remaining distance in remaining ticks
                let remaining = if let EntityKind::Troop(ref t) = state.entities[i].kind {
                    t.dash_timer.max(1)
                } else { 1 };
                let dist_d = (dist_sq_d as f64).sqrt() as i32;
                (dist_d / remaining).max(1)
            };
            let step_sq = (step_speed as i64) * (step_speed as i64);
            if dist_sq_d <= step_sq {
                // Close enough — snap to target (landing handled in tick_combat)
                state.entities[i].x = dash_tx;
                state.entities[i].y = dash_ty;
            } else {
                let dist_d = (dist_sq_d as f64).sqrt();
                let move_x = (dx as f64 / dist_d * step_speed as f64) as i32;
                let move_y = (dy as f64 / dist_d * step_speed as f64) as i32;
                state.entities[i].x += move_x;
                state.entities[i].y += move_y;
            }
            continue;
        }

        // =====================================================================
        // Ally-avoidance steering — integrated into movement
        // =====================================================================
        //
        // In real CR, troops don't just walk then get pushed apart — they
        // inherently avoid overlapping allies during movement. This produces
        // side-by-side lane flow where same-team troops spread laterally.
        //
        // Algorithm:
        //   1. Compute preferred velocity toward target/waypoint (v_pref)
        //   2. Compute lateral separation from nearby same-team allies (v_avoid)
        //   3. Break symmetry deterministically for exact overlaps
        //   4. Combine: v_final = normalize(v_pref + alpha * v_avoid) * speed
        //   5. Apply movement using v_final
        //
        // All parameters are derived from entity data:
        //   - separation radius = sum of collision radii (from JSON)
        //   - avoidance strength scales with overlap depth
        //   - lateral bias uses perpendicular to v_pref (geometry, not heuristic)
        //   - symmetry break uses entity ID ordering (deterministic)
        //
        // The post-step collision pass in tick_collisions remains as a
        // lightweight residual overlap solver.

        // Step 1: Compute preferred direction toward target/waypoint
        let entity_ref = &state.entities[i];
        let my_id = entity_ref.id.0;
        let my_x = entity_ref.x;
        let my_y = entity_ref.y;
        let my_radius = entity_ref.collision_radius;

        // Get the effective waypoint (handles bridge routing internally)
        let can_skip_river = entity_ref.is_flying() || match &entity_ref.kind {
            EntityKind::Troop(t) => t.can_jump_river,
            _ => false,
        };

        let (wp_x, wp_y) = if !can_skip_river
            && needs_river_crossing(my_y, target_y, team)
            && !is_on_bridge(my_x)
        {
            next_waypoint_for_crossing(my_x, my_y, target_x, target_y, team)
        } else {
            (target_x, target_y)
        };

        // Already in attack range of ultimate target? Don't move.
        {
            let dx = (target_x - my_x) as i64;
            let dy = (target_y - my_y) as i64;
            if dx * dx + dy * dy <= range_sq {
                continue;
            }
        }

        // Preferred velocity direction (toward waypoint)
        let pref_dx = (wp_x - my_x) as f64;
        let pref_dy = (wp_y - my_y) as f64;
        let pref_len = (pref_dx * pref_dx + pref_dy * pref_dy).sqrt();

        if pref_len < 1.0 {
            continue; // At waypoint, skip
        }

        let pref_nx = pref_dx / pref_len;
        let pref_ny = pref_dy / pref_len;

        // Step 2: Compute ally-separation steering
        // Perpendicular to preferred direction (for lateral avoidance)
        let perp_x = -pref_ny; // rotate 90° CCW
        let perp_y = pref_nx;

        let mut avoid_lat = 0.0f64; // lateral (perpendicular) avoidance
        let mut avoid_fwd = 0.0f64; // forward/backward avoidance (weak)
        let mut has_exact_overlap = false;

        // Only consider the 2 nearest same-team allies to avoid O(N) scaling
        // in swarms. In real CR, a skeleton in a 15-skeleton army doesn't
        // avoid all 14 allies equally — it reacts to immediate neighbors.
        // This prevents over-separation where troops fan out into a wide line.
        let mut nearest_allies: [(f64, usize); 2] = [(f64::MAX, 0); 2];

        for j in 0..len {
            if i == j { continue; }
            let other = &state.entities[j];
            if !other.alive || other.deploy_timer > 0 { continue; }
            if other.team != team { continue; }
            if !other.is_troop() { continue; }

            let odx = (my_x - other.x) as f64;
            let ody = (my_y - other.y) as f64;
            let odist_sq = odx * odx + ody * ody;
            let sep_radius = (my_radius + other.collision_radius) as f64;

            // Tighter influence radius: 1.0× sum of radii (not 1.5×).
            // Troops only react to allies that are actually overlapping or
            // nearly so. This prevents distant allies from contributing.
            if odist_sq >= (sep_radius * 1.2) * (sep_radius * 1.2) {
                continue;
            }

            // Track 2 nearest
            if odist_sq < nearest_allies[0].0 {
                nearest_allies[1] = nearest_allies[0];
                nearest_allies[0] = (odist_sq, j);
            } else if odist_sq < nearest_allies[1].0 {
                nearest_allies[1] = (odist_sq, j);
            }
        }

        // Process only the 2 nearest allies
        for &(dist_sq, j) in &nearest_allies {
            if dist_sq >= f64::MAX { continue; }
            let other = &state.entities[j];
            let odx = (my_x - other.x) as f64;
            let ody = (my_y - other.y) as f64;
            let odist = dist_sq.sqrt();
            let sep_radius = (my_radius + other.collision_radius) as f64;

            if odist < 1.0 {
                has_exact_overlap = true;
                continue;
            }

            // Direction away from ally
            let away_x = odx / odist;
            let away_y = ody / odist;

            // Avoidance strength: stronger when closer, zero at sep_radius*1.2
            let penetration = 1.0 - (odist / (sep_radius * 1.2));
            let strength = penetration.max(0.0);

            // Project onto lateral (perpendicular) and forward axes.
            // Lateral: troops spread side-by-side (full weight).
            // Forward: troops don't bounce backward (0.3× weight).
            let lat_component = away_x * perp_x + away_y * perp_y;
            let fwd_component = away_x * pref_nx + away_y * pref_ny;

            avoid_lat += lat_component * strength;
            avoid_fwd += fwd_component * strength * 0.3;
        }

        // Step 3: Deterministic symmetry break for exact overlaps
        if has_exact_overlap {
            let bias = if my_id % 2 == 0 { 1.0 } else { -1.0 };
            avoid_lat += bias * 1.0;
        }

        // Clamp total avoidance so lateral never overwhelms forward velocity.
        // Max avoidance magnitude = 1.0 (same as forward component).
        // This prevents swarms from fanning out wider than a few collision radii.
        let avoid_mag = (avoid_lat * avoid_lat + avoid_fwd * avoid_fwd).sqrt();
        if avoid_mag > 1.0 {
            avoid_lat /= avoid_mag;
            avoid_fwd /= avoid_mag;
        }

        // Step 4: Combine preferred velocity with avoidance
        // alpha = 0.25: avoidance can deflect path up to ~14° from forward.
        // This produces CR-like side-by-side flow without excessive fanning.
        // At 0.5, staggered deploy groups (Barbarians, Skeleton Army) spread
        // ~5.5 tiles wide; at 0.3 they stay within ~3 tiles — matching real CR.
        // The symmetry break for exact overlaps (bias=1.0) still produces
        // enough deflection at alpha=0.25 to separate stacked troops.
        let alpha = 0.4;
        let final_dx = pref_nx + alpha * (avoid_lat * perp_x + avoid_fwd * pref_nx);
        let final_dy = pref_ny + alpha * (avoid_lat * perp_y + avoid_fwd * pref_ny);
        let final_len = (final_dx * final_dx + final_dy * final_dy).sqrt();

        if final_len < 0.001 {
            continue;
        }

        // Step 5: Apply movement
        let move_x = (final_dx / final_len * effective_speed as f64) as i32;
        let move_y = (final_dy / final_len * effective_speed as f64) as i32;

        let entity = &mut state.entities[i];
        entity.x += move_x;
        entity.y += move_y;
        entity.x = entity.x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
        entity.y = entity.y.clamp(-ARENA_HALF_H, ARENA_HALF_H);

        // River barrier: ground troops cannot be inside the river unless on a bridge.
        // After movement, push them back to their OWN side of the river.
        //
        // BUG FIX: Previously used "nearest edge" logic which pushed to whichever
        // river edge (Y=-1200 or Y=1200) was closer. This caused troops to slip
        // through the river when ally-avoidance or collision pushed them past the
        // river midpoint (Y=0) — the nearest edge would flip to the enemy side,
        // pulling the troop through instead of pushing it back.
        //
        // Fix: Use team-aware pushback. P1 troops (moving +Y) are pushed back to
        // RIVER_Y_MIN (their side). P2 troops (moving -Y) are pushed back to
        // RIVER_Y_MAX (their side). A troop can never be pushed to the wrong side.
        if !can_skip_river && entity.y > RIVER_Y_MIN && entity.y < RIVER_Y_MAX
            && !is_on_bridge(entity.x)
        {
            entity.y = match entity.team {
                Team::Player1 => RIVER_Y_MIN, // P1 side: push back to Y=-1200
                Team::Player2 => RIVER_Y_MAX, // P2 side: push back to Y=+1200
            };
        }

        // Fix 2: Bridge lateral clamp — prevent ally-avoidance from pushing troops
        // off the bridge while crossing the river.
        //
        // BUG FIX: Ally-avoidance steering (alpha=0.4 lateral push) can nudge
        // troops sideways off the bridge edge by a few units per tick. Once off-bridge,
        // the river barrier pushes them back to their own side, making them unable
        // to cross. Example: skeleton at x=-3898 is 2 units outside bridge bounds
        // (bridge edge = -3900), gets river-barriered, bounces back.
        //
        // Fix: While inside the river Y band, clamp X to the nearest bridge's X
        // range. This keeps troops "on the bridge" even with lateral steering.
        // Only fires inside the river zone — doesn't affect normal movement.
        if !can_skip_river && entity.y > RIVER_Y_MIN && entity.y < RIVER_Y_MAX {
            // Find which bridge the troop is closer to
            let dist_left = (entity.x - BRIDGE_LEFT_X).abs();
            let dist_right = (entity.x - BRIDGE_RIGHT_X).abs();
            let (bridge_cx, _) = if dist_left <= dist_right {
                (BRIDGE_LEFT_X, dist_left)
            } else {
                (BRIDGE_RIGHT_X, dist_right)
            };
            // Clamp X to stay within the nearest bridge's bounds
            entity.x = entity.x.clamp(bridge_cx - BRIDGE_HALF_W, bridge_cx + BRIDGE_HALF_W);
        }
    }
}

// =========================================================================
// Collision resolution — entity separation + building blocking
// =========================================================================

/// Snapshot of an entity's collision-relevant state for the separation pass.
struct CollisionBody {
    idx: usize,
    id: EntityId,
    x: i32,
    y: i32,
    radius: i32,
    mass: i32,
    is_flying: bool,
    is_building: bool,
    is_troop: bool,
    target_x: i32,
    target_y: i32,
    /// Current target entity ID (for skipping attacker↔target collision).
    target_id: Option<EntityId>,
}

/// Post-movement collision resolution. Called after tick_movement.
///
/// Two passes:
///   1. **Entity-building**: troops pushed out of overlapping buildings,
///      with tangential sliding to flow around the obstacle toward target.
///   2. **Entity-entity separation**: overlapping troops push each other apart,
///      weighted by mass (heavier units displace lighter ones more).
///
/// Flying units only collide with other flying units.
/// Ground units collide with ground units and buildings.
/// Projectiles and spell zones are ignored entirely.
pub fn tick_collisions(state: &mut GameState) {
    let len = state.entities.len();
    if len < 2 {
        return;
    }

    // Snapshot target positions for building slide computation
    let target_snapshots = build_target_snapshots(state);

    // Build collision body snapshots
    let bodies: Vec<CollisionBody> = state.entities.iter().enumerate()
        .filter_map(|(idx, e)| {
            if !e.alive || e.deploy_timer > 0 {
                return None;
            }
            // Skip projectiles and spell zones — they don't collide
            if matches!(e.kind, EntityKind::Projectile(_) | EntityKind::SpellZone(_)) {
                return None;
            }
            let radius = e.collision_radius;
            if radius <= 0 {
                return None;
            }

            // Resolve target position for troops (for tangential sliding)
            let (tx, ty) = if e.is_troop() {
                if let Some(tid) = e.target {
                    target_snapshots.iter()
                        .find(|s| s.id == tid)
                        .map(|s| (s.x, s.y))
                        .unwrap_or_else(|| default_target_for_troop(state, e.team, e.x))
                } else {
                    default_target_for_troop(state, e.team, e.x)
                }
            } else {
                (e.x, e.y)
            };

            Some(CollisionBody {
                idx,
                id: e.id,
                x: e.x,
                y: e.y,
                radius,
                mass: e.mass.max(1),
                is_flying: e.is_flying(),
                is_building: e.is_building(),
                is_troop: e.is_troop(),
                target_x: tx,
                target_y: ty,
                target_id: e.target,
            })
        })
        .collect();

    // ── Inject towers as immovable collision bodies ──
    // Towers live in PlayerState, not the entity Vec, so tick_collisions
    // previously ignored them. Troops walked straight through their own
    // (and enemy) towers. Adding them here as synthetic collision bodies
    // with is_building=true and a sentinel idx (usize::MAX) ensures troops
    // get pushed out of tower footprints via the building-collision path.
    // Tower collision radius: ~1000 units (roughly 1.7 tiles), matching
    // the visual footprint in real CR.
    const TOWER_COLLISION_RADIUS: i32 = 1000;
    let tower_entries: [(EntityId, Team, &TowerState); 6] = [
        (P1_KING_TOWER_ID, Team::Player1, &state.player1.king),
        (P1_PRINCESS_LEFT_ID, Team::Player1, &state.player1.princess_left),
        (P1_PRINCESS_RIGHT_ID, Team::Player1, &state.player1.princess_right),
        (P2_KING_TOWER_ID, Team::Player2, &state.player2.king),
        (P2_PRINCESS_LEFT_ID, Team::Player2, &state.player2.princess_left),
        (P2_PRINCESS_RIGHT_ID, Team::Player2, &state.player2.princess_right),
    ];
    let mut bodies = bodies; // make mutable so we can push tower bodies
    for (tid, team, tower) in &tower_entries {
        if !tower.alive {
            continue;
        }
        bodies.push(CollisionBody {
            idx: usize::MAX, // Sentinel: towers are never pushed (immovable)
            id: *tid,
            x: tower.pos.0,
            y: tower.pos.1,
            radius: TOWER_COLLISION_RADIUS,
            mass: 1000, // Effectively infinite — towers don't move
            is_flying: false,
            is_building: true,
            is_troop: false,
            target_x: tower.pos.0,
            target_y: tower.pos.1,
            target_id: None,
        });
    }

    // Collect displacement vectors: (entity_index, push_x, push_y)
    let mut pushes: Vec<(usize, i32, i32)> = Vec::new();

    let body_count = bodies.len();
    for i in 0..body_count {
        let a = &bodies[i];

        // Only troops get pushed (buildings are immovable)
        if !a.is_troop {
            continue;
        }

        for j in (i + 1)..body_count {
            let b = &bodies[j];

            // Flying/ground layer check: flying only collides with flying,
            // ground only with ground (and buildings which are always ground).
            if a.is_flying != b.is_flying && !b.is_building {
                continue;
            }
            // Flying troops don't collide with ground buildings
            if a.is_flying && b.is_building {
                continue;
            }

            // Skip collision between a troop and its current attack target.
            // A Knight targeting a Giant needs to be AT the Giant to attack —
            // collision shouldn't push them apart and prevent combat.
            if a.is_troop && a.target_id == Some(b.id) {
                continue;
            }
            if b.is_troop && b.target_id == Some(a.id) {
                continue;
            }

            let dx = (a.x - b.x) as i64;
            let dy = (a.y - b.y) as i64;
            let dist_sq = dx * dx + dy * dy;
            let min_dist = (a.radius + b.radius) as i64;
            let min_dist_sq = min_dist * min_dist;

            if dist_sq >= min_dist_sq {
                continue; // No overlap
            }

            let dist = if dist_sq > 0 {
                (dist_sq as f64).sqrt() as i32
            } else {
                1 // Exact overlap — push in arbitrary direction
            };

            let overlap = (a.radius + b.radius) - dist;
            if overlap <= 0 {
                continue;
            }

            // Separation direction: from B toward A (pushes A away from B)
            let sep_x = if dist > 0 { dx } else { 1 }; // Arbitrary if coincident
            let sep_y = if dist > 0 { dy } else { 0 };

            if b.is_building {
                // Building collision: push troop out + slide tangentially.
                //
                // Pure radial push causes jitter (troop walks into building,
                // gets pushed out, walks in again). Instead:
                //   1. Push radially just enough to resolve overlap
                //   2. Add tangential component that slides the troop along the
                //      building's edge toward its movement target
                //
                // The tangent is perpendicular to the separation vector,
                // oriented toward the troop's target.

                // Radial push-out (resolve overlap + 1 unit of clearance)
                let push_out = overlap + 1;
                let rad_x = (sep_x * push_out as i64 / dist as i64) as i32;
                let rad_y = (sep_y * push_out as i64 / dist as i64) as i32;

                // Tangential slide: perpendicular to separation, toward target.
                // Two perpendicular candidates: (-sep_y, sep_x) and (sep_y, -sep_x).
                // Pick the one whose dot product with (target - troop) is positive.
                let to_target_x = (a.target_x - a.x) as i64;
                let to_target_y = (a.target_y - a.y) as i64;

                let tan1_x = -sep_y;
                let tan1_y = sep_x;
                let dot1 = tan1_x * to_target_x + tan1_y * to_target_y;

                let (tan_x, tan_y) = if dot1 >= 0 {
                    (tan1_x, tan1_y)
                } else {
                    (sep_y, -sep_x)
                };

                // Scale tangential slide to ~half the troop's speed worth of
                // movement. This keeps the troop flowing around the building
                // without overshooting.
                let tan_len_sq = tan_x * tan_x + tan_y * tan_y;
                let tan_len = if tan_len_sq > 0 {
                    (tan_len_sq as f64).sqrt() as i32
                } else {
                    1
                };
                // Slide amount: proportional to overlap (deeper = stronger slide)
                // Capped at overlap to prevent overshooting
                let slide = overlap.min(30); // Cap at ~1 tick of medium speed
                let slide_x = (tan_x * slide as i64 / tan_len as i64) as i32;
                let slide_y = (tan_y * slide as i64 / tan_len as i64) as i32;

                pushes.push((a.idx, rad_x + slide_x, rad_y + slide_y));
            } else if b.is_troop {
                // Troop-troop separation: split the overlap by inverse mass ratio.
                // Heavier troop gets pushed less, lighter troop gets pushed more.
                // All values come from entity data: mass (from JSON), collision_radius
                // (from JSON), and the computed overlap from current positions.
                let total_mass = (a.mass + b.mass) as i64;

                // A gets pushed by B's share of mass (B is heavier → A moves more)
                let a_share = b.mass as i64;
                let b_share = a.mass as i64;

                let a_push_x = (sep_x * overlap as i64 * a_share / (total_mass * dist as i64).max(1)) as i32;
                let a_push_y = (sep_y * overlap as i64 * a_share / (total_mass * dist as i64).max(1)) as i32;
                let b_push_x = (-sep_x * overlap as i64 * b_share / (total_mass * dist as i64).max(1)) as i32;
                let b_push_y = (-sep_y * overlap as i64 * b_share / (total_mass * dist as i64).max(1)) as i32;

                pushes.push((a.idx, a_push_x, a_push_y));
                pushes.push((b.idx, b_push_x, b_push_y));
            }
        }
    }

    // Apply all pushes with per-entity cap.
    // Multiple overlapping allies can accumulate large pushes on edge troops
    // (5 barbarians: edge troop gets pushed by 4 others). Without a cap,
    // this launches troops out of formation. In real CR, troops in a cluster
    // gently separate — they don't explode apart.
    //
    // Cap: each entity's total displacement per tick is limited to its own
    // collision_radius (from JSON data). A troop can be pushed at most one
    // body-width per tick, regardless of how many allies overlap it.
    // This is data-driven (collision_radius varies per troop type).
    //
    // First accumulate per-entity, then cap, then apply.
    let mut accumulated: Vec<(i64, i64)> = vec![(0, 0); state.entities.len()];
    for (idx, px, py) in pushes {
        if idx < accumulated.len() {
            accumulated[idx].0 += px as i64;
            accumulated[idx].1 += py as i64;
        }
    }
    for (idx, (ax, ay)) in accumulated.iter().enumerate() {
        if *ax == 0 && *ay == 0 { continue; }
        if idx >= state.entities.len() || !state.entities[idx].alive { continue; }

        let mut px = *ax as i32;
        let mut py = *ay as i32;

        // Cap total push magnitude to this entity's collision_radius
        let cap = state.entities[idx].collision_radius;
        let mag_sq = (px as i64) * (px as i64) + (py as i64) * (py as i64);
        let cap_sq = (cap as i64) * (cap as i64);
        if mag_sq > cap_sq && mag_sq > 0 {
            let mag = (mag_sq as f64).sqrt();
            px = (px as f64 * cap as f64 / mag) as i32;
            py = (py as f64 * cap as f64 / mag) as i32;
        }

        state.entities[idx].x += px;
        state.entities[idx].y += py;
        state.entities[idx].x = state.entities[idx].x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
        state.entities[idx].y = state.entities[idx].y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
    }
}

// =========================================================================
// Combat — melee + ranged attacks
// =========================================================================

pub struct AttackEvent {
    pub attacker_x: i32,
    pub attacker_y: i32,
    pub attacker_team: Team,
    pub target_id: EntityId,
    pub target_x: i32,
    pub target_y: i32,
    pub damage: i32,
    pub is_ranged: bool,
    pub projectile_key: Option<String>,
    pub splash_radius: i32,
    pub crown_tower_damage_percent: i32,
    pub source_id: EntityId,
    pub multiple_projectiles: i32,
    /// Phase 3: index of attacker in entities vec (for evo callbacks).
    pub attacker_idx: usize,
    /// If true, melee splash centers on attacker (Valkyrie 360° spin).
    /// If false, melee splash centers on target. Data-driven from self_as_aoe_center.
    pub self_as_aoe_center: bool,
    /// Number of separate targets to hit simultaneously (E-Wiz=2). Data-driven.
    pub multiple_targets: i32,
    /// Custom first projectile key (Princess, Hunter). None after first shot.
    pub custom_first_projectile: Option<String>,
    /// Buff key to apply to hit targets on each normal attack (EWiz stun, Zappies stun).
    /// Data-driven from CharacterStats.buff_on_damage via TroopData.buff_on_damage_key.
    pub buff_on_damage_key: Option<String>,
    /// Duration of the on-hit buff in ticks.
    pub buff_on_damage_ticks: i32,
    /// Horizontal offset from attacker center where the projectile spawns.
    /// Data-driven from CharacterStats.projectile_start_radius via TroopData.
    /// Shifts the projectile spawn point along the attacker→target direction
    /// so projectiles originate at the troop's weapon/hand, not dead center.
    /// 0 = spawn at attacker center (legacy default).
    pub projectile_start_radius: i32,
    /// Vertical (Z) offset for the projectile spawn point.
    /// Data-driven from CharacterStats.projectile_start_z via TroopData.
    /// Stored for replay fidelity; affects the projectile entity's initial z.
    pub projectile_start_z: i32,
    /// Attacker's attack range squared. Used to limit secondary targets
    /// (multiple_targets > 1, e.g., E-Wiz) to within the attacker's range.
    /// In real CR, E-Wiz's second bolt only hits a target within attack range,
    /// not across the entire arena. Data-driven from TroopData.range_sq or
    /// BuildingData.range_sq.
    pub attacker_range_sq: i64,
    /// Fix #6: Melee pushback distance applied to TARGET on each melee hit.
    /// Data-driven from CharacterStats.melee_pushback via TroopData.
    /// Separate from attack_push_back (self-knockback on ATTACKER).
    /// 0 = no melee pushback (default).
    pub melee_pushback: i32,
    /// If true, melee pushback affects ALL enemies in splash radius.
    /// Data-driven from CharacterStats.is_melee_pushback_all via TroopData.
    pub melee_pushback_all: bool,
}

/// Apply crown tower damage reduction.
/// Handles two data formats:
///   Positive (e.g., 35): deal 35% of normal damage to towers
///   Negative (e.g., -75): reduce damage by 75% (= deal 25% of normal)
///   Zero: no reduction (full damage)
pub fn apply_ct_reduction(damage: i32, ct_percent: i32) -> i32 {
    if ct_percent > 0 && ct_percent < 100 {
        // Positive: treat as "deal ct_percent% of normal"
        (damage as i64 * ct_percent as i64 / 100) as i32
    } else if ct_percent < 0 && ct_percent > -100 {
        // Negative: treat as "reduce by |ct_percent|%"
        // e.g., -75 means deal (100-75)% = 25% of normal
        (damage as i64 * (100 + ct_percent) as i64 / 100) as i32
    } else {
        damage
    }
}

/// Run combat: collect attacks, resolve melee, spawn projectiles.
pub fn tick_combat(state: &mut GameState, data: &GameData) {
    let mut attacks: Vec<AttackEvent> = Vec::new();
    // Deferred dash landing pushbacks: (center_x, center_y, team, radius, push_distance)
    let mut dash_pushbacks: Vec<(i32, i32, Team, i32, i32)> = Vec::new();
    // Fix #9: Deferred area_effect_on_dash zone spawns: (team, spell_key, x, y, level)
    let mut dash_area_effects: Vec<(Team, String, i32, i32, usize)> = Vec::new();

    // Fix #16: Snapshot which entities have shields before combat resolves.
    // After combat, any entity that had shield_hp > 0 and now has shield_hp == 0
    // triggers shield_die_pushback (if set). (entity_id, x, y, team, pushback_dist)
    let shields_before: Vec<(EntityId, i32)> = state.entities.iter()
        .filter_map(|e| {
            if !e.alive || e.shield_hp <= 0 { return None; }
            if let EntityKind::Troop(ref t) = e.kind {
                if t.shield_die_pushback > 0 {
                    return Some((e.id, t.shield_die_pushback));
                }
            }
            None
        })
        .collect();

    let snapshots = build_target_snapshots(state);
    let entity_count = state.entities.len();

    // Kamikaze events: (attacker_idx, target_id, impact_x, impact_y, damage,
    //   splash_radius, team, ct_pct, buff_key, buff_time)
    struct KamikazeEvent {
        attacker_idx: usize,
        impact_x: i32,
        impact_y: i32,
        damage: i32,
        splash_radius: i32,
        team: Team,
        crown_tower_damage_percent: i32,
        buff_key: Option<String>,
        buff_time: i32,
    }
    let mut kamikaze_events: Vec<KamikazeEvent> = Vec::new();

    for ei in 0..entity_count {
        let entity = &state.entities[ei];
        // FIX: Dashing troops with dash immunity (is_targetable() == false) must still
        // have their dash_timer decremented and dash landing processed. Without this
        // bypass, Bandit/Golden Knight's dash_timer never counts down because tick_combat
        // skips them at this gate → is_dashing stays true forever → permanent freeze.
        // The dash processing block (below) checks is_dashing independently and handles
        // the full dash lifecycle: timer decrement, position snap, damage on landing,
        // transition to backswing, and cooldown reset.
        let is_dashing_immune = match &entity.kind {
            EntityKind::Troop(ref t) => t.is_dashing && t.dash_immune_remaining > 0,
            _ => false,
        };
        if !entity.is_targetable() && !is_dashing_immune {
            continue;
        }

        // Phase 3: Skip combat if immobilized
        if entity.is_immobilized() {
            // Stun/Freeze breaks inferno beam — reset ramp
            let entity = &mut state.entities[ei];
            match &mut entity.kind {
                EntityKind::Troop(ref mut t) => {
                    if t.ramp_damage3 > 0 { t.ramp_ticks = 0; }
                    // Stun/freeze cancels attack animation AND resets attack cooldown.
                    // In real CR, stun is a true interrupt — the troop must wait a full
                    // hit_speed cycle after the stun before attacking again. This is why
                    // EWiz hard-counters slow melee troops like PEKKA.
                    t.attack_phase = crate::entities::AttackPhase::Idle;
                    t.phase_timer = 0;
                    t.windup_target = None;
                    t.attack_cooldown = t.hit_speed; // Force full reload
                    // Stun/freeze resets charge (Prince must re-charge after stun)
                    if t.charge_range > 0 {
                        t.is_charging = false;
                        t.charge_hit_ready = false;
                        t.charge_distance_remaining = t.charge_range;
                    }
                    // Cancel in-progress dash
                    t.is_dashing = false;
                    t.dash_timer = 0;
                    t.dash_immune_remaining = 0;
                }
                EntityKind::Building(ref mut b) if b.ramp_damage3 > 0 => { b.ramp_ticks = 0; }
                _ => {}
            }
            continue;
        }

        let target_id = match entity.target {
            Some(t) => t,
            None => {
                // No target — cancel any in-progress windup
                if let EntityKind::Troop(ref mut t) = state.entities[ei].kind {
                    // FIX: If this troop is mid-dash, continue ticking the dash timer
                    // even without a target. The dash was already committed — the troop
                    // should fly to its dash_target position, land, deal damage (to
                    // whatever is there), and complete the backswing. Without this, a
                    // troop that loses its target mid-dash (target dies or moves out of
                    // sight) would have its dash_timer frozen by the `continue` below,
                    // causing a permanent stall identical to the is_targetable bug.
                    if t.is_dashing {
                        t.dash_timer -= 1;
                        if t.dash_immune_remaining > 0 {
                            t.dash_immune_remaining -= 1;
                        }
                        if t.dash_timer <= 0 {
                            // Dash landing — snap to target position, end dash.
                            // Read target coords into locals first to satisfy the
                            // borrow checker (can't write entity.x while holding
                            // ref mut t into entity.kind).
                            let land_x = t.dash_target_x;
                            let land_y = t.dash_target_y;
                            let landing_ticks = t.dash_landing_ticks;
                            let cooldown_max = t.dash_cooldown_max;
                            t.is_dashing = false;
                            t.dash_immune_remaining = 0;
                            // No damage dealt (no valid target at landing point).
                            // Transition to backswing so the troop pauses briefly
                            // before resuming normal behavior.
                            t.attack_phase = crate::entities::AttackPhase::Backswing;
                            t.phase_timer = landing_ticks.max(1);
                            t.dash_cooldown = cooldown_max;
                            // Drop the mutable borrow of `t` before writing x/y
                            let _ = t; // End mutable borrow of t before writing entity x/y
                            state.entities[ei].x = land_x;
                            state.entities[ei].y = land_y;
                        }
                        continue;
                    }
                    if t.attack_phase == crate::entities::AttackPhase::Windup {
                        t.attack_phase = crate::entities::AttackPhase::Idle;
                        t.phase_timer = 0;
                        t.windup_target = None;
                    }
                    // Still tick down backswing if in that phase (damage was already dealt)
                    if t.attack_phase == crate::entities::AttackPhase::Backswing {
                        t.phase_timer -= 1;
                        if t.phase_timer <= 0 {
                            if t.stop_time_after_attack > 0 {
                                t.attack_phase = crate::entities::AttackPhase::PostAttackStop;
                                t.phase_timer = t.stop_time_after_attack;
                            } else {
                                t.attack_phase = crate::entities::AttackPhase::Idle;
                                t.phase_timer = 0;
                            }
                        }
                    }
                    if t.attack_phase == crate::entities::AttackPhase::PostAttackStop {
                        t.phase_timer -= 1;
                        if t.phase_timer <= 0 {
                            t.attack_phase = crate::entities::AttackPhase::Idle;
                            t.phase_timer = 0;
                        }
                    }
                }
                continue;
            }
        };

        // Find target position from snapshots
        let target_snap = match snapshots.iter().find(|s| s.id == target_id) {
            Some(s) => s.clone(),
            None => {
                // Target not in snapshots (dead/cleaned up) — cancel windup
                if let EntityKind::Troop(ref mut t) = state.entities[ei].kind {
                    // FIX: If mid-dash, keep ticking the dash timer even though the
                    // target is gone. The troop is already committed to the dash
                    // trajectory and should land at dash_target, complete backswing,
                    // then resume normal behavior. Same logic as the None-target fix.
                    if t.is_dashing {
                        t.dash_timer -= 1;
                        if t.dash_immune_remaining > 0 {
                            t.dash_immune_remaining -= 1;
                        }
                        if t.dash_timer <= 0 {
                            // Read target coords into locals to satisfy borrow checker.
                            let land_x = t.dash_target_x;
                            let land_y = t.dash_target_y;
                            let landing_ticks = t.dash_landing_ticks;
                            let cooldown_max = t.dash_cooldown_max;
                            t.is_dashing = false;
                            t.dash_immune_remaining = 0;
                            t.attack_phase = crate::entities::AttackPhase::Backswing;
                            t.phase_timer = landing_ticks.max(1);
                            t.dash_cooldown = cooldown_max;
                            let _ = t; // End mutable borrow of t before writing entity x/y
                            state.entities[ei].x = land_x;
                            state.entities[ei].y = land_y;
                        }
                        continue;
                    }
                    if t.attack_phase == crate::entities::AttackPhase::Windup {
                        t.attack_phase = crate::entities::AttackPhase::Idle;
                        t.phase_timer = 0;
                        t.windup_target = None;
                    }
                    if t.attack_phase == crate::entities::AttackPhase::Backswing {
                        t.phase_timer -= 1;
                        if t.phase_timer <= 0 {
                            if t.stop_time_after_attack > 0 {
                                t.attack_phase = crate::entities::AttackPhase::PostAttackStop;
                                t.phase_timer = t.stop_time_after_attack;
                            } else {
                                t.attack_phase = crate::entities::AttackPhase::Idle;
                                t.phase_timer = 0;
                            }
                        }
                    }
                    if t.attack_phase == crate::entities::AttackPhase::PostAttackStop {
                        t.phase_timer -= 1;
                        if t.phase_timer <= 0 {
                            t.attack_phase = crate::entities::AttackPhase::Idle;
                            t.phase_timer = 0;
                        }
                    }
                }
                continue;
            }
        };

        // Phase 3: Compute buff-modified damage
        let damage_mult = entity.damage_multiplier();
        let effective_damage = (entity.damage as i64 * damage_mult as i64 / 100) as i32;

        // Phase 3: Compute buff-modified hit speed
        let hitspeed_mult = entity.hitspeed_multiplier();

        let entity = &mut state.entities[ei];
        match &mut entity.kind {
            EntityKind::Troop(ref mut troop) => {
                let dx = (entity.x - target_snap.x) as i64;
                let dy = (entity.y - target_snap.y) as i64;
                let dist_sq = dx * dx + dy * dy;

                // Inferno Dragon ramp: track how long we've been attacking the same target.
                // Reset ramp when: target changes, out of range, or immobilized (stun/freeze).
                if troop.ramp_damage3 > 0 {
                    if troop.ramp_target != Some(target_id) {
                        troop.ramp_ticks = 0;
                        troop.ramp_target = Some(target_id);
                    }
                    if dist_sq <= troop.range_sq {
                        troop.ramp_ticks += 1;
                    } else {
                        // Beam broken — target out of range, reset ramp
                        troop.ramp_ticks = 0;
                    }
                }

                // Compute ramp-adjusted damage (Inferno Dragon)
                let ramp_dmg = if troop.ramp_damage3 > 0 {
                    let t = troop.ramp_ticks;
                    if t > troop.ramp_time1 + troop.ramp_time2 {
                        troop.ramp_damage3
                    } else if t > troop.ramp_time1 {
                        troop.ramp_damage2
                    } else {
                        effective_damage
                    }
                } else {
                    effective_damage
                };

                // ─── Attack state machine ────────────────────────────────
                // Phase: Idle → Windup → (hit frame → damage) → Backswing → Idle
                //
                // Windup can be CANCELLED if the target changes or dies
                // (the "retarget reset" exploit). Backswing is committed —
                // damage was already dealt, troop just recovers.
                //
                // Kamikaze troops bypass the state machine (instant self-destruct).
                // Charge troops deal damage_special on their first hit after charge.
                // Dash troops lunge to distant targets (bypasses normal movement).

                // Kamikaze: self-destruct on reaching attack range (bypasses state machine)
                // ─── Fix #13: kamikaze_time delay (data-driven) ───
                // Some kamikaze troops (Skeleton Barrel=500ms) have a delay before
                // detonating. When entering range, prime the kamikaze timer. The troop
                // stops moving and counts down. Detonation fires when timer expires.
                if troop.kamikaze && dist_sq <= troop.range_sq && troop.attack_cooldown <= 0 {
                    // If kamikaze_delay > 0, use a countdown before detonation
                    if troop.kamikaze_delay > 0 && !troop.kamikaze_primed {
                        // Prime the kamikaze — start countdown, stop further movement
                        troop.kamikaze_primed = true;
                        troop.kamikaze_timer = troop.kamikaze_delay;
                        continue; // Don't detonate yet
                    }
                    if troop.kamikaze_primed && troop.kamikaze_timer > 0 {
                        troop.kamikaze_timer -= 1;
                        continue; // Still counting down
                    }
                    // Timer expired (or no delay) — detonate now
                    let (kz_damage, kz_radius, kz_buff, kz_buff_time) = if let Some(ref proj_key) = troop.projectile_key {
                        // FIX: Try direct lookup, then normalized key (for IceSpiritsProjectile etc.)
                        let ps_opt = data.projectiles.get(proj_key)
                            .or_else(|| {
                                let norm = proj_key.to_lowercase().replace(' ', "-");
                                data.projectiles.get(&norm)
                            });
                        if let Some(ps) = ps_opt {
                            let pdmg = if ramp_dmg > 0 {
                                ramp_dmg
                            } else if !ps.damage_per_level.is_empty() && troop.level > 0 {
                                let idx = (troop.level - 1).min(ps.damage_per_level.len() - 1);
                                ps.damage_per_level[idx]
                            } else {
                                ps.damage
                            };
                            let pradius = if ps.radius > 0 { ps.radius } else { troop.kamikaze_damage_radius };
                            let pbuff = ps.target_buff.clone().or(troop.kamikaze_buff.clone());
                            let pbuff_time = if ps.buff_time > 0 {
                                crate::entities::ms_to_ticks(ps.buff_time)
                            } else {
                                troop.kamikaze_buff_time
                            };
                            (pdmg, pradius, pbuff, pbuff_time)
                        } else {
                            // Projectile key exists but not found in data — fallback.
                            // Try death_damage from CharacterStats as kamikaze source.
                            let fallback_dmg = data.characters.get(&entity.card_key)
                                .map(|s| s.death_damage_at_level(troop.level))
                                .filter(|d| *d > 0)
                                .unwrap_or(ramp_dmg);
                            (fallback_dmg, troop.kamikaze_damage_radius, troop.kamikaze_buff.clone(), troop.kamikaze_buff_time)
                        }
                    } else {
                        // No projectile key at all — use death_damage from CharacterStats,
                        // falling back to entity.damage (ramp_dmg) if death_damage is 0.
                        let fallback_dmg = data.characters.get(&entity.card_key)
                            .map(|s| s.death_damage_at_level(troop.level))
                            .filter(|d| *d > 0)
                            .unwrap_or(ramp_dmg);
                        (fallback_dmg, troop.kamikaze_damage_radius, troop.kamikaze_buff.clone(), troop.kamikaze_buff_time)
                    };

                    kamikaze_events.push(KamikazeEvent {
                        attacker_idx: ei,
                        impact_x: target_snap.x,  // FIX: AoE centers on target (where spirit lands)
                        impact_y: target_snap.y,   // Was: entity.x/y — missed clustered enemies
                        damage: kz_damage,
                        splash_radius: kz_radius,
                        team: entity.team,
                        crown_tower_damage_percent: troop.crown_tower_damage_percent,
                        buff_key: kz_buff,
                        buff_time: kz_buff_time,
                    });
                    // Kamikaze replaces normal attack — skip state machine
                    continue;
                }

                // ─── Dash: tick in-progress dash ───
                if troop.is_dashing {
                    troop.dash_timer -= 1;

                    // Tick down invulnerability window
                    if troop.dash_immune_remaining > 0 {
                        troop.dash_immune_remaining -= 1;
                    }

                    if troop.dash_timer <= 0 {
                        // Dash landing — teleport to target, deal damage
                        troop.is_dashing = false;
                        troop.dash_immune_remaining = 0; // Invulnerability ends on landing
                        entity.x = troop.dash_target_x;
                        entity.y = troop.dash_target_y;

                        // Deal dash damage as AoE or single target
                        let d_damage = troop.dash_impact_damage;
                        let d_radius = troop.dash_radius;
                        let d_pushback = troop.dash_push_back;
                        if d_radius > 0 {
                            // AoE dash (Mega Knight jump)
                            attacks.push(AttackEvent {
                                attacker_x: entity.x,
                                attacker_y: entity.y,
                                attacker_team: entity.team,
                                target_id,
                                target_x: target_snap.x,
                                target_y: target_snap.y,
                                damage: d_damage,
                                is_ranged: false,
                                projectile_key: None,
                                splash_radius: d_radius,
                                crown_tower_damage_percent: troop.crown_tower_damage_percent,
                                source_id: entity.id,
                                multiple_projectiles: 1,
                                attacker_idx: ei,
                                self_as_aoe_center: false,
                                multiple_targets: 1,
                                custom_first_projectile: None,
                                buff_on_damage_key: None,
                                buff_on_damage_ticks: 0,
                                // Melee/dash/building attack: no projectile spawn offset.
                                projectile_start_radius: 0,
                                projectile_start_z: 0,
                                attacker_range_sq: troop.range_sq,
                                melee_pushback: troop.melee_pushback,
                                melee_pushback_all: troop.melee_pushback_all,
                            });
                            // Apply knockback to enemies in splash radius
                            if d_pushback > 0 {
                                dash_pushbacks.push((entity.x, entity.y, entity.team, d_radius, d_pushback));
                            }
                        } else {
                            // Single-target dash (Bandit)
                            attacks.push(AttackEvent {
                                attacker_x: entity.x,
                                attacker_y: entity.y,
                                attacker_team: entity.team,
                                target_id,
                                target_x: target_snap.x,
                                target_y: target_snap.y,
                                damage: d_damage,
                                is_ranged: false,
                                projectile_key: None,
                                splash_radius: 0,
                                crown_tower_damage_percent: troop.crown_tower_damage_percent,
                                source_id: entity.id,
                                multiple_projectiles: 1,
                                attacker_idx: ei,
                                self_as_aoe_center: false,
                                multiple_targets: 1,
                                custom_first_projectile: None,
                                buff_on_damage_key: None,
                                buff_on_damage_ticks: 0,
                                // Melee/dash/building attack: no projectile spawn offset.
                                projectile_start_radius: 0,
                                projectile_start_z: 0,
                                attacker_range_sq: troop.range_sq,
                                melee_pushback: troop.melee_pushback,
                                melee_pushback_all: troop.melee_pushback_all,
                            });
                        }
                        // Start backswing after dash landing
                        troop.attack_phase = crate::entities::AttackPhase::Backswing;
                        troop.phase_timer = troop.dash_landing_ticks.max(1);
                        troop.dash_cooldown = troop.dash_cooldown_max;

                        // Fix #9: area_effect_on_dash — spawn spell zone at landing point.
                        // Data-driven from CharacterStats.area_effect_on_dash via TroopData.
                        if let Some(ref area_key) = troop.area_effect_on_dash {
                            if !area_key.is_empty() {
                                dash_area_effects.push((
                                    entity.team,
                                    area_key.clone(),
                                    entity.x,
                                    entity.y,
                                    troop.level,
                                ));
                            }
                        }
                    }
                    continue; // Skip normal attack while dashing
                }

                // ─── Dash: trigger a new dash if conditions met ───
                if troop.dash_damage > 0 && troop.dash_cooldown <= 0 && !troop.is_dashing {
                    let dist = (dist_sq as f64).sqrt() as i32;
                    let in_dash_range = dist >= troop.dash_min_range && dist <= troop.dash_max_range;
                    if in_dash_range && troop.attack_phase == crate::entities::AttackPhase::Idle {
                        // Start dash — troop lunges to target position
                        troop.is_dashing = true;

                        // Compute travel time:
                        //   - dash_travel_ticks > 0 (MegaKnight): fixed duration from dash_constant_time
                        //   - dash_jump_speed > 0 (Bandit): distance-based travel time
                        //   - neither: fallback to 1 tick (instant teleport)
                        troop.dash_timer = if troop.dash_travel_ticks > 0 {
                            troop.dash_travel_ticks
                        } else if troop.dash_jump_speed > 0 {
                            (dist / troop.dash_jump_speed).max(1)
                        } else {
                            1
                        };

                        troop.dash_target_x = target_snap.x;
                        troop.dash_target_y = target_snap.y;
                        troop.dash_impact_damage = troop.dash_damage;

                        // Set dash invulnerability (Bandit, Golden Knight).
                        // In real CR, dashing troops are invulnerable for the ENTIRE
                        // dash travel duration. The data field dash_immune_to_damage_time
                        // (100ms) represents a startup buffer before immunity begins,
                        // but in our tick-based model dashes start instantly, so we set
                        // immunity equal to the full dash travel time.
                        // MK has dash_immune_ticks=0 → no immunity (correct: MK jump
                        // is not invulnerable).
                        troop.dash_immune_remaining = if troop.dash_immune_ticks > 0 {
                            troop.dash_timer // Full dash duration
                        } else {
                            0
                        };

                        continue; // Skip normal attack this tick
                    }
                }

                // Tick dash cooldown
                if troop.dash_cooldown > 0 {
                    troop.dash_cooldown -= 1;
                }

                use crate::entities::AttackPhase;

                // Safety check: verify target is valid for this troop's air/ground capability.
                if target_snap.is_flying && !troop.attacks_air {
                    state.entities[ei].target = None;
                    if let EntityKind::Troop(ref mut t) = state.entities[ei].kind {
                        if t.attack_phase == AttackPhase::Windup {
                            t.attack_phase = AttackPhase::Idle;
                            t.phase_timer = 0;
                            t.windup_target = None;
                        }
                    }
                    continue;
                }
                if !target_snap.is_flying && !troop.attacks_ground {
                    state.entities[ei].target = None;
                    if let EntityKind::Troop(ref mut t) = state.entities[ei].kind {
                        if t.attack_phase == AttackPhase::Windup {
                            t.attack_phase = AttackPhase::Idle;
                            t.phase_timer = 0;
                            t.windup_target = None;
                        }
                    }
                    continue;
                }

                match troop.attack_phase {
                    AttackPhase::Idle => {
                        // Tick down the legacy cooldown (used for load_first_hit /
                        // load_after_retarget initial delay before the very first attack).
                        // Scale by hitspeed multiplier: Rage (135%) → decrement faster,
                        // Slow (50%) → decrement slower. This matches real CR where
                        // Rage speeds up the initial attack windup.
                        if troop.attack_cooldown > 0 {
                            let cd_decrement = (hitspeed_mult as i64 / 100).max(1) as i32;
                            troop.attack_cooldown = (troop.attack_cooldown - cd_decrement).max(0);
                        }

                        // Start windup if in range and ready
                        if dist_sq <= troop.range_sq && troop.attack_cooldown <= 0 {
                            if troop.windup_ticks > 0 {
                                troop.attack_phase = AttackPhase::Windup;
                                // Scale windup duration by hit speed buff.
                                // Rage (135%) → windup takes 100/135 = 74% of normal time.
                                // Slow (50%) → windup takes 100/50 = 200% of normal time.
                                let scaled_windup = (troop.windup_ticks as i64 * 100 / hitspeed_mult as i64) as i32;
                                troop.phase_timer = scaled_windup.max(1);
                                troop.windup_target = Some(target_id);
                            } else {
                                // Zero windup (e.g., very fast attackers) — hit immediately
                                let num_proj = troop.multiple_projectiles.max(1);
                                // Charge: use special damage on first hit after charge
                                let hit_dmg = if troop.charge_hit_ready && troop.charge_damage > 0 {
                                    troop.charge_hit_ready = false;
                                    troop.is_charging = false;
                                    troop.charge_distance_remaining = troop.charge_range;
                                    troop.charge_damage
                                } else {
                                    ramp_dmg
                                };
                                attacks.push(AttackEvent {
                                    attacker_x: entity.x,
                                    attacker_y: entity.y,
                                    attacker_team: entity.team,
                                    target_id,
                                    target_x: target_snap.x,
                                    target_y: target_snap.y,
                                    damage: hit_dmg,
                                    is_ranged: troop.is_ranged,
                                    projectile_key: troop.projectile_key.clone(),
                                    splash_radius: troop.area_damage_radius,
                                    crown_tower_damage_percent: troop.crown_tower_damage_percent,
                                    source_id: entity.id,
                                    multiple_projectiles: num_proj,
                                    attacker_idx: ei,
                                    self_as_aoe_center: troop.self_as_aoe_center,
                                    multiple_targets: troop.multiple_targets,
                                    custom_first_projectile: if !troop.has_fired_first { troop.custom_first_projectile.clone() } else { None },
                                    buff_on_damage_key: troop.buff_on_damage_key.clone(),
                                    buff_on_damage_ticks: troop.buff_on_damage_ticks,
                                    // FIX 1: Data-driven projectile spawn offset from CharacterStats.
                                    projectile_start_radius: troop.projectile_start_radius,
                                    projectile_start_z: troop.projectile_start_z,
                                    attacker_range_sq: troop.range_sq,
                                melee_pushback: troop.melee_pushback,
                                melee_pushback_all: troop.melee_pushback_all,
                                });
                                // Go to backswing (or idle if no backswing)
                                if troop.backswing_ticks > 0 {
                                    let base_bs = troop.backswing_ticks;
                                    troop.attack_phase = AttackPhase::Backswing;
                                    troop.phase_timer = (base_bs as i64 * 100 / hitspeed_mult as i64) as i32;
                                } else {
                                    troop.attack_phase = AttackPhase::Idle;
                                    // Full hit_speed cycle as cooldown
                                    let base_hs = troop.hit_speed;
                                    troop.attack_cooldown = (base_hs as i64 * 100 / hitspeed_mult as i64) as i32;
                                }
                                troop.windup_target = None;
                            }
                        }
                    }
                    AttackPhase::Windup => {
                        // Check if target is still valid:
                        // - Same target (windup_target matches current target)
                        // - Target not WAY out of range (generous 2x leash)
                        //
                        // In real CR, once a melee troop commits to an attack
                        // animation, the hit lands even if the target walks slightly
                        // out of range (the troop "lunges"). But if the target
                        // moves very far (e.g., pulled by Tornado, or retargeted),
                        // the attack cancels.
                        //
                        // We use 4x range_sq (= 2x range distance) as the leash.
                        // This prevents the "infinite windup cancel" bug for slow
                        // attackers (MK, PEKKA) while still cancelling attacks
                        // when the target genuinely escapes.
                        let target_changed = troop.windup_target != Some(target_id);
                        let too_far = dist_sq > troop.range_sq * 4; // 2x range distance

                        if target_changed || too_far {
                            // CANCEL — target switched (retarget reset exploit)
                            troop.attack_phase = AttackPhase::Idle;
                            troop.phase_timer = 0;
                            troop.windup_target = None;
                        } else {
                            // Tick down windup timer.
                            // Timer was already scaled by hitspeed_mult when entering
                            // Windup, so we always decrement by 1.
                            troop.phase_timer -= 1;

                            if troop.phase_timer <= 0 {
                                // ═══ HIT FRAME ═══
                                // Windup complete — deal damage!
                                let num_proj = troop.multiple_projectiles.max(1);
                                // Charge: use special damage on first hit after charge
                                let hit_dmg = if troop.charge_hit_ready && troop.charge_damage > 0 {
                                    troop.charge_hit_ready = false;
                                    troop.is_charging = false;
                                    troop.charge_distance_remaining = troop.charge_range;
                                    troop.charge_damage
                                } else {
                                    ramp_dmg
                                };
                                attacks.push(AttackEvent {
                                    attacker_x: entity.x,
                                    attacker_y: entity.y,
                                    attacker_team: entity.team,
                                    target_id,
                                    target_x: target_snap.x,
                                    target_y: target_snap.y,
                                    damage: hit_dmg,
                                    is_ranged: troop.is_ranged,
                                    projectile_key: troop.projectile_key.clone(),
                                    splash_radius: troop.area_damage_radius,
                                    crown_tower_damage_percent: troop.crown_tower_damage_percent,
                                    source_id: entity.id,
                                    multiple_projectiles: num_proj,
                                    attacker_idx: ei,
                                    self_as_aoe_center: troop.self_as_aoe_center,
                                    multiple_targets: troop.multiple_targets,
                                    custom_first_projectile: if !troop.has_fired_first { troop.custom_first_projectile.clone() } else { None },
                                    buff_on_damage_key: troop.buff_on_damage_key.clone(),
                                    buff_on_damage_ticks: troop.buff_on_damage_ticks,
                                    // FIX 1: Data-driven projectile spawn offset from CharacterStats.
                                    projectile_start_radius: troop.projectile_start_radius,
                                    projectile_start_z: troop.projectile_start_z,
                                    attacker_range_sq: troop.range_sq,
                                melee_pushback: troop.melee_pushback,
                                melee_pushback_all: troop.melee_pushback_all,
                                });
                                troop.windup_target = None;

                                // Transition to backswing
                                if troop.backswing_ticks > 0 {
                                    let base_bs = troop.backswing_ticks;
                                    troop.attack_phase = AttackPhase::Backswing;
                                    troop.phase_timer = (base_bs as i64 * 100 / hitspeed_mult as i64) as i32;
                                } else {
                                    troop.attack_phase = AttackPhase::Idle;
                                    troop.phase_timer = 0;
                                }
                            }
                        }
                    }
                    AttackPhase::Backswing => {
                        // Recovery phase — damage was already dealt, just count down.
                        // Troop cannot move or start new attacks during backswing
                        // (movement is blocked in tick_movement).
                        troop.phase_timer -= 1;
                        if troop.phase_timer <= 0 {
                            // ─── Fix #6: stop_time_after_attack (data-driven) ───
                            // If the troop has a post-attack pause, transition there
                            // before returning to Idle. Otherwise go directly to Idle.
                            if troop.stop_time_after_attack > 0 {
                                troop.attack_phase = AttackPhase::PostAttackStop;
                                troop.phase_timer = troop.stop_time_after_attack;
                            } else {
                                troop.attack_phase = AttackPhase::Idle;
                                troop.phase_timer = 0;
                            }
                            // No additional cooldown — backswing IS the cooldown.
                            // Troop can immediately start a new windup next tick.
                        }
                    }
                    AttackPhase::PostAttackStop => {
                        // Post-attack pause — troop cannot move but the attack cycle
                        // is complete. In real CR, troops like PEKKA pause briefly
                        // after their swing before walking again.
                        troop.phase_timer -= 1;
                        if troop.phase_timer <= 0 {
                            troop.attack_phase = AttackPhase::Idle;
                            troop.phase_timer = 0;
                        }
                    }
                }
            }
            EntityKind::Building(ref mut bld) => {
                if bld.hit_speed <= 0 {
                    continue;
                }

                // Safety check: verify target is valid for this building's air/ground capability.
                // tick_targeting should only assign valid targets, but this guards against edge cases.
                if target_snap.is_flying && !bld.attacks_air {
                    state.entities[ei].target = None;
                    continue;
                }
                if !target_snap.is_flying && !bld.attacks_ground {
                    state.entities[ei].target = None;
                    continue;
                }

                let dx = (entity.x - target_snap.x) as i64;
                let dy = (entity.y - target_snap.y) as i64;
                let dist_sq = dx * dx + dy * dy;

                // Always tick down cooldown (scaled by hitspeed buff)
                if bld.attack_cooldown > 0 {
                    let cd_decrement = (hitspeed_mult as i64 / 100).max(1) as i32;
                    bld.attack_cooldown = (bld.attack_cooldown - cd_decrement).max(0);
                }

                // Inferno ramp: track how long we've been attacking the same target.
                // Reset ramp when: target changes or out of range.
                if bld.ramp_damage3 > 0 {
                    if bld.ramp_target != Some(target_id) {
                        bld.ramp_ticks = 0;
                        bld.ramp_target = Some(target_id);
                    }
                    // Increment ramp ticks each tick we're locked on and in range
                    if dist_sq <= bld.range_sq {
                        bld.ramp_ticks += 1;
                    } else {
                        // Beam broken — target out of range, reset ramp
                        bld.ramp_ticks = 0;
                    }
                }

                if dist_sq <= bld.range_sq && dist_sq >= bld.min_range_sq && bld.attack_cooldown <= 0 {
                    // Compute ramp-adjusted damage
                    let ramp_dmg = if bld.ramp_damage3 > 0 {
                        let t = bld.ramp_ticks;
                        if t > bld.ramp_time1 + bld.ramp_time2 {
                            bld.ramp_damage3  // Stage 3: max damage
                        } else if t > bld.ramp_time1 {
                            bld.ramp_damage2  // Stage 2: mid damage
                        } else {
                            entity.damage     // Stage 1: base damage
                        }
                    } else {
                        entity.damage
                    };

                    attacks.push(AttackEvent {
                        attacker_x: entity.x,
                        attacker_y: entity.y,
                        attacker_team: entity.team,
                        target_id,
                        target_x: target_snap.x,
                        target_y: target_snap.y,
                        damage: ramp_dmg,
                        is_ranged: bld.is_ranged,
                        projectile_key: bld.projectile_key.clone(),
                        // Data-driven from CharacterStats.area_damage_radius via BuildingData.
                        // Bomb Tower and other splash buildings now deal AoE on direct attacks.
                        // Ranged buildings get splash from ProjectileStats (projectile path).
                        splash_radius: bld.area_damage_radius,
                        crown_tower_damage_percent: bld.crown_tower_damage_percent,
                        source_id: entity.id,
                        multiple_projectiles: 1,
                        attacker_idx: ei,
                        // Data-driven from CharacterStats.self_as_aoe_center via BuildingData.
                        self_as_aoe_center: bld.self_as_aoe_center,
                        multiple_targets: 1,
                        custom_first_projectile: None,
                        // Data-driven from CharacterStats.buff_on_damage via BuildingData.
                        buff_on_damage_key: bld.buff_on_damage_key.clone(),
                        buff_on_damage_ticks: bld.buff_on_damage_ticks,
                        // Building attack: no projectile spawn offset (buildings are static).
                        projectile_start_radius: 0,
                        projectile_start_z: 0,
                        // Data-driven from BuildingData.range_sq for secondary target filtering.
                        attacker_range_sq: bld.range_sq,
                        melee_pushback: 0,
                        melee_pushback_all: false,
                    });
                    bld.attack_cooldown = (bld.hit_speed as i64 * 100 / hitspeed_mult as i64).max(1) as i32;
                }
            }
            _ => {}
        }
    }

    // ─── Fix #4: Expand multiple_targets attacks (E-Wiz) ───
    // If a troop has multiple_targets > 1, find additional targets and create
    // duplicate attack events for each. Each target gets a separate attack.
    // Secondary targets are limited to the attacker's attack range (data-driven
    // from attacker_range_sq). In real CR, E-Wiz's second bolt only hits enemies
    // within attack range, not across the entire arena.
    {
        let snapshots = build_target_snapshots(state);
        let mut extra_attacks: Vec<AttackEvent> = Vec::new();
        for atk in &attacks {
            if atk.multiple_targets <= 1 {
                continue;
            }
            // Find additional targets beyond the primary
            let mut used_targets = vec![atk.target_id];
            for _ in 1..atk.multiple_targets {
                // Find nearest enemy not already targeted, within attack range.
                let mut best_id: Option<EntityId> = None;
                let mut best_dist: i64 = i64::MAX;
                for snap in &snapshots {
                    if !snap.targetable || snap.team == atk.attacker_team {
                        continue;
                    }
                    if snap.is_invisible {
                        continue;
                    }
                    if used_targets.contains(&snap.id) {
                        continue;
                    }
                    let dx = (atk.attacker_x - snap.x) as i64;
                    let dy = (atk.attacker_y - snap.y) as i64;
                    let dist = dx * dx + dy * dy;
                    // Data-driven range limit: secondary targets must be within the
                    // attacker's attack range. attacker_range_sq is populated from
                    // TroopData.range_sq or BuildingData.range_sq at attack creation.
                    // 0 = no limit (fallback for legacy/dash attacks).
                    if atk.attacker_range_sq > 0 && dist > atk.attacker_range_sq {
                        continue;
                    }
                    if dist < best_dist {
                        best_dist = dist;
                        best_id = Some(snap.id);
                    }
                }
                if let Some(tid) = best_id {
                    used_targets.push(tid);
                    if let Some(tsnap) = snapshots.iter().find(|s| s.id == tid) {
                        let extra = AttackEvent {
                            attacker_x: atk.attacker_x,
                            attacker_y: atk.attacker_y,
                            attacker_team: atk.attacker_team,
                            target_id: tid,
                            target_x: tsnap.x,
                            target_y: tsnap.y,
                            damage: atk.damage,
                            is_ranged: atk.is_ranged,
                            projectile_key: atk.projectile_key.clone(),
                            splash_radius: atk.splash_radius,
                            crown_tower_damage_percent: atk.crown_tower_damage_percent,
                            source_id: atk.source_id,
                            multiple_projectiles: atk.multiple_projectiles,
                            attacker_idx: atk.attacker_idx,
                            self_as_aoe_center: atk.self_as_aoe_center,
                            multiple_targets: 1, // Already expanded
                            custom_first_projectile: None, // Only first shot uses custom
                            buff_on_damage_key: atk.buff_on_damage_key.clone(),
                            buff_on_damage_ticks: atk.buff_on_damage_ticks,
                            // FIX 1: Propagate projectile spawn offset to expanded multi-target attacks.
                            projectile_start_radius: atk.projectile_start_radius,
                            projectile_start_z: atk.projectile_start_z,
                            attacker_range_sq: atk.attacker_range_sq,
                            melee_pushback: atk.melee_pushback,
                            melee_pushback_all: atk.melee_pushback_all,
                        };
                        extra_attacks.push(extra);
                    }
                }
            }
        }
        attacks.extend(extra_attacks);
    }

    // ─── Fix #10: Mark has_fired_first for custom_first_projectile ───
    // After collecting attacks, mark troops that fired their first shot.
    for atk in &attacks {
        if atk.custom_first_projectile.is_some() {
            if atk.attacker_idx < state.entities.len() {
                if let EntityKind::Troop(ref mut t) = state.entities[atk.attacker_idx].kind {
                    t.has_fired_first = true;
                }
            }
        }
    }

    // ─── Fix #3: buff_after_hits — increment hit counter and apply buff at threshold ───
    // Evo Prince: PrinceBuff gains PrinceRageBuff1 after 2 hits, PrinceRageBuff2 after 4,
    // PrinceRageBuff3 after 6. The counter is cumulative across the troop's lifetime.
    // When the counter reaches buff_after_hits_count, the buff is applied and the counter
    // resets. The CharacterStats may have multiple buff_after_hits entries at different
    // thresholds — in the real data, these are separate character entries (PrinceBuff at
    // buff_after_hits_count=2, then a second entry at 4, third at 6). Our implementation
    // handles the single-entry case: one buff key + one threshold, counter resets on apply.
    // For multi-stage buffs, the data loading would need to chain them — but this covers
    // the primary data-driven path.
    {
        // Collect buff applications: (entity_idx, buff_key, duration_ticks)
        let mut buff_after_hit_events: Vec<(usize, String, i32)> = Vec::new();

        for atk in &attacks {
            if atk.attacker_idx >= state.entities.len() {
                continue;
            }
            if !state.entities[atk.attacker_idx].alive {
                continue;
            }
            if let EntityKind::Troop(ref mut t) = state.entities[atk.attacker_idx].kind {
                if t.buff_after_hits_key.is_some() && t.buff_after_hits_count > 0 {
                    t.buff_after_hits_counter += 1;
                    if t.buff_after_hits_counter >= t.buff_after_hits_count {
                        t.buff_after_hits_counter = 0;
                        let key = t.buff_after_hits_key.clone().unwrap();
                        let duration = t.buff_after_hits_time;
                        buff_after_hit_events.push((atk.attacker_idx, key, duration));
                    }
                }
            }
        }

        // Apply the collected buff_after_hits buffs (data-driven via from_buff_stats)
        for (idx, buff_key, duration) in buff_after_hit_events {
            if idx < state.entities.len() && state.entities[idx].alive {
                if let Some(bs) = data.buffs.get(&buff_key) {
                    state.entities[idx].add_buff(
                        crate::entities::ActiveBuff::from_buff_stats(buff_key, duration, bs)
                    );
                }
            }
        }
    }

    // Process attacks
    // Phase 3: Collect attacker indices for evo callbacks
    let mut evo_attack_indices: Vec<usize> = Vec::new();

    for atk in &attacks {
        evo_attack_indices.push(atk.attacker_idx);

        if atk.is_ranged {
            // ─── Fix #10: custom_first_projectile override ───
            // If this attack has a custom_first_projectile set (Princess, Hunter first shot),
            // use that projectile key instead of the normal one for this attack only.
            let effective_proj_key: Option<&String> = atk.custom_first_projectile.as_ref()
                .or(atk.projectile_key.as_ref());

            // Look up projectile data, with Deco fallback:
            // Princess uses "PrincessProjectileDeco" (damage=0, radius=0) but the
            // real damage/radius is on "PrincessProjectile". Try stripping "Deco".
            let proj_data = effective_proj_key
                .and_then(|k| {
                    let primary = data.projectiles.get(k.as_str());
                    // If primary has zero damage, try the non-Deco variant
                    if primary.map_or(true, |p| p.damage <= 0 && p.damage_per_level.is_empty()) {
                        let k_str: &str = k.as_str();
                        if let Some(stripped) = k_str.strip_suffix("Deco") {
                            data.projectiles.get(stripped).or(primary)
                        } else {
                            primary
                        }
                    } else {
                        primary
                    }
                });

            // Attacker-to-target distance — needed by hit_biggest zone radius,
            // gravity arc speed, and scatter geometry below.
            let dx_at = (atk.attacker_x - atk.target_x) as i64;
            let dy_at = (atk.attacker_y - atk.target_y) as i64;
            let dist_to_target = ((dx_at * dx_at + dy_at * dy_at) as f64).sqrt() as i32;

            // ─── Resolve per-projectile damage from projectile data ───
            // Hoisted before hit_biggest check since the zone needs per_proj_damage.
            // Hunter's character damage is 0; the HunterProjectile carries the damage.
            // Princess's PrincessProjectileDeco has 0 damage; PrincessProjectile has 140.
            let per_proj_damage = if let Some(pd) = proj_data {
                if !pd.damage_per_level.is_empty() {
                    let level = state.entities.iter()
                        .find(|e| e.id == atk.source_id)
                        .and_then(|e| if let EntityKind::Troop(ref t) = e.kind { Some(t.level) } else { None })
                        .unwrap_or(11);
                    let idx = (level.saturating_sub(1)).min(pd.damage_per_level.len() - 1);
                    pd.damage_per_level[idx]
                } else if pd.damage > 0 {
                    pd.damage
                } else {
                    atk.damage
                }
            } else {
                atk.damage
            };
            // Use attacker damage if projectile damage resolved to 0
            let per_proj_damage = if per_proj_damage > 0 { per_proj_damage } else { atk.damage };

            // ─── Data-driven hit_biggest for troop-fired projectiles ───
            // In real CR, if a troop's projectile has hit_biggest=true in its
            // ProjectileStats, the attack should target the N highest-HP enemies
            // within the projectile's radius — identical to Lightning-style spells.
            // Previously hit_biggest was only checked for spell cards (lib.rs),
            // so a troop whose projectile had this flag would fire standard
            // projectiles instead of targeted-strike zones. Now we check the
            // flag here and create a spell zone instead of spawning projectiles,
            // matching the same data-driven path used by spell projectiles.
            let is_hit_biggest = proj_data.map(|p| p.hit_biggest).unwrap_or(false);
            if is_hit_biggest {
                let splash = proj_data.map(|p| p.radius).unwrap_or(0).max(atk.splash_radius);
                let ct_pct = proj_data.map(|p| p.crown_tower_damage_percent)
                    .unwrap_or(atk.crown_tower_damage_percent);
                // Look up target_buff from the projectile (e.g., ZapFreeze stun)
                let spell_proj_key = effective_proj_key.cloned();
                let id = state.alloc_id();
                let zone = Entity::new_spell_zone(
                    id,
                    atk.attacker_team,
                    &atk.source_id.0.to_string(),
                    atk.target_x,
                    atk.target_y,
                    splash,             // radius of target selection area
                    1,                  // duration: single volley, dies after one hit cycle
                    0,                  // zone damage = 0 (damage comes from projectile_damage)
                    1,                  // hit_interval: fire once
                    true, true,         // affects_air, affects_ground
                    None,               // buff_key (applied via spell_projectile_key instead)
                    0,                  // buff_duration
                    true,               // only_enemies
                    false,              // only_own
                    ct_pct,
                    0,                  // attract_strength
                    None, 0, 0, 11,     // no spawner, default level
                    true,               // hit_biggest_targets = TRUE (data-driven)
                    3,                  // max_hit_targets fallback (Lightning default)
                    per_proj_damage,    // projectile_damage (per-strike)
                    ct_pct,             // projectile crown tower damage percent
                    spell_proj_key,     // spell_projectile_key for target_buff lookup
                    0,                  // spawn_min_radius
                    0,                  // heal_per_hit
                    0, false,           // no pushback
                    0, 0,               // no distance-scaled pushback
                    false,              // no_effect_to_crown_towers
                    true,               // affects_hidden (hit_biggest can hit hidden Tesla)
                    1, 1,               // level_scale: N/A (damage already level-scaled)
                );
                state.entities.push(zone);
                // Skip normal projectile spawning — the zone handles targeting.
                continue;
            }

            // ─── Data-driven gravity arc for troop-fired projectiles ───
            // In real CR, gravity-affected projectiles (gravity > 0 in ProjectileStats)
            // follow a parabolic trajectory. The travel time depends on distance and
            // gravity, not just horizontal speed. We use the same formula as spell
            // projectiles in lib.rs: t = sqrt(2 * distance / gravity_accel), then
            // effective_speed = distance / t. This gives correct timing for all
            // distances (near target = fast, far target = slow arc).
            // Previously this was only applied to spell-card projectiles, so troop-fired
            // projectiles with gravity would use the flat speed formula instead.
            let proj_gravity = proj_data.map(|p| p.gravity).unwrap_or(0);
            let proj_speed = if proj_gravity > 0 && dist_to_target > 0 {
                // Parabolic arc: t = sqrt(2 * distance / gravity_accel).
                // gravity in data units (e.g., 40). Scale factor converts data gravity
                // to internal-units/tick²: gravity * 0.3
                let g_accel = (proj_gravity as f64) * 0.3;
                let travel_dist = dist_to_target as f64;
                let arc_ticks = (2.0 * travel_dist / g_accel.max(1.0)).sqrt().max(1.0);
                (travel_dist / arc_ticks) as i32
            } else {
                proj_data.map(|p| p.speed).unwrap_or(60)
            };
            let homing = proj_data.map(|p| p.homing).unwrap_or(true);

            // FIX A: Check for scatter=Line (Hunter 10-bullet shotgun spread)
            let is_scatter = proj_data
                .and_then(|p| p.scatter.as_ref())
                .map(|s| s == "Line")
                .unwrap_or(false);

            // Get splash radius from projectile data (Princess: radius=2000).
            // For scatter projectiles (Hunter), projectile_radius is the per-bullet
            // hit radius (300u), which is more appropriate than area_damage_radius (70u).
            let proj_splash = if let Some(pd) = proj_data {
                if pd.radius > 0 {
                    pd.radius
                } else if is_scatter && pd.projectile_radius > 0 {
                    pd.projectile_radius
                } else {
                    atk.splash_radius
                }
            } else {
                atk.splash_radius
            };

            // Scatter geometry for multi-projectile spread — uses dx_at, dy_at,
            // dist_to_target computed above.

            // Perpendicular direction for scatter offset
            let (perp_x, perp_y) = if dist_to_target > 0 {
                ((-dy_at * 1000 / dist_to_target as i64) as i32,
                 (dx_at * 1000 / dist_to_target as i64) as i32)
            } else {
                (1000, 0)
            };

            let num_proj = atk.multiple_projectiles.max(1);

            // ── Model C: volley dedup for non-scatter multi-projectile ──
            // Non-scatter multi-arrow troops (Princess) fire multiple projectiles per
            // attack, each carrying FULL projectile damage with independent splash.
            // A target can only be hit by ONE arrow per volley — the multiple arrows
            // cover a wider area, they don't stack on the same target.
            //
            // Scatter attacks (Hunter shotgun) intentionally stack all bullets that
            // converge on the same point at close range — no dedup for scatter.
            //
            // We allocate a shared volley_id for all sibling projectiles. The impact
            // resolver in tick_projectiles uses this to skip entities already hit by
            // a sibling from the same volley.
            let volley_id = if !is_scatter && num_proj > 1 {
                state.alloc_id().0  // Unique ID shared by all arrows in this volley
            } else {
                0  // No dedup: single projectile or scatter (Hunter)
            };

            for i in 0..num_proj {
                let id = state.alloc_id();

                // FIX A: Scatter offset using data fields
                let (tx, ty) = if is_scatter && num_proj > 1 {
                    let center = (num_proj - 1) as f64 / 2.0;
                    let deviation = (i as f64 - center) / center.max(1.0);
                    // scatter=Line: bullets fan from origin. At projectile_range,
                    // the outermost bullet is projectile_start_extra_radius from center.
                    // spread_at_dist = projectile_start_extra_radius * dist / projectile_range
                    let extra_radius = proj_data
                        .map(|p| p.projectile_start_extra_radius)
                        .unwrap_or(650);
                    let proj_range = proj_data
                        .map(|p| p.projectile_range)
                        .unwrap_or(6500)
                        .max(1);
                    let spread_at_dist = (extra_radius as i64 * dist_to_target as i64 / proj_range as i64) as i32;
                    let offset = (deviation * spread_at_dist as f64) as i32;
                    let ox = (perp_x as i64 * offset as i64 / 1000) as i32;
                    let oy = (perp_y as i64 * offset as i64 / 1000) as i32;
                    (atk.target_x + ox, atk.target_y + oy)
                } else if num_proj > 1 {
                    // Non-scatter multi-projectile (Princess): small fixed spread
                    // so arrows don't all converge on the exact same point.
                    // In real CR, Princess arrows land in a small cluster pattern.
                    let center = (num_proj - 1) as f64 / 2.0;
                    let deviation = (i as f64 - center) / center.max(1.0);
                    let spread = 400; // Small fixed spread radius
                    let offset = (deviation * spread as f64) as i32;
                    let ox = (perp_x as i64 * offset as i64 / 1000) as i32;
                    let oy = (perp_y as i64 * offset as i64 / 1000) as i32;
                    (atk.target_x + ox, atk.target_y + oy)
                } else {
                    (atk.target_x, atk.target_y)
                };

                // ── Model C: each arrow carries full projectile damage ──
                // In real CR, each Princess arrow deals the full PrincessProjectile
                // damage (358 at lv11) to every target within its splash radius.
                // Damage stacking is prevented by volley dedup at impact time (each
                // target can only be hit once per volley), NOT by splitting damage.
                //
                // Scatter (Hunter): each bullet also carries full damage — stacking
                // IS correct for scatter (all 10 bullets hitting point-blank = 10× damage).
                let this_damage = per_proj_damage;

                // FIX 1: Data-driven projectile spawn offset.
                // Shift the projectile's origin along the attacker→target direction
                // by projectile_start_radius units (from CharacterStats). This places
                // the projectile at the troop's weapon position instead of dead center,
                // matching real CR where archers fire from beside their body, Executioner
                // throws from arm's reach, etc. Prevents projectiles from clipping
                // through the troop's own collision body on spawn.
                let (spawn_x, spawn_y) = if atk.projectile_start_radius > 0 {
                    let dx = (atk.target_x - atk.attacker_x) as f64;
                    let dy = (atk.target_y - atk.attacker_y) as f64;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist > 0.0 {
                        let offset = atk.projectile_start_radius as f64;
                        (
                            atk.attacker_x + (dx / dist * offset) as i32,
                            atk.attacker_y + (dy / dist * offset) as i32,
                        )
                    } else {
                        (atk.attacker_x, atk.attacker_y)
                    }
                } else {
                    (atk.attacker_x, atk.attacker_y)
                };

                // FIX 7: Data-driven aoe_to_air / aoe_to_ground from ProjectileStats.
                // Previously hardcoded to (true, true), which meant a ground-only
                // projectile (if one existed in the data) would incorrectly hit air
                // units. Now we read hits_air and aoe_to_air/aoe_to_ground from the
                // projectile data. Default to (true, true) if no projectile data is
                // available (safety fallback for unresolved projectile keys).
                let (proj_aoe_air, proj_aoe_ground) = if let Some(pd) = proj_data {
                    // ProjectileStats has three relevant fields:
                    //   aoe_to_air: splash damages air units
                    //   aoe_to_ground: splash damages ground units
                    //   hits_air: single-target can hit air (used for aoe fallback)
                    // If aoe_to_air/aoe_to_ground are both false but hits_air is set,
                    // treat it as aoe_to_air=true (the projectile CAN hit air targets).
                    let air = pd.aoe_to_air || pd.hits_air;
                    let ground = pd.aoe_to_ground || (!pd.aoe_to_air && !pd.hits_air);
                    // Default both to true if neither flag is set in the data (most
                    // projectiles just hit everything — only rare entries restrict this).
                    if !air && !ground {
                        (true, true) // No targeting flags set → hit everything
                    } else {
                        (air, ground)
                    }
                } else {
                    (true, true) // No projectile data → legacy fallback
                };

                let mut proj = Entity::new_projectile(
                    id,
                    atk.attacker_team,
                    atk.source_id,
                    spawn_x,   // FIX 1: offset spawn position
                    spawn_y,   // FIX 1: offset spawn position
                    atk.target_id,
                    tx, ty,
                    // Projectile speed conversion: raw ProjectileStats.speed values
                    // are in data units (100-5000). Standard conversion: speed * 6 / 10
                    // gives internal units/tick matching observed CR travel times.
                    // When gravity > 0, proj_speed was already computed as distance/arc_ticks
                    // (in internal units/tick) — no conversion needed.
                    if proj_gravity > 0 {
                        proj_speed.max(60)
                    } else {
                        (proj_speed * 6 / 10).max(60)
                    },
                    this_damage,
                    proj_splash,
                    homing && !is_scatter,
                    atk.crown_tower_damage_percent,
                    proj_aoe_air,   // FIX 7: data-driven from ProjectileStats
                    proj_aoe_ground, // FIX 7: data-driven from ProjectileStats
                );
                // FIX 1: Set projectile initial z from data-driven projectile_start_z.
                // This places the projectile at the correct vertical origin (e.g.,
                // Baby Dragon breathes fire from flying height + start_z offset).
                if atk.projectile_start_z > 0 {
                    proj.z = atk.projectile_start_z;
                }
                // FIX: Wire target_buff from ProjectileStats onto the spawned
                // projectile entity. Previously target_buff was always None,
                // so Ice Wizard slow, Snowball slow, etc. were never applied
                // via the normal ranged attack path.
                if let Some(pd) = proj_data {
                    if let EntityKind::Projectile(ref mut p) = proj.kind {
                        // ── Gravity arc (Fireball, Rocket, etc.) ──
                        // When gravity > 0, this projectile follows a ballistic arc to a
                        // fixed landing point. It does NOT track the target entity — moving
                        // troops can dodge it. In real CR, gravity-affected projectiles land
                        // at the cast location, not on the troop.
                        if pd.gravity > 0 {
                            p.is_gravity_arc = true;
                            p.homing = false; // Never track — fly to fixed (target_x, target_y)
                        }
                        if pd.target_buff.is_some() {
                            p.target_buff = pd.target_buff.clone();
                            p.target_buff_time = crate::entities::ms_to_ticks(pd.buff_time);
                            p.apply_buff_before_damage = pd.apply_buff_before_damage;
                        }
                        // Wire chain lightning fields (Electro Dragon, Electro Spirit)
                        if pd.chained_hit_count > 1 && pd.chained_hit_radius > 0 {
                            p.chained_hit_count = pd.chained_hit_count;
                            p.chained_hit_radius = pd.chained_hit_radius;
                        }
                        // Boomerang projectile (Executioner axe): travels to target
                        // then returns to source, dealing AoE damage both ways.
                        if pd.pingpong_visual_time > 0 {
                            p.is_boomerang = true;
                            p.boomerang_source_x = atk.attacker_x;
                            p.boomerang_source_y = atk.attacker_y;
                            p.boomerang_radius = if pd.projectile_radius > 0 {
                                pd.projectile_radius
                            } else {
                                pd.radius
                            };
                            // Non-homing: flies straight to the target point, not tracking
                            p.homing = false;
                        }
                    }
                }
                // Set card_key so the replay viewer can identify projectile types
                // (e.g., rolling Log/Bowler vs standard arrow).
                if let Some(ref pk) = atk.projectile_key {
                    proj.card_key = pk.clone();
                }
                // Model C: set shared volley_id so impact resolver can dedup
                // sibling arrows from the same attack volley.
                if volley_id > 0 {
                    if let EntityKind::Projectile(ref mut p) = proj.kind {
                        p.volley_id = volley_id;
                    }
                }
                state.entities.push(proj);
            }


            // attack_push_back: push the ATTACKER backward after firing.
            // HIGH CONFIDENCE: Firecracker (1500u) — visible recoil in real CR,
            // She jumps backward after each shot. Well-documented mechanic.
            // UNVERIFIED SAME BEHAVIOR: ZapMachine/Sparky (750u), SuperArcher (1000u)
            // — same field name in data, plausible self-recoil, but not confirmed
            // from real CR frame-by-frame that the mechanic is identical to
            // Firecracker's. The field may encode a different variant of pushback
            // Direction: opposite to the vector from attacker → target.
            // for these troops. Treat as best-effort until validated.
            if atk.attacker_idx < state.entities.len() {
                let attacker = &state.entities[atk.attacker_idx];
                if let EntityKind::Troop(ref t) = attacker.kind {
                    if t.attack_push_back > 0 && attacker.alive {
                        let push = t.attack_push_back;
                        let dx = atk.attacker_x - atk.target_x;
                        let dy = atk.attacker_y - atk.target_y;
                        let dist = ((dx as i64 * dx as i64 + dy as i64 * dy as i64) as f64).sqrt() as i64;
                        if dist > 0 {
                            let push_x = (dx as i64 * push as i64 / dist) as i32;
                            let push_y = (dy as i64 * push as i64 / dist) as i32;
                            let a = &mut state.entities[atk.attacker_idx];
                            a.x += push_x;
                            a.y += push_y;
                        }
                    }
                }
            }
        } else {
            // Melee damage — may hit a tower
            let effective_dmg = if is_tower_id(atk.target_id) {
                apply_ct_reduction(atk.damage, atk.crown_tower_damage_percent)
            } else {
                atk.damage
            };

            if is_tower_id(atk.target_id) {
                apply_damage_to_tower(state, atk.target_id, effective_dmg);
            } else {
                // Track if target dies for kill notification
                let target_was_alive = state.entities.iter()
                    .find(|e| e.id == atk.target_id)
                    .map_or(false, |e| e.alive);

                if let Some(target) = state.entities.iter_mut().find(|e| e.id == atk.target_id) {
                    apply_damage_to_entity(target, effective_dmg);
                }

                // E-Giant reflect: cached on TroopData at spawn time (reflect_damage,
                // reflect_radius, reflect_buff_key, reflect_buff_ticks). No GameData
                // lookup needed — all fields are level-scaled and ready to use.
                {
                    // Read reflect fields from the target entity's TroopData.
                    // These were set in Entity::new_troop() from CharacterStats.
                    let reflect_info = state.entities.iter()
                        .find(|e| e.id == atk.target_id)
                        .and_then(|e| {
                            if let EntityKind::Troop(ref t) = e.kind {
                                if t.reflect_damage > 0 {
                                    Some((e.x, e.y, t.reflect_damage, t.reflect_radius,
                                          t.reflect_buff_key.clone(), t.reflect_buff_ticks))
                                } else { None }
                            } else { None }
                        });

                    if let Some((egx, egy, reflect_dmg, reflect_radius, buff_key_opt, buff_ticks)) = reflect_info {
                        let reflect_radius_sq = (reflect_radius as i64) * (reflect_radius as i64);

                        // Pre-resolve buff stats once (not per-entity).
                        // Data-driven via from_buff_stats: ALL BuffStats fields are wired.
                        let reflect_buff: Option<crate::entities::ActiveBuff> = buff_key_opt.as_ref()
                            .filter(|_| buff_ticks > 0)
                            .and_then(|bk| {
                                data.buffs.get(bk.as_str()).map(|bs| {
                                    crate::entities::ActiveBuff::from_buff_stats(
                                        bk.clone(), buff_ticks, bs,
                                    )
                                })
                            });

                        // Apply reflect damage + buff to ALL enemies within radius
                        for entity in state.entities.iter_mut() {
                            if !entity.alive || entity.team == atk.attacker_team {
                                continue;
                            }
                            if !entity.is_targetable() {
                                continue;
                            }
                            let dx = (entity.x - egx) as i64;
                            let dy = (entity.y - egy) as i64;
                            if dx * dx + dy * dy > reflect_radius_sq {
                                continue;
                            }
                            apply_damage_to_entity(entity, reflect_dmg);
                            if let Some(ref template) = reflect_buff {
                                entity.add_buff(template.clone());
                            }
                        }
                    }
                }

                // Phase 3: Notify evo damaged on target
                if let Some(target_idx) = state.entities.iter().position(|e| e.id == atk.target_id) {
                    crate::evo_system::notify_evo_damaged(state, data, target_idx);

                    // Check if target just died → notify killer
                    let target_now_dead = !state.entities[target_idx].alive;
                    if target_was_alive && target_now_dead {
                        crate::evo_system::notify_evo_kill(state, data, atk.attacker_idx);

                        // buff_on_kill: apply buff to the killer when it scores a kill.
                        // Data-driven from CharacterStats.buff_on_kill + buff_on_kill_time.
                        if atk.attacker_idx < state.entities.len() && state.entities[atk.attacker_idx].alive {
                            let killer_key = state.entities[atk.attacker_idx].card_key.clone();
                            if let Some(stats) = data.characters.get(&killer_key) {
                                if let Some(ref buff_key) = stats.buff_on_kill {
                                    if !buff_key.is_empty() && stats.buff_on_kill_time > 0 {
                                        let duration = crate::entities::ms_to_ticks(stats.buff_on_kill_time);
                                        if let Some(bs) = data.buffs.get(buff_key.as_str()) {
                                            // Data-driven: from_buff_stats wires ALL BuffStats fields
                                            // (building_damage_percent, death_spawn, invisible, etc.)
                                            state.entities[atk.attacker_idx].add_buff(
                                                crate::entities::ActiveBuff::from_buff_stats(
                                                    buff_key.clone(), duration, bs,
                                                )
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Melee splash — damage nearby enemies too
            // Data-driven: self_as_aoe_center determines splash origin.
            // true (Valkyrie, MK, Dark Prince): splash centers on ATTACKER (360° spin).
            // false: splash centers on TARGET position (directional).
            if atk.splash_radius > 0 {
                let splash_sq = (atk.splash_radius as i64) * (atk.splash_radius as i64);
                let (splash_cx, splash_cy) = if atk.self_as_aoe_center {
                    (atk.attacker_x, atk.attacker_y)
                } else {
                    (atk.target_x, atk.target_y)
                };
                for entity in state.entities.iter_mut() {
                    if !entity.alive
                        || entity.team == atk.attacker_team
                        || entity.id == atk.target_id
                    {
                        continue;
                    }
                    let d = entity.dist_sq_to(splash_cx, splash_cy);
                    if d <= splash_sq {
                        apply_damage_to_entity(entity, effective_dmg);
                    }
                }
            }

            // ─── Fix #6: Melee pushback — push TARGET away on melee hit ───
            // Data-driven from CharacterStats.melee_pushback via TroopData.
            // Separate from attack_push_back (self-knockback on attacker) and
            // dash_push_back (AoE knockback on dash landing).
            // melee_pushback_all: push ALL enemies in splash radius (not just primary).
            if atk.melee_pushback > 0 && !atk.is_ranged {
                let push_dist = atk.melee_pushback;
                let push_all = atk.melee_pushback_all && atk.splash_radius > 0;
                let splash_sq = (atk.splash_radius as i64) * (atk.splash_radius as i64);

                for entity in state.entities.iter_mut() {
                    if !entity.alive || entity.team == atk.attacker_team {
                        continue;
                    }
                    if entity.is_building() || matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                        continue;
                    }
                    if entity_ignores_pushback(entity) {
                        continue;
                    }

                    // Determine if this entity should be pushed:
                    // - Primary target always gets pushed
                    // - If melee_pushback_all, all enemies in splash radius get pushed
                    let is_target = entity.id == atk.target_id;
                    let in_splash = if push_all {
                        let (cx, cy) = if atk.self_as_aoe_center {
                            (atk.attacker_x, atk.attacker_y)
                        } else {
                            (atk.target_x, atk.target_y)
                        };
                        entity.dist_sq_to(cx, cy) <= splash_sq
                    } else {
                        false
                    };

                    if !is_target && !in_splash {
                        continue;
                    }

                    // Push away from attacker position
                    let dx = entity.x - atk.attacker_x;
                    let dy = entity.y - atk.attacker_y;
                    let dist = ((dx as i64 * dx as i64 + dy as i64 * dy as i64) as f64).sqrt() as i64;
                    if dist > 0 {
                        entity.x += (dx as i64 * push_dist as i64 / dist) as i32;
                        entity.y += (dy as i64 * push_dist as i64 / dist) as i32;
                        entity.x = entity.x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
                        entity.y = entity.y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
                    }
                }
            }
        }
    }

    // ─── BUG 3 FIX: buff_on_damage — apply on-hit stun/debuff (EWiz, Zappies) ───
    // For melee attacks: apply the buff directly to the target.
    // For ranged attacks: the buff was already wired onto the projectile's target_buff
    // during projectile spawn above (via ProjectileStats.target_buff). But EWiz has
    // projectile=None (melee with multiple_targets=2), so this melee path is critical.
    // Also applies to any future melee troop with buff_on_damage.
    {
        for atk in &attacks {
            if let Some(ref buff_key) = atk.buff_on_damage_key {
                if buff_key.is_empty() || atk.buff_on_damage_ticks <= 0 {
                    continue;
                }
                // For ranged attacks, the buff is on the projectile — skip here
                // to avoid double-applying. Only apply for melee hits.
                if atk.is_ranged {
                    continue;
                }
                let bs = match data.buffs.get(buff_key.as_str()) {
                    Some(b) => b,
                    None => continue,
                };

                // Apply to the primary target — data-driven via from_buff_stats().
                // All BuffStats fields (building_damage_percent, death_spawn, invisible,
                // stun/freeze detection, etc.) are wired automatically.
                if !is_tower_id(atk.target_id) {
                    if let Some(target) = state.entities.iter_mut().find(|e| e.id == atk.target_id && e.alive) {
                        target.add_buff(crate::entities::ActiveBuff::from_buff_stats(
                            buff_key.clone(), atk.buff_on_damage_ticks, bs,
                        ));
                    }
                }
            }
        }
    }

    // ─── Fix #12: remove_on_attack (data-driven) ───
    // After all attacks resolve, remove buffs flagged with remove_on_attack=true
    // from any entity that attacked this tick. This handles Royal Ghost Invisibility,
    // TripleDamage, and any future buffs with this behavior generically.
    {
        let mut attacker_indices: Vec<usize> = attacks.iter().map(|a| a.attacker_idx).collect();
        attacker_indices.sort_unstable();
        attacker_indices.dedup();
        for idx in attacker_indices {
            if idx < state.entities.len() && state.entities[idx].alive {
                state.entities[idx].buffs.retain(|b| {
                    !(b.remove_on_attack && b.remaining_ticks > 0)
                });
            }
        }
    }

    // Apply dash landing pushbacks (Mega Knight jump knockback).
    // Pushes enemies away from the landing point within dash_radius.
    for (cx, cy, team, radius, push_dist) in &dash_pushbacks {
        let radius_sq = (*radius as i64) * (*radius as i64);
        for entity in state.entities.iter_mut() {
            if !entity.alive || entity.team == *team {
                continue;
            }
            if entity.is_building()
                || matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_))
            {
                continue;
            }
            // ignore_pushback: tanks resist MK jump knockback
            if entity_ignores_pushback(entity) {
                continue;
            }
            let dx = entity.x - cx;
            let dy = entity.y - cy;
            let dist_sq = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
            if dist_sq <= radius_sq && dist_sq > 0 {
                let dist = (dist_sq as f64).sqrt() as i32;
                entity.x += (dx as i64 * *push_dist as i64 / dist as i64) as i32;
                entity.y += (dy as i64 * *push_dist as i64 / dist as i64) as i32;
                entity.x = entity.x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
                entity.y = entity.y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
            }
        }
    }

    // FIX: area_effect_on_hit — BattleHealer creates a heal zone on each attack.
    // After resolving all attacks, check if any attacker has area_effect_on_hit
    // and spawn a transient spell zone at their position.
    {
        let mut area_hit_zones: Vec<(Team, String, i32, i32, usize)> = Vec::new();
        for atk in &attacks {
            if atk.attacker_idx < state.entities.len() {
                let attacker = &state.entities[atk.attacker_idx];
                let card_key = attacker.card_key.clone();
                let level = match &attacker.kind {
                    EntityKind::Troop(t) => t.level,
                    _ => 11,
                };
                if let Some(stats) = data.characters.get(&card_key) {
                    if let Some(ref area_key) = stats.area_effect_on_hit {
                        if !area_key.is_empty() {
                            area_hit_zones.push((
                                attacker.team, area_key.clone(),
                                attacker.x, attacker.y, level,
                            ));
                        }
                    }
                }
            }
        }
        for (team, area_key, x, y, level) in area_hit_zones {
            if let Some(spell) = data.spells.get(&area_key) {
                let radius = spell.radius;
                let duration_ticks = if spell.life_duration > 0 {
                    (spell.life_duration * 20 + 999) / 1000
                } else {
                    1
                };
                let hit_interval = if spell.hit_speed > 0 {
                    (spell.hit_speed * 20 + 999) / 1000
                } else {
                    duration_ticks
                };
                let buff_key = spell.buff.clone();
                let buff_time = if spell.buff_time > 0 {
                    (spell.buff_time * 20 + 999) / 1000
                } else {
                    duration_ticks
                };
                let id = state.alloc_id();
                let zone = Entity::new_spell_zone(
                    id, team, &area_key, x, y, radius, duration_ticks,
                    0, hit_interval, // damage=0 for heal zones
                    spell.aoe_to_air || spell.hits_air,
                    spell.aoe_to_ground || spell.hits_ground,
                    buff_key, buff_time,
                    spell.only_enemies, spell.only_own_troops,
                    spell.crown_tower_damage_percent,
                    0, None, 0, 0, level,
                    false, 0, 0, 0, None,
                    0, // spawn_min_radius (0 = full radius)
                    0, // heal_per_hit
                    0, false, // no pushback
                    0, 0,     // no distance-scaled pushback
                    // Combine SpellStats + BuffStats no_effect_to_crown_towers.
                    // Data-driven: if either the spell or its buff says no crown tower
                    // damage, the zone respects it. Prevents latent bugs where a buff
                    // like PoisonModePoison (no_effect_to_crown_towers=true) is applied
                    // via a spell that doesn't set the flag itself.
                    spell.no_effect_to_crown_towers
                        || spell.buff.as_ref()
                            .and_then(|bk| data.buffs.get(bk))
                            .map(|bs| bs.no_effect_to_crown_towers)
                            .unwrap_or(false),
                    spell.affects_hidden,             // baked from SpellStats
                    1, 1, // level_scale: secondary zone, DOT not primary damage
                );
                state.entities.push(zone);
            }
        }
    }

    // ─── Fix #9: area_effect_on_dash — spawn spell zones at dash landing points ───
    // Same pattern as area_effect_on_hit: look up spell data, create a transient zone.
    // Data-driven from CharacterStats.area_effect_on_dash via TroopData.
    for (team, area_key, x, y, level) in dash_area_effects {
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
            let buff_key = spell.buff.clone();
            let buff_time = if spell.buff_time > 0 {
                (spell.buff_time * 20 + 999) / 1000
            } else {
                duration_ticks
            };
            let id = state.alloc_id();
            let zone = Entity::new_spell_zone(
                id, team, &area_key, x, y, radius, duration_ticks,
                damage, hit_interval,
                spell.aoe_to_air || spell.hits_air,
                spell.aoe_to_ground || spell.hits_ground,
                buff_key, buff_time,
                spell.only_enemies, spell.only_own_troops,
                spell.crown_tower_damage_percent,
                0, None, 0, 0, level,
                false, 0, 0, 0, None,
                0, 0, 0, false,
                0, 0, // no distance-scaled pushback
                // Combine SpellStats + BuffStats no_effect_to_crown_towers.
                spell.no_effect_to_crown_towers
                    || spell.buff.as_ref()
                        .and_then(|bk| data.buffs.get(bk))
                        .map(|bs| bs.no_effect_to_crown_towers)
                        .unwrap_or(false),
                spell.affects_hidden,
                1, 1,
            );
            state.entities.push(zone);
        }
    }

    // Process kamikaze events: self-destruct, deal AoE damage, apply buffs
    for kz in &kamikaze_events {
        // Kill the kamikaze troop
        if kz.attacker_idx < state.entities.len() {
            state.entities[kz.attacker_idx].hp = 0;
            state.entities[kz.attacker_idx].alive = false;
        }

        let splash_sq = (kz.splash_radius as i64) * (kz.splash_radius as i64);

        // Deal AoE damage to enemy entities
        for entity in state.entities.iter_mut() {
            if !entity.alive || entity.team == kz.team {
                continue;
            }
            if matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                continue;
            }
            let d = entity.dist_sq_to(kz.impact_x, kz.impact_y);
            if d <= splash_sq {
                apply_damage_to_entity(entity, kz.damage);
            }
        }

        // Deal AoE damage to enemy towers
        let enemy_towers = enemy_tower_ids(kz.team);
        for tid in &enemy_towers {
            if let Some(tpos) = tower_pos(state, *tid) {
                let ddx = (kz.impact_x - tpos.0) as i64;
                let ddy = (kz.impact_y - tpos.1) as i64;
                if ddx * ddx + ddy * ddy <= splash_sq {
                    let dmg = apply_ct_reduction(kz.damage, kz.crown_tower_damage_percent);
                    apply_damage_to_tower(state, *tid, dmg);
                }
            }
        }

        // Apply kamikaze buff to nearby friendly entities (Heal Spirit heals friendlies)
        // or enemy entities depending on buff type
        if let Some(ref buff_key) = kz.buff_key {
            if !buff_key.is_empty() {
                let bs = data.buffs.get(buff_key);

                // Determine buff targets: heals apply to friendlies, debuffs to enemies.
                // Data-driven: check heal_per_second vs damage_per_second to decide.
                let targets_friendlies = bs.map(|b| {
                    b.heal_per_second > 0 && b.damage_per_second == 0
                }).unwrap_or(false);

                for entity in state.entities.iter_mut() {
                    if !entity.alive || entity.deploy_timer > 0 {
                        continue;
                    }
                    if matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                        continue;
                    }
                    let is_friendly = entity.team == kz.team;
                    if targets_friendlies && !is_friendly {
                        continue;
                    }
                    if !targets_friendlies && is_friendly {
                        continue;
                    }
                    let d = entity.dist_sq_to(kz.impact_x, kz.impact_y);
                    if d <= splash_sq {
                        use crate::entities::ActiveBuff;
                        // Data-driven buff application via from_buff_stats().
                        // All BuffStats fields (stun, freeze, building_damage_percent,
                        // death_spawn, invisible, etc.) are wired automatically — no
                        // hardcoded "contains Zap" heuristics.
                        if let Some(b) = bs {
                            // Refresh if already has this buff, otherwise apply new
                            let existing = entity.buffs.iter_mut()
                                .find(|eb| eb.key == *buff_key && !eb.is_expired());
                            if let Some(eb) = existing {
                                eb.remaining_ticks = kz.buff_time;
                            } else {
                                entity.buffs.push(ActiveBuff::from_buff_stats(
                                    buff_key.clone(), kz.buff_time, b,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // FIX: spawn_area_effect_object — create spell zone on kamikaze impact.
    // HealSpiritProjectile has spawn_area_effect_object=HealSpirit, which spawns
    // a heal zone (only_own_troops, buff=HealSpiritBuff) at the impact point.
    for kz in &kamikaze_events {
        if kz.attacker_idx >= state.entities.len() {
            continue;
        }
        let card_key = state.entities[kz.attacker_idx].card_key.clone();
        let proj_key_opt = match &state.entities[kz.attacker_idx].kind {
            EntityKind::Troop(t) => t.projectile_key.clone(),
            _ => None,
        };
        let level = match &state.entities[kz.attacker_idx].kind {
            EntityKind::Troop(t) => t.level,
            _ => 11,
        };

        if let Some(proj_key) = proj_key_opt {
            let area_key = data.projectiles.get(&proj_key)
                .and_then(|ps| ps.spawn_area_effect_object.clone());
            if let Some(ref area_spell_key) = area_key {
                if let Some(spell) = data.spells.get(area_spell_key) {
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

                    let id = state.alloc_id();
                    let zone = Entity::new_spell_zone(
                        id, kz.team, area_spell_key, kz.impact_x, kz.impact_y,
                        radius, duration_ticks, damage, hit_interval,
                        affects_air, affects_ground,
                        buff_key, buff_time,
                        spell.only_enemies, spell.only_own_troops,
                        spell.crown_tower_damage_percent,
                        0, // attract_strength
                        None, 0, 0, level, // no spawner
                        false, 0, 0, 0, // no hit_biggest
                        None, // no spell_projectile_key
                        0, // spawn_min_radius (0 = full radius)
                        0, // heal_per_hit
                        0, false, // no pushback
                        0, 0,     // no distance-scaled pushback
                        // Combine SpellStats + BuffStats no_effect_to_crown_towers.
                        spell.no_effect_to_crown_towers
                            || spell.buff.as_ref()
                                .and_then(|bk| data.buffs.get(bk))
                                .map(|bs| bs.no_effect_to_crown_towers)
                                .unwrap_or(false),
                        spell.affects_hidden,
                        1, 1, // level_scale: secondary zone
                    );
                    state.entities.push(zone);
                }
            }
        }
    }

    // Phase 3: Fire evo on-attack callbacks for all attackers
    for idx in evo_attack_indices {
        crate::evo_system::notify_evo_attack(state, data, idx);
    }

    // ─── Fix #16: shield_die_pushback — push enemies when shield breaks ───
    // Check entities that had shields before combat and now have shield_hp == 0.
    // Data-driven from CharacterStats.shield_die_pushback via TroopData.
    // Two-pass to satisfy borrow checker: first collect positions, then push.
    {
        let mut shield_pushes: Vec<(i32, i32, Team, i32)> = Vec::new(); // (x, y, team, push_dist)
        for (shield_eid, push_dist) in &shields_before {
            if let Some(e) = state.entities.iter().find(|e| e.id == *shield_eid && e.alive && e.shield_hp <= 0) {
                shield_pushes.push((e.x, e.y, e.team, *push_dist));
            }
        }
        for (cx, cy, team, push_dist) in &shield_pushes {
            let radius_sq = (*push_dist as i64) * (*push_dist as i64);
            for target in state.entities.iter_mut() {
                if !target.alive || target.team == *team {
                    continue;
                }
                if target.is_building()
                    || matches!(target.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_))
                {
                    continue;
                }
                if entity_ignores_pushback(target) {
                    continue;
                }
                let dx = (target.x - cx) as i64;
                let dy = (target.y - cy) as i64;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= radius_sq && dist_sq > 0 {
                    let dist = (dist_sq as f64).sqrt() as i64;
                    target.x += (dx * *push_dist as i64 / dist) as i32;
                    target.y += (dy * *push_dist as i64 / dist) as i32;
                    target.x = target.x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
                    target.y = target.y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
                }
            }
        }
    }
}

// =========================================================================
// Unified idle-buff system: buff_when_not_attacking (data-driven)
// =========================================================================

/// Tick the buff_when_not_attacking mechanic for ALL troops that use it.
///
/// This is a **unified, data-driven** system — it does NOT branch on whether the
/// buff provides invisibility, heal, speed boost, or any other effect. The behavior
/// is entirely determined by the `BuffStats` entry referenced by the troop's
/// `buff_when_not_attacking` field in CharacterStats. `ActiveBuff::from_buff_stats()`
/// wires every field from `BuffStats` into the runtime buff.
///
/// Mechanic:
///   - While the troop is idle (not in attack animation), an idle tick counter increments.
///   - After `invis_idle_threshold` ticks of idling, the buff is applied.
///   - When the troop starts attacking, the buff is removed and the idle counter resets.
///   - If the buff has `invisible=true` (Royal Ghost), the troop becomes untargetable
///     via the normal `Entity::is_invisible()` → `ActiveBuff.invisible` check.
///   - If the buff has `heal_per_second > 0` (BattleHealer), the troop heals via
///     the normal `tick_buffs()` → `heal_per_tick` pathway.
///   - Any future buff_when_not_attacking entry (rage, shield, etc.) will work
///     automatically without code changes.
///
/// Cards using this mechanic:
///   - Royal Ghost: buff=Invisibility (invisible=true) — becomes untargetable when idle
///   - Battle Healer: buff=BattleHealerSelf (heal_per_second=16) — heals while idle
///
/// Previously this was split into two functions (tick_invisibility + tick_buff_when_not_attacking)
/// with hardcoded TroopData.is_invisible toggling for Royal Ghost. That approach meant any
/// new buff_when_not_attacking entry with invisibility=true would NOT work, and any entry
/// with other effects would silently lose fields like building_damage_percent or death_spawn.
pub fn tick_idle_buff(state: &mut GameState, data: &GameData) {
    for entity in state.entities.iter_mut() {
        if !entity.alive || entity.deploy_timer > 0 {
            continue;
        }

        // Extract troop idle-buff state: (buff_key, is_attacking, has_buff_active)
        // Action encoding: 0 = no-op, 1 = remove buff (attacking), 2 = add buff (idle threshold reached)
        let action = match &mut entity.kind {
            EntityKind::Troop(ref mut t) => {
                let bk = match &t.invis_buff_key {
                    Some(k) => k.clone(),
                    None => continue,
                };
                if t.invis_idle_threshold <= 0 {
                    continue;
                }
                let is_attacking = t.attack_phase != crate::entities::AttackPhase::Idle;
                let has_buff = entity.buffs.iter().any(|b| b.key == bk && !b.is_expired());

                if is_attacking {
                    t.invis_idle_ticks = 0;
                    if has_buff {
                        1 // Remove the idle buff
                    } else {
                        continue
                    }
                } else {
                    t.invis_idle_ticks += 1;
                    if !has_buff && t.invis_idle_ticks >= t.invis_idle_threshold {
                        2 // Apply the idle buff
                    } else {
                        continue
                    }
                }
            }
            _ => continue,
        };

        // Re-extract buff key after the mutable borrow of entity.kind is dropped.
        let buff_key = match &entity.kind {
            EntityKind::Troop(t) => t.invis_buff_key.clone(),
            _ => None,
        };

        let bk = match buff_key {
            Some(k) => k,
            None => continue,
        };

        if action == 1 {
            // Troop started attacking → remove the idle buff.
            entity.remove_buff(&bk);
        } else if action == 2 {
            // Idle threshold reached → apply the buff from BuffStats (data-driven).
            // ActiveBuff::from_buff_stats() wires ALL fields: invisible, heal, speed,
            // building_damage_percent, death_spawn, etc. No manual field-by-field copy.
            if let Some(bs) = data.buffs.get(&bk) {
                let buff = crate::entities::ActiveBuff::from_buff_stats(
                    bk,
                    i32::MAX, // Permanent while idle — removed on attack
                    bs,
                );
                entity.add_buff(buff);
            }
            // If the buff has invisible=true (Royal Ghost), clear the target so the troop
            // stops chasing and enters full stealth. This is a targeting optimization, not
            // a hardcoded behavior — we check the buff data, not the buff key.
            let is_invis = entity.buffs.iter().any(|b| b.invisible && !b.is_expired());
            if is_invis {
                entity.target = None;
            }
        }
    }
}

// =========================================================================
// Fisherman hook special attack
// =========================================================================

/// Tick Fisherman hook mechanic:
/// - If target is in [special_min_range, special_range], fire hook projectile
/// - Hook projectile drags target back toward Fisherman on hit
pub fn tick_fisherman_hook(state: &mut GameState, data: &GameData) {
    let snapshots = build_target_snapshots(state);
    let mut hook_projectiles: Vec<Entity> = Vec::new();

    for entity in state.entities.iter_mut() {
        if !entity.alive || entity.deploy_timer > 0 {
            continue;
        }
        let (eid, eteam, ex, ey) = (entity.id, entity.team, entity.x, entity.y);
        let troop = match &mut entity.kind {
            EntityKind::Troop(ref mut t) => t,
            _ => continue,
        };
        // Only troops with projectile_special (Fisherman)
        if troop.projectile_special.is_none() || troop.special_range_sq <= 0 {
            continue;
        }
        // Tick down cooldown
        if troop.special_cooldown > 0 {
            troop.special_cooldown -= 1;
            continue;
        }
        // Don't fire hook while in normal attack animation
        if troop.attack_phase != crate::entities::AttackPhase::Idle {
            continue;
        }
        // Fix #15: Don't fire hook while in post-special recovery
        if troop.special_recovery_timer > 0 {
            troop.special_recovery_timer -= 1;
            continue;
        }

        // Find nearest enemy in [min_range, max_range]
        let mut best_id: Option<EntityId> = None;
        let mut best_dist: i64 = i64::MAX;
        for snap in &snapshots {
            if !snap.targetable || snap.team == eteam || snap.is_invisible {
                continue;
            }
            // Hook targets ground units only (Fisherman attacks_ground=true, attacks_air=false)
            if snap.is_flying {
                continue;
            }
            let dx = (ex - snap.x) as i64;
            let dy = (ey - snap.y) as i64;
            let dsq = dx * dx + dy * dy;
            if dsq >= troop.special_min_range_sq && dsq <= troop.special_range_sq && dsq < best_dist {
                best_dist = dsq;
                best_id = Some(snap.id);
            }
        }

        if let Some(hook_target_id) = best_id {
            // Find target position
            if let Some(tsnap) = snapshots.iter().find(|s| s.id == hook_target_id) {
                // Look up projectile stats for drag speed
                let proj_key = troop.projectile_special.clone().unwrap();
                let drag_speed = data.projectiles.get(&proj_key)
                    .map(|ps| {
                        let raw = ps.drag_back_speed;
                        if raw > 0 { crate::entities::speed_to_units_per_tick(raw) } else { 50 }
                    })
                    .unwrap_or(50);
                let proj_speed = data.projectiles.get(&proj_key)
                    .map(|ps| crate::entities::speed_to_units_per_tick(ps.speed))
                    .unwrap_or(60);
                let target_buff = data.projectiles.get(&proj_key)
                    .and_then(|ps| ps.target_buff.clone());
                let buff_time = data.projectiles.get(&proj_key)
                    .map(|ps| crate::entities::ms_to_ticks(ps.buff_time))
                    .unwrap_or(0);

                let id = EntityId(0); // Will be replaced with alloc_id below
                let mut proj = Entity::new_projectile(
                    id, eteam, eid,
                    ex, ey,
                    hook_target_id, tsnap.x, tsnap.y,
                    proj_speed,
                    0, // Hook doesn't deal impact damage
                    0, // No splash
                    true, // Homing
                    0, // No crown tower damage reduction
                    true, true, // aoe_to_air, aoe_to_ground
                );
                if let EntityKind::Projectile(ref mut pd) = proj.kind {
                    pd.drag_back = true;
                    // Data-driven: drag_back_as_attractor determines pull direction.
                    // true = pull target to source (Fisherman pulls enemy to himself).
                    // false = pull source to target (self-pull only).
                    pd.drag_back_as_attractor = data.projectiles.get(&proj_key)
                        .map(|ps| ps.drag_back_as_attractor)
                        .unwrap_or(true);
                    pd.drag_back_speed = drag_speed;
                    pd.drag_source_x = ex;
                    pd.drag_source_y = ey;
                    pd.target_buff = target_buff;
                    pd.target_buff_time = buff_time;
                    // Data-driven self-pull speed: when the projectile's attractor
                    // mode doesn't apply (e.g., hooking a building), the SOURCE is
                    // pulled toward the TARGET at this speed instead.
                    pd.drag_self_speed = data.projectiles.get(&proj_key)
                        .map(|ps| {
                            let raw = ps.drag_self_speed;
                            if raw > 0 { crate::entities::speed_to_units_per_tick(raw) } else { 0 }
                        })
                        .unwrap_or(0);
                    // Data-driven drag margin from ProjectileStats.
                    pd.drag_margin = data.projectiles.get(&proj_key)
                        .map(|ps| ps.drag_margin)
                        .unwrap_or(200); // Fallback to 200 if not in data
                }
                proj.card_key = proj_key;
                hook_projectiles.push(proj);

                // Set cooldown: base hook cooldown + special_charge_time (Fix #15).
                // special_charge_ticks is the charge-up before the special fires.
                // Data-driven from CharacterStats.special_charge_time.
                troop.special_cooldown = troop.special_load_ticks.max(troop.hit_speed)
                    + troop.special_charge_ticks;
                // Fix #15: Post-special recovery pause — troop is immobilized.
                // Data-driven from CharacterStats.stop_time_after_special_attack.
                if troop.stop_after_special_ticks > 0 {
                    troop.special_recovery_timer = troop.stop_after_special_ticks;
                }
            }
        }
    }

    // Allocate IDs and push hook projectiles
    for mut proj in hook_projectiles {
        let id = state.alloc_id();
        proj.id = id;
        state.entities.push(proj);
    }
}

// =========================================================================
// Projectile movement & impact
// =========================================================================

pub fn tick_projectiles(state: &mut GameState, data: &GameData) {
    // Snapshot for homing updates — includes tower positions
    let snapshots = build_target_snapshots(state);

    let mut impacts: Vec<ProjectileImpact> = Vec::new();

    // ── Rolling projectile per-tick damage ──
    // Collect rolling hits separately: (entity_index, damage, pushback, push_dir_x, push_dir_y)
    let mut rolling_entity_hits: Vec<(usize, i32, i32, i32, i32)> = Vec::new();
    let mut rolling_tower_hits: Vec<(EntityId, i32)> = Vec::new();

    // Snapshot entity positions for rolling hit detection (avoids borrow conflict)
    struct RollingTarget {
        idx: usize,
        id_raw: u32,
        team: Team,
        x: i32,
        y: i32,
        is_flying: bool,
        alive: bool,
    }
    let rolling_targets: Vec<RollingTarget> = state.entities.iter().enumerate()
        .filter(|(_, e)| e.alive && (e.is_troop() || e.is_building()))
        .map(|(idx, e)| RollingTarget {
            idx, id_raw: e.id.0, team: e.team, x: e.x, y: e.y,
            is_flying: e.is_flying(), alive: e.alive,
        })
        .collect();

    // Snapshot tower positions for rolling hit detection (avoids borrow conflict)
    struct TowerSnapshot {
        id: EntityId,
        team: Team,
        x: i32,
        y: i32,
    }
    let tower_snaps: Vec<TowerSnapshot> = [
        (P1_PRINCESS_LEFT_ID, Team::Player1, &state.player1.princess_left),
        (P1_PRINCESS_RIGHT_ID, Team::Player1, &state.player1.princess_right),
        (P1_KING_TOWER_ID, Team::Player1, &state.player1.king),
        (P2_PRINCESS_LEFT_ID, Team::Player2, &state.player2.princess_left),
        (P2_PRINCESS_RIGHT_ID, Team::Player2, &state.player2.princess_right),
        (P2_KING_TOWER_ID, Team::Player2, &state.player2.king),
    ].iter()
        .filter(|(_, _, t)| t.alive)
        .map(|(id, team, t)| TowerSnapshot { id: *id, team: *team, x: t.pos.0, y: t.pos.1 })
        .collect();

    for entity in state.entities.iter_mut() {
        if !entity.alive {
            continue;
        }
        let proj = match &mut entity.kind {
            EntityKind::Projectile(ref mut p) => p,
            _ => continue,
        };

        if proj.is_rolling {
            // ── Rolling projectile (Log, Barb Barrel) ──
            // Moves forward each tick. Damages enemies within its rectangular
            // hitbox as it passes through them. Each enemy is hit only once.

            // Move forward along the line toward target
            let dx = proj.target_x - entity.x;
            let dy = proj.target_y - entity.y;
            let dist_sq = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
            let dist = (dist_sq as f64).sqrt() as i32;

            let (move_x, move_y) = if dist > 0 {
                let mx = (dx as i64 * proj.speed as i64 / dist as i64) as i32;
                let my = (dy as i64 * proj.speed as i64 / dist as i64) as i32;
                (mx, my)
            } else {
                (0, 0)
            };

            entity.x += move_x;
            entity.y += move_y;
            proj.distance_traveled += proj.speed;

            // ── River boundary check for rolling projectiles ──
            // In real CR, ground-rolling projectiles (The Log, Barbarian Barrel)
            // stop at the river edge — they cannot cross water. The river zone
            // spans Y = RIVER_Y_MIN (-1200) to RIVER_Y_MAX (+1200).
            // P1 rolls in +Y direction → stops at RIVER_Y_MIN (near edge).
            // P2 rolls in -Y direction → stops at RIVER_Y_MAX (near edge).
            if entity.y >= RIVER_Y_MIN && entity.y <= RIVER_Y_MAX {
                // Clamp to the approaching river edge
                entity.y = if entity.team == Team::Player1 { RIVER_Y_MIN } else { RIVER_Y_MAX };
                entity.alive = false;
                // Damage hitbox checks below still run for this final position,
                // so the Log can hit anything at the river edge before dying.
            }

            // Check hitbox against all enemy entities.
            // The hitbox is a rectangle centered on the projectile:
            //   X: ±rolling_radius_x (width, perpendicular to travel)
            //   Y: ±rolling_radius_y (depth, along travel direction)
            // For simplicity with axis-aligned checks, we use the radii directly.
            let rx = proj.rolling_radius_x as i64;
            let ry = if proj.rolling_radius_y > 0 { proj.rolling_radius_y as i64 } else { rx };
            let proj_team = entity.team;
            let proj_x = entity.x;
            let proj_y = entity.y;
            let proj_damage = proj.impact_damage;
            let proj_ct_pct = proj.crown_tower_damage_percent;
            let proj_pushback = proj.pushback;
            // Rolling projectile travel direction for pushback.
            // Use team forward direction rather than remaining-distance vector,
            // because near the end of travel, remaining dist→0 which would
            // zero out the pushback direction entirely.
            let roll_dir_x = 0i32;
            let roll_dir_y = if entity.team == Team::Player1 { 1000 } else { -1000 };

            for target in &rolling_targets {
                if !target.alive || target.team == proj_team {
                    continue;
                }
                // Air/ground filter
                if target.is_flying && !proj.aoe_to_air {
                    continue;
                }
                if !target.is_flying && !proj.aoe_to_ground {
                    continue;
                }
                // Already hit?
                if proj.hit_entities.contains(&target.id_raw) {
                    continue;
                }
                // Rectangle hitbox check
                let ex = (target.x - proj_x).abs() as i64;
                let ey = (target.y - proj_y).abs() as i64;
                if ex <= rx && ey <= ry {
                    proj.hit_entities.push(target.id_raw);
                    rolling_entity_hits.push((target.idx, proj_damage, proj_pushback, roll_dir_x, roll_dir_y));
                }
            }

            // Check hitbox against enemy towers (using pre-snapshotted positions)
            if proj.aoe_to_ground {
                for ts in &tower_snaps {
                    if ts.team == proj_team {
                        continue; // Skip own towers
                    }
                    if proj.hit_towers.contains(&ts.id.0) {
                        continue;
                    }
                    let ex = (ts.x - proj_x).abs() as i64;
                    let ey = (ts.y - proj_y).abs() as i64;
                    if ex <= rx && ey <= ry {
                        proj.hit_towers.push(ts.id.0);
                        let dmg = apply_ct_reduction(proj_damage, proj_ct_pct);
                        rolling_tower_hits.push((ts.id, dmg));
                    }
                }
            }

            // Die when we've traveled the full range or passed the target
            if proj.distance_traveled >= proj.rolling_range || dist <= proj.speed {
                entity.alive = false;
            }
        } else if proj.is_boomerang {
            // ── Boomerang projectile (Executioner axe) ──
            // Flies to target point dealing AoE damage to enemies in its radius
            // along the path. When it reaches the target, it reverses direction
            // back to the source, clearing hit_entities so enemies can be hit
            // again on the return trip. Dies when it returns to source.

            // Determine current destination
            let (dest_x, dest_y) = if proj.boomerang_returning {
                (proj.boomerang_source_x, proj.boomerang_source_y)
            } else {
                (proj.target_x, proj.target_y)
            };

            let dx = dest_x - entity.x;
            let dy = dest_y - entity.y;
            let dist_sq = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
            let dist = (dist_sq as f64).sqrt() as i32;

            // Move toward destination
            if dist > proj.speed && dist > 0 {
                entity.x += (dx as i64 * proj.speed as i64 / dist as i64) as i32;
                entity.y += (dy as i64 * proj.speed as i64 / dist as i64) as i32;
            } else {
                entity.x = dest_x;
                entity.y = dest_y;

                if proj.boomerang_returning {
                    // Returned to source — die
                    entity.alive = false;
                } else {
                    // Reached target — reverse direction, clear hit lists
                    proj.boomerang_returning = true;
                    proj.hit_entities.clear();
                    proj.hit_towers.clear();
                }
            }

            // AoE damage to enemies within boomerang_radius (both outbound and return)
            if entity.alive || !proj.boomerang_returning {
                let br = proj.boomerang_radius as i64;
                let br_sq = br * br;
                let proj_team = entity.team;
                let proj_x = entity.x;
                let proj_y = entity.y;
                let proj_damage = proj.impact_damage;
                let proj_ct_pct = proj.crown_tower_damage_percent;

                for target in &rolling_targets {
                    if !target.alive || target.team == proj_team {
                        continue;
                    }
                    if target.is_flying && !proj.aoe_to_air {
                        continue;
                    }
                    if !target.is_flying && !proj.aoe_to_ground {
                        continue;
                    }
                    if proj.hit_entities.contains(&target.id_raw) {
                        continue;
                    }
                    let edx = (target.x - proj_x) as i64;
                    let edy = (target.y - proj_y) as i64;
                    if edx * edx + edy * edy <= br_sq {
                        proj.hit_entities.push(target.id_raw);
                        rolling_entity_hits.push((target.idx, proj_damage, 0, 0, 0));
                    }
                }

                // Tower hits
                if proj.aoe_to_ground {
                    for ts in &tower_snaps {
                        if ts.team == proj_team { continue; }
                        if proj.hit_towers.contains(&ts.id.0) { continue; }
                        let edx = (ts.x - proj_x) as i64;
                        let edy = (ts.y - proj_y) as i64;
                        if edx * edx + edy * edy <= br_sq {
                            proj.hit_towers.push(ts.id.0);
                            let dmg = apply_ct_reduction(proj_damage, proj_ct_pct);
                            rolling_tower_hits.push((ts.id, dmg));
                        }
                    }
                }
            }
        } else {
            // ── Standard projectile (homing or point-target) ──
            // Update homing target position — but NOT for gravity-arc projectiles.
            // Gravity-arc projectiles (Fireball, Rocket, Arrows, Goblin Barrel, etc.)
            // fly to the fixed (target_x, target_y) computed at launch time. In real CR,
            // these land at the cast location — moving troops can dodge them.
            // Data-driven: is_gravity_arc is set from ProjectileStats.gravity > 0 at spawn.
            if proj.homing && !proj.is_gravity_arc {
                if let Some(snap) = snapshots.iter().find(|s| s.id == proj.target_id) {
                    proj.target_x = snap.x;
                    proj.target_y = snap.y;
                }
            }

            let dx = proj.target_x - entity.x;
            let dy = proj.target_y - entity.y;
            let dist_sq = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
            let dist = (dist_sq as f64).sqrt() as i32;

            if dist <= proj.speed || dist == 0 {
                entity.x = proj.target_x;
                entity.y = proj.target_y;
                entity.alive = false;

                impacts.push(ProjectileImpact {
                    target_id: proj.target_id,
                    impact_x: proj.target_x,
                    impact_y: proj.target_y,
                    damage: proj.impact_damage,
                    team: entity.team,
                    splash_radius: proj.splash_radius,
                    crown_tower_damage_percent: proj.crown_tower_damage_percent,
                    aoe_to_air: proj.aoe_to_air,
                    aoe_to_ground: proj.aoe_to_ground,
                    volley_id: proj.volley_id,
                    pushback: proj.pushback,
                    pushback_all: proj.pushback_all,
                    min_pushback: proj.min_pushback,
                    max_pushback: proj.max_pushback,
                    target_buff: proj.target_buff.clone(),
                    target_buff_time: proj.target_buff_time,
                    apply_buff_before_damage: proj.apply_buff_before_damage,
                    drag_back: proj.drag_back,
                    drag_back_as_attractor: proj.drag_back_as_attractor,
                    drag_back_speed: proj.drag_back_speed,
                    drag_source_x: proj.drag_source_x,
                    drag_source_y: proj.drag_source_y,
                    drag_self_speed: proj.drag_self_speed,
                    drag_margin: proj.drag_margin,
                    source_id: proj.source_id,
                    chained_hit_radius: proj.chained_hit_radius,
                    chained_hit_count: proj.chained_hit_count,
                });
            } else {
                entity.x += (dx as i64 * proj.speed as i64 / dist as i64) as i32;
                entity.y += (dy as i64 * proj.speed as i64 / dist as i64) as i32;
            }
        }
    }

    // Apply rolling projectile hits (damage + pushback)
    for (idx, dmg, pushback, dir_x, dir_y) in rolling_entity_hits {
        if idx < state.entities.len() && state.entities[idx].alive {
            apply_damage_to_entity(&mut state.entities[idx], dmg);
            // Apply pushback in the rolling direction
            if pushback > 0 && !state.entities[idx].is_building()
                && !entity_ignores_pushback(&state.entities[idx])
            {
                let push_x = (dir_x as i64 * pushback as i64 / 1000) as i32;
                let push_y = (dir_y as i64 * pushback as i64 / 1000) as i32;
                state.entities[idx].x += push_x;
                state.entities[idx].y += push_y;
                state.entities[idx].x = state.entities[idx].x.clamp(-crate::game_state::ARENA_HALF_W, crate::game_state::ARENA_HALF_W);
                state.entities[idx].y = state.entities[idx].y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);

                // FIX C: Brief knockback stun (0.3s = 6 ticks).
                // In real CR, knockback always includes a movement interrupt —
                // the troop freezes briefly after being pushed. Without this,
                // the troop immediately walks back, negating the displacement.
                state.entities[idx].add_buff(crate::entities::ActiveBuff {
                    key: "knockback_stun".to_string(),
                    remaining_ticks: 6,
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
    for (tid, dmg) in rolling_tower_hits {
        apply_damage_to_tower(state, tid, dmg);
    }

    // Apply standard projectile impacts
    //
    // ── Model C volley dedup ──────────────────────────────────────────────
    // Multi-arrow troops (Princess) fire several projectiles per attack, each
    // carrying FULL damage with independent splash. In real CR, a target can
    // only be damaged by ONE projectile per attack volley — the multiple arrows
    // cover a wider area, they don't stack damage on the same target.
    //
    // We track (volley_id, entity_id) pairs: when a splash impact with volley_id > 0
    // hits an entity, we record it. Subsequent sibling projectiles from the same
    // volley skip that entity. volley_id == 0 means no dedup (single projectiles,
    // scatter attacks like Hunter where stacking IS correct, spell projectiles).
    //
    // This is separate from the rolling projectile hit_entities dedup (which prevents
    // a SINGLE rolling projectile from damaging the same entity twice as it passes).
    let mut volley_hits_entities: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut volley_hits_towers: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();

    for impact in &impacts {
        // ── Pre-damage buff: apply debuff BEFORE damage (Mother Witch VoodooCurse) ──
        // This ensures the curse is on the target when the damage kills it,
        // so the death_spawn (VoodooHog) fires correctly.
        if impact.apply_buff_before_damage {
            apply_impact_target_buff(state, data, impact);
        }

        if impact.splash_radius > 0 {
            let splash_sq = (impact.splash_radius as i64) * (impact.splash_radius as i64);

            // Splash hits entities — with air/ground filtering and volley dedup
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == impact.team {
                    continue;
                }
                if matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                    continue;
                }
                if entity.is_flying() && !impact.aoe_to_air {
                    continue;
                }
                if !entity.is_flying() && !impact.aoe_to_ground {
                    continue;
                }
                let d = entity.dist_sq_to(impact.impact_x, impact.impact_y);
                if d <= splash_sq {
                    // Model C: skip if this entity was already hit by a sibling
                    // projectile from the same attack volley.
                    if impact.volley_id > 0 {
                        if !volley_hits_entities.insert((impact.volley_id, entity.id.0)) {
                            continue; // Already hit by a sibling — skip
                        }
                    }
                    apply_damage_to_entity(entity, impact.damage);
                }
            }

            // Splash hits towers (towers are always ground) — with volley dedup
            if impact.aoe_to_ground {
                let enemy_towers = enemy_tower_ids(impact.team);
                for tid in &enemy_towers {
                    if let Some(tpos) = tower_pos(state, *tid) {
                        let dx = (impact.impact_x - tpos.0) as i64;
                        let dy = (impact.impact_y - tpos.1) as i64;
                        if dx * dx + dy * dy <= splash_sq {
                            // Model C: skip if this tower was already hit by a sibling
                            if impact.volley_id > 0 {
                                if !volley_hits_towers.insert((impact.volley_id, tid.0)) {
                                    continue; // Already hit by a sibling — skip
                                }
                            }
                            let dmg =
                                apply_ct_reduction(impact.damage, impact.crown_tower_damage_percent);
                            apply_damage_to_tower(state, *tid, dmg);
                        }
                    }
                }
            }
        } else {
            // Single target
            let effective_dmg = if is_tower_id(impact.target_id) {
                apply_ct_reduction(impact.damage, impact.crown_tower_damage_percent)
            } else {
                impact.damage
            };

            if is_tower_id(impact.target_id) {
                apply_damage_to_tower(state, impact.target_id, effective_dmg);
            } else if let Some(target) = state
                .entities
                .iter_mut()
                .find(|e| e.id == impact.target_id && e.alive)
            {
                apply_damage_to_entity(target, effective_dmg);
            }
        }

        // ── Pushback: displace affected entities away from impact point ──
        if impact.pushback > 0 {
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == impact.team {
                    continue;
                }
                if entity.is_building() || matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                    continue;
                }
                // ignore_pushback: tanks (Golem, Giant, etc.) resist knockback
                if entity_ignores_pushback(entity) {
                    continue;
                }
                // Check if entity was in the splash area (or is the direct target)
                // Spell projectiles (Fireball, Snowball) target a position, not an entity,
                // so target_id is EntityId(0). In that case, always use area check.
                let is_spell_projectile = impact.target_id == EntityId(0);
                let in_area = if impact.splash_radius > 0 && (impact.pushback_all || is_spell_projectile) {
                    let splash_sq = (impact.splash_radius as i64) * (impact.splash_radius as i64);
                    entity.dist_sq_to(impact.impact_x, impact.impact_y) <= splash_sq
                } else {
                    entity.id == impact.target_id
                };
                if !in_area { continue; }

                // Push away from impact center.
                // Distance-based scaling: if min_pushback and max_pushback are set,
                // interpolate linearly: max_pushback at center → min_pushback at edge.
                // This matches real CR where Fireball/Snowball push harder near center.
                let dx = entity.x - impact.impact_x;
                let dy = entity.y - impact.impact_y;
                let dist = ((dx as i64 * dx as i64 + dy as i64 * dy as i64) as f64).sqrt() as i32;

                let effective_pushback = if impact.min_pushback > 0 && impact.max_pushback > 0 && impact.splash_radius > 0 && dist > 0 {
                    // Linear interpolation: center(0) → max_pushback, edge(radius) → min_pushback
                    let ratio = (dist as i64).min(impact.splash_radius as i64) * 1000 / impact.splash_radius as i64;
                    let pb = impact.max_pushback as i64
                        - (ratio * (impact.max_pushback as i64 - impact.min_pushback as i64) / 1000);
                    pb.max(impact.min_pushback as i64) as i32
                } else {
                    impact.pushback
                };

                if dist > 0 {
                    let push_x = (dx as i64 * effective_pushback as i64 / dist as i64) as i32;
                    let push_y = (dy as i64 * effective_pushback as i64 / dist as i64) as i32;
                    entity.x += push_x;
                    entity.y += push_y;
                    entity.x = entity.x.clamp(-crate::game_state::ARENA_HALF_W, crate::game_state::ARENA_HALF_W);
                    entity.y = entity.y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);
                } else {
                    // Entity is at impact center — push in the team's forward direction
                    let push_dir = if impact.team == Team::Player1 { 1 } else { -1 };
                    entity.y += push_dir * effective_pushback;
                    entity.y = entity.y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);
                }

                // Knockback micro-stun: in real CR, ALL knockback includes a brief
                // movement interrupt — the troop freezes and its attack animation
                // resets. Without this, the troop immediately walks back, negating
                // the displacement. 0.3s = 6 ticks, matching the rolling projectile
                // knockback_stun already applied by Log/Barb Barrel.
                // Only apply to troops (buildings are immovable, already filtered above).
                if entity.alive && !entity.is_building() {
                    entity.add_buff(crate::entities::ActiveBuff {
                        key: "knockback_stun".to_string(),
                        remaining_ticks: 6,
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

        // ── Post-damage buff: normal path (Snowball slow, Fisherman slow, etc.) ──
        if !impact.apply_buff_before_damage {
            apply_impact_target_buff(state, data, impact);
        }

        // ── Drag: data-driven from drag_back_as_attractor ──
        // drag_back_as_attractor=true: pull TARGET toward SOURCE (Fisherman pulls enemy).
        //   Exception: buildings/towers can't be moved — fall back to self-pull if drag_self_speed > 0.
        // drag_back_as_attractor=false: pull SOURCE toward TARGET (self-pull only).
        if impact.drag_back && impact.drag_back_speed > 0 {
            // Determine if the target is immovable (building or tower)
            let target_is_immovable = is_tower_id(impact.target_id)
                || state.entities.iter()
                    .find(|e| e.id == impact.target_id && e.alive)
                    .map_or(false, |e| e.is_building());

            // Decide drag mode based on data-driven flag:
            // - attractor=true AND target is movable → pull target to source
            // - attractor=true AND target is immovable → fall back to self-pull if available
            // - attractor=false → always self-pull (pull source to target)
            let use_attractor = impact.drag_back_as_attractor && !target_is_immovable;
            let use_self_pull = (!impact.drag_back_as_attractor || target_is_immovable)
                && impact.drag_self_speed > 0;

            if use_attractor {
                // Attractor: pull enemy toward the source position
                if let Some(target) = state.entities.iter_mut().find(|e| e.id == impact.target_id && e.alive) {
                    let dx = impact.drag_source_x - target.x;
                    let dy = impact.drag_source_y - target.y;
                    let dist = ((dx as i64 * dx as i64 + dy as i64 * dy as i64) as f64).sqrt() as i32;
                    if dist > 0 {
                        let drag_margin = if impact.drag_margin > 0 { impact.drag_margin } else { 200 };
                        let move_dist = (dist - drag_margin).max(0);
                        let move_x = (dx as i64 * move_dist as i64 / dist as i64) as i32;
                        let move_y = (dy as i64 * move_dist as i64 / dist as i64) as i32;
                        target.x += move_x;
                        target.y += move_y;
                        target.x = target.x.clamp(-crate::game_state::ARENA_HALF_W, crate::game_state::ARENA_HALF_W);
                        target.y = target.y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);
                    }
                }
            } else if use_self_pull {
                // Self-pull: move the source entity toward the target
                let target_pos = if is_tower_id(impact.target_id) {
                    tower_pos(state, impact.target_id)
                } else {
                    state.entities.iter()
                        .find(|e| e.id == impact.target_id && e.alive)
                        .map(|e| (e.x, e.y))
                };
                if let Some((tx, ty)) = target_pos {
                    if let Some(source) = state.entities.iter_mut()
                        .find(|e| e.id == impact.source_id && e.alive)
                    {
                        let dx = tx - source.x;
                        let dy = ty - source.y;
                        let dist = ((dx as i64 * dx as i64 + dy as i64 * dy as i64) as f64).sqrt() as i32;
                        if dist > 0 {
                            let drag_margin = if impact.drag_margin > 0 { impact.drag_margin } else { 400 };
                            let move_dist = (dist - drag_margin).max(0);
                            source.x += (dx as i64 * move_dist as i64 / dist as i64) as i32;
                            source.y += (dy as i64 * move_dist as i64 / dist as i64) as i32;
                            source.x = source.x.clamp(-crate::game_state::ARENA_HALF_W, crate::game_state::ARENA_HALF_W);
                            source.y = source.y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);
                        }
                        // Retarget source to the entity it just pulled to
                        source.target = Some(impact.target_id);
                    }
                }
            }
        }
    }

    // Chain lightning bounce (Electro Dragon, Electro Spirit).
    // Chain fields are stored directly on ProjectileImpact (from ProjectileData).
    // No heuristic matching needed — just check impact.chained_hit_count.
    {
        for impact in &impacts {
            if impact.chained_hit_count <= 1 || impact.chained_hit_radius <= 0 {
                continue;
            }

            let mut bounce_x = impact.impact_x;
            let mut bounce_y = impact.impact_y;
            let mut hit_ids: Vec<EntityId> = vec![impact.target_id];

            for _ in 0..(impact.chained_hit_count - 1) {
                let chain_radius_sq = (impact.chained_hit_radius as i64) * (impact.chained_hit_radius as i64);
                let mut best_idx: Option<usize> = None;
                let mut best_dist_sq: i64 = i64::MAX;

                for (idx, entity) in state.entities.iter().enumerate() {
                    if !entity.alive || entity.team == impact.team {
                        continue;
                    }
                    if matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                        continue;
                    }
                    // Chain bounces hit both air and ground targets.
                    // Unlike splash AoE (which uses aoe_to_air/aoe_to_ground),
                    // chain lightning is a separate targeting mechanic that can
                    // bounce to any valid enemy. In real CR, E-Dragon chains to
                    // ground troops, air troops, and buildings equally.
                    if hit_ids.contains(&entity.id) {
                        continue;
                    }
                    let dx = (entity.x - bounce_x) as i64;
                    let dy = (entity.y - bounce_y) as i64;
                    let d = dx * dx + dy * dy;
                    if d <= chain_radius_sq && d < best_dist_sq {
                        best_dist_sq = d;
                        best_idx = Some(idx);
                    }
                }

                if let Some(idx) = best_idx {
                    let target_id = state.entities[idx].id;
                    bounce_x = state.entities[idx].x;
                    bounce_y = state.entities[idx].y;
                    hit_ids.push(target_id);

                    // Deal chain damage
                    apply_damage_to_entity(&mut state.entities[idx], impact.damage);

                    // Apply target_buff (ZapFreeze stun) to chain target — data-driven
                    if let Some(ref buff_key) = impact.target_buff {
                        if !buff_key.is_empty() && impact.target_buff_time > 0 {
                            if let Some(bs) = data.buffs.get(buff_key) {
                                state.entities[idx].add_buff(
                                    crate::entities::ActiveBuff::from_buff_stats(
                                        buff_key.clone(), impact.target_buff_time, bs,
                                    )
                                );
                            }
                        }
                    }
                } else {
                    break; // No more targets in range
                }
            }
        }
    }
}

struct ProjectileImpact {
    target_id: EntityId,
    impact_x: i32,
    impact_y: i32,
    damage: i32,
    team: Team,
    splash_radius: i32,
    crown_tower_damage_percent: i32,
    aoe_to_air: bool,
    aoe_to_ground: bool,
    /// Volley dedup ID (Model C). When > 0, splash processing skips entities
    /// already damaged by a sibling projectile from the same attack volley.
    /// This ensures multi-arrow troops like Princess deal full per-arrow damage
    /// but each target is only hit once per volley — matching real CR behavior.
    volley_id: u32,
    pushback: i32,
    pushback_all: bool,
    /// Distance-based pushback scaling. When both > 0, pushback interpolates
    /// linearly from max_pushback (at center) to min_pushback (at radius edge).
    min_pushback: i32,
    max_pushback: i32,
    target_buff: Option<String>,
    target_buff_time: i32,
    apply_buff_before_damage: bool,
    drag_back: bool,
    /// Data-driven from ProjectileStats.drag_back_as_attractor.
    /// true = pull target toward source (attractor mode).
    /// false = pull source toward target (self-pull mode).
    drag_back_as_attractor: bool,
    drag_back_speed: i32,
    drag_source_x: i32,
    drag_source_y: i32,
    /// Speed for self-pull (Fisherman pulls himself to buildings). 0 = no self-pull.
    drag_self_speed: i32,
    /// Margin distance to stop dragging. Data-driven from ProjectileStats.drag_margin.
    drag_margin: i32,
    /// Source entity ID (needed for Fisherman self-pull to move the source entity).
    source_id: EntityId,
    // Chain lightning (Electro Dragon, Electro Spirit)
    chained_hit_radius: i32,
    chained_hit_count: i32,
}

/// Apply target buff from a projectile impact to affected enemies.
/// Extracted as a helper so it can be called before or after damage
/// depending on `apply_buff_before_damage` (Mother Witch VoodooCurse).
fn apply_impact_target_buff(
    state: &mut GameState,
    data: &GameData,
    impact: &ProjectileImpact,
) {
    if let Some(ref buff_key) = impact.target_buff {
        if !buff_key.is_empty() && impact.target_buff_time > 0 {
            if let Some(bs) = data.buffs.get(buff_key) {
                let buff_duration = impact.target_buff_time;
                // Apply to entities in splash area or direct target
                for entity in state.entities.iter_mut() {
                    if !entity.alive || entity.team == impact.team {
                        continue;
                    }
                    if matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                        continue;
                    }
                    // FIX 4: ignore_buildings — Mother Witch VoodooCurse has
                    // ignore_buildings=true, meaning the curse should not be
                    // applied to building-type entities (only troops).
                    if bs.ignore_buildings && entity.is_building() {
                        continue;
                    }
                    let in_area = if impact.splash_radius > 0 {
                        let splash_sq = (impact.splash_radius as i64) * (impact.splash_radius as i64);
                        entity.dist_sq_to(impact.impact_x, impact.impact_y) <= splash_sq
                    } else {
                        entity.id == impact.target_id
                    };
                    if !in_area { continue; }

                    // Data-driven via from_buff_stats(): ALL BuffStats fields are wired
                    // (death_spawn, death_spawn_is_enemy, building_damage_percent, etc.).
                    // Previously this was the ONLY site that manually wired death_spawn;
                    // now all buff application paths use the same factory.
                    entity.add_buff(crate::entities::ActiveBuff::from_buff_stats(
                        buff_key.clone(), buff_duration, bs,
                    ));
                }
            }
        }
    }
}

// =========================================================================
// Tower buff system — Rage on towers (data-driven)
// =========================================================================

/// Recompute tower hitspeed multipliers from overlapping friendly spell zones.
///
/// In real CR 2024+, Rage affects friendly towers — they attack 35% faster
/// (hitspeed_multiplier=135) while inside a Rage zone. This applies to
/// princess towers and king tower equally.
///
/// Implementation: Each tick, scan all alive spell zones with `only_own=true`
/// (friendly buff zones like Rage). For each zone, check if any friendly tower
/// is within its radius. If the zone's buff has a hitspeed_multiplier > 100,
/// apply it to the tower's `rage_hitspeed` field.
///
/// Data-driven: the hitspeed boost comes from BuffStats.hit_speed_multiplier
/// for whatever buff the spell zone references. No hardcoded "Rage" check —
/// any future friendly spell zone with a hitspeed buff will automatically
/// affect towers.
///
/// Called each tick from engine.rs before tick_towers.
pub fn tick_tower_buffs(state: &mut GameState, data: &GameData) {
    // Reset all towers to base speed (100 = normal). Buff is recomputed fresh
    // each tick from active spell zones — no persistent state to track.
    for player in [&mut state.player1, &mut state.player2] {
        player.princess_left.rage_hitspeed = 100;
        player.princess_right.rage_hitspeed = 100;
        player.king.rage_hitspeed = 100;
    }

    // Collect active friendly buff zones that could affect towers.
    // A zone affects a friendly tower when:
    //   1. only_own == true (friendly buff zone, e.g., Rage)
    //   2. Zone has a buff_key that maps to a BuffStats with hit_speed_multiplier > 100
    //   3. Tower is within the zone's radius
    struct TowerBuff {
        team: crate::entities::Team,
        x: i32,
        y: i32,
        radius_sq: i64,
        hitspeed_mult: i32, // Absolute: 135 = +35% faster
    }
    let mut tower_buffs: Vec<TowerBuff> = Vec::new();

    for entity in state.entities.iter() {
        if !entity.alive {
            continue;
        }
        if let crate::entities::EntityKind::SpellZone(ref sz) = entity.kind {
            // Only friendly buff zones (only_own = true, e.g., Rage)
            if !sz.only_own {
                continue;
            }
            // Look up the buff's hitspeed multiplier
            if let Some(ref buff_key) = sz.buff_key {
                if let Some(bs) = data.buffs.get(buff_key.as_str()) {
                    if bs.hit_speed_multiplier > 100 {
                        tower_buffs.push(TowerBuff {
                            team: entity.team,
                            x: entity.x,
                            y: entity.y,
                            radius_sq: (sz.radius as i64) * (sz.radius as i64),
                            hitspeed_mult: bs.hit_speed_multiplier,
                        });
                    }
                }
            }
        }
    }

    if tower_buffs.is_empty() {
        return;
    }

    // Apply buffs to friendly towers within zone radius.
    // If multiple zones overlap a tower, take the strongest buff (highest multiplier).
    for tb in &tower_buffs {
        let player = match tb.team {
            crate::entities::Team::Player1 => &mut state.player1,
            crate::entities::Team::Player2 => &mut state.player2,
        };

        // Check each tower position against the zone radius
        let towers: [&mut TowerState; 3] = [
            &mut player.princess_left,
            &mut player.princess_right,
            &mut player.king,
        ];
        for tower in towers {
            if !tower.alive {
                continue;
            }
            let dx = (tower.pos.0 - tb.x) as i64;
            let dy = (tower.pos.1 - tb.y) as i64;
            if dx * dx + dy * dy <= tb.radius_sq {
                // Apply the strongest hitspeed buff (highest multiplier wins)
                tower.rage_hitspeed = tower.rage_hitspeed.max(tb.hitspeed_mult);
            }
        }
    }
}

// =========================================================================
// Tower attacks
// =========================================================================

pub fn tick_towers(state: &mut GameState) {
    let targets: Vec<(EntityId, Team, i32, i32, bool, bool, usize)> = state
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_targetable() && (e.is_troop() || e.is_building()))
        .map(|(idx, e)| (e.id, e.team, e.x, e.y, e.is_flying(), e.alive, idx))
        .collect();

    // FIX 4: Track which specific tower fired, not just that "some tower fired".
    // Each event now carries (entity_index, damage, team, tower_id) where tower_id
    // is 0=princess_left, 1=princess_right, 2=king. This way we only reset the
    // cooldown for the exact tower that found a target and attacked.
    struct TowerAttack {
        entity_idx: usize,
        damage: i32,
        team: Team,
        tower_id: u8,  // 0=princess_left, 1=princess_right, 2=king
    }
    let mut tower_attacks: Vec<TowerAttack> = Vec::new();

    for player_team in [Team::Player1, Team::Player2] {
        let enemy_team = player_team.opponent();

        // Extract tower info: (x, y, range, damage, ready, tower_id)
        let tower_infos: Vec<(i32, i32, i32, i32, bool, u8)> = {
            let player = state.player(player_team);
            let mut infos = Vec::new();

            if player.princess_left.alive {
                infos.push((
                    player.princess_left.pos.0,
                    player.princess_left.pos.1,
                    PRINCESS_TOWER_RANGE,
                    PRINCESS_TOWER_DMG,
                    player.princess_left.attack_cooldown <= 0,
                    0u8, // princess_left
                ));
            }
            if player.princess_right.alive {
                infos.push((
                    player.princess_right.pos.0,
                    player.princess_right.pos.1,
                    PRINCESS_TOWER_RANGE,
                    PRINCESS_TOWER_DMG,
                    player.princess_right.attack_cooldown <= 0,
                    1u8, // princess_right
                ));
            }
            if player.king.alive && player.king.activated {
                infos.push((
                    player.king.pos.0,
                    player.king.pos.1,
                    KING_TOWER_RANGE,
                    KING_TOWER_DMG,
                    player.king.attack_cooldown <= 0,
                    2u8, // king
                ));
            }
            infos
        };

        for (tx, ty, range, damage, ready, tower_id) in &tower_infos {
            if !ready {
                continue;
            }
            let range_sq = range_squared(*range);
            let mut best_idx: Option<usize> = None;
            let mut best_dist = i64::MAX;

            for (_, team, ex, ey, _, alive, entity_idx) in &targets {
                if !alive || *team != enemy_team {
                    continue;
                }
                let dx = (*tx - ex) as i64;
                let dy = (*ty - ey) as i64;
                let dist = dx * dx + dy * dy;
                if dist <= range_sq && dist < best_dist {
                    best_dist = dist;
                    best_idx = Some(*entity_idx);
                }
            }

            if let Some(idx) = best_idx {
                tower_attacks.push(TowerAttack {
                    entity_idx: idx,
                    damage: *damage,
                    team: player_team,
                    tower_id: *tower_id,
                });
            }
        }

        // Tick all tower cooldowns (independent of whether they fired)
        let player = state.player_mut(player_team);
        if player.princess_left.alive {
            if player.princess_left.attack_cooldown > 0 {
                player.princess_left.attack_cooldown -= 1;
            }
        }
        if player.princess_right.alive {
            if player.princess_right.attack_cooldown > 0 {
                player.princess_right.attack_cooldown -= 1;
            }
        }
        if player.king.alive && player.king.activated {
            if player.king.attack_cooldown > 0 {
                player.king.attack_cooldown -= 1;
            }
        }
    }

    // Apply tower damage to entities
    for atk in &tower_attacks {
        if state.entities[atk.entity_idx].alive {
            apply_damage_to_entity(&mut state.entities[atk.entity_idx], atk.damage);
        }
    }

    // FIX 4: Reset cooldown ONLY for the specific tower that fired.
    // Rage on towers: scale cooldown by rage_hitspeed (135 = 35% faster).
    // TOWER_HIT_SPEED * 100 / 135 ≈ 12 ticks instead of 16. Data-driven
    // from BuffStats.hit_speed_multiplier via tick_tower_buffs().
    for atk in &tower_attacks {
        let player = state.player_mut(atk.team);
        let (tower, _) = match atk.tower_id {
            0 => (&mut player.princess_left, 0),
            1 => (&mut player.princess_right, 1),
            2 => (&mut player.king, 2),
            _ => continue,
        };
        let hs = tower.rage_hitspeed.max(10); // Min 10% to avoid division by zero
        tower.attack_cooldown = (TOWER_HIT_SPEED as i64 * 100 / hs as i64).max(1) as i32;
    }

    // King tower activation
    for player_team in [Team::Player1, Team::Player2] {
        let king_pos = match player_team {
            Team::Player1 => P1_KING_POS,
            Team::Player2 => P2_KING_POS,
        };
        let king = match player_team {
            Team::Player1 => &mut state.player1.king,
            Team::Player2 => &mut state.player2.king,
        };

        if king.alive && !king.activated {
            let activation_sq = range_squared(KING_ACTIVATION_RANGE);
            let enemy_team = player_team.opponent();
            for (_, team, ex, ey, _, alive, _) in &targets {
                if *alive && *team == enemy_team {
                    let dx = (king_pos.0 - ex) as i64;
                    let dy = (king_pos.1 - ey) as i64;
                    if dx * dx + dy * dy <= activation_sq {
                        king.activated = true;
                        break;
                    }
                }
            }

            // Also activate if a princess tower was destroyed
            if !king.activated {
                let player = match player_team {
                    Team::Player1 => &state.player1,
                    Team::Player2 => &state.player2,
                };
                if !player.princess_left.alive || !player.princess_right.alive {
                    // Check: did it JUST die this tick? We can't easily tell, so
                    // just activate if any princess is dead. This is slightly aggressive
                    // but matches real game behavior (king activates on princess death).
                    let king = match player_team {
                        Team::Player1 => &mut state.player1.king,
                        Team::Player2 => &mut state.player2.king,
                    };
                    king.activated = true;
                }
            }
        }
    }
}

// =========================================================================
// Death processing — death spawns + death damage
// =========================================================================

/// Entities that just died: collect death effects, then apply them.
pub fn tick_deaths(state: &mut GameState, data: &GameData) {
    // Phase 3: Fire evo on_death callbacks first
    let mut evo_death_indices: Vec<usize> = Vec::new();
    for (i, entity) in state.entities.iter().enumerate() {
        if !entity.alive && entity.hp <= 0 && entity.evo_state.is_some() {
            evo_death_indices.push(i);
        }
    }
    for idx in evo_death_indices {
        crate::evo_system::notify_evo_death(state, data, idx);
    }

    // Collect death events from entities that are now dead (hp <= 0)
    let mut death_spawns: Vec<(Team, String, i32, i32, usize, i32, String)> = Vec::new();
    // (team, spawn_key, x, y, level, count, source_card_key)
    let mut death_damages: Vec<(Team, i32, i32, i32, i32, i32)> = Vec::new();
    // (team, x, y, damage, radius, push_back)
    let mut death_elixir_grants: Vec<(Team, i32)> = Vec::new();
    // (team, elixir_amount) — Elixir Collector mana_on_death
    // ─── Fix #9: Real death projectiles (Phoenix, SuperLavaHound) ───
    // Instead of instant AoE, spawn actual traveling projectile entities.
    // (team, proj_name, from_x, from_y, damage, radius, speed, pushback,
    //  crown_tower_damage_percent, aoe_to_air, aoe_to_ground, target_buff, buff_time, level)
    struct DeathProjectile {
        team: Team,
        from_x: i32,
        from_y: i32,
        damage: i32,
        radius: i32,
        speed: i32,
        pushback: i32,
        crown_tower_damage_percent: i32,
        aoe_to_air: bool,
        aoe_to_ground: bool,
        homing: bool,
        target_buff: Option<String>,
        buff_time: i32,
        source_id: EntityId,
    }
    let mut death_projectiles: Vec<DeathProjectile> = Vec::new();

    for entity in state.entities.iter() {
        if entity.alive || entity.hp > 0 {
            continue; // Still alive or already processed
        }

        // Troops: death spawns + death damage (Golem → Golemites, Giant Skeleton bomb, etc.)
        if let EntityKind::Troop(ref troop) = entity.kind {
            if let Some(stats) = data.characters.get(&entity.card_key) {
                // Death spawn (Golem → Golemites, Lava Hound → Pups)
                if let Some(ref spawn_key) = stats.death_spawn_character {
                    if stats.death_spawn_count > 0 {
                        death_spawns.push((
                            entity.team,
                            spawn_key.clone(),
                            entity.x,
                            entity.y,
                            troop.level,
                            stats.death_spawn_count,
                            entity.card_key.clone(),
                        ));
                    }
                }
                // Second death spawn (if any)
                if let Some(ref spawn_key2) = stats.death_spawn_character2 {
                    if stats.death_spawn_count2 > 0 {
                        death_spawns.push((
                            entity.team,
                            spawn_key2.clone(),
                            entity.x,
                            entity.y,
                            troop.level,
                            stats.death_spawn_count2,
                            entity.card_key.clone(),
                        ));
                    }
                }

                // Death damage (Giant Skeleton bomb, Golem, Balloon, etc.)
                if stats.death_damage > 0 {
                    death_damages.push((
                        entity.team,
                        entity.x,
                        entity.y,
                        stats.death_damage_at_level(troop.level),
                        stats.death_damage_radius,
                        stats.death_push_back,
                    ));
                }

                // Egg-based respawn (Phoenix → PhoenixEgg → PhoenixNoRespawn).
                // When a troop has death_spawn_projectile set (e.g., "PhoenixFireball"),
                // check if a "{Name}Egg" character exists. If so, spawn it as a death spawn.
                // This is how real CR implements the Phoenix lifecycle: the egg is an
                // implicit death spawn tied to the death_spawn_projectile field.
                if stats.death_spawn_projectile.is_some() && stats.death_spawn_character.is_none() {
                    let egg_name = format!("{}Egg", stats.name);
                    if data.find_character(&egg_name).is_some() {
                        death_spawns.push((
                            entity.team,
                            egg_name,
                            entity.x,
                            entity.y,
                            troop.level,
                            1, // One egg
                            entity.card_key.clone(),
                        ));
                    }
                }

                // Death projectile (Phoenix → PhoenixFireball, SuperLavaHound → FireWallProjectile).
                // ─── Fix #9: Spawn a real traveling projectile entity instead of instant AoE ───
                // The projectile travels from the death position toward the nearest enemy,
                // then explodes on arrival dealing AoE damage with proper radius, pushback,
                // and crown tower reduction — all read from ProjectileStats (data-driven).
                if let Some(ref proj_name) = stats.death_spawn_projectile {
                    if let Some(proj) = data.projectiles.get(proj_name.as_str()) {
                        let dmg = if !proj.damage_per_level.is_empty() && troop.level > 0 {
                            let idx = (troop.level - 1).min(proj.damage_per_level.len() - 1);
                            proj.damage_per_level[idx]
                        } else {
                            proj.damage
                        };
                        let radius = if stats.death_spawn_radius > 0 {
                            stats.death_spawn_radius
                        } else {
                            proj.radius
                        };
                        if dmg > 0 && radius > 0 {
                            let speed = if proj.speed > 0 {
                                // Convert projectile speed from data units to engine units/tick.
                                // Projectile speeds in JSON are in the same internal-units space.
                                // Standard conversion: speed / 20 (ticks per second).
                                // But many projectiles use a different scale — use raw value
                                // divided by a factor. Real CR projectile speed=600 means
                                // the projectile covers ~600 units per game tick at 20 tps.
                                // Let's use the same conversion as other projectile spawns.
                                proj.speed
                            } else {
                                600 // Fallback: fast projectile
                            };
                            death_projectiles.push(DeathProjectile {
                                team: entity.team,
                                from_x: entity.x,
                                from_y: entity.y,
                                damage: dmg,
                                radius,
                                speed,
                                pushback: proj.pushback,
                                crown_tower_damage_percent: proj.crown_tower_damage_percent,
                                aoe_to_air: proj.aoe_to_air,
                                aoe_to_ground: proj.aoe_to_ground,
                                homing: proj.homing,
                                target_buff: proj.target_buff.clone(),
                                buff_time: proj.buff_time,
                                source_id: entity.id,
                            });
                        }
                    }
                }
            }
        }

        // Troop: elixir granted to OPPONENT on death (Elixir Golem penalty).
        // mana_on_death_for_opponent: 1000 = 1 elixir in data units.
        // Convert to engine fixed-point: multiply by 10 (engine uses 10_000 = 1 elixir).
        if let EntityKind::Troop(ref troop) = entity.kind {
            if troop.elixir_on_death_for_opponent > 0 {
                let fixed_point = troop.elixir_on_death_for_opponent * 10;
                death_elixir_grants.push((entity.team.opponent(), fixed_point));
            }
        }

        // Buildings: death spawns (Goblin Cage → Goblin Brawler, Tombstone → Skeletons, etc.)
        // Building stats live in data.buildings, not data.characters.
        if let EntityKind::Building(ref bld) = entity.kind {
            if let Some(stats) = data.buildings.get(&entity.card_key) {
                if let Some(ref spawn_key) = stats.death_spawn_character {
                    if stats.death_spawn_count > 0 {
                        death_spawns.push((
                            entity.team,
                            spawn_key.clone(),
                            entity.x,
                            entity.y,
                            bld.level,
                            stats.death_spawn_count,
                            entity.card_key.clone(),
                        ));
                    }
                }
                if let Some(ref spawn_key2) = stats.death_spawn_character2 {
                    if stats.death_spawn_count2 > 0 {
                        death_spawns.push((
                            entity.team,
                            spawn_key2.clone(),
                            entity.x,
                            entity.y,
                            bld.level,
                            stats.death_spawn_count2,
                            entity.card_key.clone(),
                        ));
                    }
                }
                if stats.death_damage > 0 {
                    death_damages.push((
                        entity.team,
                        entity.x,
                        entity.y,
                        stats.death_damage_at_level(bld.level),
                        stats.death_damage_radius,
                        stats.death_push_back,
                    ));
                }
            }

            // Elixir on death (Elixir Collector: mana_on_death=1)
            // Convert whole elixir to engine fixed-point (1 elixir = 10_000)
            if bld.elixir_on_death > 0 {
                death_elixir_grants.push((entity.team, bld.elixir_on_death * 10_000));
            }
        }
    }

    // ─── Fix #5a: Bottle buildings → spell zone on death (Lumberjack Rage) ───
    // "Bottle" buildings (RageBarbarianBottle, HealBottle, CloneBottle, etc.) are
    // empty containers that should spawn a spell zone when they expire. The mapping
    // is data-driven: strip "Bottle" suffix to get the spell name, then try known
    // patterns in data.spells.
    let mut death_spell_zones: Vec<(Team, String, i32, i32, usize)> = Vec::new();
    for entity in state.entities.iter() {
        if entity.alive || entity.hp > 0 {
            continue;
        }
        if let EntityKind::Building(ref bld) = entity.kind {
            let card_key = &entity.card_key;
            // Only process bottle-like buildings (name ends in Bottle or card_key contains "bottle")
            let stats = data.buildings.get(card_key.as_str());
            let name = stats.map(|s| s.name.as_str()).unwrap_or("");
            if name.contains("Bottle") || card_key.contains("bottle") || card_key.contains("Bottle") {
                // The bottle has no intrinsic spell reference, so we derive the spell name.
                // Strip "Bottle" from name to get the base, then try known spell patterns.
                // "RageBarbarianBottle" → base "RageBarbarian" → try "BarbarianRage", "Rage", base
                // "HealBottle" → base "Heal" → try "Heal"
                let base = name.replace("Bottle", "").replace("bottle", "");
                let base_trimmed = base.trim().to_string();

                let mut found_spell = None;
                // Try common patterns (data-driven: no card names, just string transforms)
                let try_spells = [
                    // Reverse CamelCase halves: "RageBarbarian" → "BarbarianRage"
                    {
                        let chars: Vec<char> = base_trimmed.chars().collect();
                        let split_pos = chars.iter().skip(1).position(|c| c.is_uppercase()).map(|p| p + 1);
                        if let Some(pos) = split_pos {
                            let first: String = chars[..pos].iter().collect();
                            let second: String = chars[pos..].iter().collect();
                            format!("{}{}", second, first)
                        } else {
                            base_trimmed.clone()
                        }
                    },
                    base_trimmed.clone(),
                    // Try just the first CamelCase word (e.g. "Rage" from "RageBarbarian")
                    {
                        let chars: Vec<char> = base_trimmed.chars().collect();
                        let end = chars.iter().skip(1).position(|c| c.is_uppercase()).map(|p| p + 1).unwrap_or(chars.len());
                        chars[..end].iter().collect()
                    },
                ];
                for spell_name in &try_spells {
                    if spell_name.is_empty() { continue; }
                    if data.spells.contains_key(spell_name.as_str()) {
                        found_spell = Some(spell_name.clone());
                        break;
                    }
                    // Also try lowercase
                    let lower = spell_name.to_lowercase();
                    if data.spells.contains_key(&lower) {
                        found_spell = Some(lower);
                        break;
                    }
                }
                // Fallback: scan all spells for name match
                if found_spell.is_none() {
                    let base_lower = base.trim().to_lowercase();
                    for (k, s) in &data.spells {
                        let sn = s.name.to_lowercase();
                        if sn.contains(&base_lower) || base_lower.contains(&sn) {
                            if s.buff.is_some() || s.heal_per_second > 0 {
                                found_spell = Some(k.clone());
                                break;
                            }
                        }
                    }
                }
                if let Some(spell_key) = found_spell {
                    death_spell_zones.push((entity.team, spell_key.clone(), entity.x, entity.y, bld.level));

                    // ─── Companion damage zone (data-driven) ───
                    // Some buff spells have a paired "{Name}Damage" spell in the data
                    // that deals instant AoE damage when the bottle explodes.
                    //   BarbarianRage  → BarbarianRageDamage (damage=160, only_enemies=true)
                    //   Rage           → RageDamage          (damage=120, only_enemies=true)
                    // These companion spells have life_duration=1 (single-tick), damage > 0,
                    // and only_enemies=true. We detect them purely from the spell data:
                    // look up "{spell_key}Damage" in data.spells. If it exists and has
                    // damage > 0, spawn it as an additional zone at the same position.
                    // Also check by name: if the resolved spell has a name, try "{name}Damage".
                    let damage_key_direct = format!("{}Damage", spell_key);
                    let damage_key_by_name = data.spells.get(&spell_key)
                        .map(|s| format!("{}Damage", s.name));

                    // Try direct key first, then name-derived key, then scan by name.
                    let companion_key = if data.spells.contains_key(&damage_key_direct) {
                        Some(damage_key_direct)
                    } else if let Some(ref nk) = damage_key_by_name {
                        if data.spells.contains_key(nk.as_str()) {
                            Some(nk.clone())
                        } else {
                            // Scan: find any spell whose name matches "{primary_name}Damage"
                            damage_key_by_name.as_ref().and_then(|target_name| {
                                data.spells.iter()
                                    .find(|(_, v)| v.name == *target_name)
                                    .map(|(k, _)| k.clone())
                            })
                        }
                    } else {
                        None
                    };

                    if let Some(ck) = companion_key {
                        if let Some(ds) = data.spells.get(&ck) {
                            if ds.damage > 0 {
                                death_spell_zones.push((entity.team, ck, entity.x, entity.y, bld.level));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Buff death spawns (Mother Witch VoodooCurse → VoodooHog) ──
    // When a unit dies while carrying a buff with death_spawn set,
    // spawn the specified character. If death_spawn_is_enemy, the spawned
    // unit fights for the OPPOSITE team (the team that applied the curse).
    for entity in state.entities.iter() {
        if entity.alive || entity.hp > 0 {
            continue;
        }
        let level = match &entity.kind {
            EntityKind::Troop(t) => t.level,
            EntityKind::Building(b) => b.level,
            _ => continue,
        };
        for buff in &entity.buffs {
            if let Some(ref spawn_key) = buff.death_spawn {
                let count = if buff.death_spawn_count > 0 { buff.death_spawn_count } else { 1 };
                let spawn_team = if buff.death_spawn_is_enemy {
                    entity.team.opponent()
                } else {
                    entity.team
                };
                death_spawns.push((
                    spawn_team,
                    spawn_key.clone(),
                    entity.x,
                    entity.y,
                    level,
                    count,
                    entity.card_key.clone(),
                ));
            }
        }
    }

    // Spawn death spawn units
    let mut death_damage_fallbacks: Vec<(Team, String, i32, i32, usize)> = Vec::new(); // (team, source_card_key, x, y, level)
    for (team, key, x, y, level, count, source_card_key) in &death_spawns {
        let spawn_stats = data.find_character(key)
            .or_else(|| {
                let singular = key.strip_suffix('s').unwrap_or(key);
                data.find_character(singular)
            });
        if let Some(stats) = spawn_stats {
            let cr = stats.collision_radius.max(200);
            let n = *count as usize;

            // Fix #10: death_spawn_min_radius — minimum scatter distance from death position.
            // When > 0, death spawns are placed in a ring between min_radius and the normal
            // formation radius instead of from the center. Prevents Lava Pups/Golemites from
            // all stacking on the exact death point. Data-driven from source card stats.
            let min_spawn_radius = data.characters.get(source_card_key.as_str())
                .or_else(|| data.buildings.get(source_card_key.as_str()))
                .map(|s| s.death_spawn_min_radius)
                .unwrap_or(0);

            // Build formation offsets based on count.
            // Same patterns as play_card formations — adapted for death spawns.
            // When death_spawn_min_radius > 0, all placements use the ring pattern
            // with inner radius = min_spawn_radius to avoid center stacking.
            let mut offsets: Vec<(i32, i32)> = Vec::new();
            if min_spawn_radius > 0 && n > 1 {
                // Ring placement: all units placed at min_spawn_radius distance
                // in an equal-division circle. No units at center.
                let radius = min_spawn_radius.max(cr) as f64;
                for i in 0..n {
                    let angle = (i as f64) * 2.0 * std::f64::consts::PI / n as f64;
                    offsets.push(((angle.cos() * radius) as i32, (angle.sin() * radius) as i32));
                }
            } else if n == 1 {
                offsets.push((0, 0));
            } else if n == 2 {
                // Lateral pair (Golemites, Battle Ram barbarians)
                offsets.push((-cr, 0));
                offsets.push((cr, 0));
            } else if n == 3 {
                // Triangle (Night Witch bats on death, etc.)
                let fwd = team.forward_y();
                offsets.push((0, cr * fwd));
                offsets.push((-cr, 0));
                offsets.push((cr, 0));
            } else if n <= 8 {
                // Single ring (Lava Pups 6, etc.)
                let radius = (cr * 2) as f64;
                for i in 0..n {
                    let angle = (i as f64) * 2.0 * std::f64::consts::PI / n as f64;
                    offsets.push(((angle.cos() * radius) as i32, (angle.sin() * radius) as i32));
                }
            } else {
                // Concentric rings for large counts (Skeleton Barrel 7+)
                offsets.push((0, 0));
                let inner_count = ((n - 1) as f64 * 0.35).round() as usize;
                let outer_count = n - 1 - inner_count;
                let inner_r = cr as f64;
                let outer_r = (cr * 2) as f64;
                for i in 0..inner_count {
                    let angle = (i as f64) * 2.0 * std::f64::consts::PI / inner_count as f64;
                    offsets.push(((angle.cos() * inner_r) as i32, (angle.sin() * inner_r) as i32));
                }
                for i in 0..outer_count {
                    let angle = (i as f64) * 2.0 * std::f64::consts::PI / outer_count as f64;
                    offsets.push(((angle.cos() * outer_r) as i32, (angle.sin() * outer_r) as i32));
                }
            }

            for (i, &(ox, oy)) in offsets.iter().enumerate().take(n) {
                let id = state.alloc_id();
                let mut troop =
                    Entity::new_troop(id, *team, stats, x + ox, y + oy, *level, false);
                // Fix #5: death_spawn_deploy_time — death-spawned troops wait before acting.
                // Battle Ram=1000ms, Goblin Giant=700ms. Data-driven from the SOURCE
                // card's CharacterStats.death_spawn_deploy_time (the card that died,
                // not the spawned unit). 0 = use the spawned entity's own deploy_time
                // (from Entity::new_troop → stats.deploy_time). This preserves
                // PhoenixEgg's own deploy_time=1000ms when Phoenix dies (Fix #14).
                let deploy_delay = data.characters.get(source_card_key.as_str())
                    .or_else(|| data.buildings.get(source_card_key.as_str()))
                    .map(|s| s.death_spawn_deploy_time)
                    .unwrap_or(0);
                if deploy_delay > 0 {
                    // Source card specifies an explicit deploy delay for death spawns
                    troop.deploy_timer = crate::entities::ms_to_ticks(deploy_delay);
                } else if troop.deploy_timer <= 0 {
                    // No source delay AND spawned entity has no deploy_time of its own
                    // → instant deployment (backwards-compatible default)
                    troop.deploy_timer = 0;
                }
                // else: spawned entity has its own deploy_time (e.g., PhoenixEgg=1000ms)
                // → keep it as-is from Entity::new_troop()
                state.entities.push(troop);
            }
        } else {
            // Try buildings table — bomb entities (GiantSkeletonBomb, BalloonBomb,
            // MightyMinerBomb, etc.) are in data.buildings, not data.characters.
            // They have hp=0, speed=0, deploy_time=fuse timer, death_damage=explosion.
            // Spawn as a building with lifetime = deploy_time ticks (fuse).
            let bomb_key_lower = key.to_lowercase().replace(' ', "-");
            let bomb_stats = data.buildings.get(&bomb_key_lower)
                .or_else(|| data.buildings.get(key.as_str()))
                .or_else(|| {
                    // Try name-based lookup across all buildings
                    data.buildings.values().find(|b| {
                        b.name.eq_ignore_ascii_case(key)
                            || b.key.eq_ignore_ascii_case(key)
                    })
                });

            if let Some(bstats) = bomb_stats {
                // Spawn as building. Use deploy_time as the fuse (lifetime).
                // When lifetime expires → alive=false → tick_deaths fires death_damage.
                let fuse_ticks = ms_to_ticks(bstats.deploy_time);
                for i in 0..*count {
                    let id = state.alloc_id();
                    let offset_x = ((i % 3) as i32 - 1) * bstats.collision_radius.max(200);
                    let offset_y = ((i / 3) as i32) * bstats.collision_radius.max(200);
                    let mut bomb = Entity::new_building(
                        id, *team, bstats, x + offset_x, y + offset_y, *level,
                    );
                    // Override lifetime to be the fuse timer (deploy_time from data).
                    // The bomb's life_time=0 in data, so new_building sets lifetime=0.
                    // We use deploy_time as the actual fuse duration.
                    if let EntityKind::Building(ref mut bd) = bomb.kind {
                        bd.lifetime = if fuse_ticks > 0 { fuse_ticks } else { 1 };
                        bd.lifetime_remaining = bd.lifetime;
                    }
                    bomb.deploy_timer = 0; // Instantly active (already "deployed" by death)
                    // Bomb buildings have hp=0 in the CSV (they're not meant to
                    // be attacked). Set hp=1 so the entity stays alive during the
                    // fuse. When lifetime_remaining hits 0, alive→false triggers
                    // tick_deaths which fires death_damage AoE.
                    if bomb.hp == 0 {
                        bomb.hp = 1;
                        bomb.max_hp = 1;
                    }
                    state.entities.push(bomb);
                }
            } else {
                // Truly unresolvable — try projectile fallback
                death_damage_fallbacks.push((*team, source_card_key.clone(), *x, *y, *level));
            }
        }
    }

    // ─── Fix #11: death_spawn_pushback — push enemies away when death spawns appear ───
    // Golem, Lava Hound, SuperLavaHound have death_spawn_pushback=True.
    // This is separate from death_push_back (explosion pushback from death_damage).
    // death_spawn_pushback fires when the spawned units materialize, pushing nearby
    // enemies away from the death position. Uses death_spawn_radius (or death_damage_radius
    // as fallback) for the AoE and collision_radius * 2 as push distance.
    // Data-driven: only fires if the SOURCE card has death_spawn_pushback=true in JSON.
    {
        // We already iterated death_spawns above; collect pushback events here by
        // re-checking the source cards. Use a separate pass to avoid borrow conflicts.
        let mut spawn_pushback_events: Vec<(Team, i32, i32, i32, i32)> = Vec::new(); // (team, x, y, radius, push_dist)
        // Deduplicate by source position — each death position only pushes once
        let mut seen_positions: Vec<(i32, i32)> = Vec::new();

        for (team, _key, x, y, _level, _count, source_card_key) in &death_spawns {
            if seen_positions.contains(&(*x, *y)) {
                continue;
            }
            let has_pushback = data.characters.get(source_card_key.as_str())
                .or_else(|| data.buildings.get(source_card_key.as_str()))
                .map(|s| s.death_spawn_pushback)
                .unwrap_or(false);
            if has_pushback {
                let radius = data.characters.get(source_card_key.as_str())
                    .or_else(|| data.buildings.get(source_card_key.as_str()))
                    .map(|s| {
                        if s.death_spawn_radius > 0 { s.death_spawn_radius }
                        else if s.death_damage_radius > 0 { s.death_damage_radius }
                        else { s.collision_radius * 3 }
                    })
                    .unwrap_or(600);
                let push_dist = radius; // Push distance = radius (reasonable default)
                spawn_pushback_events.push((*team, *x, *y, radius, push_dist));
                seen_positions.push((*x, *y));
            }
        }

        for (team, cx, cy, radius, push_dist) in spawn_pushback_events {
            let radius_sq = (radius as i64) * (radius as i64);
            for entity in state.entities.iter_mut() {
                if !entity.alive || entity.team == team {
                    continue;
                }
                if entity.is_building() || matches!(entity.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                    continue;
                }
                if entity_ignores_pushback(entity) {
                    continue;
                }
                let dx = (entity.x - cx) as i64;
                let dy = (entity.y - cy) as i64;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= radius_sq && dist_sq > 0 {
                    let dist = (dist_sq as f64).sqrt() as i64;
                    entity.x += (dx * push_dist as i64 / dist) as i32;
                    entity.y += (dy * push_dist as i64 / dist) as i32;
                    entity.x = entity.x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
                    entity.y = entity.y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
                }
            }
        }
    }

    // FIX 5: Fallback for unresolvable death_spawn_character entries (e.g., BombTowerBomb).
    // When a building's death_spawn_character doesn't resolve as a character, look up the
    // building's projectile data and apply its damage + radius as instant AoE death damage.
    for (team, source_card_key, x, y, level) in &death_damage_fallbacks {
        // Look up the source building's stats to find its projectile
        let bstats = data.buildings.get(source_card_key.as_str())
            .or_else(|| data.characters.get(source_card_key.as_str()));
        if let Some(stats) = bstats {
            // Try direct projectile key, then "{summon_character}Projectile" pattern
            let proj = stats.projectile.as_ref()
                .and_then(|k| data.projectiles.get(k.as_str()))
                .or_else(|| {
                    let sc = stats.summon_character.as_deref().unwrap_or("");
                    if !sc.is_empty() {
                        let key_try = format!("{}Projectile", sc);
                        data.projectiles.get(&key_try)
                    } else {
                        None
                    }
                });

            if let Some(proj_stats) = proj {
                let dmg = if !proj_stats.damage_per_level.is_empty() && *level > 0 {
                    let idx = (*level - 1).min(proj_stats.damage_per_level.len() - 1);
                    proj_stats.damage_per_level[idx]
                } else {
                    proj_stats.damage
                };
                let radius = proj_stats.radius;
                if dmg > 0 && radius > 0 {
                    death_damages.push((*team, *x, *y, dmg, radius, 0));
                }
            }
        }
    }

    // Apply death damage + death_push_back
    for (team, dx, dy, damage, radius, push_back) in death_damages {
        let radius_sq = (radius as i64) * (radius as i64);

        // Hit enemy entities
        for entity in state.entities.iter_mut() {
            if !entity.alive || entity.team == team {
                continue;
            }
            let d = entity.dist_sq_to(dx, dy);
            if d <= radius_sq {
                apply_damage_to_entity(entity, damage);

                // Apply death_push_back: push enemy radially away from explosion center.
                // Matches real CR: GiantSkeletonBomb death_push_back=1800,
                // Golem death_push_back=1800, CommonBomb=1500, etc.
                // Troops with ignore_pushback are NOT exempt from death pushback in real CR
                // (Giant Skeleton bomb pushes everything including Golem).
                if push_back > 0 && entity.alive && !entity.is_building() {
                    let ex_dx = (entity.x - dx) as i64;
                    let ex_dy = (entity.y - dy) as i64;
                    let dist = ((ex_dx * ex_dx + ex_dy * ex_dy) as f64).sqrt() as i64;
                    if dist > 0 {
                        let push_x = (ex_dx * push_back as i64 / dist) as i32;
                        let push_y = (ex_dy * push_back as i64 / dist) as i32;
                        entity.x += push_x;
                        entity.y += push_y;
                    }
                }
            }
        }

        // Hit enemy towers
        let enemy_towers = enemy_tower_ids(team);
        for tid in &enemy_towers {
            if let Some(tpos) = tower_pos(state, *tid) {
                let ddx = (dx - tpos.0) as i64;
                let ddy = (dy - tpos.1) as i64;
                if ddx * ddx + ddy * ddy <= radius_sq {
                    apply_damage_to_tower(state, *tid, damage);
                }
            }
        }
    }

    // ─── Fix #9: Spawn real death projectiles (Phoenix, SuperLavaHound) ───
    // Find the nearest enemy to each death position and spawn a traveling projectile.
    // The projectile uses ProjectileStats data for speed, radius, pushback, etc.
    // When it arrives and impacts, the normal projectile resolution in tick_combat
    // handles AoE damage, pushback, crown tower reduction, and buff application.
    for dp in death_projectiles {
        // Find the nearest enemy entity or tower as the projectile target
        let mut best_target_id: Option<EntityId> = None;
        let mut best_target_x = dp.from_x;
        let mut best_target_y = dp.from_y;
        let mut best_dist_sq: i64 = i64::MAX;

        // Check enemy entities
        for entity in state.entities.iter() {
            if !entity.alive || entity.team == dp.team {
                continue;
            }
            if !entity.is_targetable() {
                continue;
            }
            let dx = (entity.x - dp.from_x) as i64;
            let dy = (entity.y - dp.from_y) as i64;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                best_target_id = Some(entity.id);
                best_target_x = entity.x;
                best_target_y = entity.y;
            }
        }

        // Check enemy towers
        let enemy_towers = enemy_tower_ids(dp.team);
        for tid in &enemy_towers {
            if let Some(tpos) = tower_pos(state, *tid) {
                let dx = (tpos.0 - dp.from_x) as i64;
                let dy = (tpos.1 - dp.from_y) as i64;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best_target_id = Some(*tid);
                    best_target_x = tpos.0;
                    best_target_y = tpos.1;
                }
            }
        }

        // If no target found, fire projectile at the death position (instant AoE fallback)
        let target_id = best_target_id.unwrap_or(EntityId(0));
        let proj_id = state.alloc_id();
        let mut proj = Entity::new_projectile(
            proj_id,
            dp.team,
            dp.source_id,
            dp.from_x,
            dp.from_y,
            target_id,
            best_target_x,
            best_target_y,
            dp.speed,
            dp.damage,
            dp.radius,
            dp.homing,
            dp.crown_tower_damage_percent,
            dp.aoe_to_air,
            dp.aoe_to_ground,
        );
        // Set pushback from projectile data
        if let EntityKind::Projectile(ref mut pd) = proj.kind {
            pd.pushback = dp.pushback;
            pd.pushback_all = dp.pushback > 0; // Death projectile pushback hits all targets
            if let Some(ref buff_key) = dp.target_buff {
                pd.target_buff = Some(buff_key.clone());
                pd.target_buff_time = crate::entities::ms_to_ticks(dp.buff_time);
            }
        }
        state.entities.push(proj);
    }

    // Update crowns
    state.player1.crowns = state.player2.recount_opponent_crowns();
    state.player2.crowns = state.player1.recount_opponent_crowns();

    // Apply deferred elixir grants from deaths (Elixir Collector, Elixir Golem).
    // Amounts are already in engine fixed-point (10_000 = 1 elixir).
    for (team, amount) in death_elixir_grants {
        let player = state.player_mut(team);
        player.elixir += amount;
        if player.elixir > crate::game_state::MAX_ELIXIR {
            player.elixir = crate::game_state::MAX_ELIXIR;
        }
    }

    // ─── Fix #5a: Spawn spell zones from Bottle building deaths ───
    for (team, spell_key, x, y, level) in death_spell_zones {
        if let Some(spell) = data.spells.get(&spell_key) {
            let radius = spell.radius;
            let duration_ticks = if spell.life_duration > 0 {
                crate::entities::ms_to_ticks(spell.life_duration)
            } else {
                1
            };
            let hit_interval = if spell.hit_speed > 0 {
                crate::entities::ms_to_ticks(spell.hit_speed)
            } else {
                duration_ticks
            };
            let buff_key = spell.buff.clone();
            let buff_time = if spell.buff_time > 0 {
                crate::entities::ms_to_ticks(spell.buff_time)
            } else {
                duration_ticks
            };
            let id = state.alloc_id();
            let zone = Entity::new_spell_zone(
                id,
                team,
                &spell_key,      // card_key
                x,
                y,
                radius,
                duration_ticks,
                spell.damage,    // damage_per_tick
                hit_interval,
                spell.aoe_to_air || spell.hits_air,   // affects_air
                spell.aoe_to_ground || spell.hits_ground, // affects_ground
                buff_key,        // buff_key
                buff_time,       // buff_duration
                spell.only_enemies,
                spell.only_own_troops,
                spell.crown_tower_damage_percent,
                0,               // attract_strength (Rage has none)
                None,            // spawn_character
                0,               // spawn_interval
                0,               // spawn_initial_delay
                level,           // spawn_level
                false,           // hit_biggest_targets
                0,               // max_hit_targets
                0,               // projectile_damage
                0,               // projectile_ct_pct
                None,            // spell_projectile_key
                0, // spawn_min_radius (0 = full radius)
                0, // heal_per_hit
                0, false, // no pushback
                0, 0,     // no distance-scaled pushback
                // Combine SpellStats + BuffStats no_effect_to_crown_towers.
                spell.no_effect_to_crown_towers
                    || spell.buff.as_ref()
                        .and_then(|bk| data.buffs.get(bk))
                        .map(|bs| bs.no_effect_to_crown_towers)
                        .unwrap_or(false),
                spell.affects_hidden,
                1, 1, // level_scale: death zone, DOT not primary damage
            );
            state.entities.push(zone);
        }
    }
}

// =========================================================================
// Damage helpers
// =========================================================================

/// Returns true if this entity is immune to knockback/pushback effects.
/// Data-driven from CharacterStats.ignore_pushback (Golem, Giant, Lava Hound, etc.).
pub fn entity_ignores_pushback(entity: &Entity) -> bool {
    match &entity.kind {
        EntityKind::Troop(t) => t.ignore_pushback,
        _ => false,
    }
}

/// Apply damage to an entity (shield → HP). Respects damage reduction buffs.
pub fn apply_damage_to_entity(entity: &mut Entity, damage: i32) {
    if !entity.alive {
        return;
    }

    // Dash invulnerability: troops with active dash immunity (Bandit, Golden Knight)
    // cannot take damage. dash_immune_remaining is set when a dash starts and
    // decremented each tick in tick_combat.
    if let EntityKind::Troop(ref t) = entity.kind {
        if t.is_dashing && t.dash_immune_remaining > 0 {
            return;
        }
    }

    // Phase 3: Apply damage reduction from buffs
    let reduction = entity.damage_reduction();
    let effective_damage = if reduction > 0 {
        (damage as i64 * (100 - reduction) as i64 / 100) as i32
    } else {
        damage
    };

    let mut remaining = effective_damage;

    if entity.shield_hp > 0 {
        if remaining <= entity.shield_hp {
            entity.shield_hp -= remaining;
            return;
        } else {
            // Shield destroyed — absorbs the ENTIRE hit, no bleed-through.
            // For the classic shield troops (Dark Prince, Royal Recruits, Guards),
            // shield HP is a separate layer. If a single hit destroys the shield,
            // any excess damage from that same hit is negated and does not carry
            // over to the troop's normal HP. This is why very high single-hit
            // damage (Lightning, Rocket, Sparky) cannot one-shot these troops
            // through an intact shield.
            entity.shield_hp = 0;
            return;
        }
    }

    entity.hp -= remaining;
    if entity.hp <= 0 {
        entity.hp = 0;
        entity.alive = false;
    }
}

/// Apply damage to a tower by sentinel ID.
/// Also activates the king tower if it takes direct damage (matches real CR).
pub fn apply_damage_to_tower(state: &mut GameState, id: EntityId, damage: i32) {
    if let Some(tower) = tower_mut(state, id) {
        tower.take_damage(damage);
    }
    // King activation from direct damage — in real CR, any damage to the
    // king tower (spell, projectile, troop) immediately activates it.
    if id == P1_KING_TOWER_ID && state.player1.king.alive && !state.player1.king.activated {
        state.player1.king.activated = true;
    }
    if id == P2_KING_TOWER_ID && state.player2.king.alive && !state.player2.king.activated {
        state.player2.king.activated = true;
    }
}

// =========================================================================
// Spell zone ticks (moved from engine.rs, now tower-aware)
// =========================================================================

pub fn tick_spell_zones(state: &mut GameState, data: &GameData) {
    // ── Displacement pass (Tornado pull) ──────────────────────────────
    // Runs every tick (not gated by hit_timer) for zones with has_displacement.
    // Pulls affected entities toward zone center. Pull strength inversely
    // proportional to entity mass (heavy units resist more).
    // Buildings and projectiles are immune.
    {
        // Collect displacement zones first (avoids borrow conflict)
        struct DisplacementZone {
            x: i32,
            y: i32,
            radius_sq: i64,
            strength: i32,
            team: Team,
            affects_air: bool,
            affects_ground: bool,
            only_enemies: bool,
            /// FIX 4: Data-driven mass resistance factor from BuffStats.push_mass_factor.
            /// Controls how strongly mass resists displacement. In real CR, this is
            /// per-buff: Tornado has push_mass_factor=100, meaning mass divides pull
            /// by (mass * push_mass_factor / 100 + 1). A factor of 0 means mass has
            /// no effect (all units pulled equally). Default fallback uses the legacy
            /// formula if not set in data.
            push_mass_factor: i32,
            /// Data-driven from BuffStats.controlled_by_parent.
            /// When true: the zone drives displacement (bypasses stun/freeze resistance).
            /// When false: displacement acts as if applied by the buff itself, so
            ///   stunned/frozen entities resist it (they can't be moved by their own buffs).
            controlled_by_parent: bool,
        }
        let zones: Vec<DisplacementZone> = state.entities.iter()
            .filter_map(|e| {
                if !e.alive { return None; }
                if let EntityKind::SpellZone(ref sz) = e.kind {
                    if sz.has_displacement && sz.attract_strength > 0 {
                        // FIX 4: Look up push_mass_factor and controlled_by_parent from the buff.
                        let buff_data = sz.buff_key.as_ref()
                            .and_then(|bk| data.buffs.get(bk.as_str()));
                        let push_mass_factor = buff_data.map(|bs| bs.push_mass_factor).unwrap_or(0);
                        let controlled_by_parent = buff_data.map(|bs| bs.controlled_by_parent).unwrap_or(true);
                        return Some(DisplacementZone {
                            x: e.x,
                            y: e.y,
                            radius_sq: (sz.radius as i64) * (sz.radius as i64),
                            strength: sz.attract_strength,
                            team: e.team,
                            affects_air: sz.affects_air,
                            affects_ground: sz.affects_ground,
                            only_enemies: sz.only_enemies,
                            push_mass_factor,
                            controlled_by_parent,
                        });
                    }
                }
                None
            })
            .collect();

        if !zones.is_empty() {
            // Collect displacements: (entity_index, delta_x, delta_y)
            let mut displacements: Vec<(usize, i32, i32)> = Vec::new();

            for zone in &zones {
                for (idx, target) in state.entities.iter().enumerate() {
                    if !target.is_targetable() { continue; }
                    // Buildings immune to pull
                    if target.is_building() { continue; }
                    // Skip other spell zones and projectiles
                    if matches!(target.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
                        continue;
                    }
                    if zone.only_enemies && target.team == zone.team { continue; }
                    if target.is_flying() && !zone.affects_air { continue; }
                    if !target.is_flying() && !zone.affects_ground { continue; }

                    // Data-driven stun/freeze resistance from controlled_by_parent.
                    // When controlled_by_parent=true: the ZONE drives displacement,
                    //   so stun/freeze does NOT block the pull (zone overrides entity state).
                    // When controlled_by_parent=false: displacement acts like a buff effect,
                    //   so stunned/frozen entities resist it (they can't be moved by buffs).
                    if !zone.controlled_by_parent && target.is_immobilized() {
                        continue;
                    }

                    let dist_sq = target.dist_sq_to(zone.x, zone.y);
                    if dist_sq > zone.radius_sq || dist_sq == 0 { continue; }

                    let dist = (dist_sq as f64).sqrt() as i32;
                    if dist == 0 { continue; }

                    let dx = zone.x - target.x;
                    let dy = zone.y - target.y;

                    // FIX 4: Data-driven mass-based resistance using push_mass_factor from BuffStats.
                    // When push_mass_factor > 0 (from the buff's JSON data), the formula is:
                    //   effective = strength * push_speed_factor / (mass * push_mass_factor / 100 + push_speed_factor)
                    // This means push_mass_factor controls how strongly mass resists pull:
                    //   push_mass_factor=100 (Tornado): mass=1 → ~50% pull, mass=20 → ~5% pull
                    //   push_mass_factor=0: mass has no effect (all units pulled equally)
                    //
                    // When push_mass_factor is 0 or not set in data, fall back to the legacy
                    // formula: effective = strength * 4 / (mass + 3), which gives reasonable
                    // behavior for displacement zones without explicit mass factor data.
                    let entity_mass = target.mass.max(1);
                    let effective_strength = if zone.push_mass_factor > 0 {
                        // Data-driven formula: push_mass_factor scales mass resistance.
                        // Higher push_mass_factor → mass matters more → heavy units resist more.
                        // The denominator is: (mass * push_mass_factor / 100) + 1
                        // so at push_mass_factor=100, mass=1 → denom=2, mass=20 → denom=21.
                        let mass_contribution = (entity_mass as i64 * zone.push_mass_factor as i64 / 100).max(0);
                        let denominator = mass_contribution + 1;
                        (zone.strength as i64 / denominator) as i32
                    } else {
                        // Legacy fallback: hardcoded constants for zones without push_mass_factor.
                        (zone.strength as i64 * 4 / (entity_mass as i64 + 3)) as i32
                    };

                    // Don't overshoot — cap movement at remaining distance
                    let pull_x = ((dx as i64 * effective_strength as i64) / dist as i64) as i32;
                    let pull_y = ((dy as i64 * effective_strength as i64) / dist as i64) as i32;

                    displacements.push((idx, pull_x, pull_y));
                }
            }

            // Apply displacements
            for (idx, dx, dy) in displacements {
                if idx < state.entities.len() && state.entities[idx].alive {
                    state.entities[idx].x += dx;
                    state.entities[idx].y += dy;
                    state.entities[idx].x = state.entities[idx].x.clamp(-ARENA_HALF_W, ARENA_HALF_W);
                    state.entities[idx].y = state.entities[idx].y.clamp(-ARENA_HALF_H, ARENA_HALF_H);
                }
            }
        }
    }

    // ── Damage / buff pass (existing logic) ───────────────────────────
    let mut damage_events: Vec<(usize, i32)> = Vec::new();
    let mut tower_damage_events: Vec<(EntityId, i32)> = Vec::new();
    let mut buff_events: Vec<(usize, String, i32, i32, i64, i64)> = Vec::new(); // (entity_idx, buff_key, duration, zone_remaining, level_scale_num, level_scale_den)
    // FIX 1+6: Zone pushback events: (entity_idx, push_dx, push_dy)
    let mut pushback_events: Vec<(usize, i32, i32)> = Vec::new();
    // Track zone IDs that fired pushback this tick (to mark pushback_applied)
    let mut zones_that_pushed: Vec<EntityId> = Vec::new();
    // FIX 1: Heal events from spell zones with heal_per_second (Heal spell).
    let mut heal_events: Vec<(usize, i32)> = Vec::new();

    // Collect spell zone info for zones ready to fire this tick
    struct SpellHit {
        team: Team,
        x: i32,
        y: i32,
        radius: i32,
        damage: i32,
        affects_air: bool,
        affects_ground: bool,
        only_enemies: bool,
        only_own: bool,
        ct_pct: i32,
        buff_key: Option<String>,
        buff_duration: i32,
        /// DOT damage per tick from the buff (for tower damage).
        /// Towers can't hold buffs, so the spell zone applies DOT directly.
        buff_dot_per_tick: i32,
        /// Crown tower damage percent from the buff (e.g., -70 for Poison).
        buff_ct_pct: i32,
        /// Building damage percent from the buff (e.g., 350 for Earthquake).
        buff_building_damage_percent: i32,
        /// Lightning: target N highest-HP entities per hit tick.
        hit_biggest_targets: bool,
        max_hit_targets: i32,
        projectile_damage: i32,
        projectile_ct_pct: i32,
        /// Projectile key for looking up target_buff (Lightning → LighningSpell → ZapFreeze).
        spell_projectile_key: Option<String>,
        /// Zone remaining ticks (for capping buff duration).
        zone_remaining: i32,
        /// If true, this spell/buff deals NO damage or DOT to crown towers.
        /// Data-driven from SpellStats.no_effect_to_crown_towers or
        /// BuffStats.no_effect_to_crown_towers (Earthquake, Tornado, etc.).
        no_effect_to_crown_towers: bool,
        /// FIX 1: Direct heal per hit tick for friendly troops (Heal spell).
        heal_per_hit: i32,
        /// FIX 2: If true, this spell can hit hidden buildings (Tesla).
        affects_hidden: bool,
        /// Pushback distance from zone center on first hit. 0 = no pushback.
        /// Data-driven from SpellStats.pushback.
        pushback: i32,
        /// If true, pushback all enemies in zone.
        pushback_all: bool,
        /// Minimum pushback distance (at edge of radius). 0 = flat pushback.
        /// Data-driven from SpellStats.min_pushback. When both min and max are > 0,
        /// pushback interpolates: center → max_pushback, edge → min_pushback.
        min_pushback: i32,
        /// Maximum pushback distance (at center). 0 = flat pushback.
        /// Data-driven from SpellStats.max_pushback.
        max_pushback: i32,
        /// Entity id of the zone (for marking pushback_applied).
        zone_id: EntityId,
        /// Level scaling ratio from SpellZoneData for buff DOT/heal scaling.
        level_scale_num: i64,
        level_scale_den: i64,
    }

    let spell_hits: Vec<SpellHit> = state
        .entities
        .iter()
        .filter_map(|e| {
            if !e.alive {
                return None;
            }
            if let EntityKind::SpellZone(ref sz) = e.kind {
                if sz.hit_timer <= 0 {
                    // Look up buff DOT stats for tower damage.
                    // Tower DOT is applied once per hit_interval, not per tick, so
                    // we compute damage_per_hit = damage_per_second * hit_interval / 20.
                    // This avoids integer truncation that kills small per-tick values
                    // (e.g., Poison: 57/20=2 per tick, after -70% CT = 0).
                    let hit_interval_ticks = if sz.hit_interval > 0 { sz.hit_interval } else { 1 };
                    let (buff_dot_per_hit, buff_ct, buff_bldg) = sz.buff_key.as_ref()
                        .and_then(|bk| data.buffs.get(bk))
                        .map(|bs| {
                            // FIX 1: Level-scale buff DOT using the zone's baked ratio.
                            // BuffStats.damage_per_second is always the base (level 1) value.
                            // In real CR, Poison/Earthquake DOT scales identically to spell
                            // damage per level. The ratio level_scale_num/level_scale_den
                            // was computed from SpellStats.damage_per_level at zone creation.
                            let base_dot = if bs.damage_per_second > 0 {
                                (bs.damage_per_second as i64 * hit_interval_ticks as i64 / 20) as i32
                            } else {
                                0
                            };
                            let scaled_dot = if base_dot > 0 && sz.level_scale_den > 0 {
                                (base_dot as i64 * sz.level_scale_num / sz.level_scale_den) as i32
                            } else {
                                base_dot
                            };
                            (scaled_dot, bs.crown_tower_damage_percent, bs.building_damage_percent)
                        })
                        .unwrap_or((0, 0, 0));

                    // no_effect_to_crown_towers and affects_hidden are baked into
                    // SpellZoneData at creation time — no runtime GameData lookup needed.

                    Some(SpellHit {
                        team: e.team,
                        x: e.x,
                        y: e.y,
                        radius: sz.radius,
                        damage: sz.damage_per_tick,
                        affects_air: sz.affects_air,
                        affects_ground: sz.affects_ground,
                        only_enemies: sz.only_enemies,
                        only_own: sz.only_own,
                        ct_pct: sz.crown_tower_damage_percent,
                        buff_key: sz.buff_key.clone(),
                        buff_duration: sz.buff_duration,
                        buff_dot_per_tick: buff_dot_per_hit,
                        buff_ct_pct: buff_ct,
                        buff_building_damage_percent: buff_bldg,
                        hit_biggest_targets: sz.hit_biggest_targets,
                        max_hit_targets: sz.max_hit_targets,
                        projectile_damage: sz.projectile_damage,
                        projectile_ct_pct: sz.projectile_ct_pct,
                        spell_projectile_key: sz.spell_projectile_key.clone(),
                        zone_remaining: sz.remaining,
                        no_effect_to_crown_towers: sz.no_effect_to_crown_towers,
                        heal_per_hit: sz.heal_per_hit,
                        affects_hidden: sz.affects_hidden,
                        pushback: if !sz.pushback_applied { sz.pushback } else { 0 },
                        pushback_all: sz.pushback_all,
                        min_pushback: if !sz.pushback_applied { sz.min_pushback } else { 0 },
                        max_pushback: if !sz.pushback_applied { sz.max_pushback } else { 0 },
                        zone_id: e.id,
                        level_scale_num: sz.level_scale_num,
                        level_scale_den: sz.level_scale_den,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    for hit in &spell_hits {
        let radius_sq = (hit.radius as i64) * (hit.radius as i64);

        if hit.hit_biggest_targets && hit.max_hit_targets > 0 {
            // ── Lightning-style targeting: hit N highest-HP targets ──
            // Collect all valid entity targets in radius with their HP
            let mut candidates: Vec<(usize, i32)> = Vec::new(); // (entity_idx, hp)
            for (idx, target) in state.entities.iter().enumerate() {
                // FIX 2: affects_hidden — Lightning can hit hidden Tesla
                let targetable = if hit.affects_hidden {
                    target.alive && target.deploy_timer <= 0
                        && !matches!(target.kind, EntityKind::Projectile(_) | EntityKind::SpellZone(_))
                } else {
                    target.is_targetable()
                };
                if !targetable { continue; }
                if matches!(target.kind, EntityKind::SpellZone(_)) { continue; }
                if hit.only_enemies && target.team == hit.team { continue; }
                if hit.only_own && target.team != hit.team { continue; }
                if target.is_flying() && !hit.affects_air { continue; }
                if !target.is_flying() && !hit.affects_ground { continue; }
                let dist = target.dist_sq_to(hit.x, hit.y);
                if dist <= radius_sq {
                    candidates.push((idx, target.hp));
                }
            }

            // Collect enemy towers in radius as candidates too
            let mut tower_candidates: Vec<(EntityId, i32)> = Vec::new(); // (tower_id, hp)
            if hit.only_enemies || !hit.only_own {
                let enemy_towers = enemy_tower_ids(hit.team);
                for tid in &enemy_towers {
                    if let Some(tpos) = tower_pos(state, *tid) {
                        let dx = (hit.x - tpos.0) as i64;
                        let dy = (hit.y - tpos.1) as i64;
                        if dx * dx + dy * dy <= radius_sq {
                            let hp = tower_hp(state, *tid).unwrap_or(0);
                            if hp > 0 {
                                tower_candidates.push((*tid, hp));
                            }
                        }
                    }
                }
            }

            // Merge candidates: use negative entity index to distinguish towers
            // Sort by HP descending, take top N
            #[derive(Clone)]
            enum HitTarget {
                Entity(usize),
                Tower(EntityId),
            }
            let mut all_candidates: Vec<(HitTarget, i32)> = Vec::new();
            for (idx, hp) in &candidates {
                all_candidates.push((HitTarget::Entity(*idx), *hp));
            }
            for (tid, hp) in &tower_candidates {
                all_candidates.push((HitTarget::Tower(*tid), *hp));
            }
            all_candidates.sort_by(|a, b| b.1.cmp(&a.1));

            let proj_dmg = hit.projectile_damage;
            let proj_ct = hit.projectile_ct_pct;
            for (target, _hp) in all_candidates.iter().take(hit.max_hit_targets as usize) {
                match target {
                    HitTarget::Entity(idx) => {
                        damage_events.push((*idx, proj_dmg));
                    }
                    HitTarget::Tower(tid) => {
                        let tower_dmg = apply_ct_reduction(proj_dmg, proj_ct);
                        tower_damage_events.push((*tid, tower_dmg));
                    }
                }
            }

            // Also apply buff to all entities in radius (if any)
            if let Some(ref bk) = hit.buff_key {
                if !bk.is_empty() {
                    for (idx, _hp) in &candidates {
                        buff_events.push((*idx, bk.clone(), hit.buff_duration, hit.zone_remaining, hit.level_scale_num, hit.level_scale_den));
                    }
                }
            }

            // Lightning fix: spell has buff_key=None but projectile (LighningSpell) has
            // target_buff=ZapFreeze with buff_time=500ms. Apply the projectile's target_buff
            // to each struck target (top-N only, not all in radius).
            if hit.buff_key.is_none() && hit.hit_biggest_targets {
                if let Some(ref pk) = hit.spell_projectile_key {
                    if let Some(ps) = data.projectiles.get(pk.as_str()) {
                        if let Some(ref tb) = ps.target_buff {
                            let tb_time = crate::entities::ms_to_ticks(ps.buff_time);
                            if tb_time > 0 {
                                for (target, _) in all_candidates.iter().take(hit.max_hit_targets as usize) {
                                    if let HitTarget::Entity(idx) = target {
                                        buff_events.push((*idx, tb.clone(), tb_time, hit.zone_remaining, hit.level_scale_num, hit.level_scale_den));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // ── Standard zone damage: hit ALL entities in radius ──
            for (idx, target) in state.entities.iter().enumerate() {
                // FIX 2: affects_hidden — if the spell can hit hidden buildings,
                // use a relaxed targetable check that skips the hidden-building filter.
                // Standard is_targetable() returns false for hidden Tesla, but spells
                // with affects_hidden=true (Earthquake, Lightning) should still hit them.
                let targetable = if hit.affects_hidden {
                    // Relaxed check: alive, deployed, not a projectile/spell zone.
                    // Hidden buildings ARE targetable by this spell.
                    target.alive && target.deploy_timer <= 0
                        && !matches!(target.kind, EntityKind::Projectile(_) | EntityKind::SpellZone(_))
                } else {
                    target.is_targetable()
                };
                if !targetable {
                    continue;
                }
                if matches!(target.kind, EntityKind::SpellZone(_)) {
                    continue;
                }
                if hit.only_enemies && target.team == hit.team {
                    continue;
                }
                if hit.only_own && target.team != hit.team {
                    continue;
                }
                if target.is_flying() && !hit.affects_air {
                    continue;
                }
                if !target.is_flying() && !hit.affects_ground {
                    continue;
                }
                let dist = target.dist_sq_to(hit.x, hit.y);
                if dist <= radius_sq {
                    if hit.damage > 0 {
                        damage_events.push((idx, hit.damage));
                    }
                    // FIX 1: Apply direct heal to friendly troops within the zone.
                    // The Heal spell has heal_per_hit > 0 and only_own=true.
                    if hit.heal_per_hit > 0 && target.team == hit.team {
                        // Heal events: (entity_idx, heal_amount)
                        // We reuse damage_events with negative values? No — collect separately.
                        // Use a simple direct heal approach: we'll apply after damage.
                        heal_events.push((idx, hit.heal_per_hit));
                    }
                    if let Some(ref bk) = hit.buff_key {
                        if !bk.is_empty() {
                            buff_events.push((idx, bk.clone(), hit.buff_duration, hit.zone_remaining, hit.level_scale_num, hit.level_scale_den));
                        }
                    }
                    // FIX 1+6: Zone pushback (Zap knockback on first hit).
                    // Distance-scaled: if min_pushback and max_pushback are both > 0,
                    // interpolate linearly from zone center to edge, matching real CR
                    // behavior and the projectile pushback path (tick_projectiles).
                    // Data-driven from SpellStats.pushback / min_pushback / max_pushback.
                    if hit.pushback > 0 && target.team != hit.team {
                        // Push radially away from zone center
                        let pdx = target.x - hit.x;
                        let pdy = target.y - hit.y;
                        let pdist = ((pdx as f64).powi(2) + (pdy as f64).powi(2)).sqrt();
                        if pdist > 1.0 {
                            // Distance-based scaling: if min_pushback and max_pushback
                            // are set, interpolate linearly:
                            //   center(0) → max_pushback, edge(radius) → min_pushback.
                            // Otherwise fall back to flat pushback distance.
                            let effective_pushback = if hit.min_pushback > 0 && hit.max_pushback > 0 && hit.radius > 0 {
                                let dist_i = pdist as i64;
                                let radius_i = hit.radius as i64;
                                let ratio = dist_i.min(radius_i) * 1000 / radius_i;
                                let pb = hit.max_pushback as i64
                                    - (ratio * (hit.max_pushback as i64 - hit.min_pushback as i64) / 1000);
                                pb.max(hit.min_pushback as i64) as i32
                            } else {
                                hit.pushback
                            };
                            let push_x = (pdx as f64 / pdist * effective_pushback as f64) as i32;
                            let push_y = (pdy as f64 / pdist * effective_pushback as f64) as i32;
                            // Check pushback immunity
                            let immune = match &target.kind {
                                EntityKind::Troop(t) => t.ignore_pushback,
                                _ => target.is_building(),
                            };
                            if !immune {
                                pushback_events.push((idx, push_x, push_y));
                            }
                        }
                        zones_that_pushed.push(hit.zone_id);
                    }
                }
            }
        }

        // Hit enemy towers (direct damage + buff DOT since towers can't hold buffs)
        // Skip entirely if no_effect_to_crown_towers is true (Earthquake, some buff DOTs).
        if !hit.no_effect_to_crown_towers && (hit.only_enemies || !hit.only_own) {
            let enemy_towers = enemy_tower_ids(hit.team);
            for tid in &enemy_towers {
                if let Some(tpos) = tower_pos(state, *tid) {
                    let dx = (hit.x - tpos.0) as i64;
                    let dy = (hit.y - tpos.1) as i64;
                    if dx * dx + dy * dy <= radius_sq {
                        // Direct spell damage (e.g., Zap instant)
                        if hit.damage > 0 {
                            let tower_dmg = apply_ct_reduction(hit.damage, hit.ct_pct);
                            tower_damage_events.push((*tid, tower_dmg));
                        }
                        // Buff-based DOT damage applied directly to towers since
                        // towers can't hold ActiveBuff objects. Poison & Earthquake
                        // deal their damage via damage_per_second in BuffStats.
                        // Crown towers are buildings so Earthquake's building_damage_percent
                        // applies, but crown_tower_damage_percent takes precedence for
                        // crown tower reduction (both Poison -70% and Earthquake -35%).
                        if hit.buff_dot_per_tick > 0 {
                            let mut dot = hit.buff_dot_per_tick;
                            // Apply building damage bonus (Earthquake 350%)
                            if hit.buff_building_damage_percent > 0 {
                                dot = (dot as i64 * hit.buff_building_damage_percent as i64 / 100) as i32;
                            }
                            // Apply crown tower damage reduction.
                            // Use ceiling division to avoid truncating small DOT to 0.
                            // Without this, Tornado (dot=5, -83% → 5*17/100=0) deals
                            // zero tower damage due to integer truncation.
                            let tower_dot = if hit.buff_ct_pct != 0 {
                                let pct = if hit.buff_ct_pct < 0 {
                                    (100 + hit.buff_ct_pct).max(0) as i64
                                } else if hit.buff_ct_pct < 100 {
                                    hit.buff_ct_pct as i64
                                } else {
                                    100i64
                                };
                                // Ceiling: (dot * pct + 99) / 100 — ensures ≥1 when dot*pct > 0
                                ((dot as i64 * pct + 99) / 100) as i32
                            } else {
                                dot
                            };
                            if tower_dot > 0 {
                                tower_damage_events.push((*tid, tower_dot));
                            }
                        }
                    }
                }
            }
        }
    }

    // Apply entity damage
    for (idx, dmg) in damage_events {
        if idx < state.entities.len() && state.entities[idx].alive {
            apply_damage_to_entity(&mut state.entities[idx], dmg);
        }
    }

    // FIX 1: Apply heal from spell zones with heal_per_second (Heal spell).
    // Heals friendly troops within the zone radius each hit interval.
    for (idx, heal_amount) in heal_events {
        if idx < state.entities.len() && state.entities[idx].alive {
            let e = &mut state.entities[idx];
            e.hp = (e.hp + heal_amount).min(e.max_hp);
        }
    }

    // Apply tower damage
    for (tid, dmg) in tower_damage_events {
        apply_damage_to_tower(state, tid, dmg);
    }

    // FIX 1+6: Apply zone pushback (Zap knockback on first hit).
    // Pushback displaces entities radially away from the zone center.
    // Only fires once per zone (first hit tick), then marked as applied.
    for (idx, push_x, push_y) in pushback_events {
        if idx < state.entities.len() && state.entities[idx].alive {
            state.entities[idx].x += push_x;
            state.entities[idx].y += push_y;
        }
    }
    // Mark zones that fired pushback so they don't fire again
    for zone_id in zones_that_pushed {
        if let Some(zone) = state.entities.iter_mut().find(|e| e.id == zone_id) {
            if let EntityKind::SpellZone(ref mut sz) = zone.kind {
                sz.pushback_applied = true;
            }
        }
    }

    // Apply buffs to entities using GameData buff stats
    // If entity already has an active buff with the same key, refresh duration
    // instead of stacking duplicates.
    //
    // Clone special case: if the buff has clone=true, duplicate the entity
    // with 1 HP instead of applying stat modifiers.
    let mut clone_spawns: Vec<(Team, String, i32, i32, usize, i32, i32)> = Vec::new();
    // (team, card_key, x, y, level, damage, max_hp — for cloned entity)

    for (idx, buff_key, duration, zone_remaining, ls_num, ls_den) in buff_events {
        if idx < state.entities.len() && state.entities[idx].alive {
            use crate::entities::ActiveBuff;

            // Check if this is a Clone buff
            let is_clone = data.buffs.get(&buff_key).map(|b| b.clone).unwrap_or(false);

            if is_clone {
                // Clone: duplicate this entity with 1 HP
                let e = &state.entities[idx];
                if e.is_troop() {
                    if let EntityKind::Troop(ref t) = e.kind {
                        clone_spawns.push((
                            e.team,
                            e.card_key.clone(),
                            e.x,
                            e.y,
                            t.level,
                            e.damage,
                            e.max_hp,
                        ));
                    }
                }
                continue; // Don't apply the buff as a stat modifier
            }

            // ─── Fix #8: enable_stacking (data-driven) ───
            // If enable_stacking is true for this buff (Poison, Tornado, Heal, etc.),
            // always push a new instance instead of refreshing the existing one.
            let bs = data.buffs.get(&buff_key);
            let should_stack = bs.map(|b| b.enable_stacking).unwrap_or(false);

            // Check if this entity already has this buff active
            if !should_stack {
                let existing = state.entities[idx].buffs.iter_mut()
                    .find(|b| b.key == buff_key && !b.is_expired());
                if let Some(existing_buff) = existing {
                    // Refresh duration. Only cap for long DOT zones (Poison) where
                    // buff should track zone lifetime. For instant spells (Zap, Freeze),
                    // the buff outlives the zone by design — don't cap.
                    let capped = if zone_remaining < duration {
                        duration.min(zone_remaining.max(1) + 1)
                    } else {
                        duration
                    };
                    existing_buff.remaining_ticks = capped;
                    continue;
                }
            }

            // Build the buff data-driven via from_buff_stats(), then override
            // heal/DOT with level-scaled values. from_buff_stats() handles all
            // field wiring (stun/freeze detection, death_spawn, building_damage_percent,
            // invisible, etc.) — no hardcoded heuristics like "contains Zap".
            let mut buff = if let Some(b) = bs {
                ActiveBuff::from_buff_stats(buff_key, duration, b)
            } else {
                // No BuffStats entry found — create a minimal buff with just the key.
                // This shouldn't happen in practice (buff_key comes from spell data).
                ActiveBuff::from_buff_stats(buff_key, duration, &crate::data_types::BuffStats {
                    name: String::new(), rarity: String::new(),
                    speed_multiplier: 0, hit_speed_multiplier: 0,
                    damage_per_second: 0, heal_per_second: 0,
                    damage_reduction: 0, spawn_speed_multiplier: 0,
                    no_effect_to_crown_towers: false, building_damage_percent: 0,
                    crown_tower_damage_percent: 0, hit_frequency: 0,
                    attract_percentage: 0, push_speed_factor: 0,
                    push_mass_factor: 0, controlled_by_parent: false,
                    clone: false, invisible: false, enable_stacking: false,
                    remove_on_attack: false, allowed_over_heal_perc: 0,
                    death_spawn: None, death_spawn_count: 0,
                    death_spawn_is_enemy: false, ignore_buildings: false,
                    damage_multiplier: 0, hitpoint_multiplier: 0,
                })
            };

            // FIX 1+2: Level-scale heal and DOT from BuffStats using the zone's
            // baked level_scale ratio. BuffStats values are always base (level 1);
            // in real CR they scale identically to spell damage per level.
            // Override the from_buff_stats() computed values with level-scaled ones.
            if let Some(b) = bs {
                if b.heal_per_second > 0 {
                    let base = b.heal_per_second / 20;
                    buff.heal_per_tick = if ls_den > 0 { (base as i64 * ls_num / ls_den) as i32 } else { base };
                }
                if b.damage_per_second > 0 && b.hit_frequency > 0 {
                    let base_per_hit = (b.damage_per_second as i64 * b.hit_frequency as i64 / 1000) as i32;
                    let scaled = if ls_den > 0 { (base_per_hit as i64 * ls_num / ls_den) as i32 } else { base_per_hit };
                    let interval_ticks = (b.hit_frequency * 20 + 999) / 1000;
                    buff.damage_per_tick = scaled;
                    buff.damage_hit_interval = interval_ticks;
                    buff.damage_hit_timer = interval_ticks.max(1);
                } else if b.damage_per_second > 0 {
                    let base = b.damage_per_second / 20;
                    buff.damage_per_tick = if ls_den > 0 { (base as i64 * ls_num / ls_den) as i32 } else { base };
                }
            }

            // Override stacking behavior from zone context (Clone spell).
            buff.enable_stacking = should_stack;

            state.entities[idx].buffs.push(buff);
        }
    }

    // Spawn cloned troops (Clone spell)
    for (team, card_key, x, y, level, damage, _max_hp) in clone_spawns {
        if let Some(stats) = data.characters.get(&card_key) {
            let id = state.alloc_id();
            let mut cloned = Entity::new_troop(id, team, stats, x + 100, y, level, false);
            // Clone has 1 HP but retains damage
            cloned.hp = 1;
            cloned.max_hp = 1;
            cloned.shield_hp = 0;
            cloned.deploy_timer = 0; // Deploys instantly
            state.entities.push(cloned);
        }
    }

    // ─── Spawner spell zones (Graveyard) ───
    // Collect spawn events first, then apply (avoids borrow conflict).
    // Each entry: (team, character_key, zone_x, zone_y, radius, level, spawn_min_radius)
    let mut zone_spawns: Vec<(Team, String, i32, i32, i32, usize, i32)> = Vec::new();

    for entity in state.entities.iter_mut() {
        if !entity.alive {
            continue;
        }
        if let EntityKind::SpellZone(ref mut sz) = entity.kind {
            // Tick spawn timer for spawner zones (Graveyard)
            if let Some(ref spawn_key) = sz.spawn_character {
                if sz.spawn_interval > 0 {
                    sz.spawn_timer -= 1;
                    if sz.spawn_timer <= 0 {
                        sz.spawn_timer = sz.spawn_interval;
                        zone_spawns.push((
                            sz.spawn_team,
                            spawn_key.clone(),
                            entity.x,
                            entity.y,
                            sz.radius,
                            sz.spawn_level,
                            sz.spawn_min_radius,
                        ));
                    }
                }
            }
        }
    }

    // Spawn troops from zone spawners
    for (team, key, zx, zy, radius, level, min_radius) in zone_spawns {
        let lookup_key = key.to_lowercase().replace(' ', "-");
        let stats_opt = data.characters.get(&lookup_key)
            .or_else(|| data.characters.get(&key));
        if let Some(stats) = stats_opt {
            let id = state.alloc_id();
            // ─── Circular ring placement (data-driven) ───
            // Deterministic pseudo-random position within the spawn ring.
            // If spawn_min_radius > 0 (Graveyard=3000), spawns in a ring
            // between min_radius and radius. Otherwise full circle.
            //
            // Uses a multiplicative hash of (tick + entity_id) for deterministic
            // but well-scattered positions across the ring. Each spawn gets a
            // unique seed so consecutive spawns don't cluster.
            let tick = state.tick;
            let seed = (tick as u64)
                .wrapping_mul(2654435761)  // Knuth multiplicative hash
                .wrapping_add(id.0 as u64 * 6364136223846793005);

            // Angle: full 360° circle, quantized to 0.01° steps
            let angle_steps = (seed % 36000) as f64;
            let angle = angle_steps * std::f64::consts::PI / 18000.0;

            // Radius: between min_radius and radius (ring placement)
            let min_r = min_radius.max(0) as f64;
            let max_r = radius.max(1) as f64;
            let r_frac = ((seed / 36000) % 10000) as f64 / 10000.0;
            let r = if min_r > 0.0 && min_r < max_r {
                // Ring: interpolate between min and max radius
                min_r + r_frac * (max_r - min_r)
            } else {
                // Full circle: use sqrt for uniform area distribution
                r_frac.sqrt() * max_r
            };

            let offset_x = (angle.cos() * r) as i32;
            let offset_y = (angle.sin() * r) as i32;
            let sx = zx + offset_x;
            let sy = zy + offset_y;
            let mut troop = Entity::new_troop(id, team, stats, sx, sy, level, false);
            troop.deploy_timer = 0; // Graveyard skeletons deploy instantly
            state.entities.push(troop);
        }
    }

    // Tick spell zone timers
    for entity in state.entities.iter_mut() {
        if let EntityKind::SpellZone(ref mut sz) = entity.kind {
            if sz.hit_timer <= 0 {
                sz.hit_timer = sz.hit_interval;
            }
            sz.hit_timer -= 1;
            sz.remaining -= 1;
            if sz.remaining <= 0 {
                entity.alive = false;
            }
        }
    }
}
// =========================================================================
// Fix #4: Morph system — Cannon Cart shield break → stationary cannon
// =========================================================================

/// Check for troops whose shield just broke and that have morph_character set.
/// When detected, the troop entity is killed and replaced with the morph target
/// (a building or different character) at the same position.
///
/// Cannon Cart (MovingCannon): shield breaks → morphs into stationary cannon.
/// heal_on_morph=true → the morphed entity spawns at full HP.
/// morph_time → deploy timer on the morphed entity (transition animation).
///
/// Called each tick from engine.rs after combat resolution.
pub fn tick_morphs(state: &mut GameState, data: &GameData) {
    // Collect morph events: (entity_idx, morph_key, x, y, team, level, heal, morph_time, source_hp)
    // source_hp: the HP of the original troop at the moment of morph. When heal_on_morph
    // is false, the morphed entity inherits this HP (capped at its own max_hp).
    // When heal_on_morph is true, the morphed entity starts at full HP (Cannon Cart).
    let mut morph_events: Vec<(usize, String, i32, i32, crate::entities::Team, usize, bool, i32, i32)> = Vec::new();

    for (idx, entity) in state.entities.iter().enumerate() {
        if !entity.alive {
            continue;
        }
        if let EntityKind::Troop(ref t) = entity.kind {
            if let Some(ref morph_key) = t.morph_character {
                // Shield-break morph: trigger when the troop originally had a shield
                // (from CharacterStats) and that shield is now depleted. We look up
                // the original shield_hitpoints from GameData to avoid false positives
                // on troops that never had a shield (shield_hp starts at 0).
                // Data-driven: CharacterStats.shield_hitpoints > 0 means "had a shield".
                let originally_had_shield = data.characters.get(&entity.card_key)
                    .or_else(|| data.find_character(&entity.card_key))
                    .map(|s| s.shield_hitpoints > 0)
                    .unwrap_or(false);

                let shield_broken = originally_had_shield && entity.shield_hp <= 0;

                if shield_broken {
                    morph_events.push((
                        idx,
                        morph_key.clone(),
                        entity.x,
                        entity.y,
                        entity.team,
                        t.level,
                        t.morph_heal,
                        t.morph_time,
                        entity.hp, // Fix D: capture source HP for inheritance
                    ));
                }
            }
        }
    }

    // Apply morphs: kill the original troop, spawn the morph target
    for (idx, morph_key, x, y, team, level, heal, morph_time, source_hp) in morph_events {
        // Kill the original entity
        if idx < state.entities.len() {
            state.entities[idx].alive = false;
            state.entities[idx].hp = 0;
        }

        // Try to spawn as a building first (Cannon Cart → stationary cannon),
        // then fall back to character (for future morph-to-troop cases).
        let spawned = if let Some(bld_stats) = data.buildings.get(&morph_key)
            .or_else(|| {
                let lower = morph_key.to_lowercase().replace(' ', "-");
                data.buildings.get(&lower)
            })
            .or_else(|| {
                // Name-based fallback: search buildings by name
                data.buildings.values().find(|b| {
                    b.name.eq_ignore_ascii_case(&morph_key)
                        || b.key.eq_ignore_ascii_case(&morph_key)
                })
            })
        {
            let id = state.alloc_id();
            let mut building = Entity::new_building(id, team, bld_stats, x, y, level);
            if heal {
                // heal_on_morph=true: morphed entity starts at full HP
                building.hp = building.max_hp;
            } else {
                // Fix D: Inherit source troop's remaining HP (capped at morph target's max).
                // In real CR, BrokenCannon retains Cannon Cart's remaining HP, not the
                // building's base HP. When heal_on_morph is false, the morphed entity
                // receives the source troop's HP at the moment of morph.
                building.hp = source_hp.min(building.max_hp).max(1);
            }
            // morph_time: deploy timer for the transition animation
            if morph_time > 0 {
                building.deploy_timer = morph_time;
            }
            state.entities.push(building);
            true
        } else if let Some(char_stats) = data.find_character(&morph_key) {
            let id = state.alloc_id();
            let mut troop = Entity::new_troop(id, team, char_stats, x, y, level, false);
            if heal {
                troop.hp = troop.max_hp;
            } else {
                // Fix D: Inherit source HP for troop morphs too
                troop.hp = source_hp.min(troop.max_hp).max(1);
            }
            if morph_time > 0 {
                troop.deploy_timer = morph_time;
            }
            state.entities.push(troop);
            true
        } else {
            false
        };

        if !spawned {
            // Morph target not found — leave the entity dead (shouldn't happen with valid data)
        }
    }
}
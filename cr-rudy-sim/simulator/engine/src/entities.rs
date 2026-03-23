//! Game entities — Troop, Building, Projectile, Spell
//!
//! Flat Vec<Entity> layout for cache-friendly iteration.

// TODO: Phase 1
//! Entity types for the simulation: troops, buildings, projectiles, spell zones.
//!
//! All entities live in a flat `Vec<Entity>` inside `GameState` for cache-friendly
//! iteration. No heap-allocated trait objects or pointer chasing.
//!
//! Coordinate system matches `game_state.rs` — integer units, Y+ = player 2 side.

use crate::data_types::CharacterStats;

// =========================================================================
// Active Buff — runtime buff state on an entity
// =========================================================================

/// A buff/debuff currently active on an entity.
#[derive(Debug, Clone)]
pub struct ActiveBuff {
    /// Identifies the buff source (e.g., "evo_barbarians_rage", "hero_knight_taunt").
    pub key: String,
    /// Ticks remaining. <=0 means expired. i32::MAX = permanent (while condition holds).
    pub remaining_ticks: i32,
    /// Speed multiplier delta in percent (e.g., +30 or -30). Stacks additively.
    pub speed_percent: i32,
    /// Hit speed multiplier delta in percent (e.g., +30 means 30% faster attacks).
    pub hitspeed_percent: i32,
    /// Damage multiplier delta in percent (e.g., +50 means 50% more damage).
    pub damage_percent: i32,
    /// Damage reduction in percent (e.g., 50 means take 50% less damage).
    pub damage_reduction: i32,
    /// Heal per tick (fixed-point, actual HP per tick).
    pub heal_per_tick: i32,
    /// Damage per tick (e.g., Poison DOT). Applied each tick the buff is active
    /// (if damage_hit_interval == 0) or every N ticks (if damage_hit_interval > 0).
    pub damage_per_tick: i32,
    /// Tick interval for pulsed damage (Poison, Earthquake). 0 = every tick (legacy).
    /// When > 0, damage_per_tick is only applied when damage_hit_timer counts to 0.
    pub damage_hit_interval: i32,
    /// Countdown for next damage pulse. Decremented each tick; fires at 0 then resets.
    pub damage_hit_timer: i32,
    /// Building damage multiplier percent (e.g., 350 = deal 3.5× to buildings).
    /// Earthquake uses this. 0 means no building bonus.
    pub building_damage_percent: i32,
    /// Crown tower damage percent for DOT (e.g., -70 = reduce by 70%).
    /// Used for Poison/Earthquake DOT applied to towers via spell zones.
    pub crown_tower_damage_percent: i32,
    /// Is this entity stunned (cannot move or attack)?
    pub stun: bool,
    /// Is this entity frozen (cannot move or attack)?
    pub freeze: bool,
    /// Is this entity invisible (untargetable)? Used by ArcherQueenRapid buff.
    pub invisible: bool,
    /// Forced target override (taunt). If Some, this entity must target this ID.
    pub taunt_target: Option<EntityId>,
    /// Character key to spawn when the buffed entity dies (e.g., "VoodooHog" from VoodooCurse).
    pub death_spawn: Option<String>,
    /// Number of units to spawn on death (default 1).
    pub death_spawn_count: i32,
    /// If true, the death spawn fights for the ENEMY of the dying unit
    /// (i.e., the team that applied the curse). Mother Witch's cursed hogs
    /// fight for Mother Witch's team, not the dying troop's team.
    pub death_spawn_is_enemy: bool,
    /// If true, this buff is removed when the entity attacks.
    /// Data-driven from BuffStats.remove_on_attack.
    pub remove_on_attack: bool,
    /// Spawn speed multiplier (e.g., 135 = spawner runs at 135% speed → shorter intervals).
    /// Data-driven from BuffStats.spawn_speed_multiplier. 0 = no effect.
    pub spawn_speed_multiplier: i32,
    /// If true, multiple instances of this buff stack instead of refreshing duration.
    /// Data-driven from BuffStats.enable_stacking.
    pub enable_stacking: bool,
    /// Allowed overheal percentage above max_hp. 0 = no overheal (heal caps at max_hp).
    /// Data-driven from BuffStats.allowed_over_heal_perc.
    /// e.g., 100 means heal can bring HP up to 200% of max_hp.
    pub allowed_over_heal_perc: i32,
    /// Hitpoint multiplier from BuffStats.hitpoint_multiplier.
    /// GrowthBoost=120 means scale max_hp to 120%. Applied once when the
    /// buff is first added (in Entity::add_buff). 0 = no HP modification.
    /// Both max_hp and current hp are scaled proportionally so the entity
    /// doesn't lose effective HP percentage.
    pub hp_multiplier: i32,
}

impl ActiveBuff {
    pub fn is_expired(&self) -> bool {
        self.remaining_ticks <= 0
    }

    /// Data-driven factory: create an `ActiveBuff` from `BuffStats` data.
    ///
    /// This is the **single authoritative path** for converting a `BuffStats` entry
    /// into a runtime `ActiveBuff`. Every field on `BuffStats` is wired here, so
    /// adding a new field to `BuffStats` only requires updating this one function.
    ///
    /// Previously, `ActiveBuff` was constructed manually at ~15 call sites in combat.rs,
    /// each hardcoding `building_damage_percent: 0`, `death_spawn: None`, etc. This meant
    /// that buffs applied via `buff_on_kill`, `starting_buff`, reflect, etc. silently
    /// dropped data like Earthquake's building multiplier or VoodooCurse's death_spawn.
    ///
    /// `duration_ticks`: how long the buff lasts (caller-provided, since BuffStats doesn't
    ///   store duration — it comes from the applying mechanic, e.g., buff_on_kill_time).
    /// `bs`: the BuffStats data from GameData.buffs.
    pub fn from_buff_stats(key: String, duration_ticks: i32, bs: &crate::data_types::BuffStats) -> Self {

        // Negative values are already deltas (e.g., -15 means -15%).
        // Positive values are absolute (e.g., 135 means 135% → delta = +35).
        // Speed/hitspeed multipliers in BuffStats are absolute (100 = no change, 130 = +30%).

        // Zero means "no modifier."
        let speed_pct = if bs.speed_multiplier > 0 { bs.speed_multiplier - 100 }
                        else { bs.speed_multiplier }; // negative or zero: use as-is
        let hitspeed_pct = if bs.hit_speed_multiplier > 0 { bs.hit_speed_multiplier - 100 }
                        else { bs.hit_speed_multiplier }; // negative or zero: use as-is

        // Stun: both speed and hitspeed reduced to zero or below (delta ≤ -100).
        // In CR data, ZapFreeze has speed_multiplier=0 and hit_speed_multiplier=0,
        // which converts to delta = -100 for both → stun.
        // Freeze: hitspeed zeroed but speed not fully zeroed (slow + can't attack).
        // Data-driven from the multiplier values — no hardcoded buff key checks.
        let is_stun = speed_pct <= -100 && hitspeed_pct <= -100;
        let is_freeze = !is_stun && hitspeed_pct <= -100;

        // Heal: pulsed if hit_frequency > 0, otherwise per-tick.
        // BattleHealerSelf: heal_per_second=16, hit_frequency=500ms → 8 HP every 10 ticks.
        let (heal_per_pulse, heal_interval) = if bs.heal_per_second > 0 && bs.hit_frequency > 0 {
            let per_pulse = (bs.heal_per_second as i64 * bs.hit_frequency as i64 / 1000) as i32;
            let interval = (bs.hit_frequency * 20 + 999) / 1000; // ms → ticks, round up
            (per_pulse.max(1), interval)
        } else if bs.heal_per_second > 0 {
            (bs.heal_per_second / 20, 0) // Legacy: per-tick heal
        } else {
            (0, 0)
        };

        // DOT: pulsed if hit_frequency > 0, otherwise per-tick.
        let (dot_per_pulse, dot_interval) = if bs.damage_per_second > 0 && bs.hit_frequency > 0 {
            let per_pulse = (bs.damage_per_second as i64 * bs.hit_frequency as i64 / 1000) as i32;
            let interval = (bs.hit_frequency * 20 + 999) / 1000;
            (per_pulse.max(1), interval)
        } else if bs.damage_per_second > 0 {
            (bs.damage_per_second / 20, 0)
        } else {
            (0, 0)
        };

        // Use the longer of heal_interval and dot_interval as the shared pulse timer.
        // In practice, a single buff won't have both heal and DOT, but if it did,
        // the shared timer handles it correctly (tick_buffs fires both on the same pulse).
        let combined_interval = heal_interval.max(dot_interval);

        ActiveBuff {
            key,
            remaining_ticks: duration_ticks,
            speed_percent: speed_pct,
            hitspeed_percent: hitspeed_pct,
            // Fix #7: Data-driven damage multiplier from BuffStats.damage_multiplier.
            // TripleDamage=300 (3× damage), GrowthBoost=120 (1.2× damage).
            // Value is absolute (100 = no change), stored as delta (300 → +200%).
            // 0 in the data means "not set" → delta = 0.
            damage_percent: if bs.damage_multiplier != 0 { bs.damage_multiplier - 100 } else { 0 },
            damage_reduction: bs.damage_reduction,
            heal_per_tick: heal_per_pulse,
            damage_per_tick: dot_per_pulse,
            damage_hit_interval: combined_interval,
            damage_hit_timer: combined_interval.max(1),
            building_damage_percent: bs.building_damage_percent,
            crown_tower_damage_percent: bs.crown_tower_damage_percent,
            stun: is_stun,
            freeze: is_freeze,
            invisible: bs.invisible,
            taunt_target: None, // Taunt is set by the caller (hero system), not from BuffStats
            death_spawn: bs.death_spawn.clone(),
            death_spawn_count: if bs.death_spawn_count > 0 {
                bs.death_spawn_count
            } else if bs.death_spawn.is_some() {
                1 // Normalize: if death_spawn is set, count defaults to 1
            } else {
                0
            },
            death_spawn_is_enemy: bs.death_spawn_is_enemy,
            remove_on_attack: bs.remove_on_attack,
            spawn_speed_multiplier: bs.spawn_speed_multiplier,
            enable_stacking: bs.enable_stacking,
            allowed_over_heal_perc: bs.allowed_over_heal_perc,
            // Fix #8: Data-driven hitpoint multiplier from BuffStats.hitpoint_multiplier.
            // GrowthBoost=120 (1.2× max HP). 0 = not set → no HP change.
            // Applied once when buff is added: scales both max_hp and hp proportionally.
            hp_multiplier: bs.hitpoint_multiplier,
        }
    }
}

// =========================================================================
// EvoState — per-entity evolution ability runtime state
// =========================================================================

/// Tracks evo ability runtime state for an evolved troop.
#[derive(Debug, Clone, Default)]
pub struct EvoState {
    /// Cooldown ticks remaining for the evo ability (0 = ready).
    pub cooldown: i32,
    /// Stack count for stackable abilities.
    pub stacks: i32,
    /// Whether the on_deploy ability has already fired.
    pub deploy_fired: bool,
    /// Whether the on_death ability has already fired.
    pub death_fired: bool,
    /// Number of kills scored (for on_kill triggers).
    pub kill_count: i32,
    /// Number of attacks performed (for attack-count gating).
    pub attack_count: i32,
    /// Whether the troop has been hit at least once (for after_first_hit).
    pub been_hit: bool,
    /// Whether shield was destroyed (for shield_destroyed triggers).
    pub shield_destroyed: bool,
    /// Distance traveled in internal units (for distance-based triggers).
    pub distance_traveled: i32,
    /// Previous position for distance tracking.
    pub prev_x: i32,
    pub prev_y: i32,
}

// =========================================================================
// HeroState — per-entity hero ability runtime state
// =========================================================================

/// Tracks hero ability runtime state.
#[derive(Debug, Clone, Default)]
pub struct HeroState {
    /// Whether this entity is a hero (has hero ability).
    pub is_hero: bool,
    /// Base card key for hero lookup.
    pub hero_key: String,
    /// Whether the ability has been activated this deployment.
    pub ability_active: bool,
    /// Remaining duration of active ability effect (ticks).
    pub ability_remaining: i32,
    /// Whether the hero is currently flying (Wizard hero).
    pub is_flying_override: bool,
}

// =========================================================================
// Team
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Team {
    Player1,
    Player2,
}

impl Team {
    /// The opponent team.
    pub fn opponent(self) -> Team {
        match self {
            Team::Player1 => Team::Player2,
            Team::Player2 => Team::Player1,
        }
    }

    /// Movement direction on the Y axis (+1 = towards P2, -1 = towards P1).
    pub fn forward_y(self) -> i32 {
        match self {
            Team::Player1 => 1,
            Team::Player2 => -1,
        }
    }
}

// =========================================================================
// EntityId
// =========================================================================

/// Unique identifier for an entity within a match (monotonically increasing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub u32);

// =========================================================================
// EntityKind — discriminant for entity-specific data
// =========================================================================

#[derive(Debug, Clone)]
pub enum EntityKind {
    Troop(TroopData),
    Building(BuildingData),
    Projectile(ProjectileData),
    SpellZone(SpellZoneData),
}

// =========================================================================
// Entity — the unified struct stored in the flat pool
// =========================================================================

/// A single entity on the arena. All fields that are common across entity types
/// live directly on `Entity`; type-specific data is in the `kind` enum.
#[derive(Debug, Clone)]
pub struct Entity {
    // Identity
    pub id: EntityId,
    pub team: Team,
    /// Card key (e.g., "Knight", "Fireball") — used to look up stats in GameData.
    pub card_key: String,

    // Spatial
    pub x: i32,
    pub y: i32,
    /// For flying units, z > 0; ground units z = 0.
    pub z: i32,
    pub collision_radius: i32,
    /// Mass (from CharacterStats). Affects Tornado pull resistance.
    /// Heavier units (Golem=20) are pulled less than light ones (Skeleton=1).
    pub mass: i32,

    // Combat — copied from stats at spawn time (can be modified by buffs)
    pub hp: i32,
    pub max_hp: i32,
    pub shield_hp: i32,
    pub damage: i32,

    // Status
    pub alive: bool,
    /// Deploy timer: entity is inert until this reaches 0.
    pub deploy_timer: i32,
    /// Current target entity ID (if any).
    pub target: Option<EntityId>,
    /// Parent entity this troop is attached to (rider mechanic).
    /// Attached troops follow their parent's position and die when parent dies.
    /// Used by: SpearGoblinGiant (rides on GoblinGiant).
    pub attached_to: Option<EntityId>,

    // Type-specific payload
    pub kind: EntityKind,

    // === Phase 3: Buff system ===
    /// Active buffs/debuffs on this entity.
    pub buffs: Vec<ActiveBuff>,
    /// Buff key this entity is immune to (data-driven from CharacterStats.ignore_buff).
    /// When a buff with this key is applied, it is silently rejected.
    /// Examples: Golem/LavaHound/BattleRam immune to "VoodooCurse" (Mother Witch).
    pub ignore_buff: Option<String>,

    // === Phase 3: Evolution ability state ===
    pub evo_state: Option<EvoState>,

    // === Phase 3: Hero ability state ===
    pub hero_state: Option<HeroState>,
}

impl Entity {
    /// Is this entity a valid target (alive and fully deployed)?
    pub fn is_targetable(&self) -> bool {
        if !self.alive || self.deploy_timer > 0 {
            return false;
        }
        if matches!(self.kind, EntityKind::SpellZone(_) | EntityKind::Projectile(_)) {
            return false;
        }
        // Burrowing troops (Miner underground) are untargetable
        // Dash-immune troops (Bandit mid-dash) are untargetable
        if let EntityKind::Troop(ref t) = self.kind {
            if t.is_burrowing {
                return false;
            }
            if t.is_dashing && t.dash_immune_remaining > 0 {
                return false;
            }
        }
        // Hidden buildings (Tesla) are untargetable while hidden.
        // Data-driven from hides_when_not_attacking.
        if let EntityKind::Building(ref b) = self.kind {
            if b.hides_when_not_attacking && b.is_hidden {
                return false;
            }
        }
        true
    }

    /// Is this entity flying?
    pub fn is_flying(&self) -> bool {
        self.z > 0
    }

    /// Is this entity invisible (Royal Ghost, Archer Queen, or any buff with invisible=true)?
    /// Invisible entities cannot be targeted by enemies.
    /// Fully data-driven: checks active buffs for invisible=true. No hardcoded troop flags.
    pub fn is_invisible(&self) -> bool {
        match &self.kind {
            EntityKind::Troop(_) => {
                // Check active buffs for invisible=true (Invisibility, ArcherQueenRapid, etc.)
                for buff in &self.buffs {
                    if buff.invisible && buff.remaining_ticks > 0 {
                        return true;
                    }
                }
                false
            },
            _ => false,
        }
    }

    /// Is this entity a building (including spawner buildings)?
    pub fn is_building(&self) -> bool {
        matches!(self.kind, EntityKind::Building(_))
    }

    /// Is this entity a troop?
    pub fn is_troop(&self) -> bool {
        matches!(self.kind, EntityKind::Troop(_))
    }

    /// Is this entity currently burrowing underground (Miner travel phase)?
    pub fn is_burrowing(&self) -> bool {
        match &self.kind {
            EntityKind::Troop(t) => t.is_burrowing,
            _ => false,
        }
    }

    /// Euclidean distance squared to another entity (avoids sqrt).
    pub fn dist_sq(&self, other: &Entity) -> i64 {
        let dx = (self.x - other.x) as i64;
        let dy = (self.y - other.y) as i64;
        dx * dx + dy * dy
    }

    /// Euclidean distance squared to a point.
    pub fn dist_sq_to(&self, px: i32, py: i32) -> i64 {
        let dx = (self.x - px) as i64;
        let dy = (self.y - py) as i64;
        dx * dx + dy * dy
    }

    // === Phase 3: Buff helpers ===

    /// Tick all active buffs: decrement timers, apply heal-over-time, apply damage-over-time, remove expired.
    pub fn tick_buffs(&mut self) {
        // Apply heal-over-time and damage-over-time from buffs.
        // Pulsed DOT (Poison, Earthquake): only fires when damage_hit_timer reaches 0.
        // Pulsed Heal (BattleHealerSelf): same timer mechanism for heal_per_tick.
        let mut heal = 0i32;
        let mut dot = 0i32;
        let is_building = self.is_building();
        for buff in &mut self.buffs {
            if buff.is_expired() {
                continue;
            }

            // Heal-over-time: supports both per-tick and pulsed modes.
            // If damage_hit_interval > 0 AND heal_per_tick > 0, heal fires on the
            // same pulse timer as DOT (shared timer). This handles BattleHealerSelf
            // (heal_per_second=16, hit_frequency=500ms → 8 HP every 10 ticks).
            if buff.heal_per_tick > 0 {
                if buff.damage_hit_interval > 0 {
                    // Pulsed heal: only fire on timer (timer is ticked below in DOT block)
                    // We peek at the timer: if it would fire this tick, add heal.
                    // The timer decrement happens in the DOT block below.
                    if buff.damage_hit_timer <= 1 {
                        heal += buff.heal_per_tick;
                    }
                } else {
                    // Legacy: every-tick heal
                    heal += buff.heal_per_tick;
                }
            }

            if buff.damage_per_tick > 0 || (buff.heal_per_tick > 0 && buff.damage_hit_interval > 0) {
                // Tick the shared pulse timer for DOT and/or pulsed heal
                let mut tick_dot = 0;
                if buff.damage_hit_interval > 0 {
                    // Pulsed: only apply on timer fire
                    buff.damage_hit_timer -= 1;
                    if buff.damage_hit_timer <= 0 {
                        buff.damage_hit_timer = buff.damage_hit_interval;
                        tick_dot = buff.damage_per_tick;
                    }
                } else if buff.damage_per_tick > 0 {
                    // Legacy: every-tick DOT
                    tick_dot = buff.damage_per_tick;
                }
                // Earthquake building bonus: building_damage_percent (e.g., 350 = 3.5×)
                if is_building && buff.building_damage_percent > 0 && tick_dot > 0 {
                    tick_dot = (tick_dot as i64 * buff.building_damage_percent as i64 / 100) as i32;
                }
                dot += tick_dot;
            }
        }
        if heal > 0 && self.alive {
            // FIX 5: Use per-buff allowed_over_heal_perc instead of hardcoded 2x.
            // Find the maximum overheal allowance among active heal buffs.
            // 0 = no overheal (cap at max_hp). 100 = allow up to 2x max_hp.
            let max_overheal_pct = self.buffs.iter()
                .filter(|b| !b.is_expired() && b.heal_per_tick > 0)
                .map(|b| b.allowed_over_heal_perc)
                .max()
                .unwrap_or(0);
            let heal_cap = if max_overheal_pct > 0 {
                (self.max_hp as i64 * (100 + max_overheal_pct as i64) / 100) as i32
            } else {
                self.max_hp
            };
            self.hp = (self.hp + heal).min(heal_cap);
        }
        // Apply damage-over-time (Poison, Earthquake, etc.)
        if dot > 0 && self.alive {
            self.hp -= dot;
            if self.hp <= 0 {
                self.hp = 0;
                self.alive = false;
            }
        }

        // Decrement timers and remove expired
        for buff in &mut self.buffs {
            if buff.remaining_ticks < i32::MAX {
                buff.remaining_ticks -= 1;
            }
        }
        self.buffs.retain(|b| !b.is_expired());
    }

    /// Is this entity stunned by any active buff?
    pub fn is_stunned(&self) -> bool {
        self.buffs.iter().any(|b| !b.is_expired() && b.stun)
    }

    /// Is this entity frozen by any active buff?
    pub fn is_frozen(&self) -> bool {
        self.buffs.iter().any(|b| !b.is_expired() && b.freeze)
    }

    /// Is this entity immobilized (stunned or frozen)?
    pub fn is_immobilized(&self) -> bool {
        self.is_stunned() || self.is_frozen()
    }

    /// Get the effective speed multiplier from all active buffs (100 = no change).
    pub fn speed_multiplier(&self) -> i32 {
        let mut pct = 100i32;
        for buff in &self.buffs {
            if !buff.is_expired() {
                pct += buff.speed_percent;
            }
        }
        pct.max(0) // Don't go negative
    }

    /// Get the effective hit speed multiplier from all active buffs (100 = no change).
    pub fn hitspeed_multiplier(&self) -> i32 {
        let mut pct = 100i32;
        for buff in &self.buffs {
            if !buff.is_expired() {
                pct += buff.hitspeed_percent;
            }
        }
        pct.max(10) // Minimum 10% speed
    }

    /// Get the effective damage multiplier from all active buffs (100 = no change).
    pub fn damage_multiplier(&self) -> i32 {
        let mut pct = 100i32;
        for buff in &self.buffs {
            if !buff.is_expired() {
                pct += buff.damage_percent;
            }
        }
        pct.max(0)
    }

    /// Get the effective damage reduction from all active buffs (0 = no reduction).
    pub fn damage_reduction(&self) -> i32 {
        let mut red = 0i32;
        for buff in &self.buffs {
            if !buff.is_expired() {
                red += buff.damage_reduction;
            }
        }
        red.min(90) // Cap at 90% reduction
    }

    /// Get taunt override target (highest priority taunt).
    pub fn taunt_override(&self) -> Option<EntityId> {
        for buff in &self.buffs {
            if !buff.is_expired() {
                if let Some(tid) = buff.taunt_target {
                    return Some(tid);
                }
            }
        }
        None
    }

    /// Add a buff, replacing any existing buff with the same key.
    /// Respects ignore_buff immunity: if this entity is immune to a buff key,
    /// the buff is silently rejected (Fix #2: data-driven buff immunity).
    /// Applies hp_multiplier on first application (Fix #8: GrowthBoost=120 → 1.2× HP).
    pub fn add_buff(&mut self, buff: ActiveBuff) {
        // Fix #2: Check buff immunity before applying.
        // Data-driven from CharacterStats.ignore_buff (e.g., Golem immune to VoodooCurse).
        if let Some(ref immune_key) = self.ignore_buff {
            if buff.key == *immune_key {
                return; // Silently reject — this entity is immune to this buff
            }
        }

        // Fix #8: Apply hp_multiplier when the buff is first added (not a refresh).
        // GrowthBoost=120 means scale max_hp and hp to 120%.
        // Only apply if this is a NEW buff (not refreshing an existing one),
        // to avoid stacking the HP bonus on every refresh.
        let is_new = !self.buffs.iter().any(|b| b.key == buff.key && !b.is_expired());
        if is_new && buff.hp_multiplier > 0 && buff.hp_multiplier != 100 {
            let old_max = self.max_hp;
            self.max_hp = (self.max_hp as i64 * buff.hp_multiplier as i64 / 100) as i32;
            // Scale current HP proportionally so the entity keeps its HP%
            self.hp = (self.hp as i64 * self.max_hp as i64 / old_max.max(1) as i64) as i32;
        }

        // Remove existing buff with same key (refresh)
        self.buffs.retain(|b| b.key != buff.key);
        self.buffs.push(buff);
    }

    /// Remove all buffs matching a key.
    pub fn remove_buff(&mut self, key: &str) {
        self.buffs.retain(|b| b.key != key);
    }

    /// Check if entity has a specific buff active.
    pub fn has_buff(&self, key: &str) -> bool {
        self.buffs.iter().any(|b| b.key == key && !b.is_expired())
    }
}

// =========================================================================
// AttackPhase — state machine for the windup/hit/backswing model
// =========================================================================

/// Models the three-phase attack animation:
///   Idle → Windup → Hit (damage applied) → Backswing → Idle
///
/// Real CR timing from JSON data:
///   - `load_time` = windup duration (ms before the hit frame)
///   - `hit_speed - load_time` = backswing duration (ms after hit, before next attack)
///   - `load_first_hit` = initial windup before the very first attack (usually same as load_time)
///
/// Crucially, if the troop's target becomes invalid during Windup, the attack
/// is **cancelled** — the troop returns to Idle without dealing damage. This is
/// the "retarget reset" exploit that skilled players use to waste enemy DPS.
///
/// During Backswing the troop is committed — it cannot move or start a new
/// attack, but the damage has already been dealt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackPhase {
    /// Not attacking. Can move, acquire targets, and start a new windup.
    Idle,
    /// Winding up the attack. Timer counts down to 0, then damage is dealt.
    /// If target becomes invalid during this phase, attack is cancelled → Idle.
    Windup,
    /// Recovering after dealing damage. Timer counts down to 0, then → PostAttackStop or Idle.
    /// Troop cannot move during backswing.
    Backswing,
    /// Post-attack pause before troop can move again (PEKKA=200ms, Giant Skeleton=200ms).
    /// Data-driven from stop_time_after_attack. Troop cannot move but CAN start a new
    /// attack if a target is in range (matches real CR behavior).
    PostAttackStop,
}

impl Default for AttackPhase {
    fn default() -> Self {
        AttackPhase::Idle
    }
}

// =========================================================================
// TroopData — type-specific fields for troops
// =========================================================================

#[derive(Debug, Clone)]
pub struct TroopData {
    /// Speed in internal units per tick (pre-computed from stats.speed).
    pub speed: i32,
    /// Range² (squared, for fast distance checks).
    pub range_sq: i64,
    /// Sight range² (squared).
    pub sight_range_sq: i64,
    /// Attack cooldown: ticks between attacks (from hit_speed).
    pub hit_speed: i32,
    /// Ticks remaining until this troop can attack again.
    pub attack_cooldown: i32,
    /// Retarget load time (ticks). Applied as the minimum attack_cooldown when
    /// a troop switches to a new target, modelling the real CR retarget delay.
    pub load_after_retarget: i32,

    // Targeting preferences
    pub attacks_ground: bool,
    pub attacks_air: bool,
    pub target_only_buildings: bool,
    pub target_only_troops: bool,
    pub target_only_towers: bool,
    /// If true, prioritize lowest-HP enemy in sight range.
    pub target_lowest_hp: bool,
    /// If true, re-evaluate target every tick (don't stick to current target).
    pub retarget_each_tick: bool,

    /// Is this troop ranged? (has a projectile)
    pub is_ranged: bool,
    /// Projectile key (if ranged).
    pub projectile_key: Option<String>,

    /// Area damage radius (0 = single target).
    pub area_damage_radius: i32,

    /// Self-knockback distance on attack (Firecracker=1500, Sparky=750).
    /// Pushes THIS troop backward (away from target) when it fires.
    pub attack_push_back: i32,

    /// Crown tower damage percent (if != 0, damage is reduced against crown towers).
    pub crown_tower_damage_percent: i32,

    /// Level (for per-level scaling lookups).
    pub level: usize,

    /// Is evolved?
    pub is_evolved: bool,

    /// Can this troop jump the river without using a bridge?
    /// True for: Hog Rider, Royal Hogs, Ram Rider.
    /// These troops leap over the river toward their target from any position.
    /// Flying troops don't need this — they ignore the river via is_flying().
    pub can_jump_river: bool,

    /// Kamikaze: self-destructs on contact, dealing AoE damage (Spirits, Wall Breakers).
    pub kamikaze: bool,
    /// Kamikaze death damage radius (used as AoE splash). Falls back to area_damage_radius.
    pub kamikaze_damage_radius: i32,
    /// Buff to apply on kamikaze impact (e.g., Heal Spirit applies HealSpiritBuff to friendlies,
    /// Ice Spirit applies freeze, Electro Spirit chains). Optional.
    pub kamikaze_buff: Option<String>,
    /// Duration in ticks for the kamikaze buff.
    pub kamikaze_buff_time: i32,

    // Inferno-style damage ramp (Inferno Dragon).
    // Same 3-stage system as BuildingData. All zero = no ramp.
    pub ramp_damage2: i32,
    pub ramp_damage3: i32,
    pub ramp_time1: i32,
    pub ramp_time2: i32,
    /// Ticks spent attacking the current target continuously. Reset on retarget.
    pub ramp_ticks: i32,
    /// The entity ID of the target being ramped against.
    pub ramp_target: Option<EntityId>,

    // === Attack animation state machine ===
    /// Current attack phase (Idle / Windup / Backswing).
    pub attack_phase: AttackPhase,
    /// Timer for the current phase (ticks remaining).
    pub phase_timer: i32,
    /// Windup duration in ticks (from load_time). Damage is dealt when this expires.
    pub windup_ticks: i32,
    /// Backswing duration in ticks (hit_speed - load_time). Recovery after hit.
    pub backswing_ticks: i32,
    /// The target ID that was locked when windup started. If this changes
    /// during windup, the attack is cancelled (retarget reset exploit).
    pub windup_target: Option<EntityId>,

    // ─── Charge state (Prince, Dark Prince, Battle Ram) ───
    /// charge_range from data (internal units). 0 = no charge.
    pub charge_range: i32,
    /// Speed multiplier when charging (e.g., 200 = 2× speed). 0 = no charge.
    pub charge_speed_multiplier: i32,
    /// Damage dealt on the first hit after charge completes (damage_special).
    pub charge_damage: i32,
    /// Distance remaining before charge activates. Starts at charge_range,
    /// decrements as the troop walks toward a target. At 0, charge is active.
    pub charge_distance_remaining: i32,
    /// Previous position for charge distance tracking (updated each movement tick).
    pub charge_prev_x: i32,
    pub charge_prev_y: i32,
    /// Whether this troop is currently in charge state (speed boosted, next hit = special).
    pub is_charging: bool,
    /// Whether the next hit should deal charge_damage (set when charge activates,
    /// cleared after the hit lands).
    pub charge_hit_ready: bool,

    // ─── Dash state (Bandit, Mega Knight, Golden Knight) ───
    /// dash_damage from data. 0 = no dash ability.
    pub dash_damage: i32,
    /// Minimum range to trigger a dash (internal units).
    pub dash_min_range: i32,
    /// Maximum range to trigger a dash (internal units).
    pub dash_max_range: i32,
    /// AoE radius of the dash landing (MK jump). 0 = single target.
    pub dash_radius: i32,
    /// Cooldown between dashes in ticks.
    pub dash_cooldown_max: i32,
    /// Current dash cooldown remaining. 0 = ready.
    pub dash_cooldown: i32,
    /// Duration of dash travel in ticks (dash_constant_time).
    pub dash_travel_ticks: i32,
    /// Duration of landing recovery in ticks (dash_landing_time).
    pub dash_landing_ticks: i32,
    /// Invulnerability window during dash in ticks.
    pub dash_immune_ticks: i32,
    /// Remaining invulnerability ticks during the current dash. While > 0 the
    /// entity cannot be targeted or damaged (Bandit, Golden Knight).
    pub dash_immune_remaining: i32,
    /// Dash travel speed in units/tick (from jump_speed). When > 0 and
    /// dash_travel_ticks == 0, travel time is computed per-dash as
    /// distance / dash_jump_speed.
    pub dash_jump_speed: i32,
    /// Pushback applied to enemies at dash landing.
    pub dash_push_back: i32,
    /// Maximum number of chain dashes (Golden Knight ability). 0 = no chain dash.
    /// Data-driven from CharacterStats.dash_count.
    pub dash_count: i32,
    /// Whether a dash is currently in progress.
    pub is_dashing: bool,
    /// Ticks remaining in the current dash (travel + landing).
    pub dash_timer: i32,
    /// Dash destination (where the troop will land).
    pub dash_target_x: i32,
    pub dash_target_y: i32,
    /// The damage to deal on dash impact.
    pub dash_impact_damage: i32,

    // ─── Spawn splash (Mega Knight) ───
    /// AoE damage dealt on deploy completion. 0 = none.
    pub spawn_splash_damage: i32,
    /// Radius of spawn splash.
    pub spawn_splash_radius: i32,
    /// Pushback distance applied to enemies hit by spawn splash.
    /// Data-driven from CharacterStats.spawn_pushback.
    /// MK: 1800 units. 0 = no knockback.
    pub spawn_splash_pushback: i32,
    /// Whether spawn splash has already fired.
    pub spawn_splash_fired: bool,

    // ─── Troop spawner (Witch, Night Witch) ───
    /// Character key to spawn periodically (e.g., "Skeleton", "Bat").
    pub troop_spawn_character: Option<String>,
    /// Number of units per spawn wave.
    pub troop_spawn_number: i32,
    /// Interval between spawn waves in ticks (from spawn_pause_time).
    pub troop_spawn_interval: i32,
    /// Countdown to next spawn wave.
    pub troop_spawn_timer: i32,
    /// Radius for circular placement of spawned units (from spawn_radius in data).
    pub troop_spawn_radius: i32,

    // ─── Attached troops (Goblin Giant + SpearGoblins) ───
    /// Character key to spawn attached at deploy (spawn_attach=True).
    pub spawn_attach_character: Option<String>,
    /// Number of attached troops.
    pub spawn_attach_count: i32,

    // ─── Projectile origin offset (data-driven from CharacterStats) ───
    /// Horizontal offset from troop center where the projectile spawns.
    /// Data-driven from CharacterStats.projectile_start_radius.
    /// In real CR, ranged troops fire from an offset (e.g., Archer fires from
    /// beside her body, Executioner throws from arm's reach). This shifts the
    /// projectile's spawn point along the attacker→target axis by this many
    /// internal units, preventing projectiles from clipping through the troop's
    /// own collision body and giving melee-range interactions correct geometry.
    /// 0 = spawn at troop center (legacy default).
    pub projectile_start_radius: i32,
    /// Vertical (Z) offset where the projectile spawns. Data-driven from
    /// CharacterStats.projectile_start_z. Flying-height offset for the projectile
    /// origin — e.g., Baby Dragon breathes fire from flying_height + start_z.
    /// This is cosmetic for 2D simulation but stored for replay fidelity and
    /// potential future 3D collision checks. 0 = ground level / troop z.
    pub projectile_start_z: i32,

    // ─── Multi-projectile (Hunter 10-bullet, Princess 5-arrow) ───
    /// Number of projectiles fired per attack. 0 or 1 = single projectile.
    pub multiple_projectiles: i32,

    // ─── Multiple targets (Electro Wizard hits 2 targets simultaneously) ───
    /// Number of separate targets this troop attacks per cycle. 0 or 1 = single target.
    /// Each target receives a separate attack (and separate projectile if ranged).
    /// Data-driven from CharacterStats.multiple_targets.
    pub multiple_targets: i32,

    // ─── Custom first projectile (Princess, Hunter) ───
    /// Alternate projectile key used for the troop's very first attack only.
    /// After the first shot, normal projectile_key is used for all subsequent attacks.
    /// Data-driven from CharacterStats.custom_first_projectile.
    pub custom_first_projectile: Option<String>,
    /// Whether this troop has already fired its first attack.
    pub has_fired_first: bool,

    // ─── Post-attack stop time (PEKKA=200ms, Giant Skeleton=200ms, etc.) ───
    /// Ticks the troop must pause after backswing before it can move again.
    /// Data-driven from CharacterStats.stop_time_after_attack (ms → ticks).
    pub stop_time_after_attack: i32,

    // ─── Self-as-AoE-center (Valkyrie, Mega Knight, Dark Prince) ───
    /// If true, melee splash is centered on the attacker's position (360° spin).
    /// If false, melee splash is centered on the target's position (directional).
    /// Data-driven from CharacterStats.self_as_aoe_center.
    pub self_as_aoe_center: bool,

    // ─── Kamikaze delay (Skeleton Barrel=500ms) ───
    /// Delay in ticks before kamikaze self-destruct after entering attack range.
    /// 0 = instant detonation. Data-driven from CharacterStats.kamikaze_time (ms → ticks).
    pub kamikaze_delay: i32,
    /// Countdown for kamikaze delay. Set to kamikaze_delay when entering range.
    pub kamikaze_timer: i32,
    /// Whether this troop has begun its kamikaze countdown.
    pub kamikaze_primed: bool,

    // ─── Fisherman hook special attack ───
    /// Projectile key for special ranged attack (e.g., "FishermanProjectile").
    pub projectile_special: Option<String>,
    /// Maximum range for the special attack (internal units).
    pub special_range_sq: i64,
    /// Minimum range for the special attack (internal units).
    pub special_min_range_sq: i64,
    /// Load time for the special attack (ticks).
    pub special_load_ticks: i32,
    /// Cooldown remaining for the special attack.
    pub special_cooldown: i32,
    /// Whether a hook drag is in progress.
    pub hook_dragging: bool,
    /// Target being dragged by hook.
    pub hook_drag_target: Option<EntityId>,
    /// Drag speed (internal units per tick).
    pub hook_drag_speed: i32,

    // ─── Invisibility (Royal Ghost) ───
    /// Buff key applied when not attacking (e.g., "Invisibility").
    pub invis_buff_key: Option<String>,
    /// Ticks of idle time before invisibility activates.
    pub invis_idle_threshold: i32,
    /// Ticks spent idle (no attack). Reset on attack.
    pub invis_idle_ticks: i32,

    // ─── Pushback immunity (Golem, Giant, Lava Hound, etc.) ───
    /// If true, this troop is immune to knockback (pushback) effects from
    /// spells, projectiles, rolling logs, and death explosions.
    /// Data-driven from CharacterStats.ignore_pushback.
    pub ignore_pushback: bool,

    // ─── On-hit buff (Electro Wizard stun, Zappies stun) ───
    /// Buff key applied to the target on each normal attack hit.
    /// Data-driven from CharacterStats.buff_on_damage.
    /// NOT used for kamikaze (kamikaze uses kamikaze_buff instead).
    /// EWiz: "ZapFreeze", Zappies: "ZapFreeze".
    pub buff_on_damage_key: Option<String>,
    /// Duration of the on-hit buff in ticks.
    /// Data-driven from CharacterStats.buff_on_damage_time (ms → ticks).
    pub buff_on_damage_ticks: i32,

    // ─── Elixir Golem: opponent gets elixir on death ───
    /// Elixir granted to the OPPONENT when this troop dies.
    /// In internal units (1000 = 1 elixir). Elixir Golem penalty.
    pub elixir_on_death_for_opponent: i32,

    // ─── Underground travel / burrow (Miner, Goblin Drill dig) ───
    /// Whether this troop is currently burrowing underground toward its target.
    /// While burrowing: invisible, untargetable, cannot be damaged, moves toward
    /// burrow_target at burrow_speed units per tick. deploy_timer is paused until
    /// the troop arrives, then counts down normally (emerge animation).
    pub is_burrowing: bool,
    /// Target X coordinate the troop is traveling toward underground.
    pub burrow_target_x: i32,
    /// Target Y coordinate the troop is traveling toward underground.
    pub burrow_target_y: i32,
    /// Underground travel speed in internal units per tick (from spawn_pathfind_speed).
    pub burrow_speed: i32,
    /// Deploy timer value saved before burrow travel starts. Applied when the troop
    /// arrives at its destination to play the emerge animation.
    pub burrow_deploy_ticks: i32,

    // ─── Reflected attack (Electro Giant zap) — cached at spawn ───
    /// AoE damage dealt to all enemies within reflect_radius when this troop
    /// is hit by a melee attack. Level-scaled at spawn time via
    /// CharacterStats::reflected_attack_damage_at_level(). 0 = no reflect.
    pub reflect_damage: i32,
    /// Radius within which the reflected damage hits enemies.
    pub reflect_radius: i32,
    /// Buff key applied to enemies hit by the reflect (e.g., "ZapFreeze").
    pub reflect_buff_key: Option<String>,
    /// Duration of the reflect buff in ticks.
    pub reflect_buff_ticks: i32,

    // ─── Melee pushback (Fix #6: melee_pushback / is_melee_pushback_all) ───
    /// Knockback distance applied to the TARGET on each melee hit.
    /// Separate from attack_push_back (which pushes the ATTACKER backward).
    /// Data-driven from CharacterStats.melee_pushback. 0 = no melee pushback.
    pub melee_pushback: i32,
    /// If true, melee pushback affects ALL enemies in the splash radius.
    /// Data-driven from CharacterStats.is_melee_pushback_all.
    pub melee_pushback_all: bool,

    // ─── Buff after N hits (Fix #3: buff_after_hits) ───
    /// Buff key applied after a cumulative number of attack hits.
    /// Evo Prince: PrinceBuff gains PrinceRageBuff1 after 2 hits.
    /// Data-driven from CharacterStats.buff_after_hits.
    pub buff_after_hits_key: Option<String>,
    /// Hit count threshold for triggering the buff.
    /// Data-driven from CharacterStats.buff_after_hits_count.
    pub buff_after_hits_count: i32,
    /// Duration of the buff in ticks.
    /// Data-driven from CharacterStats.buff_after_hits_time (ms → ticks).
    pub buff_after_hits_time: i32,
    /// Cumulative hit counter for buff_after_hits. Incremented on each
    /// successful attack hit. When it reaches buff_after_hits_count,
    /// the buff is applied and the counter resets to 0.
    pub buff_after_hits_counter: i32,

    // ─── Morph system (Fix #4: morph_character) ───
    /// Character key this troop morphs into when its shield breaks or
    /// after morph_after_hits_count hits. Cannon Cart morphs into a
    /// stationary cannon building.
    /// Data-driven from CharacterStats.morph_character.
    pub morph_character: Option<String>,
    /// If true, heal to full HP on morph.
    /// Data-driven from CharacterStats.heal_on_morph.
    pub morph_heal: bool,
    /// Morph transition time in ticks.
    /// Data-driven from CharacterStats.morph_time (ms → ticks).
    pub morph_time: i32,

    // ─── Fix #9: area_effect_on_dash ───
    /// Spell zone key created at the dash landing point.
    /// Data-driven from CharacterStats.area_effect_on_dash.
    pub area_effect_on_dash: Option<String>,

    // ─── Fix #12: target_only_king_tower ───
    /// If true, this troop skips princess towers and targets only the king tower.
    /// Data-driven from CharacterStats.target_only_king_tower.
    pub target_only_king_tower: bool,

    // ─── Fix #13: deprioritize / ignore targets with buff ───
    /// If true AND ignore_targets_with_buff is Some, enemies with that buff are
    /// deprioritized (only targeted when no unbuffed enemies are in range).
    /// Ram Rider: deprioritizes already-snared targets (BolaSnare).
    pub deprioritize_targets_with_buff: bool,
    /// Buff key used by deprioritize/ignore logic. Enemies with this buff active
    /// are either skipped entirely (for the special attack) or deprioritized
    /// (for normal targeting). Data-driven from CharacterStats.ignore_targets_with_buff.
    pub ignore_targets_with_buff: Option<String>,

    // ─── Fix #14: untargetable_when_spawned ───
    /// If true, this entity cannot be targeted while deploy_timer > 0.
    /// PhoenixEgg: untargetable during the hatching animation.
    /// Note: normal is_targetable() already returns false when deploy_timer > 0,
    /// but death-spawned troops get deploy_timer = 0 by default. This flag is
    /// used by entities that need a nonzero deploy_timer AND untargetability
    /// during that window (the flag is checked in is_targetable for consistency).
    pub untargetable_when_spawned: bool,

    // ─── Fix #15: special attack timing ───
    /// Additional charge-up time in ticks before the special attack fires.
    /// Added to the hook cooldown. Data-driven from CharacterStats.special_charge_time.
    pub special_charge_ticks: i32,
    /// Recovery pause in ticks after a special attack completes.
    /// The troop cannot move or attack during this window.
    /// Data-driven from CharacterStats.stop_time_after_special_attack.
    pub stop_after_special_ticks: i32,
    /// Countdown for post-special-attack recovery. Decremented each tick.
    /// While > 0, troop is immobilized (treated like PostAttackStop).
    pub special_recovery_timer: i32,

    // ─── Fix #16: shield_die_pushback ───
    /// Pushback distance applied to nearby enemies when this troop's shield breaks.
    /// Guards=0, Dark Prince=0 in current data, but structurally ready.
    /// Data-driven from CharacterStats.shield_die_pushback.
    pub shield_die_pushback: i32,
}

// =========================================================================
// BuildingData — type-specific fields for buildings
// =========================================================================

#[derive(Debug, Clone)]
pub struct BuildingData {
    /// Lifetime in ticks (building decays after this).
    pub lifetime: i32,
    /// Ticks remaining before building expires.
    pub lifetime_remaining: i32,
    /// Attack fields (some buildings attack, e.g., Tesla, Inferno Tower).
    pub hit_speed: i32,
    pub attack_cooldown: i32,
    pub range_sq: i64,
    /// Minimum attack range squared. Targets closer than this are ignored (Mortar dead zone).
    pub min_range_sq: i64,
    pub attacks_ground: bool,
    pub attacks_air: bool,
    pub is_ranged: bool,
    pub projectile_key: Option<String>,
    /// Spawn character (for Furnace, Goblin Hut, etc.).
    pub spawn_character: Option<String>,
    /// Wave interval in ticks — time between successive spawn waves.
    /// Derived from `spawn_pause_time` in the JSON data.
    pub spawn_interval: i32,
    /// Countdown until the next wave fires.
    pub spawn_timer: i32,
    /// Number of units per wave.
    pub spawn_count: i32,
    /// Stagger delay in ticks between individual units within a wave.
    /// Derived from `spawn_interval` in the JSON data (e.g., 500ms = 10 ticks).
    /// 0 means all units in the wave spawn simultaneously.
    pub spawn_stagger: i32,
    /// Number of staggered units still pending from the current wave.
    /// When a wave fires, this is set to `spawn_count - 1` (the first unit
    /// spawns immediately). Each tick decrements `spawn_stagger_timer` and
    /// when it reaches 0, one more unit spawns and the counter resets.
    pub spawn_stagger_remaining: i32,
    /// Countdown for the next staggered unit within a wave.
    pub spawn_stagger_timer: i32,

    pub crown_tower_damage_percent: i32,
    pub level: usize,

    // Inferno-style damage ramp — three stages of increasing damage.
    // Stage 1: base damage for `ramp_time1` ticks.
    // Stage 2: `ramp_damage2` per hit for `ramp_time2` ticks.
    // Stage 3: `ramp_damage3` per hit (unlimited).
    // All zero = no ramp (flat damage).
    pub ramp_damage2: i32,
    pub ramp_damage3: i32,
    /// Duration of stage 1 in ticks. Stage 2 starts after this many ticks of
    /// continuous attacking the same target.
    pub ramp_time1: i32,
    /// Duration of stage 2 in ticks. Stage 3 starts after ramp_time1 + ramp_time2.
    pub ramp_time2: i32,
    /// Ticks spent attacking the current target continuously. Reset on retarget.
    pub ramp_ticks: i32,
    /// The entity ID of the target being ramped against. Reset triggers ramp_ticks = 0.
    pub ramp_target: Option<EntityId>,

    // Elixir generation (Elixir Collector)
    /// Elixir produced per collection cycle. 0 = not a collector.
    pub elixir_per_collect: i32,
    /// Interval between collections in ticks.
    pub elixir_generate_interval: i32,
    /// Countdown to next collection.
    pub elixir_generate_timer: i32,
    /// Elixir given to owner on death.
    pub elixir_on_death: i32,

    // ─── Tesla hide mechanic ───
    /// If true, building hides when idle and pops up to attack.
    /// Data-driven from CharacterStats.hides_when_not_attacking.
    pub hides_when_not_attacking: bool,
    /// Whether the building is currently hidden (untargetable/invisible).
    pub is_hidden: bool,
    /// Duration in ticks the building stays visible after attacking.
    /// Data-driven from CharacterStats.up_time_ms (ms → ticks).
    pub up_time_ticks: i32,
    /// Duration in ticks before the building hides again after up_time expires.
    /// Data-driven from CharacterStats.hide_time_ms (ms → ticks).
    pub hide_time_ticks: i32,
    /// Countdown timer for hide/show transitions.
    pub hide_timer: i32,

    // ─── Spawn limit ───
    /// Maximum number of alive spawned units allowed. 0 = unlimited.
    /// Data-driven from CharacterStats.spawn_limit.
    pub spawn_limit: i32,

    // ─── Splash damage (Bomb Tower, etc.) ───
    /// Area damage radius for buildings with melee/direct splash attacks.
    /// Data-driven from CharacterStats.area_damage_radius.
    /// 0 = single-target. When > 0, the building's attack deals AoE damage
    /// centered on the target (or on self if self_as_aoe_center is true).
    /// Ranged buildings get their splash from ProjectileStats instead;
    /// this field covers the non-projectile (direct attack) path.
    pub area_damage_radius: i32,
    /// If true, melee splash is centered on the building's position (360° AoE).
    /// If false, splash is centered on the target's position (directional).
    /// Data-driven from CharacterStats.self_as_aoe_center.
    pub self_as_aoe_center: bool,

    // ─── On-hit buff (data-driven from CharacterStats.buff_on_damage) ───
    /// Buff key applied to the target on each attack hit.
    /// Enables buildings with stun/slow-on-hit mechanics (e.g., Zap Tower).
    pub buff_on_damage_key: Option<String>,
    /// Duration of the on-hit buff in ticks.
    pub buff_on_damage_ticks: i32,
}

// =========================================================================
// ProjectileData
// =========================================================================

#[derive(Debug, Clone)]
pub struct ProjectileData {
    /// Speed in internal units per tick.
    pub speed: i32,
    /// Target entity ID — projectile homes towards this.
    pub target_id: EntityId,
    /// Target position (for non-homing or if target dies).
    pub target_x: i32,
    pub target_y: i32,
    /// Damage this projectile deals on impact (already computed from attacker stats).
    pub impact_damage: i32,
    /// Splash radius (0 = single target).
    pub splash_radius: i32,
    /// Is homing?
    pub homing: bool,
    /// Source entity (for crown tower damage reduction).
    pub source_id: EntityId,
    pub crown_tower_damage_percent: i32,
    /// Whether splash hits air units.
    pub aoe_to_air: bool,
    /// Whether splash hits ground units.
    pub aoe_to_ground: bool,

    // ── Volley dedup (Model C: Princess multi-arrow) ──
    // In real CR, multi-arrow troops like Princess fire several projectiles per
    // attack, each carrying FULL damage with independent splash. However, a
    // given target can only be damaged by ONE projectile per attack volley —
    // the multiple arrows exist to cover a wider area, not to stack damage.
    // All projectiles from the same volley share the same volley_id (> 0).
    // At impact time, splash processing skips entities already hit by a sibling.
    // volley_id == 0 means no dedup (standard single-projectile attacks, scatter
    // attacks like Hunter where stacking IS intended, spell projectiles, etc.).
    /// Shared ID linking projectiles from the same multi-arrow volley.
    /// 0 = no volley dedup (default for single projectiles and scatter).
    pub volley_id: u32,

    // ── Rolling projectile (Log, Barb Barrel) ──
    /// If true, this projectile is a rolling AoE that damages enemies every tick
    /// as it passes through them, rather than dealing a single impact at the end.
    pub is_rolling: bool,
    /// Half-width of the rolling hitbox (X extent). 0 = use splash_radius.
    pub rolling_radius_x: i32,
    /// Half-depth of the rolling hitbox (Y extent). 0 = use rolling_radius_x.
    pub rolling_radius_y: i32,
    /// Total distance this projectile will travel before dying.
    pub rolling_range: i32,
    /// Distance traveled so far (accumulated each tick).
    pub distance_traveled: i32,
    /// Entity IDs already hit by this rolling projectile (prevents double-damage).
    pub hit_entities: Vec<u32>,
    /// Tower sentinel IDs already hit by this rolling projectile.
    pub hit_towers: Vec<u32>,

    // ── Boomerang projectile (Executioner axe) ──
    /// If true, this projectile travels to its target point, then returns to
    /// its source position, dealing AoE damage to enemies in its path both ways.
    /// On the return trip, hit_entities is cleared so enemies can be hit again.
    pub is_boomerang: bool,
    /// True when the projectile is on its return trip back to source.
    pub boomerang_returning: bool,
    /// Source position to return to (the Executioner's position at launch).
    pub boomerang_source_x: i32,
    pub boomerang_source_y: i32,
    /// Radius for AoE damage along the boomerang path.
    pub boomerang_radius: i32,

    // ─── Pushback (Log, Snowball, Fireball) ───
    /// Pushback distance in internal units. 0 = no pushback.
    pub pushback: i32,
    /// If true, pushback affects all enemies in AoE (not just primary target).
    pub pushback_all: bool,
    /// Minimum pushback distance (at edge of radius). 0 = use flat pushback.
    /// When min_pushback > 0 and max_pushback > 0, pushback interpolates linearly:
    ///   at center → max_pushback, at edge → min_pushback.
    pub min_pushback: i32,
    /// Maximum pushback distance (at center of impact). 0 = use flat pushback.
    pub max_pushback: i32,

    // ─── Target buff on hit (Snowball slow, Fisherman slow) ───
    /// Buff key to apply to hit enemies (e.g., "IceWizardSlowDown").
    pub target_buff: Option<String>,
    /// Duration of the target buff in ticks.
    pub target_buff_time: i32,
    /// If true, apply target buff BEFORE damage (Mother Witch VoodooCurse).
    pub apply_buff_before_damage: bool,

    // ─── Fisherman hook drag ───
    /// If true, this projectile drags the target back to the source on hit.
    pub drag_back: bool,
    /// Data-driven from ProjectileStats.drag_back_as_attractor.
    /// When true: projectile pulls the TARGET toward the SOURCE (attractor).
    /// When false: projectile pulls the SOURCE toward the TARGET (self-pull).
    /// Fisherman: drag_back_as_attractor=true (pull enemy to self),
    ///   but for buildings: falls back to self-pull via drag_self_speed.
    pub drag_back_as_attractor: bool,
    /// Speed at which the target is dragged back (internal units per tick).
    pub drag_back_speed: i32,
    /// Source entity position for drag-back destination.
    pub drag_source_x: i32,
    pub drag_source_y: i32,
    /// Speed at which the SOURCE is pulled toward a building target.
    /// Fisherman pulls himself to buildings instead of pulling them.
    /// 0 = no self-pull (only drag target back).
    pub drag_self_speed: i32,
    /// Margin distance to stop dragging (from ProjectileStats.drag_margin).
    /// Data-driven: Fisherman hook stops this far from the destination.
    pub drag_margin: i32,

    // ─── Chain lightning (Electro Dragon, Electro Spirit) ───
    /// Maximum distance from previous hit target to next chain bounce target.
    /// ElectroDragonProjectile: 4000, ZapSpiritProjectile: 5500. 0 = no chain.
    pub chained_hit_radius: i32,
    /// Total number of targets hit (including primary). 0 or 1 = no chain.
    /// ElectroDragonProjectile: 3, ZapSpiritProjectile: 3.
    pub chained_hit_count: i32,

    // ─── Gravity arc (Fireball, Rocket, Goblin Barrel, etc.) ───
    /// If true, this projectile follows a ballistic arc (gravity > 0 in ProjectileStats).
    /// Gravity-arc projectiles are NOT homing — they fly to a fixed (target_x, target_y)
    /// computed at launch time, regardless of whether the target entity moves. In real CR,
    /// Fireball/Rocket/Arrows/Goblin Barrel land at the cast location, not on the troop.
    /// Moving troops can dodge them. The `homing` field is forced to false for gravity arcs.
    /// Speed was already computed as distance/arc_ticks at spawn time using the parabolic
    /// formula: t = sqrt(2 * distance / gravity_accel).
    pub is_gravity_arc: bool,
}

// =========================================================================
// SpellZoneData
// =========================================================================

#[derive(Debug, Clone)]
pub struct SpellZoneData {
    /// Total duration in ticks.
    pub duration: i32,
    /// Ticks remaining.
    pub remaining: i32,
    /// Damage per hit tick.
    pub damage_per_tick: i32,
    /// Tick interval between damage applications.
    pub hit_interval: i32,
    /// Countdown to next hit.
    pub hit_timer: i32,
    /// Radius of effect.
    pub radius: i32,
    /// Does it hit air? Ground?
    pub affects_air: bool,
    pub affects_ground: bool,
    /// Buff to apply (e.g., Rage, Freeze).
    pub buff_key: Option<String>,
    pub buff_duration: i32,
    /// Only hits enemies?
    pub only_enemies: bool,
    /// Only hits own troops?
    pub only_own: bool,
    pub crown_tower_damage_percent: i32,

    // ── Lightning / hit-biggest targeting ──
    /// If true, each hit tick selects the N highest-HP targets instead of all in radius.
    pub hit_biggest_targets: bool,
    /// Max targets per hit tick (Lightning = 3). 0 = unlimited (standard zone).
    pub max_hit_targets: i32,
    /// Per-strike damage from the linked projectile (Lightning uses LighningSpell projectile).
    /// Replaces damage_per_tick when hit_biggest_targets is true.
    pub projectile_damage: i32,
    /// Crown tower damage percent from the linked projectile.
    pub projectile_ct_pct: i32,
    /// Key of the linked projectile (e.g., "LighningSpell") for looking up
    /// target_buff on impact. Lightning's stun comes from LighningSpell.target_buff=ZapFreeze.
    pub spell_projectile_key: Option<String>,

    // ── Displacement / attraction (Tornado) ──
    /// Pull strength per tick (internal units). Computed from attract_percentage
    /// and push_speed_factor at spawn time. 0 = no pull (most spells).
    pub attract_strength: i32,
    /// If true, this zone pulls entities toward its center every tick.
    pub has_displacement: bool,

    // ── Spawner spell (Graveyard) ──
    /// Character key to spawn periodically (e.g., "Skeleton").
    pub spawn_character: Option<String>,
    /// Ticks between spawns.
    pub spawn_interval: i32,
    /// Countdown to next spawn.
    pub spawn_timer: i32,
    /// Initial delay before first spawn (ticks).
    pub spawn_initial_delay: i32,
    /// Level for spawned troops.
    pub spawn_level: usize,
    /// Team that owns the spawned troops.
    pub spawn_team: Team,
    /// Minimum spawn distance from zone center (Graveyard=3000).
    /// Spawned units appear in a ring between spawn_min_radius and radius.
    /// 0 = spawn anywhere within the full radius.
    /// Data-driven from SpellStats.spawn_min_radius.
    pub spawn_min_radius: i32,

    // ── Direct heal (Heal spell) ──
    /// Heal per hit tick applied to friendly troops within radius.
    /// Data-driven from SpellStats.heal_per_second (converted to per-hit-interval).
    /// 0 = no direct heal (most spells). Separate from buff-based heals.
    pub heal_per_hit: i32,

    // ── Pushback (Zap, zone-type spells) ──
    /// Pushback distance in internal units applied on zone hit. 0 = no pushback.
    /// Data-driven from SpellStats.pushback. Only applies on the FIRST hit tick.
    pub pushback: i32,
    /// If true, pushback affects all enemies in the zone (not just primary target).
    /// Data-driven from SpellStats.pushback_all.
    pub pushback_all: bool,
    /// Whether the initial pushback has already been applied.
    /// Zone pushback fires once (on first hit), not every hit interval.
    pub pushback_applied: bool,
    /// Minimum pushback distance (at edge of radius). 0 = use flat pushback.
    /// Data-driven from SpellStats.min_pushback. When min_pushback > 0 and
    /// max_pushback > 0, pushback interpolates linearly from zone center to edge:
    ///   center → max_pushback, edge → min_pushback.
    /// This matches real CR where spells like Fireball push harder near center.
    pub min_pushback: i32,
    /// Maximum pushback distance (at center of impact). 0 = use flat pushback.
    /// Data-driven from SpellStats.max_pushback.
    pub max_pushback: i32,

    // ── Data-driven flags (baked at zone creation, no runtime lookup) ──
    /// If true, this spell zone deals NO damage or DOT to crown towers.
    /// Data-driven from SpellStats.no_effect_to_crown_towers or
    /// BuffStats.no_effect_to_crown_towers (Earthquake, Tornado, etc.).
    /// Baked at creation time so tick_spell_zones doesn't need GameData lookup.
    pub no_effect_to_crown_towers: bool,
    /// If true, this spell can hit hidden buildings (Tesla while underground).
    /// Data-driven from SpellStats.affects_hidden (Earthquake, Lightning).
    /// Baked at creation time so tick_spell_zones doesn't need GameData lookup.
    pub affects_hidden: bool,

    // ── Level scaling ratio for buff DOT and heal (Fix 1+2) ──
    /// Numerator/denominator of the level scaling ratio derived from
    /// SpellStats.damage_per_level at zone creation time.
    /// Used by tick_spell_zones to scale BuffStats.damage_per_second and
    /// heal_per_second, which have no per-level arrays in the JSON data.
    /// In real CR, all spell stats scale by the same rarity-based multiplier.
    /// ratio = damage_per_level[level-1] / damage_per_level[0].
    /// (1, 1) means no scaling (base level or no damage_per_level data).
    pub level_scale_num: i64,
    pub level_scale_den: i64,
}

// =========================================================================
// Speed table — convert stats.speed (enum-like int) to units/tick
// =========================================================================

/// Clash Royale uses integer speed categories. This converts to internal units per tick.
/// Values are approximate but match observed in-game behaviour.
///   speed: 0=None, 45=Slow, 60=Medium, 90=Fast, 120=VeryFast
/// Tiles/sec → units/tick: (speed_tiles_per_sec * 600) / 20
pub fn speed_to_units_per_tick(speed: i32) -> i32 {
    // Speed field is in "hundredths of tiles per second" in some data,
    // but our JSON has it as the raw integer category.
    // Map known categories:
    match speed {
        0 => 0,
        s if s <= 45 => 18,   // Slow:     ~0.6 tiles/s
        s if s <= 60 => 30,   // Medium:   ~1.0 tiles/s
        s if s <= 90 => 45,   // Fast:     ~1.5 tiles/s
        s if s <= 120 => 60,  // VeryFast: ~2.0 tiles/s
        s => (s * 30) / 100,  // Fallback linear interpolation
    }
}

/// Convert milliseconds to ticks (1 tick = 50ms, 20 ticks/sec).
pub fn ms_to_ticks(ms: i32) -> i32 {
    (ms + 25) / 50 // Round to nearest tick
}

/// Convert a range value (in game units) to range-squared for fast distance checks.
/// CSV range values are already in game units (1 tile ≈ 1000 units).
pub fn range_squared(range: i32) -> i64 {
    (range as i64) * (range as i64)
}

// =========================================================================
// Entity constructors
// =========================================================================

impl Entity {
    /// Spawn a troop entity from CharacterStats.
    pub fn new_troop(
        id: EntityId,
        team: Team,
        stats: &CharacterStats,
        x: i32,
        y: i32,
        level: usize,
        is_evolved: bool,
    ) -> Self {
        let hp = stats.hp_at_level(level);
        let dmg = stats.damage_at_level(level);

        // Fix 3: Clamp spawn position to arena bounds.
        // Multi-unit card formations (Minion Horde, Skeleton Army) apply offsets
        // that can push units outside the arena if deployed near the edge.
        // Clamping here catches ALL spawn paths (play_card, death_spawn, building
        // spawners, etc.) without needing per-callsite fixes.
        // Uses ARENA_HALF_W and ARENA_HALF_H from game_state constants.
        let x = x.clamp(-crate::game_state::ARENA_HALF_W, crate::game_state::ARENA_HALF_W);
        let y = y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);

        Entity {
            id,
            team,
            card_key: stats.key.clone(),
            x,
            y,
            z: stats.flying_height,
            collision_radius: stats.collision_radius,
            mass: if stats.mass > 0 { stats.mass } else { 6 }, // Default mass 6 (Knight-weight)
            hp,
            max_hp: hp,
            shield_hp: stats.shield_hitpoints_at_level(level),
            damage: dmg,
            alive: true,
            deploy_timer: ms_to_ticks(stats.deploy_time),
            target: None,
            attached_to: None,
            kind: EntityKind::Troop(TroopData {
                // Fix #1: Apply walking_speed_tweak_percentage on top of base speed.
                // PEKKA=+20 (faster than other "slow"), Barbarian=-20 (slower than "medium"),
                // Golem=+15, IceWizard=-26, etc. Data-driven from CharacterStats.
                speed: {
                    let base = speed_to_units_per_tick(stats.speed);
                    if stats.walking_speed_tweak_percentage != 0 {
                        (base as i64 * (100 + stats.walking_speed_tweak_percentage as i64) / 100) as i32
                    } else {
                        base
                    }
                },
                range_sq: range_squared(stats.range),
                sight_range_sq: range_squared(stats.sight_range),
                hit_speed: ms_to_ticks(stats.hit_speed),
                // Initial attack cooldown: load_first_hit from data gives the delay
                // before the very first attack after deployment. This value is consumed
                // once (counted down in tick_combat) and never needs to be re-read,
                // so we don't store load_first_hit as a separate TroopData field.
                attack_cooldown: ms_to_ticks(stats.load_first_hit),
                load_after_retarget: ms_to_ticks(stats.load_after_retarget),
                attacks_ground: stats.attacks_ground,
                attacks_air: stats.attacks_air,
                target_only_buildings: stats.target_only_buildings,
                target_only_troops: stats.target_only_troops,
                target_only_towers: stats.target_only_towers,
                target_lowest_hp: stats.target_lowest_hp,
                retarget_each_tick: stats.retarget_each_tick,
                is_ranged: stats.is_ranged(),
                projectile_key: stats.projectile.clone(),
                area_damage_radius: stats.area_damage_radius,
                attack_push_back: stats.attack_push_back,
                crown_tower_damage_percent: stats.crown_tower_damage_percent,
                level,
                is_evolved,
                can_jump_river: {
                    // Troops that can leap over the river without using a bridge.
                    // Primary check: jump_enabled flag from the data (Prince, Dark Prince,
                    // Hog Rider, Royal Hogs, Ram, Battle Ram).
                    // Fallback: name-based matching for any edge cases.
                    if stats.jump_enabled {
                        true
                    } else {
                        let key_lower = stats.key.to_lowercase();
                        let name_lower = stats.name.to_lowercase();
                        key_lower.contains("hog-rider")
                            || key_lower.contains("hogrider")
                            || key_lower.contains("royal-hog")
                            || key_lower.contains("royalhog")
                            || key_lower.contains("ram-rider")
                            || key_lower.contains("ramrider")
                            || name_lower.contains("hogrider")
                            || name_lower.contains("royalhog")
                            || name_lower.contains("ramrider")
                    }
                },
                kamikaze: stats.kamikaze,
                kamikaze_damage_radius: if stats.death_damage_radius > 0 {
                    stats.death_damage_radius
                } else {
                    stats.area_damage_radius
                },
                kamikaze_buff: stats.buff_on_damage.clone(),
                kamikaze_buff_time: ms_to_ticks(stats.buff_on_damage_time),
                ramp_damage2: stats.variable_damage2,
                ramp_damage3: stats.variable_damage3,
                ramp_time1: ms_to_ticks(stats.variable_damage_time1),
                ramp_time2: ms_to_ticks(stats.variable_damage_time2),
                ramp_ticks: 0,
                ramp_target: None,
                // Attack animation state machine
                attack_phase: AttackPhase::Idle,
                phase_timer: 0,
                windup_ticks: ms_to_ticks(stats.load_time),
                backswing_ticks: {
                    let bs = stats.hit_speed - stats.load_time;
                    if bs > 0 { ms_to_ticks(bs) } else { 0 }
                },
                windup_target: None,
                // ─── Charge state ───
                charge_range: stats.charge_range,
                charge_speed_multiplier: stats.charge_speed_multiplier,
                charge_damage: {
                    // damage_special from JSON. If 0 but charge_range > 0,
                    // default to 2× base damage (standard CR behavior).
                    let ds = stats.damage_special;
                    if ds > 0 {
                        // Level-scale damage_special the same way as base damage
                        // (damage_special is always 2× base in CR data, so just use 2× scaled)
                        dmg * 2
                    } else if stats.charge_range > 0 {
                        dmg * 2
                    } else {
                        0
                    }
                },
                charge_distance_remaining: stats.charge_range,
                charge_prev_x: x,
                charge_prev_y: y,
                is_charging: false,
                charge_hit_ready: false,
                // ─── Dash state ───
                dash_damage: {
                    // Level-scale dash_damage proportionally to base damage.
                    // dash_damage in JSON is lv1. Scale by the same ratio as base damage.
                    if stats.dash_damage > 0 && stats.damage > 0 {
                        (stats.dash_damage as i64 * dmg as i64 / stats.damage as i64) as i32
                    } else {
                        stats.dash_damage
                    }
                },
                dash_min_range: stats.dash_min_range,
                dash_max_range: stats.dash_max_range,
                dash_radius: stats.dash_radius,
                dash_cooldown_max: ms_to_ticks(stats.dash_cooldown),
                dash_cooldown: 0,
                dash_travel_ticks: ms_to_ticks(stats.dash_constant_time),
                dash_landing_ticks: ms_to_ticks(stats.dash_landing_time),
                dash_immune_ticks: ms_to_ticks(stats.dash_immune_to_damage_time),
                // Remaining invulnerability ticks during a dash. Decremented each
                // tick while dashing. While > 0, the entity cannot be targeted or
                // take damage (Bandit, Golden Knight). Set from dash_immune_ticks
                // when a dash starts.
                dash_immune_remaining: 0,
                // Jump/dash travel speed (internal units, from jump_speed in data).
                // Bandit=500, SuperHogRider_Terry=?. When dash_constant_time=0,
                // travel time is computed per-dash as distance/jump_speed instead
                // of using a fixed duration.
                dash_jump_speed: speed_to_units_per_tick(stats.jump_speed),
                dash_push_back: stats.dash_push_back,
                dash_count: stats.dash_count,
                is_dashing: false,
                dash_timer: 0,
                dash_target_x: 0,
                dash_target_y: 0,
                dash_impact_damage: 0,
                // ─── Spawn splash ───
                spawn_splash_damage: if stats.spawn_pushback_radius > 0 && stats.dash_damage > 0 {
                    // MK spawn splash uses dash_damage value, level-scaled
                    if stats.damage > 0 {
                        (stats.dash_damage as i64 * dmg as i64 / stats.damage as i64) as i32
                    } else {
                        stats.dash_damage
                    }
                } else {
                    0
                },
                spawn_splash_radius: stats.spawn_pushback_radius,
                spawn_splash_pushback: stats.spawn_pushback,
                spawn_splash_fired: false,

                // Troop spawner (Witch, Night Witch)
                troop_spawn_character: stats.spawn_character.clone(),
                troop_spawn_number: if stats.spawn_number > 0 { stats.spawn_number } else { 1 },
                troop_spawn_interval: if stats.spawn_pause_time > 0 {
                    ms_to_ticks(stats.spawn_pause_time)
                } else {
                    0
                },
                troop_spawn_timer: if stats.spawn_start_time > 0 {
                    // Fix #19: Use spawn_start_time as the initial delay before the
                    // first spawn wave, NOT spawn_pause_time. In real CR, Witch has
                    // spawn_start_time=1000ms (first skeletons after 1s) but
                    // spawn_pause_time=7000ms (subsequent waves every 7s).
                    // Previously both used spawn_pause_time, making the first wave
                    // fire 6 seconds too late for Witch. Data-driven from JSON.
                    ms_to_ticks(stats.spawn_start_time)
                } else if stats.spawn_pause_time > 0 {
                    // Fallback: if no spawn_start_time, use one full interval
                    // (backwards-compatible with troops that don't set spawn_start_time)
                    ms_to_ticks(stats.spawn_pause_time)
                } else {
                    0
                },
                troop_spawn_radius: stats.spawn_radius,

                // Attached troops (Goblin Giant)
                spawn_attach_character: if stats.spawn_attach {
                    stats.spawn_character.clone()
                } else {
                    None
                },
                spawn_attach_count: if stats.spawn_attach { stats.spawn_number } else { 0 },

                // Projectile origin offset (data-driven from CharacterStats).
                // Controls where the projectile entity spawns relative to the troop's
                // center position. Non-zero values shift the spawn point along the
                // attacker→target direction, preventing projectiles from originating
                // inside the troop's own collision body.
                projectile_start_radius: stats.projectile_start_radius,
                projectile_start_z: stats.projectile_start_z,

                // Multi-projectile (Hunter 10, Princess 5)
                multiple_projectiles: if stats.multiple_projectiles > 1 {
                    stats.multiple_projectiles
                } else {
                    1
                },

                // Multiple targets (E-Wiz 2)
                multiple_targets: if stats.multiple_targets > 1 {
                    stats.multiple_targets
                } else {
                    1
                },

                // Custom first projectile (Princess, Hunter)
                custom_first_projectile: stats.custom_first_projectile.clone(),
                has_fired_first: false,

                // Post-attack stop time (PEKKA=200ms, etc.)
                stop_time_after_attack: ms_to_ticks(stats.stop_time_after_attack),

                // Self-as-AoE-center (Valkyrie 360° spin)
                self_as_aoe_center: stats.self_as_aoe_center,

                // Kamikaze delay (Skeleton Barrel=500ms)
                kamikaze_delay: ms_to_ticks(stats.kamikaze_time),
                kamikaze_timer: 0,
                kamikaze_primed: false,

                // Fisherman hook special attack
                projectile_special: stats.projectile_special.clone(),
                special_range_sq: range_squared(stats.special_range),
                special_min_range_sq: range_squared(stats.special_min_range),
                special_load_ticks: if stats.special_load_time > 0 {
                    ms_to_ticks(stats.special_load_time)
                } else {
                    0
                },
                special_cooldown: 0,
                hook_dragging: false,
                hook_drag_target: None,
                hook_drag_speed: 0, // Set from projectile data when hook lands

                // Idle buff (Royal Ghost invisibility, BattleHealer self-heal, etc.)
                // Data-driven from CharacterStats.buff_when_not_attacking + buff_when_not_attacking_time.
                // The buff is applied after invis_idle_threshold ticks of not attacking,
                // and removed when the troop enters attack animation. What the buff DOES
                // (invisibility, heal, speed boost, etc.) is entirely determined by the
                // BuffStats entry — no hardcoded behavior per buff type.
                invis_buff_key: stats.buff_when_not_attacking.clone(),
                invis_idle_threshold: if stats.buff_when_not_attacking_time > 0 {
                    ms_to_ticks(stats.buff_when_not_attacking_time)
                } else {
                    0
                },
                invis_idle_ticks: 0,

                ignore_pushback: stats.ignore_pushback,

                // On-hit buff (EWiz stun, Zappies stun)
                // Only for non-kamikaze troops. Kamikaze troops use kamikaze_buff.
                buff_on_damage_key: if !stats.kamikaze {
                    stats.buff_on_damage.clone()
                } else {
                    None
                },
                buff_on_damage_ticks: if !stats.kamikaze && stats.buff_on_damage_time > 0 {
                    ms_to_ticks(stats.buff_on_damage_time)
                } else {
                    0
                },

                elixir_on_death_for_opponent: stats.mana_on_death_for_opponent,

                // Burrow / underground travel (Miner, Goblin Drill dig)
                // Initialized to inactive — play_card sets these when deploying a
                // troop with spawn_pathfind_speed > 0.
                is_burrowing: false,
                burrow_target_x: 0,
                burrow_target_y: 0,
                burrow_speed: if stats.spawn_pathfind_speed > 0 {
                    speed_to_units_per_tick(stats.spawn_pathfind_speed)
                } else {
                    0
                },
                burrow_deploy_ticks: 0,

                // Reflected attack (Electro Giant): cached at spawn, level-scaled.
                // Avoids per-hit GameData lookup in tick_combat.
                reflect_damage: stats.reflected_attack_damage_at_level(level),
                reflect_radius: stats.reflected_attack_radius,
                reflect_buff_key: stats.reflected_attack_buff.clone(),
                reflect_buff_ticks: if stats.reflected_attack_buff_duration > 0 {
                    ms_to_ticks(stats.reflected_attack_buff_duration)
                } else {
                    0
                },

                // Fix #6: Melee pushback — knockback applied to TARGET on each melee hit.
                // Separate from attack_push_back (self-knockback on the ATTACKER).
                // Data-driven from CharacterStats.melee_pushback / is_melee_pushback_all.
                melee_pushback: stats.melee_pushback,
                melee_pushback_all: stats.is_melee_pushback_all,

                // Fix #3: Buff after N hits — Evo Prince gains rage buffs after hit milestones.
                // Data-driven from CharacterStats.buff_after_hits / buff_after_hits_count / buff_after_hits_time.
                buff_after_hits_key: stats.buff_after_hits.clone(),
                buff_after_hits_count: stats.buff_after_hits_count,
                buff_after_hits_time: if stats.buff_after_hits_time > 0 {
                    ms_to_ticks(stats.buff_after_hits_time)
                } else {
                    0
                },
                buff_after_hits_counter: 0,

                // Fix #4: Morph system — Cannon Cart morphs into stationary cannon on shield break.
                // Data-driven from CharacterStats.morph_character / morph_time / heal_on_morph.
                morph_character: stats.morph_character.clone(),
                morph_heal: stats.heal_on_morph,
                morph_time: if stats.morph_time > 0 { ms_to_ticks(stats.morph_time) } else { 0 },

                // Fix #9: area_effect_on_dash — spell zone on dash landing.
                area_effect_on_dash: stats.area_effect_on_dash.clone(),

                // Fix #12: target_only_king_tower — skip princess towers.
                target_only_king_tower: stats.target_only_king_tower,

                // Fix #13: deprioritize/ignore targets with buff (Ram Rider bola).
                deprioritize_targets_with_buff: stats.deprioritize_targets_with_buff,
                ignore_targets_with_buff: stats.ignore_targets_with_buff.clone(),

                // Fix #14: untargetable_when_spawned (PhoenixEgg).
                untargetable_when_spawned: stats.untargetable_when_spawned,

                // Fix #15: special attack timing (Fisherman).
                special_charge_ticks: if stats.special_charge_time > 0 {
                    ms_to_ticks(stats.special_charge_time)
                } else {
                    0
                },
                stop_after_special_ticks: if stats.stop_time_after_special_attack > 0 {
                    ms_to_ticks(stats.stop_time_after_special_attack)
                } else {
                    0
                },
                special_recovery_timer: 0,

                // Fix #16: shield_die_pushback — pushback on shield break.
                shield_die_pushback: stats.shield_die_pushback,
            }),
            buffs: Vec::new(),
            // Fix #2: Wire ignore_buff from CharacterStats onto the Entity.
            // Troops immune to specific buffs (Golem immune to VoodooCurse) will
            // silently reject those buffs in Entity::add_buff().
            ignore_buff: stats.ignore_buff.clone(),
            evo_state: if is_evolved {
                Some(EvoState { prev_x: x, prev_y: y, ..Default::default() })
            } else {
                None
            },
            hero_state: if stats.ability.is_some() {
                // Champions have an `ability` field in the JSON data.
                // Initialize hero_state so activate_hero() can work on
                // troops spawned via spawn_troop (not just play_card).
                Some(HeroState {
                    is_hero: true,
                    hero_key: stats.key.clone(),
                    ability_active: false,
                    ability_remaining: 0,
                    is_flying_override: false,
                })
            } else {
                None
            },
        }
    }
    pub fn new_building(
        id: EntityId,
        team: Team,
        stats: &CharacterStats,
        x: i32,
        y: i32,
        level: usize,
    ) -> Self {
        let hp = stats.hp_at_level(level);
        let lifetime_ticks = ms_to_ticks(stats.life_time);

        // Fix 3: Clamp spawn position to arena bounds (same as new_troop).
        let x = x.clamp(-crate::game_state::ARENA_HALF_W, crate::game_state::ARENA_HALF_W);
        let y = y.clamp(-crate::game_state::ARENA_HALF_H, crate::game_state::ARENA_HALF_H);

        Entity {
            id,
            team,
            card_key: stats.key.clone(),
            x,
            y,
            z: 0,
            collision_radius: stats.collision_radius,
            mass: 100, // Buildings are immovable
            hp,
            max_hp: hp,
            shield_hp: 0,
            damage: stats.damage_at_level(level),
            alive: true,
            deploy_timer: ms_to_ticks(stats.deploy_time),
            target: None,
            attached_to: None,
            kind: EntityKind::Building(BuildingData {
                lifetime: lifetime_ticks,
                lifetime_remaining: lifetime_ticks,
                hit_speed: ms_to_ticks(stats.hit_speed),
                attack_cooldown: ms_to_ticks(stats.load_first_hit),
                range_sq: range_squared(stats.range),
                min_range_sq: range_squared(stats.minimum_range),
                attacks_ground: stats.attacks_ground,
                attacks_air: stats.attacks_air,
                is_ranged: stats.is_ranged(),
                projectile_key: stats.projectile.clone(),
                spawn_character: stats.spawn_character.clone(),
                // spawn_pause_time is the wave cadence (e.g., Tombstone 3500ms).
                // If not set, fall back to spawn_interval for backward compat.
                spawn_interval: {
                    let wave_ms = if stats.spawn_pause_time > 0 {
                        stats.spawn_pause_time
                    } else if stats.spawn_interval > 0 {
                        stats.spawn_interval
                    } else {
                        0
                    };
                    ms_to_ticks(wave_ms)
                },
                spawn_timer: {
                    // Initial delay before first wave.
                    // spawn_start_time > 0: explicit delay (e.g., Goblin Drill 1000ms).
                    // spawn_start_time == 0: first wave fires on the first tick after
                    // deploy (timer = 1 so the countdown hits 0 immediately).
                    if stats.spawn_start_time > 0 {
                        ms_to_ticks(stats.spawn_start_time)
                    } else {
                        1 // Fire first wave on next tick after deploy
                    }
                },
                // JSON uses spawn_number for the per-wave count, spawn_count is often null/0.
                // Fall back to spawn_number when spawn_count is 0.
                spawn_count: if stats.spawn_count > 0 { stats.spawn_count } else { stats.spawn_number },
                // spawn_interval (JSON) is the stagger between individual units
                // within a wave (e.g., 500ms between each skeleton in a pair).
                spawn_stagger: if stats.spawn_pause_time > 0 && stats.spawn_interval > 0 {
                    ms_to_ticks(stats.spawn_interval)
                } else {
                    0 // No stagger — all units in the wave spawn at once
                },
                spawn_stagger_remaining: 0,
                spawn_stagger_timer: 0,
                crown_tower_damage_percent: stats.crown_tower_damage_percent,
                level,
                ramp_damage2: stats.variable_damage2,
                ramp_damage3: stats.variable_damage3,
                ramp_time1: ms_to_ticks(stats.variable_damage_time1),
                ramp_time2: ms_to_ticks(stats.variable_damage_time2),
                ramp_ticks: 0,
                ramp_target: None,
                elixir_per_collect: stats.mana_collect_amount,
                elixir_generate_interval: if stats.mana_generate_time_ms > 0 {
                    ms_to_ticks(stats.mana_generate_time_ms)
                } else {
                    0
                },
                elixir_generate_timer: if stats.mana_generate_time_ms > 0 {
                    ms_to_ticks(stats.mana_generate_time_ms)
                } else {
                    0
                },
                elixir_on_death: stats.mana_on_death,

                // Tesla hide mechanic
                hides_when_not_attacking: stats.hides_when_not_attacking,
                is_hidden: stats.hides_when_not_attacking, // Start hidden if applicable
                up_time_ticks: ms_to_ticks(stats.up_time_ms),
                hide_time_ticks: ms_to_ticks(stats.hide_time_ms),
                hide_timer: 0,

                // Spawn limit
                spawn_limit: stats.spawn_limit,

                // Splash damage — data-driven from CharacterStats.
                // Bomb Tower, etc. with area_damage_radius > 0 deal AoE on direct attacks.
                area_damage_radius: stats.area_damage_radius,
                self_as_aoe_center: stats.self_as_aoe_center,

                // On-hit buff — data-driven from CharacterStats.buff_on_damage.
                buff_on_damage_key: stats.buff_on_damage.clone(),
                buff_on_damage_ticks: if stats.buff_on_damage_time > 0 {
                    ms_to_ticks(stats.buff_on_damage_time)
                } else {
                    0
                },
            }),
            buffs: Vec::new(),
            ignore_buff: None, // Buildings don't use buff immunity
            evo_state: None,
            hero_state: None,
        }
    }

    /// Spawn a projectile entity.
    pub fn new_projectile(
        id: EntityId,
        team: Team,
        source_id: EntityId,
        from_x: i32,
        from_y: i32,
        target_id: EntityId,
        target_x: i32,
        target_y: i32,
        speed: i32,
        impact_damage: i32,
        splash_radius: i32,
        homing: bool,
        crown_tower_damage_percent: i32,
        aoe_to_air: bool,
        aoe_to_ground: bool,
    ) -> Self {
        Entity {
            id,
            team,
            card_key: String::new(),
            x: from_x,
            y: from_y,
            z: 0,
            collision_radius: 0,
            mass: 0, // Projectiles not affected by displacement
            hp: 1, // Projectiles die on impact
            max_hp: 1,
            shield_hp: 0,
            damage: impact_damage,
            alive: true,
            deploy_timer: 0,
            target: Some(target_id),
            attached_to: None,
            kind: EntityKind::Projectile(ProjectileData {
                speed,
                target_id,
                target_x,
                target_y,
                impact_damage,
                splash_radius,
                homing,
                source_id,
                crown_tower_damage_percent,
                aoe_to_air,
                aoe_to_ground,
                volley_id: 0,
                is_rolling: false,
                rolling_radius_x: 0,
                rolling_radius_y: 0,
                rolling_range: 0,
                distance_traveled: 0,
                hit_entities: Vec::new(),
                hit_towers: Vec::new(),
                is_boomerang: false,
                boomerang_returning: false,
                boomerang_source_x: from_x,
                boomerang_source_y: from_y,
                boomerang_radius: 0,
                pushback: 0,
                pushback_all: false,
                min_pushback: 0,
                max_pushback: 0,
                target_buff: None,
                target_buff_time: 0,
                apply_buff_before_damage: false,
                drag_back: false,
                drag_back_as_attractor: false,
                drag_back_speed: 0,
                drag_source_x: from_x,
                drag_source_y: from_y,
                drag_self_speed: 0,
                drag_margin: 0,
                chained_hit_radius: 0,
                chained_hit_count: 0,
                is_gravity_arc: false, // Set by caller for gravity projectiles
            }),
            buffs: Vec::new(),
            ignore_buff: None,
            evo_state: None,
            hero_state: None,
        }
    }

    /// Spawn a spell zone.
    pub fn new_spell_zone(
        id: EntityId,
        team: Team,
        card_key: &str,
        x: i32,
        y: i32,
        radius: i32,
        duration_ticks: i32,
        damage_per_tick: i32,
        hit_interval: i32,
        affects_air: bool,
        affects_ground: bool,
        buff_key: Option<String>,
        buff_duration: i32,
        only_enemies: bool,
        only_own: bool,
        crown_tower_damage_percent: i32,
        attract_strength: i32,
        spawn_character: Option<String>,
        spawn_interval: i32,
        spawn_initial_delay: i32,
        spawn_level: usize,
        hit_biggest_targets: bool,
        max_hit_targets: i32,
        projectile_damage: i32,
        projectile_ct_pct: i32,
        spell_projectile_key: Option<String>,
        spawn_min_radius: i32,
        heal_per_hit: i32,
        pushback: i32,
        pushback_all: bool,
        min_pushback: i32,
        max_pushback: i32,
        no_effect_to_crown_towers: bool,
        affects_hidden: bool,
        level_scale_num: i64,
        level_scale_den: i64,
    ) -> Self {
        Entity {
            id,
            team,
            card_key: card_key.to_string(),
            x,
            y,
            z: 0,
            collision_radius: radius,
            mass: 0,
            hp: 1,
            max_hp: 1,
            shield_hp: 0,
            damage: damage_per_tick,
            alive: true,
            deploy_timer: 0,
            target: None,
            attached_to: None,
            kind: EntityKind::SpellZone(SpellZoneData {
                duration: duration_ticks,
                remaining: duration_ticks,
                damage_per_tick,
                hit_interval,
                hit_timer: 0,
                radius,
                affects_air,
                affects_ground,
                buff_key,
                buff_duration,
                only_enemies,
                only_own,
                crown_tower_damage_percent,
                hit_biggest_targets,
                max_hit_targets,
                projectile_damage,
                projectile_ct_pct,
                spell_projectile_key,
                attract_strength,
                has_displacement: attract_strength > 0,
                spawn_character,
                spawn_interval,
                spawn_timer: spawn_initial_delay,
                spawn_initial_delay,
                spawn_level,
                spawn_team: team,
                spawn_min_radius,
                heal_per_hit,
                pushback,
                pushback_all,
                pushback_applied: false,
                min_pushback,
                max_pushback,
                no_effect_to_crown_towers,
                affects_hidden,
                level_scale_num,
                level_scale_den,
            }),
            buffs: Vec::new(),
            ignore_buff: None,
            evo_state: None,
            hero_state: None,
        }
    }
}
//! Card stats structs — deserialized from the 6 JSON data files.
//!
//! `GameData` is loaded once at startup and passed as `&GameData`
//! (immutable borrow) to every match. Zero copies during simulation.

use std::collections::HashMap;
use std::path::Path;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Custom deserializer: some JSON fields use `false` instead of `0`
// ---------------------------------------------------------------------------

/// Deserialize a field that may be either an integer or a boolean (false=0, true=1).
fn bool_or_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct BoolOrI32Visitor;

    impl<'de> de::Visitor<'de> for BoolOrI32Visitor {
        type Value = i32;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer or boolean")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<i32, E> {
            Ok(if v { 1 } else { 0 })
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i32, E> {
            Ok(v as i32)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i32, E> {
            Ok(v as i32)
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<i32, E> {
            Ok(v as i32)
        }
    }

    deserializer.deserialize_any(BoolOrI32Visitor)
}

/// Wrapper for serde default + bool_or_i32.
fn default_bool_or_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    bool_or_i32(deserializer)
}

/// Deserialize a Vec<i32> that may be null in JSON (treat null as empty vec).
fn null_or_vec_i32<'de, D>(deserializer: D) -> Result<Vec<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<Vec<i32>> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// Deserialize a String that may be null in JSON (treat null as empty string).
fn null_or_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// Deserialize an i32 that may be null in JSON (treat null as 0).
fn null_or_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<i32> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or(0))
}

/// Deserialize an i64 that may be null in JSON (treat null as 0).
fn null_or_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<i64> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or(0))
}

/// Deserialize a bool that may be null in JSON (treat null as false).
fn null_or_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<bool> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or(false))
}

// ---------------------------------------------------------------------------
// Character Stats (troops + champions) — from cards_stats_characters.json
// ---------------------------------------------------------------------------

/// Represents one troop or champion card's base stats.
/// 297 fields exist in JSON; we deserialize only the simulation-relevant ones.
/// serde will ignore unknown fields thanks to `deny_unknown_fields` NOT being set.
#[derive(Debug, Clone, Deserialize)]
pub struct CharacterStats {
    // Identity
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub key: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub sc_key: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub rarity: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub elixir: i32,
    #[serde(default, rename = "type")]
    pub card_type: String,
    #[serde(default, deserialize_with = "null_or_i64")]
    pub id: i64,

    // Core combat stats
    #[serde(default, deserialize_with = "null_or_i32")]
    pub hitpoints: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub damage: i32,
    /// True if this character's damage was patched from projectile data (Step 5b),
    /// meaning the character had damage=0 in the JSON and its primary attack IS the
    /// projectile. Melee troops with secondary projectiles (Ram Rider bola, Fisherman
    /// hook) have damage>0 in the JSON — their projectile field is for a secondary
    /// mechanic, not their primary attack.
    #[serde(skip)]
    pub damage_from_projectile: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub hit_speed: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub speed: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub range: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub deploy_time: i32,
    /// Stagger delay between individual units in a multi-unit card spawn.
    /// Skeleton=400ms, Barbarian=400ms, Wallbreaker=400ms, Musketeer=300ms, etc.
    /// Unit 0 deploys at deploy_time, unit i deploys at deploy_time + i*deploy_delay.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub deploy_delay: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub sight_range: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub load_time: i32,
    #[serde(default, deserialize_with = "default_bool_or_i32")]
    pub load_first_hit: i32,
    #[serde(default, deserialize_with = "default_bool_or_i32")]
    pub load_after_retarget: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub stop_time_after_attack: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub crown_tower_damage_percent: i32,

    // ─── Speed fine-tuning (Fix #1: walking_speed_tweak_percentage) ───
    /// Per-troop speed modifier applied ON TOP of the base speed category.
    /// In real CR, some troops are faster/slower than their speed tier implies:
    ///   PEKKA=+20 (walks faster than other "slow" troops),
    ///   Barbarian=-20 (walks slower than other "medium" troops),
    ///   Golem=+15, IceWizard=-26, Bomber=-10, Wizard=-10, GiantSkeleton=-12.
    /// Value is a signed percentage: +20 = 20% faster, -20 = 20% slower.
    /// Applied in Entity::new_troop() after speed_to_units_per_tick() conversion.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub walking_speed_tweak_percentage: i32,

    // Spatial / physics
    #[serde(default, deserialize_with = "null_or_i32")]
    pub collision_radius: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub mass: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub flying_height: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub area_damage_radius: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub self_as_aoe_center: bool,
    /// Self-knockback on attack (Firecracker=1500, Sparky/ZapMachine=750).
    /// Pushes the ATTACKER backward (away from target) by this many units
    /// each time it fires a ranged attack. 0 = no self-knockback.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub attack_push_back: i32,

    // ─── Buff immunity (Fix #2: ignore_buff) ───
    /// Buff key that this character is immune to. When a buff with this key is
    /// applied to the character, it is silently rejected. Data-driven from JSON.
    /// Examples: Golem, Lava Hound, Battle Ram, Cannon Cart, Skeleton Barrel,
    /// Goblin Giant, Ram Rider, Elixir Golem variants are immune to "VoodooCurse".
    /// This prevents Mother Witch from cursing these troops.
    #[serde(default)]
    pub ignore_buff: Option<String>,

    // ─── Melee pushback (Fix #6: melee_pushback / is_melee_pushback_all) ───
    /// Knockback distance applied to the TARGET on each melee hit (internal units).
    /// Separate from `attack_push_back` (which pushes the ATTACKER backward).
    /// Bowler, Giant Skeleton, etc. push enemies away on each melee swing.
    /// 0 = no melee pushback (default for most troops).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub melee_pushback: i32,
    /// If true, melee pushback affects ALL enemies in the splash radius,
    /// not just the primary target. Data-driven from JSON.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub is_melee_pushback_all: bool,

    // Targeting
    #[serde(default, deserialize_with = "null_or_bool")]
    pub attacks_ground: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub attacks_air: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub target_only_buildings: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub target_only_troops: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub target_only_towers: bool,

    // ─── Fix #12: target_only_king_tower ───
    /// If true, this troop ignores princess towers and targets only the king tower.
    /// Data-driven from JSON. 0/false = normal targeting.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub target_only_king_tower: bool,

    // ─── Fix #13: deprioritize / ignore targets with buff ───
    /// If true, this troop deprioritizes (avoids targeting) enemies that already
    /// have a specific buff. Ram Rider deprioritizes targets already snared.
    /// Used in conjunction with ignore_targets_with_buff to identify which buff.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub deprioritize_targets_with_buff: bool,
    /// Buff key to ignore when selecting targets. Enemies with this buff active
    /// are skipped (or deprioritized) during target selection.
    /// Ram Rider: "BolaSnare" — avoids re-snaring already-snared targets.
    #[serde(default)]
    pub ignore_targets_with_buff: Option<String>,

    // ─── Fix #14: untargetable_when_spawned ───
    /// If true, this entity cannot be targeted during its deploy animation.
    /// PhoenixEgg=True — the egg is untargetable while deploying (hatching).
    /// Data-driven from JSON. Checked in Entity::is_targetable().
    #[serde(default, deserialize_with = "null_or_bool")]
    pub untargetable_when_spawned: bool,

    // Advanced targeting flags
    /// If true, this troop prioritizes the lowest-HP enemy in range.
    /// Default: false (nearest-first targeting).
    #[serde(default, deserialize_with = "null_or_bool")]
    pub target_lowest_hp: bool,
    /// If true, this troop re-evaluates its target every tick instead of
    /// sticking to the current target until it dies or leaves leash range.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub retarget_each_tick: bool,

    // Projectile references
    #[serde(default)]
    pub projectile: Option<String>,
    #[serde(default)]
    pub custom_first_projectile: Option<String>,
    #[serde(default)]
    pub projectile_special: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub multiple_projectiles: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub multiple_targets: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub projectile_start_radius: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub projectile_start_z: i32,
    // ─── Fisherman special attack fields ───
    #[serde(default, deserialize_with = "null_or_i32")]
    pub special_range: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub special_min_range: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub special_load_time: i32,
    // ─── Fix #15: special attack timing ───
    /// Charge-up time in ms before the special attack fires (Fisherman hook windup).
    /// Separate from special_load_time (which is the projectile launch delay).
    /// 0 = no additional charge time beyond special_load_time.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub special_charge_time: i32,
    /// Recovery pause in ms after a special attack completes.
    /// The troop cannot move or attack during this window.
    /// 0 = no recovery pause after special attack.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub stop_time_after_special_attack: i32,

    // Shield
    #[serde(default, deserialize_with = "null_or_i32")]
    pub shield_hitpoints: i32,
    // ─── Fix #16: shield_die_pushback ───
    /// Pushback distance applied to nearby enemies when this troop's shield breaks.
    /// Guards, Dark Prince: enemies near the troop are pushed away on shield destruction.
    /// 0 = no pushback on shield break (default).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub shield_die_pushback: i32,

    // Dash / Jump (Mega Knight, Bandit, etc.)
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_damage: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_radius: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_min_range: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_max_range: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_cooldown: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_constant_time: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_landing_time: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub jump_speed: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_push_back: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub charge_range: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub charge_speed_multiplier: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub ignore_pushback: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub damage_special: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub jump_enabled: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_immune_to_damage_time: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub dash_count: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_pushback: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_pushback_radius: i32,

    // Building dead zone (Mortar — cannot hit targets inside this range)
    #[serde(default, deserialize_with = "null_or_i32")]
    pub minimum_range: i32,

    // Spawning (Witch, Night Witch, etc.)
    #[serde(default)]
    pub spawn_character: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_number: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_count: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_interval: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_start_time: i32,
    /// Wave interval in ms — time between successive spawn waves.
    /// This is the *real* cadence (e.g., Tombstone 3500ms = one wave every 3.5s).
    /// `spawn_interval` is the stagger *within* a wave (e.g., 500ms between each
    /// skeleton in a pair).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_pause_time: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_limit: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_radius: i32,
    /// Angular offset between spawned units in degrees (Bat: 45, DarkWitch: 90).
    /// When > 0, multi-unit spawns use circular placement at this angle step
    /// instead of the default grid pattern.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_angle_shift: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub spawn_attach: bool,

    // Elixir generation (Elixir Collector)
    /// Elixir produced per collection cycle (ElixirCollector: 1).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub mana_collect_amount: i32,
    /// Time between elixir collections in ms (ElixirCollector: 9000ms = 9s).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub mana_generate_time_ms: i32,
    /// Elixir given on death (ElixirCollector: 1, to the owner).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub mana_on_death: i32,
    /// Elixir given to the OPPONENT on death (Elixir Golem penalty).
    /// ElixirGolem1=1000, ElixirGolem2=500, ElixirGolem4=500.
    /// In internal units: 1000 = 1 elixir.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub mana_on_death_for_opponent: i32,

    // Death mechanics (Golem, Lava Hound, etc.)
    #[serde(default)]
    pub death_spawn_character: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_spawn_count: i32,
    #[serde(default)]
    pub death_spawn_character2: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_spawn_count2: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_spawn_radius: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_damage: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_damage_radius: i32,
    /// Pushback distance applied to enemies within death_damage_radius on death.
    /// GiantSkeletonBomb=1800, Golem=1800, Golemite=900, CommonBomb=1500, etc.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_push_back: i32,
    /// Projectile fired on death (Phoenix → PhoenixFireball).
    /// Also signals egg-based respawn: if "{Name}Egg" character exists, spawn it on death.
    #[serde(default)]
    pub death_spawn_projectile: Option<String>,

    // ─── Death spawn deploy delay (Fix #5: death_spawn_deploy_time) ───
    /// Deploy time in ms for troops spawned on death. Battle Ram=1000ms,
    /// Goblin Giant=700ms. Death-spawned troops wait this long before
    /// they can move or attack, matching the real CR deploy animation.
    /// 0 = instant deployment (default, backwards-compatible with old behavior).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_spawn_deploy_time: i32,

    // ─── Fix #10: death_spawn_min_radius ───
    /// Minimum scatter distance from the death position for spawned units.
    /// Controls how tightly death spawns cluster. When > 0, death spawns are
    /// placed in a ring between death_spawn_min_radius and death_spawn_radius
    /// instead of a full circle from center. Prevents Lava Pups / Golemites
    /// from all stacking on the exact death point.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_spawn_min_radius: i32,

    // ─── Fix #11: death_spawn_pushback ───
    /// If true, enemies near the death position are pushed away when death
    /// spawns appear. Golem, Lava Hound, SuperLavaHound have this set.
    /// This is separate from death_push_back (which is the explosion pushback
    /// from death_damage). death_spawn_pushback is an additional push that
    /// fires when the death-spawned units materialize.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub death_spawn_pushback: bool,

    // Kamikaze (Wall Breakers, Fire Spirit, etc.)
    #[serde(default, deserialize_with = "null_or_bool")]
    pub kamikaze: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub kamikaze_time: i32,

    // ─── Morph system (Fix #4: morph_character / morph_after_hits_count / morph_time / heal_on_morph) ───
    /// Character key this troop morphs into under certain conditions.
    /// Cannon Cart (MovingCannon) morphs into a stationary cannon building
    /// when its shield breaks. The morph replaces the troop entity with a
    /// new building entity at the same position.
    #[serde(default)]
    pub morph_character: Option<String>,
    /// Number of hits received before morph triggers. 0 = morph is triggered
    /// by shield break (Cannon Cart) rather than hit count.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub morph_after_hits_count: i32,
    /// Time in ms for the morph transition. During this period the entity
    /// is inert (cannot attack or move).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub morph_time: i32,
    /// If true, the entity is healed to full HP on morph (Cannon Cart
    /// gains full HP when it morphs into the stationary cannon).
    #[serde(default, deserialize_with = "null_or_bool")]
    pub heal_on_morph: bool,

    // Buff references
    #[serde(default)]
    pub buff_on_damage: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_on_damage_time: i32,
    #[serde(default)]
    pub buff_on_kill: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_on_kill_time: i32,
    #[serde(default)]
    pub starting_buff: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub starting_buff_time: i32,
    #[serde(default)]
    pub buff_when_not_attacking: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_when_not_attacking_time: i32,
    #[serde(default)]
    pub buff_on50_hp: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_on50_hp_time: i32,
    #[serde(default)]
    pub buff_on_xhp: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_on_xhp_percent: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_on_xhp_time: i32,

    // ─── Buff after N hits (Fix #3: buff_after_hits) ───
    /// Buff key applied after a cumulative number of attack hits.
    /// Evo Prince: PrinceBuff gains PrinceRageBuff1 after 2 hits, then
    /// PrinceRageBuff2 after 4, PrinceRageBuff3 after 6. The hit count is
    /// cumulative across the troop's lifetime (not per-target).
    /// The `buff_after_hits_count` threshold triggers the buff application.
    #[serde(default)]
    pub buff_after_hits: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_after_hits_count: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_after_hits_time: i32,

    // Champion ability reference
    #[serde(default)]
    pub ability: Option<String>,

    // Deploy area effect (IceWizard → IceWizardCold, BattleHealer → BattleHealerSpawnHeal)
    // On deploy completion, spawns a spell zone at the troop's position.
    #[serde(default)]
    pub spawn_area_object: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_area_object_level_index: i32,

    // Area effect on hit (BattleHealer → BattleHealerHeal)
    // On each attack hit, spawns a transient spell zone at the attacker's position.
    // Used by BattleHealer to create AoE heal for nearby friendlies on attack.
    #[serde(default)]
    pub area_effect_on_hit: Option<String>,

    // ─── Fix #9: area_effect_on_dash ───
    /// Spell zone created at the dash landing point when a troop completes a dash.
    /// Similar to area_effect_on_hit but triggers on dash landing instead of attack.
    /// Data-driven from JSON. The spell key is looked up in data.spells to create
    /// a transient zone at the landing position.
    #[serde(default)]
    pub area_effect_on_dash: Option<String>,

    // Building-specific (also used for buildings file)
    #[serde(default, deserialize_with = "null_or_i32")]
    pub life_time: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub hides_when_not_attacking: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub hide_time_ms: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub up_time_ms: i32,

    // Variable damage ramp (Inferno Tower, Inferno Dragon)
    // Stage 1: base `damage` for `variable_damage_time1` ms
    // Stage 2: `variable_damage2` DPS for `variable_damage_time2` ms
    // Stage 3: `variable_damage3` DPS (unlimited)
    #[serde(default, deserialize_with = "null_or_i32")]
    pub variable_damage2: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub variable_damage3: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub variable_damage_time1: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub variable_damage_time2: i32,

    // Reflected attack (Electro Giant, etc.)
    #[serde(default)]
    pub reflected_attack_buff: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub reflected_attack_buff_duration: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub reflected_attack_radius: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub reflected_attack_damage: i32,

    // Summoning metadata
    #[serde(default)]
    pub summon_character: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub summon_number: i32,
    #[serde(default)]
    pub summon_character_second: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub summon_character_second_count: i32,

    /// Spawn formation radius multiplier. Applied to collision_radius to determine
    /// how spread out a multi-unit card's formation is at deploy time.
    /// 0.0 or 1.0 = default (collision_radius). Values > 1.0 = wider spread.
    /// Set from manual_overrides during loading — not in CSV data.
    #[serde(skip)]
    pub spawn_radius_multiplier: f32,

    // ─── Underground travel (Miner, Goblin Drill dig phase) ───
    /// Speed of underground travel in internal units. When > 0, the troop
    /// is deployed at the player's side, burrows underground, and travels to
    /// the target (x, y) at this speed before emerging. During travel the
    /// entity is invisible and untargetable. Data-driven from JSON field.
    /// Miner=650, GoblinDrillDig=300.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_pathfind_speed: i32,
    /// When set, the troop morphs into a different character on arrival.
    /// GoblinDrillDig morphs into GoblinDrill (building). Not used by Miner.
    #[serde(default)]
    pub spawn_pathfind_morph: Option<String>,

    // ─── Building deploy flags (deserialized from cards_stats_building.json) ───
    /// If true, this building can be placed on the enemy side of the arena.
    /// GoblinDrill=true. Data-driven from JSON.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub can_deploy_on_enemy_side: bool,
    /// If true, this building deploys via a spell-like mechanic (GoblinDrill).
    /// The card's summon_character is spawned as a traveling entity that morphs
    /// into this building on arrival.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub spell_as_deploy: bool,

    // Evo reference
    #[serde(default, deserialize_with = "null_or_bool")]
    pub is_evolved: bool,

    // Per-level scaling
    #[serde(default, deserialize_with = "null_or_vec_i32")]
    pub hitpoints_per_level: Vec<i32>,
    #[serde(default, deserialize_with = "null_or_vec_i32")]
    pub damage_per_level: Vec<i32>,
}

impl CharacterStats {
    /// Returns true if this character is a flying unit.
    pub fn is_flying(&self) -> bool {
        self.flying_height > 0
    }

    /// Returns true if this character has splash (area) damage.
    pub fn is_splash(&self) -> bool {
        self.area_damage_radius > 0
    }

    /// Returns true if this character's primary attack is ranged (fires a projectile).
    ///
    /// Data-driven: a character is ranged iff its damage was zero in the source JSON
    /// and was patched from projectile data during loading (Step 5b). Melee troops
    /// that carry a secondary projectile (Ram Rider bola, Fisherman hook) have
    /// nonzero damage in the JSON — their projectile field describes a secondary
    /// mechanic, not their primary attack.
    pub fn is_ranged(&self) -> bool {
        self.projectile.is_some() && self.damage_from_projectile
    }

    /// Get hitpoints at a given level (1-indexed). Falls back to base if level out of range.
    pub fn hp_at_level(&self, level: usize) -> i32 {
        self.hitpoints_per_level
            .get(level.saturating_sub(1))
            .copied()
            .unwrap_or(self.hitpoints)
    }

    /// Get damage at a given level (1-indexed). Falls back to base if level out of range.
    pub fn damage_at_level(&self, level: usize) -> i32 {
        self.damage_per_level
            .get(level.saturating_sub(1))
            .copied()
            .unwrap_or(self.damage)
    }

    /// Get death_damage at a given level. The CSV doesn't have per-level arrays
    /// for death_damage, so we scale it using the same ratio as damage_per_level
    /// (or hitpoints_per_level as fallback). This matches the real game where all
    /// stats scale by the same rarity-based multiplier per level.
    pub fn death_damage_at_level(&self, level: usize) -> i32 {
        if self.death_damage == 0 {
            return 0;
        }
        // Try scaling via damage_per_level ratio
        if !self.damage_per_level.is_empty() && self.damage > 0 {
            let lvl_dmg = self.damage_per_level
                .get(level.saturating_sub(1))
                .copied()
                .unwrap_or(self.damage);
            return (self.death_damage as i64 * lvl_dmg as i64 / self.damage as i64) as i32;
        }
        // Fallback: scale via hitpoints_per_level ratio
        if !self.hitpoints_per_level.is_empty() && self.hitpoints > 0 {
            let lvl_hp = self.hitpoints_per_level
                .get(level.saturating_sub(1))
                .copied()
                .unwrap_or(self.hitpoints);
            return (self.death_damage as i64 * lvl_hp as i64 / self.hitpoints as i64) as i32;
        }
        // No scaling data available — return base
        self.death_damage
    }

    /// Get shield_hitpoints at a given level. The CSV doesn't have per-level
    /// arrays for shield_hitpoints, so we scale using hitpoints_per_level ratio.
    /// In real CR, shields scale with the same rarity multiplier as all other stats.
    pub fn shield_hitpoints_at_level(&self, level: usize) -> i32 {
        if self.shield_hitpoints == 0 {
            return 0;
        }
        // Scale via hitpoints_per_level ratio (shield scales like HP)
        if !self.hitpoints_per_level.is_empty() && self.hitpoints > 0 {
            let lvl_hp = self.hitpoints_per_level
                .get(level.saturating_sub(1))
                .copied()
                .unwrap_or(self.hitpoints);
            return (self.shield_hitpoints as i64 * lvl_hp as i64 / self.hitpoints as i64) as i32;
        }
        // No scaling data available — return base
        self.shield_hitpoints
    }

    /// Get reflected_attack_damage at a given level. The CSV doesn't have
    /// per-level arrays for reflected_attack_damage, so we scale using
    /// damage_per_level ratio (or hitpoints_per_level as fallback).
    /// Used by Electro Giant's zap reflect.
    pub fn reflected_attack_damage_at_level(&self, level: usize) -> i32 {
        if self.reflected_attack_damage == 0 {
            return 0;
        }
        // Try scaling via damage_per_level ratio
        if !self.damage_per_level.is_empty() && self.damage > 0 {
            let lvl_dmg = self.damage_per_level
                .get(level.saturating_sub(1))
                .copied()
                .unwrap_or(self.damage);
            return (self.reflected_attack_damage as i64 * lvl_dmg as i64 / self.damage as i64) as i32;
        }
        // Fallback: scale via hitpoints_per_level ratio
        if !self.hitpoints_per_level.is_empty() && self.hitpoints > 0 {
            let lvl_hp = self.hitpoints_per_level
                .get(level.saturating_sub(1))
                .copied()
                .unwrap_or(self.hitpoints);
            return (self.reflected_attack_damage as i64 * lvl_hp as i64 / self.hitpoints as i64) as i32;
        }
        self.reflected_attack_damage
    }
}

// ---------------------------------------------------------------------------
// Projectile Stats — from cards_stats_projectile.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectileStats {
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub speed: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub homing: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub damage: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub crown_tower_damage_percent: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub pushback: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub pushback_all: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub radius: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub aoe_to_air: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub aoe_to_ground: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub gravity: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub hit_biggest: bool,
    #[serde(default, deserialize_with = "null_or_vec_i32")]
    pub damage_per_level: Vec<i32>,
    // Fields used by projectile-spells (Goblin Barrel, Royal Delivery, etc.)
    #[serde(default)]
    pub spawn_character: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_character_count: i32,
    /// Buff to apply on hit (e.g., Snowball applies IceWizardSlowDown)
    #[serde(default)]
    pub target_buff: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_time: i32,

    /// If true, apply the target buff BEFORE dealing damage (Mother Witch VoodooCurse).
    #[serde(default, deserialize_with = "null_or_bool")]
    pub apply_buff_before_damage: bool,

    // ── Rolling projectile fields (Log, Barb Barrel) ──
    /// AoE radius for rolling projectiles (half-width of the rolling hitbox).
    /// Log=1950, BarbBarrel=1300. Used when `radius` is 0.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub projectile_radius: i32,
    /// Half-depth of the rolling hitbox (Y extent). Log=600, BarbBarrel=600.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub projectile_radius_y: i32,
    /// Total travel distance in internal units. Log=10100, BarbBarrel=4500.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub projectile_range: i32,
    /// Boomerang visual return time in ms. When > 0, this projectile is a boomerang
    /// (Executioner axe) — travels out to target, then returns to source, dealing
    /// AoE damage both ways. The value is the visual duration of the return trip.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub pingpong_visual_time: i32,
    /// Muzzle spread for scatter projectiles (Hunter: 650).
    /// Bullets start spread across this radius at the shooter's position.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub projectile_start_extra_radius: i32,
    /// Whether this projectile only hits air units.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub hits_air: bool,

    /// Deploy time in ms for spawned troops after projectile lands.
    /// Used by Goblin Barrel (1100ms), Barb Barrel (500ms), etc.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_character_deploy_time: i32,

    // ─── Fisherman hook drag fields ───
    /// If true, projectile drags target back to source on hit.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub drag_back_as_attractor: bool,
    /// Speed at which target is dragged back (internal units).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub drag_back_speed: i32,
    /// Speed at which source drags itself to buildings.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub drag_self_speed: i32,
    /// Margin distance to stop dragging.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub drag_margin: i32,

    /// Scatter pattern for multi-projectile attacks.
    /// "Line" = bullets spread in a cone (Hunter shotgun).
    /// None = all projectiles converge on the same target point.
    #[serde(default)]
    pub scatter: Option<String>,

    /// Chain hit radius — maximum distance from the previous hit target to the
    /// next chain bounce target. ElectroDragonProjectile: 4000, ZapSpiritProjectile: 5500.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub chained_hit_radius: i32,
    /// Chain hit count — total number of targets the projectile can hit (including primary).
    /// ElectroDragonProjectile: 3, ZapSpiritProjectile: 3.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub chained_hit_count: i32,

    /// Spell zone to spawn on impact (HealSpiritProjectile → HealSpirit zone).
    /// Creates a spell zone at the impact point that applies buffs/heals.
    #[serde(default)]
    pub spawn_area_effect_object: Option<String>,
}

// ---------------------------------------------------------------------------
// Spell Stats — from cards_stats_spell.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SpellStats {
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub key: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub sc_key: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub rarity: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub life_duration: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub life_duration_increase_per_level: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub affects_hidden: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub radius: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub pushback: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub min_pushback: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub max_pushback: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub pushback_all: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub hit_speed: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub damage: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub no_effect_to_crown_towers: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub crown_tower_damage_percent: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub only_enemies: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub only_own_troops: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub aoe_to_air: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub aoe_to_ground: bool,
    /// Alternate field names used by some spell entries
    #[serde(default, deserialize_with = "null_or_bool")]
    pub hits_air: bool,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub hits_ground: bool,
    #[serde(default)]
    pub buff: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub buff_time: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub heal_per_second: i32,
    #[serde(default, deserialize_with = "null_or_vec_i32")]
    pub damage_per_level: Vec<i32>,

    // Spawner spells (Graveyard, Skeleton King ability)
    #[serde(default)]
    pub spawn_character: Option<String>,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_interval: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_initial_delay: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_time: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_min_radius: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub can_deploy_on_enemy_side: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub maximum_targets: i32,
    #[serde(default, deserialize_with = "null_or_bool")]
    pub hit_biggest_targets: bool,
    #[serde(default)]
    pub projectile: Option<String>,
}

// ---------------------------------------------------------------------------
// Buff Stats — from cards_stats_character_buff.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct BuffStats {
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub rarity: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub speed_multiplier: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub hit_speed_multiplier: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub damage_per_second: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub heal_per_second: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub damage_reduction: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub spawn_speed_multiplier: i32,

    // ─── Multiplicative stat modifiers (Fix #7+#8) ───
    /// Damage multiplier as a percentage. TripleDamage=300 (3× damage),
    /// GrowthBoost=120 (1.2× damage). 0 = no modifier (default).
    /// Applied as a multiplicative factor: effective_damage = base * multiplier / 100.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub damage_multiplier: i32,
    /// Hitpoint multiplier as a percentage. GrowthBoost=120 (1.2× max HP).
    /// 0 = no modifier (default). When applied, both max_hp and current hp
    /// are scaled proportionally so the troop doesn't lose effective HP%.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub hitpoint_multiplier: i32,

    #[serde(default, deserialize_with = "null_or_bool")]
    pub no_effect_to_crown_towers: bool,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub building_damage_percent: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub crown_tower_damage_percent: i32,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub hit_frequency: i32,

    // ── Displacement / attraction (Tornado) ──
    /// Pull strength as a percentage. Tornado = 360.
    /// 0 = no pull. Higher = stronger attraction toward spell center.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub attract_percentage: i32,
    /// Push speed factor (affects pull velocity). Tornado = 100.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub push_speed_factor: i32,
    /// Push mass factor (heavier units resist more). Currently unused in CR data.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub push_mass_factor: i32,
    /// If true, the buff's displacement is controlled by the parent spell zone
    /// (i.e., the zone itself handles pulling, not the buff tick).
    #[serde(default, deserialize_with = "null_or_bool")]
    pub controlled_by_parent: bool,

    /// If true, this buff triggers troop duplication (Clone spell).
    /// Cloned troops have 1 HP but retain all other stats.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub clone: bool,

    /// If true, this buff makes the troop invisible (Royal Ghost, Archer Queen).
    /// Invisible troops cannot be targeted by enemies.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub invisible: bool,

    /// If true, multiple instances of this buff stack (e.g., Poison).
    #[serde(default, deserialize_with = "null_or_bool")]
    pub enable_stacking: bool,

    /// If true, this buff is removed when the troop attacks.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub remove_on_attack: bool,

    /// Allowed overheal percentage (0 = no overheal).
    #[serde(default, deserialize_with = "null_or_i32")]
    pub allowed_over_heal_perc: i32,

    // ── Death spawn (Mother Witch VoodooCurse) ──
    /// Character key to spawn when the buffed unit dies (e.g., "VoodooHog").
    #[serde(default)]
    pub death_spawn: Option<String>,
    /// Number of units to spawn on death.
    #[serde(default, deserialize_with = "null_or_i32")]
    pub death_spawn_count: i32,
    /// If true, the spawned unit fights for the ENEMY of the dying unit
    /// (i.e., the team that applied the buff/curse).
    #[serde(default, deserialize_with = "null_or_bool")]
    pub death_spawn_is_enemy: bool,
    /// If true, ignore buildings when checking curse targets.
    #[serde(default, deserialize_with = "null_or_bool")]
    pub ignore_buildings: bool,
}

// ---------------------------------------------------------------------------
// Evolution Ability — from evo_hero_abilities.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct StatModifiers {
    #[serde(default)]
    pub hitpoints_multiplier: Option<f64>,
    #[serde(default)]
    pub damage_multiplier: Option<f64>,
    #[serde(default)]
    pub hit_speed_multiplier: Option<f64>,
    #[serde(default)]
    pub speed_override: Option<i32>,
    #[serde(default)]
    pub spawn_count_override: Option<i32>,
    #[serde(default)]
    pub range_override: Option<i32>,
    #[serde(default)]
    pub shield_hitpoints: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvoTrigger {
    #[serde(default, deserialize_with = "null_or_string")]
    pub condition: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvoEffect {
    #[serde(default, deserialize_with = "null_or_string")]
    pub effect_type: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub target: String,
    #[serde(default)]
    pub radius: Option<i32>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<i32>,
    #[serde(default)]
    pub damage: Option<i32>,
    #[serde(default)]
    pub spawn_character: Option<String>,
    #[serde(default)]
    pub spawn_count: Option<i32>,
    #[serde(default)]
    pub spawn_interval_ms: Option<i32>,
    #[serde(default)]
    pub buff_reference: Option<String>,
    #[serde(default)]
    pub pull_strength: Option<i32>,
    #[serde(default)]
    pub affects_air: Option<bool>,
    #[serde(default)]
    pub affects_ground: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AbilityDef {
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default)]
    pub trigger: EvoTrigger,
    #[serde(default)]
    pub effects: Vec<EvoEffect>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvoAbility {
    #[serde(default, deserialize_with = "null_or_string")]
    pub id: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub base_card_key: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub elixir: i32,
    #[serde(default)]
    pub stat_modifiers: StatModifiers,
    #[serde(default)]
    pub ability: AbilityDef,
}

// ---------------------------------------------------------------------------
// Hero Ability — from evo_hero_abilities.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct HeroEffect {
    #[serde(default, deserialize_with = "null_or_string")]
    pub effect_type: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub target: String,
    #[serde(default)]
    pub radius: Option<i32>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<i32>,
    #[serde(default)]
    pub damage: Option<i32>,
    #[serde(default)]
    pub spawn_character: Option<String>,
    #[serde(default)]
    pub spawn_count: Option<i32>,
    #[serde(default)]
    pub buff_reference: Option<String>,
    #[serde(default)]
    pub affects_air: Option<bool>,
    #[serde(default)]
    pub affects_ground: Option<bool>,
    #[serde(default)]
    pub taunt_radius: Option<i32>,
    #[serde(default)]
    pub taunt_duration_ms: Option<i32>,
    #[serde(default)]
    pub heal_amount: Option<i32>,
    #[serde(default)]
    pub shield_hitpoints: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeroAbilityDef {
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub elixir_cost: i32,
    #[serde(default, deserialize_with = "null_or_string")]
    pub activation: String,
    #[serde(default)]
    pub effects: Vec<HeroEffect>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeroAbility {
    #[serde(default, deserialize_with = "null_or_string")]
    pub id: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub base_card_key: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub elixir: i32,
    #[serde(default)]
    pub stat_modifiers: StatModifiers,
    #[serde(default)]
    pub ability: HeroAbilityDef,
}

/// Wrapper for evo_hero_abilities.json top-level structure
#[derive(Debug, Clone, Deserialize)]
pub struct EvoHeroFile {
    #[serde(default)]
    pub evolutions: Vec<EvoAbility>,
    #[serde(default)]
    pub heroes: Vec<HeroAbility>,
}

// ---------------------------------------------------------------------------
// GameData — the master data store, loaded once, borrowed immutably
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// CardInfo — from cards.json (the playable card registry)
// ---------------------------------------------------------------------------

/// Represents a playable card from cards.json. This is the authority for
/// what cards exist, their elixir cost, and their type (Troop/Building/Spell).
/// The stats files (characters, buildings, spells) provide runtime mechanics.
#[derive(Debug, Clone, Deserialize)]
pub struct CardInfo {
    #[serde(default, deserialize_with = "null_or_string")]
    pub key: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub name: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub sc_key: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub elixir: i32,
    #[serde(default, rename = "type")]
    pub card_type: String,
    #[serde(default, deserialize_with = "null_or_string")]
    pub rarity: String,
    #[serde(default, deserialize_with = "null_or_i32")]
    pub arena: i32,
    #[serde(default, deserialize_with = "null_or_i64")]
    pub id: i64,
}

/// All game data from the JSON files, indexed by key/name for O(1) lookup.
/// Created once at startup. Passed as `&GameData` to every match.
pub struct GameData {
    pub characters: HashMap<String, CharacterStats>,
    pub buildings: HashMap<String, CharacterStats>,
    pub projectiles: HashMap<String, ProjectileStats>,
    pub spells: HashMap<String, SpellStats>,
    pub buffs: HashMap<String, BuffStats>,
    pub evolutions: HashMap<String, EvoAbility>,
    pub heroes: HashMap<String, HeroAbility>,
    /// Playable cards from cards.json — the authority for elixir costs and card types.
    pub card_registry: HashMap<String, CardInfo>,
    /// Projectile-type spells (Rocket, Fireball, Arrows, Log, Goblin Barrel, etc.)
    /// Keyed by card key (e.g., "rocket", "fireball"). These are spell cards whose
    /// stats live in cards_stats_projectile.json rather than cards_stats_spell.json.
    pub spell_projectiles: HashMap<String, ProjectileStats>,
}

/// Normalize a string for fuzzy key matching: lowercase, strip hyphens/spaces/underscores/dots.
fn normalize_key(s: &str) -> String {
    s.to_lowercase()
        .replace('-', "")
        .replace(' ', "")
        .replace('_', "")
        .replace('.', "")
}

impl GameData {
    /// Load all game data from the data/ directory.
    /// Expects:
    ///   {data_dir}/royaleapi/cards.json                      ← card registry
    ///   {data_dir}/royaleapi/cards_stats_characters.json
    ///   {data_dir}/royaleapi/cards_stats_building.json
    ///   {data_dir}/royaleapi/cards_stats_projectile.json
    ///   {data_dir}/royaleapi/cards_stats_spell.json
    ///   {data_dir}/royaleapi/cards_stats_character_buff.json
    ///   {data_dir}/wiki/evo_hero_abilities.json
    pub fn load(data_dir: &str) -> Result<Self, String> {
        let base = Path::new(data_dir);
        let royale = base.join("royaleapi");
        let wiki = base.join("wiki");

        // ── Step 1: Load cards.json (the playable card registry) ──
        // Filter out event/variant cards that aren't standard ladder cards.
        // General rule: exclude cards whose name starts with variant prefixes
        // (Super, Santa, Raging) or specific known non-standard entries.
        // This is future-proof — new "Super X" event cards auto-exclude.
        let card_list: Vec<CardInfo> = load_json(&royale.join("cards.json"))?;
        let card_registry: HashMap<String, CardInfo> = card_list
            .into_iter()
            .filter(|c| {
                if c.key.is_empty() || c.elixir <= 0 {
                    return false;
                }
                // Exclude event/variant cards by name prefix
                let dominated = c.name.starts_with("Super ")
                    || c.name.starts_with("Santa ")
                    || c.name.starts_with("Raging ");
                // Exclude specific non-standard entries
                let special = c.key == "terry";
                !dominated && !special
            })
            .map(|c| (c.key.clone(), c))
            .collect();

        // Build normalized lookup: normalized(sc_key) → CardInfo.key for elixir patching
        let mut card_by_norm: HashMap<String, (String, i32)> = HashMap::new(); // norm → (card_key, elixir)
        for ci in card_registry.values() {
            card_by_norm.insert(normalize_key(&ci.key), (ci.key.clone(), ci.elixir));
            card_by_norm.insert(normalize_key(&ci.sc_key), (ci.key.clone(), ci.elixir));
            card_by_norm.insert(normalize_key(&ci.name), (ci.key.clone(), ci.elixir));
            // Also try singular (strip trailing 's') for multi-unit cards
            let norm_sc = normalize_key(&ci.sc_key);
            if norm_sc.ends_with('s') {
                let singular = &norm_sc[..norm_sc.len()-1];
                card_by_norm.entry(singular.to_string())
                    .or_insert((ci.key.clone(), ci.elixir));
            }
        }

        // ── Step 2: Load characters, derive keys for keyless entries ──
        let chars: Vec<CharacterStats> = load_json(&royale.join("cards_stats_characters.json"))?;
        let mut characters: HashMap<String, CharacterStats> = HashMap::new();

        for mut c in chars {
            // Derive key from name if missing
            if c.key.is_empty() {
                if !c.name.is_empty() {
                    c.key = c.name.to_lowercase().replace(' ', "-");
                } else if !c.sc_key.is_empty() {
                    c.key = c.sc_key.to_lowercase();
                } else {
                    continue; // Skip completely empty entries
                }
            }

            // Patch elixir from cards.json if the character's elixir is 0
            if c.elixir <= 0 {
                let norm = normalize_key(&c.key);
                if let Some((_card_key, elixir)) = card_by_norm.get(&norm) {
                    c.elixir = *elixir;
                } else {
                    // Try name-based match
                    let norm_name = normalize_key(&c.name);
                    if let Some((_card_key, elixir)) = card_by_norm.get(&norm_name) {
                        c.elixir = *elixir;
                    }
                }
            }

            characters.insert(c.key.clone(), c);
        }

        // ── Step 3: Ensure every playable troop card has an entry in characters ──
        // For multi-unit cards (Goblins, Skeleton Army, etc.), cards.json has the card
        // but characters.json has the individual unit. We need a character entry keyed
        // by the card key so play_card can find it.

        // Manual overrides for multi-unit cards and cards whose unit name doesn't
        // match sc_key. Format: (card_key, unit_key, summon_count, secondary_unit, secondary_count, spawn_radius_mult)
        // spawn_radius_mult: multiplier on collision_radius for formation spread.
        //   1.0 = default (one collision_radius). Tune per-card for accurate CR formations.
        // These are ALL cards that deploy multiple units or whose stats entry
        // has summon_number=0 despite being a multi-deploy card.
        let manual_overrides: Vec<(&str, &str, i32, &str, i32, f32)> = vec![
            // ── Cards with mismatched names (can't resolve algorithmically) ──
            ("skeleton-army",    "skeleton",        15, "",            0, 2.5), // 15 Skeletons — wide circle
            ("minion-horde",     "minion",           6, "",            0, 2.0), // 6 Minions — wider cluster
            ("goblin-gang",      "goblin",           3, "speargoblin", 2, 1.2), // 3 Goblins + 2 Spear Goblins — hybrid cluster
            ("royal-recruits",   "recruit",          6, "",            0, 1.0), // 6 Recruits (line deploy, multiplier unused)
            ("zappies",          "minizapmachine",   3, "",            0, 1.0), // 3 Zappies
            ("rascals",          "rascalboy",        1, "rascalgirl",  2, 1.0), // 1 Boy + 2 Girls
            ("elixir-golem",     "elixirgolem1",     1, "",            0, 1.0), // 1 Golem (splits on death)
            ("heal-spirit",      "healspirit",       1, "",            0, 1.0), // 1 Spirit
            // ── Multi-unit cards resolved by singular match but missing summon_number ──
            ("barbarians",       "barbarian",        5, "",            0, 1.5), // 5 Barbarians — semi-circle arc
            ("goblins",          "goblin",           3, "",            0, 1.0), // 3 Goblins
            ("spear-goblins",    "speargoblin",      3, "",            0, 1.0), // 3 Spear Goblins
            ("skeletons",        "skeleton",         3, "",            0, 1.0), // 3 Skeletons — tight triangle
            ("minions",          "minion",           3, "",            0, 1.5), // 3 Minions — slight spread
            ("bats",             "bat",              5, "",            0, 1.0), // 5 Bats (circular via spawn_angle_shift)
            ("wall-breakers",    "wallbreaker",      2, "",            0, 1.0), // 2 Wall Breakers
            ("royal-hogs",       "royalhog",         4, "",            0, 1.5), // 4 Royal Hogs
            ("skeleton-dragons", "skeletondragon",   2, "",            0, 1.0), // 2 Skeleton Dragons
            ("three-musketeers", "musketeer",        3, "",            0, 1.0), // 3 Musketeers
            ("elite-barbarians", "angrybarbarian",   2, "",            0, 1.0), // 2 Elite Barbarians — lateral pair
            ("guards",           "skeletonwarrior",  3, "",            0, 1.0), // 3 Guards — triangle
            // ── Single-unit cards with name mismatches (safety net) ──
            ("royal-ghost",     "ghost",           1, "",            0, 1.0), // Royal Ghost
            ("ice-spirit",      "icespirits",      1, "",            0, 1.0), // Ice Spirit
            ("fire-spirit",     "firespirits",     1, "",            0, 1.0), // Fire Spirit
            ("electro-spirit",  "electrospirit",   1, "",            0, 1.0), // Electro Spirit
            ("dark-prince",     "darkprince",      1, "",            0, 1.0), // Dark Prince
            ("night-witch",     "darkwitch",       1, "",            0, 1.0), // Night Witch
            ("mega-minion",     "megaminion",      1, "",            0, 1.0), // Mega Minion
            ("inferno-dragon",  "infernodragon",   1, "",            0, 1.0), // Inferno Dragon
            ("electro-dragon",  "electrodragon",   1, "",            0, 1.0), // Electro Dragon
            ("cannon-cart",     "movingcannon",    1, "",            0, 1.0), // Cannon Cart
            ("skeleton-barrel", "skeletonballoon", 1, "",            0, 1.0), // Skeleton Barrel
            ("mother-witch",    "witchmother",     1, "",            0, 1.0), // Mother Witch
            ("dart-goblin",     "blowdartgoblin",  1, "",            0, 1.0), // Dart Goblin
            ("baby-dragon",     "babydragon",      1, "",            0, 1.0), // Baby Dragon
            ("ice-golem",       "icegolemite",     1, "",            0, 1.0), // Ice Golem
            // Ram Rider: card spawns the Ram (mount, building-target, charge).
            ("ram-rider",       "ram",             1, "",            0, 1.0), // Ram Rider → Ram mount
        ];

        // Apply manual overrides first
        for (card_key, unit_key, summon_count, second_unit, second_count, radius_mult) in &manual_overrides {
            if characters.contains_key(*card_key) {
                continue; // Already resolved
            }
            if let Some(ci) = card_registry.get(*card_key) {
                if let Some(unit_stats) = characters.get(*unit_key) {
                    let mut card_entry = unit_stats.clone();
                    card_entry.key = card_key.to_string();
                    card_entry.elixir = ci.elixir;
                    // Set multi-deploy fields
                    card_entry.summon_number = *summon_count;
                    card_entry.summon_character = Some(unit_key.to_string());
                    card_entry.spawn_radius_multiplier = *radius_mult;
                    // Secondary summon (Goblin Gang spear goblins, Rascals girls)
                    if !second_unit.is_empty() && *second_count > 0 {
                        card_entry.summon_character_second = Some(second_unit.to_string());
                        card_entry.summon_character_second_count = *second_count;
                    }
                    characters.insert(card_key.to_string(), card_entry);
                }
            }
        }

        // Then try algorithmic matching for remaining unmatched cards
        for ci in card_registry.values() {
            if ci.card_type != "Troop" {
                continue;
            }
            if characters.contains_key(&ci.key) {
                continue; // Already have it (either directly or from manual override)
            }
            // Try to find the unit stats via sc_key (normalized)
            let norm_sc = normalize_key(&ci.sc_key);
            // Try singular form
            let singular = if norm_sc.ends_with('s') {
                norm_sc[..norm_sc.len()-1].to_string()
            } else {
                norm_sc.clone()
            };

            // Search existing characters for a match
            let unit_key = characters.keys()
                .find(|k| {
                    let nk = normalize_key(k);
                    nk == norm_sc || nk == singular
                })
                .cloned();

            if let Some(ref uk) = unit_key {
                // Clone the unit stats and re-key under the card key
                let mut card_entry = characters.get(uk).unwrap().clone();
                card_entry.key = ci.key.clone();
                card_entry.elixir = ci.elixir;
                characters.insert(ci.key.clone(), card_entry);
            }
        }

        // ── Step 4: Load buildings with same key derivation ──
        let bldgs: Vec<CharacterStats> = load_json(&royale.join("cards_stats_building.json"))?;
        let mut buildings: HashMap<String, CharacterStats> = HashMap::new();

        for mut b in bldgs {
            if b.key.is_empty() {
                if !b.name.is_empty() {
                    b.key = b.name.to_lowercase().replace(' ', "-");
                } else if !b.sc_key.is_empty() {
                    b.key = b.sc_key.to_lowercase();
                } else {
                    continue;
                }
            }
            // Patch elixir from cards.json
            if b.elixir <= 0 {
                let norm = normalize_key(&b.key);
                if let Some((_card_key, elixir)) = card_by_norm.get(&norm) {
                    b.elixir = *elixir;
                }
            }
            buildings.insert(b.key.clone(), b);
        }

        // ── Step 5: Load remaining data files ──
        let projs: Vec<ProjectileStats> = load_json(&royale.join("cards_stats_projectile.json"))?;
        let projectiles: HashMap<String, ProjectileStats> = projs
            .into_iter()
            .filter(|p| !p.name.is_empty())
            .map(|p| (p.name.clone(), p))
            .collect();

        // ── Step 5b: Patch ranged troop damage from projectile data ──
        // In the RoyaleAPI data, ranged troops have damage=0 in the character entry.
        // The damage lives in the projectile file instead. We copy it over so that
        // Entity::new_troop gets the correct damage value at spawn time.
        //
        // We also set damage_from_projectile=true for these characters. This is the
        // data-driven signal that the character's primary attack IS the projectile.
        // Melee troops with secondary projectiles (Ram Rider bola, Fisherman hook)
        // have damage>0 in the JSON — they won't enter this branch, so their flag
        // stays false and is_ranged() correctly returns false for them.
        let mut ranged_patched = 0;
        for (_key, char_stats) in characters.iter_mut() {
            if let Some(ref proj_name) = char_stats.projectile {
                if char_stats.damage == 0 {
                    // Try direct lookup, then Deco fallback for Princess etc.
                    let mut proj_stats_found = projectiles.get(proj_name.as_str());
                    // If primary projectile has no damage data, try stripping "Deco" suffix
                    if let Some(ps) = proj_stats_found {
                        if ps.damage <= 0 && ps.damage_per_level.is_empty() {
                            if let Some(stripped) = proj_name.strip_suffix("Deco") {
                                if let Some(real) = projectiles.get(stripped) {
                                    proj_stats_found = Some(real);
                                }
                            }
                        }
                    }
                    if let Some(proj_stats) = proj_stats_found {
                        // Copy base damage
                        if proj_stats.damage > 0 {
                            char_stats.damage = proj_stats.damage;
                            ranged_patched += 1;
                        }
                        // Copy damage_per_level if character doesn't have its own
                        if char_stats.damage_per_level.is_empty()
                            && !proj_stats.damage_per_level.is_empty()
                        {
                            char_stats.damage_per_level = proj_stats.damage_per_level.clone();
                        }
                        // Also copy crown_tower_damage_percent if not set
                        if char_stats.crown_tower_damage_percent == 0
                            && proj_stats.crown_tower_damage_percent > 0
                        {
                            char_stats.crown_tower_damage_percent =
                                proj_stats.crown_tower_damage_percent;
                        }
                    }
                    // Mark this character as truly ranged: its primary attack is
                    // the projectile (damage came from projectile data, not character JSON).
                    char_stats.damage_from_projectile = true;
                }
            }
        }
        // Same for buildings with projectiles (Tesla, X-Bow, etc.)
        for (_key, bld_stats) in buildings.iter_mut() {
            if let Some(ref proj_name) = bld_stats.projectile {
                if bld_stats.damage == 0 {
                    if let Some(proj_stats) = projectiles.get(proj_name) {
                        if proj_stats.damage > 0 {
                            bld_stats.damage = proj_stats.damage;
                        }
                        if bld_stats.damage_per_level.is_empty()
                            && !proj_stats.damage_per_level.is_empty()
                        {
                            bld_stats.damage_per_level = proj_stats.damage_per_level.clone();
                        }
                    }
                    // Mark this building as truly ranged (same logic as characters).
                    bld_stats.damage_from_projectile = true;
                }
            }
        }
//       if ranged_patched > 0 {
//            println!("[GameData] Patched damage from projectiles for {} ranged troops", ranged_patched);
//        }

        // ── Step 5c: Normalize death_spawn_count ──
        // The RoyaleAPI data is scraped from Supercell's CSV files where blank
        // numeric cells become 0 in JSON. When death_spawn_character is set
        // (e.g., Balloon → "BalloonBomb", Lumberjack → "RageBarbarianBottle"),
        // a count of 0 means the field was blank in the source CSV, not that
        // zero entities should spawn. Normalize to 1 so combat.rs can read
        // the data as-is without runtime heuristics.
        for (_key, char_stats) in characters.iter_mut() {
            if char_stats.death_spawn_character.is_some() && char_stats.death_spawn_count == 0 {
                char_stats.death_spawn_count = 1;
            }
            if char_stats.death_spawn_character2.is_some() && char_stats.death_spawn_count2 == 0 {
                char_stats.death_spawn_count2 = 1;
            }
        }
        // Same for buildings (Bomb Tower → BombTowerBomb, etc.)
        for (_key, bld_stats) in buildings.iter_mut() {
            if bld_stats.death_spawn_character.is_some() && bld_stats.death_spawn_count == 0 {
                bld_stats.death_spawn_count = 1;
            }
            if bld_stats.death_spawn_character2.is_some() && bld_stats.death_spawn_count2 == 0 {
                bld_stats.death_spawn_count2 = 1;
            }
        }

        let spls: Vec<SpellStats> = load_json(&royale.join("cards_stats_spell.json"))?;
        let mut spells: HashMap<String, SpellStats> = HashMap::new();
        for mut s in spls {
            if s.name.is_empty() {
                continue;
            }
            // Merge alternate field names: hits_air → aoe_to_air
            if !s.aoe_to_air && s.hits_air {
                s.aoe_to_air = true;
            }
            if !s.aoe_to_ground && s.hits_ground {
                s.aoe_to_ground = true;
            }
            // Index by name (e.g., "Freeze")
            spells.insert(s.name.clone(), s.clone());
            // Also index by key if present (e.g., "freeze")
            if !s.key.is_empty() && !spells.contains_key(&s.key) {
                spells.insert(s.key.clone(), s);
            }
        }

        // Add card key aliases for spells so they can be found by cards.json key.
        // Spell stats are keyed by name (e.g., "Zap") but decks use card key (e.g., "zap").
        for ci in card_registry.values() {
            if ci.card_type != "Spell" {
                continue;
            }
            if let Some(spell) = spells.get(&ci.sc_key).cloned() {
                if !spells.contains_key(&ci.key) {
                    spells.insert(ci.key.clone(), spell);
                }
            } else {
                let norm_sc = normalize_key(&ci.sc_key);
                let match_key = spells.keys()
                    .find(|k| normalize_key(k) == norm_sc)
                    .cloned();
                if let Some(ref mk) = match_key {
                    let spell = spells.get(mk).unwrap().clone();
                    if !spells.contains_key(&ci.key) {
                        spells.insert(ci.key.clone(), spell);
                    }
                }
            }
        }

        let bffs: Vec<BuffStats> = load_json(&royale.join("cards_stats_character_buff.json"))?;
        let buffs: HashMap<String, BuffStats> = bffs
            .into_iter()
            .filter(|b| !b.name.is_empty())
            .map(|b| (b.name.clone(), b))
            .collect();

        let evo_hero: EvoHeroFile = load_json(&wiki.join("evo_hero_abilities.json"))?;
        let evolutions: HashMap<String, EvoAbility> = evo_hero
            .evolutions
            .into_iter()
            .map(|e| (e.base_card_key.clone(), e))
            .collect();
        let heroes: HashMap<String, HeroAbility> = evo_hero
            .heroes
            .into_iter()
            .map(|h| (h.base_card_key.clone(), h))
            .collect();

        // Count playable cards (elixir > 0) for the log
        let playable_chars = characters.values().filter(|c| c.elixir > 0).count();
        let playable_bldgs = buildings.values().filter(|b| b.elixir > 0).count();

        // ── Step 6b: Cross-reference projectile-type spells ──
        // Some spell cards (Rocket, Fireball, Arrows, Log, Goblin Barrel, etc.)
        // have their stats in cards_stats_projectile.json, not cards_stats_spell.json.
        // We identify them by: card_type == "Spell" AND no entry in `spells` map AND
        // a matching projectile entry exists. Keyed by card_key for play_card lookup.
        let mut spell_projectiles: HashMap<String, ProjectileStats> = HashMap::new();

        // Known mapping: card sc_key → projectile name patterns to try
        for ci in card_registry.values() {
            if ci.card_type != "Spell" {
                continue;
            }
            // Skip if already in zone spells
            if spells.contains_key(&ci.key) {
                continue;
            }
            // Try common projectile name patterns:
            //   "Rocket" → "RocketSpell"
            //   "Log" → "LogProjectileRolling" (the rolling part has damage)
            //   "BarbLog" → "BarbLogProjectileRolling"
            //   "GoblinBarrel" → "GoblinBarrelSpell"
            //   "Snowball" → "SnowballSpell"
            //   "RoyalDelivery" → "RoyalDeliveryProjectile"
            //   "Arrows" → "ArrowsSpell"
            //   "Fireball" → "FireballSpell"
            let candidates = [
                format!("{}Spell", ci.sc_key),
                format!("{}Projectile", ci.sc_key),
                format!("{}ProjectileRolling", ci.sc_key),
                ci.sc_key.clone(),
            ];

            let mut best: Option<&ProjectileStats> = None;
            for cand in &candidates {
                if let Some(ps) = projectiles.get(cand) {
                    // Prefer the candidate with highest damage, or highest spawn_count.
                    // This ensures GoblinBarrelSpell (spawn_count=3) beats
                    // GoblinBarrel (spawn_count=1), and LogProjectileRolling
                    // (damage=240) beats LogProjectile (damage=0).
                    if best.is_none() {
                        best = Some(ps);
                    } else {
                        let b = best.unwrap();
                        let ps_score = ps.damage + ps.spawn_character_count * 100;
                        let b_score = b.damage + b.spawn_character_count * 100;
                        if ps_score > b_score {
                            best = Some(ps);
                        }
                    }
                }
            }

            // Fallback: scan projectiles for any name containing the sc_key
            if best.is_none() {
                let sc_lower = ci.sc_key.to_lowercase();
                for ps in projectiles.values() {
                    let pname = ps.name.to_lowercase();
                    if pname.contains(&sc_lower) && (ps.damage > 0 || ps.spawn_character.is_some()) {
                        best = Some(ps);
                        break;
                    }
                }
            }

            if let Some(ps) = best {
                spell_projectiles.insert(ci.key.clone(), ps.clone());
            }
        }

        Ok(GameData {
            characters,
            buildings,
            projectiles,
            spells,
            buffs,
            evolutions,
            heroes,
            card_registry,
            spell_projectiles,
        })
    }

    /// Look up a character by key, with normalized fallback.
    /// Tries: direct key → lowercase → normalized (no hyphens/spaces).
    pub fn find_character(&self, name: &str) -> Option<&CharacterStats> {
        // Direct key lookup
        if let Some(stats) = self.characters.get(name) {
            return Some(stats);
        }
        // Lowercase-hyphenated key lookup
        let lower = name.to_lowercase().replace(' ', "-");
        if let Some(stats) = self.characters.get(&lower) {
            return Some(stats);
        }
        // Normalized key lookup (strips hyphens/spaces/underscores)
        let norm = normalize_key(name);
        if let Some(found) = self.characters.iter()
            .find(|(k, _)| !k.is_empty() && normalize_key(k) == norm)
            .map(|(_, v)| v)
        {
            return Some(found);
        }
        // Name-based lookup: match against CharacterStats.name field.
        // This handles characters with empty keys (e.g., Goblin, PhoenixEgg,
        // ElixirGolem2, etc.) that are internal-only and not in the card registry.
        self.characters.values()
            .find(|cs| {
                if cs.name.is_empty() { return false; }
                cs.name == name
                    || cs.name.to_lowercase() == lower
                    || normalize_key(&cs.name) == norm
            })
    }
}

/// Helper: read a JSON file and deserialize into type T.
fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

// ---------------------------------------------------------------------------
// Default implementations for serde
// ---------------------------------------------------------------------------

impl Default for StatModifiers {
    fn default() -> Self {
        StatModifiers {
            hitpoints_multiplier: None,
            damage_multiplier: None,
            hit_speed_multiplier: None,
            speed_override: None,
            spawn_count_override: None,
            range_override: None,
            shield_hitpoints: None,
        }
    }
}

impl Default for EvoTrigger {
    fn default() -> Self {
        EvoTrigger {
            condition: String::new(),
        }
    }
}

impl Default for AbilityDef {
    fn default() -> Self {
        AbilityDef {
            name: String::new(),
            trigger: EvoTrigger::default(),
            effects: Vec::new(),
        }
    }
}

impl Default for HeroAbilityDef {
    fn default() -> Self {
        HeroAbilityDef {
            name: String::new(),
            elixir_cost: 0,
            activation: String::new(),
            effects: Vec::new(),
        }
    }
}
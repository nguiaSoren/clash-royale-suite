# Engine Patches for Batch 21 Test Failures

## 19 test failures → 7 root causes → 5 engine fixes + test corrections

---

## FIX 1: Mortar `minimum_range` not parsed or enforced

**Symptoms:** Tests 1061, 1062 — Mortar damages enemies inside dead zone (min_range=3500)

**Root cause:** `minimum_range` from building JSON is never parsed into `CharacterStats`, never stored in `BuildingData`, and never checked in combat targeting.

### Patch A — `data_types.rs` (add field to CharacterStats)

After line ~238 (near `pub spawn_pushback_radius: i32`), add:

```rust
    // Building dead zone (Mortar)
    #[serde(default, deserialize_with = "null_or_i32")]
    pub minimum_range: i32,
```

### Patch B — `entities.rs` (add to BuildingData + constructor)

In `BuildingData` struct (after `pub range_sq: i64,` around line 624), add:

```rust
    /// Minimum attack range squared. Targets closer than this are ignored (Mortar dead zone).
    pub min_range_sq: i64,
```

In `new_building()` (inside the `BuildingData { ... }` initializer, after `range_sq: range_squared(stats.range),`), add:

```rust
                min_range_sq: range_squared(stats.minimum_range),
```

### Patch C — `combat.rs` (enforce min range in targeting + combat)

**In `tick_targeting`** — where buildings acquire targets (~line 240 area), when evaluating candidates for building targeting, add a minimum range check. Buildings should not acquire targets inside min_range.

Find the building targeting section where `dist_sq <= bld.range_sq` is checked for target acquisition. Add `&& dist_sq >= bld.min_range_sq` wherever target selection happens for buildings.

**In `tick_combat`** — line 1318, change:

```rust
// BEFORE:
if dist_sq <= bld.range_sq && bld.attack_cooldown <= 0 {

// AFTER:
if dist_sq <= bld.range_sq && dist_sq >= bld.min_range_sq && bld.attack_cooldown <= 0 {
```

This prevents the Mortar from attacking targets inside its 3500u dead zone.

---

## FIX 2: Lightning `hit_biggest_targets` + projectile-based damage

**Symptoms:** Tests 1001 — Lightning hits 2 (all in radius) instead of exactly 3 highest HP

**Root cause:** Lightning zone spell has `hit_biggest_targets=True` and `projectile=LighningSpell` (damage=660 lv1, 1689 lv11). The engine treats it as a generic zone spell with `damage=0` and ignores both `hit_biggest_targets` and the projectile reference.

### Patch A — `entities.rs` (add fields to SpellZoneData)

After `pub crown_tower_damage_percent: i32,` (~line 765), add:

```rust
    /// If true, this spell targets the N highest-HP entities (Lightning).
    pub hit_biggest_targets: bool,
    /// Max number of targets for hit_biggest_targets (Lightning = 3 in real CR).
    /// 0 = unlimited (standard zone behavior).
    pub max_hit_targets: i32,
    /// Per-strike damage from the linked projectile (for Lightning).
    /// This replaces damage_per_tick when hit_biggest_targets is true.
    pub projectile_damage: i32,
    /// Crown tower damage percent from the linked projectile.
    pub projectile_ct_pct: i32,
```

Update `new_spell_zone()` signature to accept these 4 new parameters and initialize them in the `SpellZoneData` block. Add default values (false, 0, 0, 0) for non-Lightning spells.

### Patch B — `lib.rs` (populate fields when creating Lightning zone)

In the `CardType::Spell` branch (~line 724), after computing `damage`, `hit_interval`, etc., add:

```rust
    // Lightning: look up projectile damage for hit_biggest_targets spells
    let (hit_biggest, proj_damage, proj_ct_pct) = if spell_stats.hit_biggest_targets {
        if let Some(ref proj_name) = spell_stats.projectile {
            if let Some(proj) = data.projectiles.get(proj_name.as_str()) {
                let pd = if !proj.damage_per_level.is_empty() && level > 0 {
                    let idx = (level - 1).min(proj.damage_per_level.len() - 1);
                    proj.damage_per_level[idx]
                } else {
                    proj.damage
                };
                (true, pd, proj.crown_tower_damage_percent)
            } else {
                (true, damage, ct_pct)
            }
        } else {
            (true, damage, ct_pct)
        }
    } else {
        (false, 0, 0)
    };
    // Lightning max targets: hardcoded 3 (standard CR constant).
    // The JSON field maximum_targets=0 for Lightning because the game
    // uses hit_biggest_targets=True as a flag for the 3-strike mechanic.
    let max_hit_targets = if hit_biggest { 3 } else { 0 };
```

Pass these through to `Entity::new_spell_zone(...)`.

### Patch C — `combat.rs` (implement top-N HP targeting in tick_spell_zones)

In the damage pass of `tick_spell_zones` (~line 2881), when processing a `SpellHit`:

```rust
// After collecting valid targets in-radius, check hit_biggest_targets:
if hit.hit_biggest_targets && hit.max_hit_targets > 0 {
    // Collect all valid targets with their HP and index
    let mut candidates: Vec<(usize, i32)> = Vec::new();  // (entity_idx, hp)
    for (idx, target) in state.entities.iter().enumerate() {
        // ... same filtering as current code (alive, team, air/ground, in radius) ...
        candidates.push((idx, target.hp));
    }
    // Also collect valid tower targets
    let mut tower_candidates: Vec<(EntityId, i32)> = Vec::new();  // (tower_id, hp)
    // ... collect enemy towers in radius with their HP ...

    // Sort by HP descending, take top N
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    // Merge entity + tower candidates, sort by HP, take top max_hit_targets
    // Apply projectile_damage (not damage_per_tick) to each selected target
    for (idx, _hp) in candidates.iter().take(hit.max_hit_targets as usize) {
        damage_events.push((*idx, hit.projectile_damage));
    }
    // Apply projectile_ct_pct (not zone ct_pct) to tower targets
} else {
    // ... existing zone damage logic (unchanged) ...
}
```

The key insight: Lightning's zone `damage=0` and its `hit_speed=460ms` controls strike cadence. Each strike interval should select the top-3 HP and apply `projectile_damage` per strike. Over 1500ms (3 intervals at 460ms), each target gets hit once per interval = up to 3 strikes total if the same targets remain highest HP.

Real CR Lightning: fires 3 bolts in ~1.5s, each targeting the current highest-HP entity. If only 1 entity exists, it gets hit 3 times. With 5 entities, the top 3 HP each get hit once per volley.

---

## FIX 3: Poison DOT ticks every frame instead of per `hit_frequency`

**Symptoms:** Test 1016 — Poison deals damage every 1 tick (50ms) instead of every 20 ticks (1000ms)

**Root cause:** Poison buff `damage_per_tick` is set to `damage_per_second / 20 = 57/20 = 2` and applied via `entity.tick_buffs()` every single tick. Real CR Poison ticks at `hit_frequency=1000ms` intervals = once per 20 ticks, dealing `damage_per_second` (57 at lv1) per pulse.

### Patch A — `entities.rs` (add hit timer to ActiveBuff)

Add to `ActiveBuff` struct:

```rust
    /// Tick interval for pulsed damage (Poison, Earthquake). 0 = every tick (legacy).
    /// When > 0, damage_per_tick is only applied when hit_timer counts to 0.
    pub damage_hit_interval: i32,
    /// Countdown for next damage pulse. Decremented each tick; fires at 0 then resets.
    pub damage_hit_timer: i32,
```

### Patch B — `entities.rs` (update tick_buffs to use pulsed DOT)

In `tick_buffs()`, change the DOT accumulation:

```rust
// BEFORE:
let mut tick_dot = buff.damage_per_tick;

// AFTER:
let mut tick_dot = 0;
if buff.damage_per_tick > 0 {
    if buff.damage_hit_interval > 0 {
        // Pulsed DOT: only fire on hit timer
        buff.damage_hit_timer -= 1;  // Note: requires &mut
        if buff.damage_hit_timer <= 0 {
            buff.damage_hit_timer = buff.damage_hit_interval;
            tick_dot = buff.damage_per_tick;
        }
    } else {
        // Legacy: every-tick DOT
        tick_dot = buff.damage_per_tick;
    }
}
```

**Note:** This requires changing the buff iteration from `for buff in &self.buffs` to `for buff in &mut self.buffs` (or restructuring to avoid the borrow issue with the mutable heal/dot accumulators).

### Patch C — `combat.rs` (set pulsed DOT when creating Poison buff)

In `tick_spell_zones`, where the `ActiveBuff` is created (~line 3053), change the DOT conversion:

```rust
// BEFORE:
let dot = bs.map(|b| {
    if b.damage_per_second > 0 { b.damage_per_second / 20 } else { 0 }
}).unwrap_or(0);

// AFTER:
// Pulsed DOT: damage_per_hit = damage_per_second * hit_frequency_ms / 1000
// hit_frequency_ticks = hit_frequency_ms * 20 / 1000
let (dot, dot_interval) = bs.map(|b| {
    if b.damage_per_second > 0 && b.hit_frequency > 0 {
        // Pulsed: Poison deals dps * (interval_ms / 1000) per pulse
        let per_hit = (b.damage_per_second as i64 * b.hit_frequency as i64 / 1000) as i32;
        let interval_ticks = (b.hit_frequency * 20 + 999) / 1000;
        (per_hit, interval_ticks)
    } else if b.damage_per_second > 0 {
        // No hit_frequency: every-tick DOT (legacy)
        (b.damage_per_second / 20, 0)
    } else {
        (0, 0)
    }
}).unwrap_or((0, 0));
```

Then in the `ActiveBuff` initialization, add:
```rust
    damage_hit_interval: dot_interval,
    damage_hit_timer: dot_interval.max(1),  // First pulse after one interval
```

**Result for Poison:** `damage_per_second=57, hit_frequency=1000` →
`per_hit = 57 * 1000 / 1000 = 57`, `interval_ticks = 1000 * 20 / 1000 = 20`.
So: 57 damage every 20 ticks (1s), for 160 ticks (8s) = 8 pulses × 57 = 456 total (lv1).

---

## FIX 4: Poison buff outlasts zone duration

**Symptoms:** Test 1019 — Poison still deals damage 80+ ticks after the zone expires at tick 160

**Root cause:** Each time the zone re-applies the Poison buff (every `hit_interval=5 ticks`), it sets `remaining_ticks = buff_duration`. Looking at `lib.rs` line 752-757:

```rust
let buff_time = if spell_stats.buff_time > 0 {
    (spell_stats.buff_time * 20 + 999) / 1000
} else {
    duration_ticks // ← THIS IS THE BUG for Poison
};
```

Poison's `buff_time=250ms` → 5 ticks. But the current code path returns 5, which is correct... unless the refresh at line 3021 (`existing_buff.remaining_ticks = duration`) resets it to a wrong value.

The issue: when the zone refreshes an existing buff at line 3021, it uses `hit.buff_duration`. For Poison, `buff_duration=5 ticks`. After the zone dies at tick 160, the last buff refresh was at ~tick 157 with 5 ticks remaining → expires at tick ~162. The test shows damage at tick 260, which is 100 ticks after expiry.

**Most likely cause:** The zone's hit_interval=5 means it fires at ticks 5, 10, 15, ... 155. But the zone `remaining` only decrements once per tick. The zone should die at tick 160 (duration=160 ticks). If the buff_duration is incorrectly set to 160 ticks instead of 5, each refresh would keep the buff alive for 160 more ticks.

**Diagnosis step:** Print `hit.buff_duration` in the `tick_spell_zones` damage pass to verify it's 5 not 160.

### Patch — `lib.rs` or `combat.rs`

Add a safeguard: when a zone spell refreshes a buff, cap the buff duration to the zone's remaining lifetime:

```rust
// In tick_spell_zones, when refreshing existing buff:
if let Some(existing_buff) = existing {
    // Cap buff duration to zone's remaining ticks to prevent outliving the zone
    existing_buff.remaining_ticks = duration.min(zone_remaining);
    continue;
}
```

This requires passing `sz.remaining` through the `SpellHit` struct so it's available during buff application.

Alternatively, add `zone_remaining: i32` to `SpellHit` and use it:

```rust
let capped_duration = hit.buff_duration.min(hit.zone_remaining + 1);
```

---

## FIX 5: BombTowerBomb death spawn fails

**Symptoms:** Test 1070b — Bomb Tower death-spawns "BombTowerBomb" but no damage occurs

**Root cause:** `death_spawn_character=BombTowerBomb` doesn't resolve to any character in `data.characters`. The `find_character()` lookup returns `None`, so nothing spawns. In real CR, the Bomb Tower's death bomb is an instant AoE damage event, not a troop spawn.

**Data:** The building JSON has `death_damage=0` and `death_damage_radius=0`, so the existing `death_damages` path also produces nothing. The death bomb damage must come from the BombTowerBomb entry, but it doesn't exist in the character data.

### Patch — `combat.rs` (`tick_deaths`)

When a building's `death_spawn_character` fails to resolve as a character, check if it's a known "death bomb" pattern (name contains "Bomb" or "Explosion") and instead apply AoE damage using the building's own attack damage and the projectile's AoE radius.

```rust
// In tick_deaths, after the death_spawns loop (line ~2593):
// Fallback: if death_spawn_character didn't resolve and looks like a death bomb,
// treat it as instant AoE damage using the building's attack stats.
for (team, key, x, y, level, count) in &unresolved_death_spawns {
    // Try to find projectile data for the building to get splash radius
    if let Some(bstats) = data.buildings.get(&entity.card_key) {
        // Use the building's summon_character to find its projectile data
        // BombTower → BombTowerProjectile → damage=105(lv1), radius=1500
        let proj_key = format!("{}Projectile", bstats.summon_character
            .as_deref().unwrap_or(""));
        if let Some(proj) = data.projectiles.get(&proj_key) {
            let dmg = if !proj.damage_per_level.is_empty() && level > 0 {
                let idx = (level - 1).min(proj.damage_per_level.len() - 1);
                proj.damage_per_level[idx]
            } else {
                proj.damage
            };
            let radius = proj.radius;
            death_damages.push((team, x, y, dmg, radius));
        }
    }
}
```

A cleaner approach: add `death_damage` and `death_damage_radius` lookup from the BombTowerProjectile data when `death_spawn_character` contains "Bomb" and fails character lookup.

---

## TEST FIXES (not engine bugs)

These failures are test setup issues, not engine defects:

### Tests 1000, 1002-1005: Lightning elixir
**Problem:** Lightning costs 6 elixir. `step_n(m, 20)` only builds ~0.36 elixir over starting 5.
**Fix:** Change to `step_n(m, 80)` minimum (builds 5 + 80×179/10000 ≈ 6.4 elixir).

### Test 1032: Rocket far enemy damage
**Problem:** Far enemy at 4000u takes 545 damage — this is tower damage over 120 ticks, not Rocket.
**Fix:** Add control match without Rocket, subtract baseline tower damage.

### Test 1050: Log damage too high (1595 vs expected 614)
**Problem:** 200-tick run includes ~1000 of tower damage on top of 614 Log damage.
**Fix:** Add control match, subtract tower baseline. Or reduce tick count to 100 and account for tower DPS.

### Test 1082b: Goblin Drill death goblins = 0
**Problem:** Death-spawned Goblins (167 HP) die instantly to surrounding enemies.
**Fix:** Track dead goblins too: use `find_all_including_dead(m, team=1, card_key_contains="goblin")`.

### Test 1091: Phoenix didn't die
**Problem:** Knights can't attack flying Phoenix (attacks_air=False).
**Fix:** Use `musketeer` or `wizard` (attacks_air=True) to kill the Phoenix.

### Test 1095: Elixir Golem didn't die
**Problem:** Knights at -5800 may not reach Golem at -6000 fast enough; EG walks away toward buildings.
**Fix:** Increase enemy count, place closer, or extend timeout to 800 ticks.

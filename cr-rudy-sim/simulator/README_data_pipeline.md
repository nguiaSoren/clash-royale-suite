# Clash Royale Simulator — Data Pipeline

## Overview

This project provides a complete data foundation for building a Clash Royale battle simulator. It combines two data sources into 6 structured JSON files covering every card, building, spell, projectile, buff, evolution ability, and hero ability in the game.

**No LLM is needed at runtime.** All data is pre-structured with typed fields and numeric values. The simulator reads JSON and applies game logic.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     DATA SOURCES                            │
├──────────────────────────┬──────────────────────────────────┤
│  RoyaleAPI cr-api-data   │  Fandom Wiki (scraped + LLM)    │
│  (5 static JSON files)   │  (1 generated JSON file)        │
│                          │                                  │
│  Base stats, physics,    │  Evolution abilities,            │
│  projectiles, spells,    │  Hero abilities,                 │
│  buffs, buildings        │  Stat modifiers                  │
└──────────────────────────┴──────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                   SIMULATOR ENGINE                          │
│                                                             │
│  Reads all 6 JSON files at startup                          │
│  Cross-references by key/name fields                        │
│  No LLM, no API calls, no network needed at runtime         │
└─────────────────────────────────────────────────────────────┘
```

---

## Files

### Data Files (6 total)

| File | Source | Entries | Description |
|------|--------|---------|-------------|
| `cards_stats_characters.json` | RoyaleAPI | 125 | Every troop and champion — 297 fields each |
| `cards_stats_building.json` | RoyaleAPI | 89 | Every building — 285 fields each |
| `cards_stats_projectile.json` | RoyaleAPI | 92 | Every projectile (arrows, fireballs, etc.) |
| `cards_stats_spell.json` | RoyaleAPI | 71 | Every spell (Fireball radius, Freeze duration, etc.) |
| `cards_stats_character_buff.json` | RoyaleAPI | 62 | Every buff/debuff (Rage, Freeze, Poison, Heal, etc.) |
| `evo_hero_abilities.json` | Wiki scrape | 39 evos + 6 heroes | Evolution and Hero ability mechanics |

**Total: 439 base game entities + 45 evo/hero ability definitions**

### Scripts

| File | Purpose |
|------|---------|
| `download_cr_data.py` | Downloads the 5 RoyaleAPI JSON files from GitHub |
| `cr_wiki_scraper.py` | Two-step pipeline: scrapes wiki → structures with LLM |
| `template_simulator_data_schema.json` | Schema reference for evo/hero abilities format |

### Dropped Files (redundant)

These files from RoyaleAPI are **not needed** — their data is already contained in the 5 files above:

| File | Why dropped |
|------|-------------|
| `cards.json` | All 11 fields already exist inside `cards_stats_characters.json` |
| `cards_stats.json` | Bundle copy of the other 5 individual files |
| `cards_stats_troop.json` | Mostly cosmetic (icons, images). Only useful field: `mirror` flag |

---

## What Each File Provides to the Simulator

### 1. `cards_stats_characters.json` — The Core Engine

**125 troops and champions, 297 fields each.**

This is the heart of the simulator. Every field you need for the combat loop:

| Question | Field | Example |
|----------|-------|---------|
| Is this troop flying? | `flying_height` | `0` = ground, `3500` = air |
| Can it hit ground? | `attacks_ground` | `true` / `false` |
| Can it hit air? | `attacks_air` | `true` / `false` |
| Does it only target buildings? | `target_only_buildings` | `true` for Giant, Golem |
| How fast does it move? | `speed` | `30`/`45`/`60`/`90`/`120` |
| How fast does it attack? | `hit_speed` | Milliseconds (e.g. `1200`) |
| How far can it reach? | `range` | Game units (e.g. `6000`) |
| Does it splash? | `area_damage_radius` | `> 0` = splash |
| Does it have a shield? | `shield_hitpoints` | `> 0` = has shield |
| Does it dash/jump? | `dash_damage` | `> 0` = has dash (Mega Knight) |
| What does it shoot? | `projectile` | Name → look up in projectile.json |
| What buff does it apply? | `buff_on_damage` | Name → look up in buff.json |
| Does it spawn things? | `spawn_character`, `spawn_count` | Witch → spawns Skeletons |
| What happens on death? | `death_spawn_character`, `death_damage` | Golem → Golemites |
| Can it be pushed back? | `ignore_pushback` | `true` for heavy troops |
| How heavy is it? | `mass` | Affects push/pull interactions |
| How big is its hitbox? | `collision_radius` | Game units |
| HP at each level? | `hitpoints_per_level` | Array of 16-19 values |
| Damage at each level? | `damage_per_level` | Array of 16-19 values |

### 2. `cards_stats_building.json` — Buildings

**89 buildings, 285 fields each.**

Same depth as characters plus building-specific mechanics:

| Field | What it does | Example |
|-------|-------------|---------|
| `life_time` | Self-destruct timer (ms) | Tesla: `30000` (30s) |
| `hides_when_not_attacking` | Untargetable when idle | Tesla: `true` |
| `hide_time_ms` | Time to go underground | `800` ms |
| `up_time_ms` | Time to pop up | `800` ms |
| `spawn_character` | What it spawns | Goblin Hut → `SpearGoblin` |
| `spawn_interval` | How often it spawns | Milliseconds |

Buildings are **not** in characters.json — they only exist here.

### 3. `cards_stats_projectile.json` — Projectiles

**92 projectiles.**

Characters reference these by name in their `projectile` field:

| Field | What it does |
|-------|-------------|
| `speed` | Travel speed (determines time-to-hit) |
| `homing` | `true` = tracks target, `false` = can miss |
| `damage` | Damage dealt on hit |
| `radius` | Splash radius (`0` = single target) |
| `gravity` | Affects arc trajectory |
| `crown_tower_damage_percent` | Reduced damage vs towers |
| `damage_per_level` | Per-level damage array |

**Without this file:** Ranged attacks would be instant (no travel time, no splash).

### 4. `cards_stats_spell.json` — Spells

**71 spells.**

| Field | What it does |
|-------|-------------|
| `damage` | Damage dealt |
| `radius` | Area of effect |
| `life_duration` | Duration for persistent spells (Poison, Rage) |
| `pushback` | Push force on hit |
| `crown_tower_damage_percent` | Reduced damage vs towers |
| `aoe_to_air` / `aoe_to_ground` | What it hits |

**Without this file:** No spells exist in the simulator.

### 5. `cards_stats_character_buff.json` — Buffs & Debuffs

**62 buff/debuff definitions.**

Characters reference these by name (`buff_on_damage`, `buff_on_kill`, etc.):

| Field | What it does | Example |
|-------|-------------|---------|
| `speed_multiplier` | Movement speed modifier | Rage: `135` (= 135% of normal) |
| `hit_speed_multiplier` | Attack speed modifier | Rage: `135` |
| `damage_per_second` | Tick damage | Poison: deals DPS |
| `heal_per_second` | Tick healing | Heal Spirit buff |
| `damage_reduction` | Incoming damage modifier | Some shields |

**Multiplier format:** `135` means 135% of base (= 35% speed boost).

**Without this file:** Rage doesn't speed up, Poison doesn't tick, Freeze doesn't freeze.

### 6. `evo_hero_abilities.json` — Evolution & Hero Abilities

**39 evolutions + 6 heroes.**

This layers on TOP of the base stats from characters.json:

| Field | What it does |
|-------|-------------|
| `base_card_key` | Links to characters.json `key` field |
| `cycle_cost` | Plays needed before evo activates (1 or 2) |
| `stat_modifiers.hitpoints_multiplier` | HP change (e.g. `1.10` = +10%) |
| `ability.trigger.condition` | When ability fires (`on_each_attack`, `while_not_attacking`, etc.) |
| `ability.effects[].effect_type` | What it does (`damage_reduction`, `area_pull`, `knockback`, etc.) |
| `ability.effects[].value` | Primary value (percentage as integer: `60` = 60%) |
| `ability.effects[].radius` | Area of effect (game units) |
| `ability.effects[].duration_ms` | How long effect lasts |
| `ability.effects[].damage` | Damage dealt by effect (tournament level 11) |

**Effect types the simulator must handle:**
`damage_reduction`, `damage_buff`, `speed_buff`, `hitspeed_buff`, `heal`, `spawn_unit`, `area_pull`, `area_damage`, `projectile_chain`, `projectile_bounce`, `shield_grant`, `freeze`, `stun`, `slow`, `rage`, `clone_on_death`, `respawn`, `knockback`, `custom`

**Without this file:** Evolutions and Heroes are just reskinned base cards with no abilities.

---

## Cross-Reference Map

```
characters.json ──"projectile" field──→ projectile.json
      │                              
      ├──"buff_on_damage" field──→ character_buff.json
      │                                  ↑
      │                          evo_hero_abilities.json
      │                          ("buff_reference" field)
      │
      ├──"key" field ←── evo_hero_abilities.json ("base_card_key")
      │
      └── (separate pool) ── building.json
                                    
spell.json ── standalone (some spells also reference projectiles)
```

### Key Linkages

| From | Field | To | Lookup |
|------|-------|----|--------|
| Character | `projectile` | Projectile | By `name` |
| Character | `buff_on_damage` | Buff | By `name` |
| Character | `buff_on_kill` | Buff | By `name` |
| Character | `ability` | Buff | By `name` (champions) |
| Evo/Hero | `base_card_key` | Character | By `key` |
| Evo effect | `buff_reference` | Buff | By `name` |
| Evo effect | `spawn_character` | Character | By `sc_key` |

---

## Card Lookup Flow (Runtime)

```
1. Player plays "Knight"
   → Find in characters.json where key="knight"
   → Load 297 combat/physics fields

2. Is it a ranged troop?
   → Check "projectile" field
   → If set, look up in projectile.json by name

3. Is evo active?
   → Find in evo_hero_abilities.json where base_card_key="knight"
   → Apply stat_modifiers (multiply base HP, damage, etc.)
   → Register ability trigger in combat engine

4. Is it a hero variant?
   → Find in evo_hero_abilities.json heroes[] where base_card_key="knight"
   → Apply stat_modifiers
   → Register ability button with elixir_cost

5. Spell played?
   → Look up in spell.json by name (not characters.json)

6. Building placed?
   → Look up in building.json by key (not characters.json)
```

---

## Scripts

### `download_cr_data.py`

Downloads the 5 RoyaleAPI JSON files from GitHub.

```bash
python download_cr_data.py
# Saves to cr_data/ folder
```

### `cr_wiki_scraper.py`

Two-step pipeline that generates `evo_hero_abilities.json`:

**Step 1 — Scrape:** Fetches wiki index pages to auto-discover all evolution and hero cards, then scrapes each individual page and saves raw text to `raw_wiki_data/`.

**Step 2 — Structure:** Sends each raw text file to Claude Sonnet via the Anthropic API, which extracts simulation-relevant data and returns structured JSON matching the schema.

```bash
pip install cloudscraper beautifulsoup4 anthropic
export ANTHROPIC_API_KEY="sk-ant-..."

# Full pipeline
python cr_wiki_scraper.py

# Or step by step
python cr_wiki_scraper.py --step1-only   # Scrape wiki (no LLM needed)
python cr_wiki_scraper.py --step2-only   # Structure with LLM (needs raw files)
```

**Cost:** ~$1.20 with Sonnet, ~$6 with Opus. Sonnet is recommended — this is structured extraction, not complex reasoning.

**Re-running:** Raw files are cached. If you re-run step 1, existing files are skipped. Delete `raw_wiki_data/` to force a full re-scrape. Delete `evo_hero_abilities.json` to force re-structuring.

### `template_simulator_data_schema.json`

Reference schema showing the exact structure of `evo_hero_abilities.json`. Includes:
- Field definitions with types and enums
- 3 evolution examples (Knight, Barbarians, Valkyrie)
- 2 hero examples (Knight, Magic Archer)
- Scraping guide with URL patterns and extraction rules

This is a **template/reference** — the actual data is in `evo_hero_abilities.json`.

---

## Value Formats

Consistent across all files:

| Type | Format | Example |
|------|--------|---------|
| Percentages | Integer (not decimal) | `60` = 60% reduction |
| Distances/radii | Game units (tiles × 1000) | `5500` = 5.5 tiles |
| Durations | Milliseconds | `3000` = 3 seconds |
| Speeds | Game speed constant | `60` = Medium, `90` = Fast |
| Buff multipliers | Percent-of-base | `135` = 135% (= +35% boost) |
| HP/Damage | Raw value at Level 1 (or tournament 11 for evos) | `690` HP |
| Per-level arrays | Array indexed from Level 1 | `[690, 759, 834, ...]` |

---

## Known Gaps

| Gap | Reason | Fix |
|-----|--------|-----|
| 4 missing heroes (Goblins, Mega Minion, Barbarian Barrel, Magic Archer) | Added in March 2026, wiki pages don't exist yet | Add manually when wiki is updated |
| RoyaleAPI stats may lag behind balance patches | Static repo, not always updated immediately | Cross-check with in-game values |
| Some evo effects use `"custom"` effect_type | Ability too unique for standard enums | Implement as special-case handlers |

---

## Quick Reference: Ground vs Air

```python
# Is a troop flying?
is_flying = character["flying_height"] > 0

# Can a troop attack air units?
can_hit_air = character["attacks_air"] == True

# Can a troop attack ground units?
can_hit_ground = character["attacks_ground"] == True

# Does it ignore troops (only targets buildings)?
buildings_only = character["target_only_buildings"] == True
```

| Troop | flying_height | attacks_ground | attacks_air | target_only_buildings |
|-------|:---:|:---:|:---:|:---:|
| Knight | 0 | ✓ | ✗ | ✗ |
| Valkyrie | 0 | ✓ | ✗ | ✗ |
| Musketeer | 0 | ✓ | ✓ | ✗ |
| Baby Dragon | 3500 | ✓ | ✓ | ✗ |
| Balloon | 3500 | ✓ | ✗ | ✓ |
| Giant | 0 | ✓ | ✗ | ✓ |
| Mega Knight | 0 | ✓ | ✗ | ✗ |

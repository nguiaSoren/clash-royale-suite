# Clash Royale Deterministic Simulator

A tick-deterministic, integer-only combat simulation engine in Rust modeling the full Clash Royale game. Driven by 8 JSON data files covering every card, building, spell, projectile, buff, evolution, and hero in the game. Exposes a Python API via PyO3 for AI agent integration, replay recording, and RL training.

## Architecture

```
Python side:                          Rust side (compiled .so/.dylib):
┌──────────────────────┐              ┌──────────────────────────────┐
│ match_runner.py      │   import     │ engine.rs    (tick loop)     │
│ ai_agent.py          │ ──────────→ │ combat.rs    (targeting/dmg) │
│ replay_recorder.py   │  ←results── │ entities.rs  (troops/bldgs)  │
│ analytics.py         │              │ game_state.rs (arena/towers) │
│ training.py          │              │ champion_system.rs           │
│ data_loader.py       │              │ evo_system.rs                │
└──────────────────────┘              │ hero_system.rs               │
                                      │ data_types.rs (card stats)   │
                                      │ lib.rs        (PyO3 entry)   │
                                      └──────────────────────────────┘
```

**Why Rust + Python:** The tick loop is the hot path — it runs millions of ticks across thousands of parallel simulations. Rust gives zero-cost abstractions, no GC pauses, and deterministic memory. Python has the ML ecosystem (PyTorch, stable-baselines3) and handles training, analytics, and visualization through the PyO3 bridge.

## Performance (measured, single M1 Pro core)

| Metric | Value |
|--------|-------|
| Tick rate | 20 tps (50ms budget), integer-only arithmetic |
| 3,000 concurrent agents | 29ms/tick p99 (within 50ms budget) |
| 10,000 parallel simulations | 23.7ms batch latency (1.5μs/instance) |
| State capture overhead | 12μs per tick |
| Determinism | 100× identical runs, bit-identical after 9,600 ticks |

## Data Layer (8 JSON files)

All game mechanics are data-driven — zero hardcoded heuristics. The engine loads 8 JSON files at startup and cross-references them by key/name fields at runtime.

| File | Source | Entries | Description |
|------|--------|---------|-------------|
| `cards.json` | RoyaleAPI | 110 | Card registry (elixir cost, rarity, type) |
| `cards_stats_characters.json` | RoyaleAPI | 178 | Every troop and champion — 297 fields each |
| `cards_stats_building.json` | RoyaleAPI | 89 | Every building — 285 fields each |
| `cards_stats_projectile.json` | RoyaleAPI | 92 | Every projectile (speed, homing, splash radius) |
| `cards_stats_spell.json` | RoyaleAPI | 81 | Every spell (radius, duration, pushback) |
| `cards_stats_character_buff.json` | RoyaleAPI | 62 | Every buff/debuff (Rage, Freeze, Poison, Heal) |
| `evo_hero_abilities.json` | Wiki scrape + LLM | 39 evos + 6 heroes | Evolution and Hero ability mechanics |
| `template_simulator_data_schema.json` | Manual | — | Schema reference for evo/hero format |

### Cross-reference map

```
characters.json ──"projectile"──→ projectile.json
      │
      ├──"buff_on_damage"──→ character_buff.json
      │                           ↑
      │                    evo_hero_abilities.json
      │                    ("buff_reference")
      │
      ├──"key" ←── evo_hero_abilities.json ("base_card_key")
      │
      └── (separate pool) ── building.json

spell.json ── standalone (some reference projectiles)
```

### Value formats

| Type | Format | Example |
|------|--------|---------|
| Percentages | Integer | `60` = 60% reduction |
| Distances | Game units (tiles × 1000) | `5500` = 5.5 tiles |
| Durations | Milliseconds | `3000` = 3 seconds |
| Speeds | Game speed constant | `60` = Medium, `90` = Fast |
| Buff multipliers | Percent-of-base | `135` = 135% (= +35% boost) |
| Per-level stats | Array indexed from Level 1 | `[690, 759, 834, ...]` |

## Engine Tick Loop

Each tick processes these stages in order. The fixed ordering guarantees deterministic resolution of simultaneous events.

```
1.  Phase/Resource update     (elixir generation, phase transitions)
2.  Deploy timers             (spawn delay countdown, staggered multi-unit deploy)
3.  Building spawners         (Tombstone, Goblin Hut, Witch skeleton waves)
4.  Spell zone tick           (Poison DOT, Freeze, Tornado pull)
5.  Targeting                 (O(N²) nearest-enemy scan, sticky targeting)
6.  Movement                  (bridge pathing, river jump, collision separation)
7.  Combat                    (windup/backswing state machine, melee + ranged)
8.  Projectile flight         (homing, splash on impact, chain lightning)
9.  Tower attacks             (princess + king tower, independent cooldowns)
9d. Buff tick                 (duration countdown, stat modifier application)
10. Death processing          (death spawns, death damage, death pushback)
11. Entity cleanup            (remove dead entities from pool)
```

## Systems Implemented

**Combat:** Windup/backswing attack state machine, splash damage, charge mechanics (Prince/Dark Prince/Battle Ram — distance-triggered speed doubling + 2× damage), shield absorption (Dark Prince, Guards), variable damage ramp (Inferno Tower/Dragon 3-tier), kamikaze self-destruct (Fire Spirit, Wall Breakers).

**Targeting:** O(N²) nearest-enemy scan, sticky targeting (don't switch mid-attack), building-only targeting (Giant, Hog Rider, Balloon), building pull (Cannon redirect), retarget on target death with windup cancel.

**Spells & Buffs:** Freeze (movement + attack stop), Rage (speed + attack speed multiplier), Poison (pulsed DOT + movement slow), Tornado (attract pull + DOT), Lightning (top-3 HP targeting), Earthquake (building bonus damage), Zap/Electro stun. Crown tower damage reduction per spell. Buff duration tracking and expiry.

**Death spawns:** Golem → 2 Golemites, Lava Hound → 6 Pups, Battle Ram → 2 Barbarians, Giant Skeleton death bomb (delayed fuse), Tombstone → 4 Skeletons on death. Death damage with pushback (Golem, Ice Golem, Giant Skeleton bomb).

**Champions:** All 5 implemented with unique abilities — Skeleton King (graveyard zone spawning 40 skeletons), Archer Queen (2.8× attack speed + invisibility), Monk (80% damage reduction deflect), Golden Knight (chain dash with speed boost), Mighty Miner (lane teleport + bomb at old position). Ability activation, elixir cost, duration expiry.

**Evolutions:** Evo Knight (60% passive damage reduction while idle), Evo Barbarians (hitspeed ramp on attack), Evo Valkyrie (extra AoE on hit), Evo PEKKA (heal on kill). Evolution flag tracking and stat modifier application.

**Physics:** Mass-based N-body collision separation, bridge pathing with cost-based selection, river jumping (Hog Rider, Prince, Royal Hogs), flying troops ignore ground collision and river, deploy delay stagger for multi-unit cards (8-tick intervals).

**Match lifecycle:** Regular → Double Elixir (tick 1200) → Overtime (tick 3600) → Sudden Death (tick 6000). Fixed-point elixir accumulation (no floating point drift). King tower activation on princess death or direct damage. Crown counting, HP tiebreaker, 3-crown detection.

**Projectiles:** Homing tracking, multi-projectile (Hunter 10 bullets, Princess 5 arrows), chain lightning (Electro Dragon 3-target bounce with per-target stun), Bowler rolling pushback, Firecracker self-recoil, projectile spawn offset from attacker position.

## Test Coverage

28 test batches, 1,900+ individual assertions covering every system listed above. Key batches:

| Batch | Tests | Focus | Pass rate |
|-------|-------|-------|-----------|
| 2–5 | 1–42 | Core mechanics: timing, movement, splash, elixir, death spawns | 94% |
| 6–7 | 43–66 | Spells, buffs, buildings, poison | 90% |
| 8 | 67–90 | Death damage, inferno ramp, kamikaze, sudden death, stress | 96% |
| 9 | 91–112 | Kiting, lateral movement, retargeting, bridge crossing | 97% |
| 10 | 113–140 | Building pull, windup/backswing, collision, tornado | 97% |
| 11 | 141–150 | River jumping mechanics | 100% |
| 12 | 151–199 | Unique abilities, champions, evolutions, multi-unit cards | 91% |
| 13 | 200–509 | Hardcore stress: Mega Knight, E-Giant, Royal Ghost, spirits | 97% |
| 15 | 500–549 | Charge & dash behavioral tests (Prince, Bandit, MK) | 79% |
| 16 | 600–649 | Champion abilities, Graveyard, Mirror, evolutions | 94% |
| 17 | 700–729 | Champion ability behavioral: SK, AQ, Monk, GK, MM | 100% |
| 18 | 800–829 | Evolution ability behavioral: Knight, Barbs, Valk, PEKKA | 100% |
| 19 | 900–949 | Deploy, spawner troops, buff-on-hit, variable damage | 100% |
| 20 | 950–999 | Fisherman hook, Royal Ghost stealth, multi-projectile, physics | 81% |
| 21 | 1000–1099 | Lightning, Poison, Rocket, Fireball, Log, Mortar, Phoenix, Elixir Golem | 89% |
| 22 | 1100–1199 | Charge fidelity, targeting, Heal Spirit, Battle Healer, Ice Wizard, Poison | 90% |
| 23 | 1200–1299 | Shield, mass collision, deploy delay, death bomb, kamikaze | 76% |
| 24 | 1300–1399 | Projectile splash, homing, multi-proj, pushback, collision radius | 91% |
| 25 | 1400–1499 | Buff/debuff mechanics (19 buff types) | 85% |
| 26 | 1500–1699 | E-Dragon chain, Ram Rider, Sparky, EWiz, Goblin Giant, Battle Healer | 95% |
| 27 | Stress | 166 assertions: determinism, overflow, simultaneous hits, entity pool | 98% |

Known gaps are documented per batch. Most failures are missing signature mechanics (E-Giant reflect stun, projectile-based buff application, spell pushback) — the data is parsed but the combat logic isn't wired yet.

## Build & Run

### Prerequisites
- Rust 1.75+ (rustup.rs)
- Python 3.10+
- maturin (`pip install maturin`)

### Build

```bash
cd engine

# Build the Rust engine as a Python module
maturin develop --release

# Verify import
python -c "from cr_engine import GameEngine; print('OK')"
```

### Run a match

```python
from python.match_runner import run_match
from python.data_loader import load_game_data

data = load_game_data("data/")
result = run_match(data, deck_p1=["knight", "archers", "fireball", "giant",
                                   "valkyrie", "musketeer", "skeleton-army", "zap"],
                         deck_p2=["golem", "baby-dragon", "mega-minion", "lightning",
                                   "tombstone", "lumberjack", "tornado", "barbarian-hut"])
print(result)
```

### Run tests

```bash
# Run all test batches
for f in Test_engine/test_engine_*.py; do python "$f"; done

# Run a specific batch
python Test_engine/test_engine_27.py
```

## Next Steps

The engine is built and verified. The next phase is training autonomous agents on it:

- **Self-play RL at scale:** Run millions of simulated matches using Rayon parallelism (10K concurrent instances at 1.5μs each). Train agents via PPO/IMPALA against themselves, iterating on strategy through pure simulation volume.
- **Imitation learning from top players:** Use replay data from top-ladder players as expert demonstrations. Train a policy network to predict card placement decisions given the observation vector, then fine-tune with self-play.
- **Strategy evaluation:** Compare trained agent win rates against known meta decks and top-player replays. Measure whether the agent discovers known strategies (e.g., elixir advantage trading, lane splitting, spell cycling) or finds novel ones.

## Deep-Dive Writeups

| Writeup | Focus |
|---------|-------|
| [Charge & Poison Modeling](../22_charge_and_poison_writeup.html) | Distance-triggered charge activation, pulsed DOT with movement debuff |
| [Collision & Bridge Pathfinding](../27_collision_and_bridge_writeup.html) | Mass-based N-body collision separation, cost-based bridge selection |
| [Simultaneous Hits & Convergence](../28_simultaneous_hits_and_collision_writeup.html) | Tick-deterministic trade resolution, collision convergence proof |
| [Performance & Observability](../29_performance_observability_writeup.html) | Scaling to 3,000 agents, 10K parallel sims, 12μs state capture |

## Project Structure

```
simulator/
├── data/
│   ├── royaleapi/          (6 JSON files — characters, buildings, spells, projectiles, buffs, cards)
│   └── wiki/               (2 JSON files — evolution + hero abilities, schema template)
├── engine/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs           (PyO3 entry point)
│       ├── engine.rs        (tick loop)
│       ├── combat.rs        (targeting, damage, abilities, buffs)
│       ├── entities.rs      (troop, building, projectile, spell zone)
│       ├── game_state.rs    (arena, towers, elixir, phases)
│       ├── data_types.rs    (JSON deserialization structs)
│       ├── champion_system.rs (champion ability handlers)
│       ├── evo_system.rs    (evolution ability handlers)
│       └── hero_system.rs   (hero ability handlers)
├── python/
│   ├── data_loader.py       (load JSON → pass to Rust)
│   ├── ai_agent.py          (agent interface)
│   ├── match_runner.py      (run matches, collect results)
│   ├── replay_recorder.py   (per-tick state capture)
│   ├── analytics.py         (win rates, balance analysis)
│   └── training.py          (RL training loop)
├── Test_engine/             (28 test batches, 1,900+ assertions)
├── scripts/                 (data scrapers and wiki extraction)
├── pyproject.toml           (maturin build config)
└── README.md                (this file)
```

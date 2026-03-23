# Clash Royale Simulator — Architecture Plan (Rust + Python)

## Why Rust Core + Python Shell

- **Rust:** The game engine (tick loop, combat, movement, targeting). This is the hot path — runs millions of ticks. Rust gives you zero-cost abstractions, no GC pauses, and ~100x faster than Python.
- **Python:** AI/ML training, analytics, data loading, visualization. Python has the ML ecosystem (PyTorch, stable-baselines3, matplotlib). You don't rewrite that in Rust.
- **Bridge:** PyO3 + maturin — Rust compiles to a Python module you `import` like any package.

```
Python side:                          Rust side (compiled .so/.dylib):
┌──────────────────────┐              ┌──────────────────────────────┐
│ training.py          │   import     │ game_engine (tick loop)      │
│ ai_agent.py          │ ──────────→ │ combat (targeting, damage)   │
│ match_runner.py      │  ←results── │ entities (troops, buildings) │
│ analytics.py         │              │ game_state (arena, towers)   │
│ data_loader.py       │              │ data_types (card stats)      │
└──────────────────────┘              └──────────────────────────────┘
```

---

## Project Structure

```
simulator/
├── data/                              ← (already built)
│   ├── royaleapi/
│   └── wiki/
├── engine/                            ← RUST CRATE
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                     ← PyO3 entry point
│       ├── data_types.rs              ← Card stats structs
│       ├── game_state.rs              ← Arena, towers, elixir
│       ├── entities.rs                ← Troop, Building, Projectile, Spell
│       ├── combat.rs                  ← Targeting, damage, abilities, buffs
│       ├── engine.rs                  ← Main tick loop
│       ├── evo_system.rs              ← Evolution ability handlers
│       └── hero_system.rs             ← Hero ability handlers
├── python/                            ← PYTHON LAYER
│   ├── data_loader.py                 ← Load JSON → pass to Rust
│   ├── ai_agent.py                    ← Agent interface
│   ├── match_runner.py                ← Run matches, collect results
│   ├── training.py                    ← RL training loop
│   └── analytics.py                   ← Win rates, balance analysis
├── scripts/                           ← (already built)
├── README_data_pipeline.md
├── README_simulator_plan.md           ← (this file)
└── pyproject.toml                     ← maturin build config
```

---

## Performance (Rust vs Python)

| Metric | Python | Rust | Rust + Rayon (8 cores) |
|--------|--------|------|------------------------|
| 1 tick | ~100μs | ~1-2μs | — |
| 1 match (4,800 ticks) | ~0.5s | ~5-10ms | — |
| 1,000 matches | ~8 min | ~5-10s | ~1-2s |
| 100,000 matches | not practical | ~8-15 min | ~2-3 min |

---

## Build Order

```
Phase 1: Rust Foundation (get a match running)
  ├── data_types.rs     ← Deserialize all 6 JSONs
  ├── game_state.rs     ← Arena, towers, elixir
  ├── entities.rs       ← Troop struct
  ├── engine.rs         ← Minimal tick loop (move + melee)
  └── lib.rs            ← PyO3: load_data + run_match

Phase 2: Combat (troops fight correctly)
  ├── combat.rs         ← Targeting, ranged attacks
  └── engine.rs         ← Projectile movement, spells

Phase 3: Abilities (evos, heroes, buffs)
  ├── evo_system.rs     ← Effect type handlers
  ├── hero_system.rs    ← Hero ability activation
  └── combat.rs         ← Buff system

Phase 4: Python Integration (agents can play)
  ├── lib.rs            ← Full PyO3 API
  ├── python/ai_agent.py
  └── python/match_runner.py

Phase 5: RL Training
  ├── python/training.py
  ├── Rayon parallelism
  └── Optimize hot loops
```

---

## What Impresses a Systems Professor

1. **Ownership model:** `GameData` borrowed immutably by all matches — no cloning, no locks
2. **Zero-copy serde:** JSON parsed directly into typed structs
3. **Cache-friendly layout:** Entities in a flat `Vec<Entity>` — no pointer chasing
4. **Rayon parallelism:** `run_batch` uses `par_iter()` for trivial multi-core scaling
5. **PyO3 FFI:** Clean Rust→Python boundary with minimal marshalling
6. **No GC:** Deterministic memory, no pauses during simulation
7. **Enum dispatch:** `EffectType` match arms — no vtable overhead

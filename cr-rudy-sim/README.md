<!-- ═══ SIMULATOR SECTION ═══ -->

### Deterministic Multi-Agent Simulator

![Simulator Architecture](simulator_architecture.svg)

A tick-deterministic, integer-only combat simulation engine in Rust, modeling 125+ heterogeneous agent types from 8 data-driven JSON definitions. Exposes a Python API via PyO3 for AI agent integration, replay recording, and RL training pipelines.

| Metric | Value |
|---|---|
| **Agent types modeled** | 178 characters, 89 buildings, 81 spells, 92 projectiles |
| **Tick rate** | 20 tps (50ms budget), integer-only arithmetic |
| **Stress test** | 3,000 concurrent agents at 29ms/tick (single M1 Pro core) |
| **Parallel sims** | 10,000 instances at 23.7ms batch latency (1.5μs/instance) |
| **Determinism** | 100× identical runs, bit-identical after 9,600 ticks |
| **Test coverage** | 28 batches, 1,900+ assertions, ~95% pass rate |

> **Engine subsystems tested:** attack timing (windup/backswing state machine), splash damage, charge mechanics (Prince/Dark Prince/Battle Ram), shield absorption, death spawns (Golem→Golemites, Lava Hound→Pups), building spawners (Witch, Tombstone, Furnace), champion abilities (Skeleton King graveyard, Archer Queen rapid fire, Monk deflect, Golden Knight dash, Mighty Miner lane switch), evolution abilities (Evo Knight passive shield, Evo PEKKA heal-on-kill), spell interactions (Freeze, Rage, Poison DOT, Tornado pull, Lightning top-3 targeting), projectile mechanics (homing, multi-projectile Hunter/Princess, chain lightning E-Dragon), collision separation (mass-based N-body), bridge pathing, river jumping, and full match lifecycle (regular → double elixir → overtime → sudden death).

#### Deep-dive writeups

| Writeup | Focus |
|---|---|
| [**Charge & Poison Modeling →**](https://nguiasoren.github.io/clash-royale-suite/cr-rudy-sim/22_charge_and_poison_writeup.html) | Distance-triggered charge activation, pulsed DOT with movement debuff |
| [**Collision & Bridge Pathfinding →**](https://nguiasoren.github.io/clash-royale-suite/cr-rudy-sim/27_collision_and_bridge_writeup.html) | Mass-based N-body collision separation, cost-based bridge selection |
| [**Simultaneous Hits & Convergence →**](https://nguiasoren.github.io/clash-royale-suite/cr-rudy-sim/28_simultaneous_hits_and_collision_writeup.html) | Tick-deterministic trade resolution, collision convergence proof |
| [**Performance & Observability →**](https://nguiasoren.github.io/clash-royale-suite/cr-rudy-sim/29_performance_observability_writeup.html) | Scaling to 3,000 agents, 10K parallel sims, 12μs state capture |

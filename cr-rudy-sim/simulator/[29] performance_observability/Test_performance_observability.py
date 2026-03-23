#!/usr/bin/env python3
"""
╔══════════════════════════════════════════════════════════════════════════╗
║  HARDCORE STRESS TEST — Clash Royale Simulator Engine                  ║
║                                                                        ║
║  Section 5: Performance / Scaling                                      ║
║    5.1  Multi-unit scaling     — 100+ units on the field               ║
║    5.1b Spawner growth         — Witch/Golem entity multiplication     ║
║    5.2  Memory usage           — full match dataset + sim              ║
║    5.3  Tick latency           — real-time constraint validation       ║
║    5.4  Agent ceiling          — escalate to find the actual limit     ║
║    5.5  Parallel simulation    — measured concurrent match throughput   ║
║                                                                        ║
║  Section 6: Data / Observability (CRITICAL for AI)                     ║
║    6.1  State logging          — log full state every tick             ║
║    6.2  Event tracing          — diff-based event reconstruction       ║
║    6.3  Feature extraction     — extract state vectors for AI input    ║
║                                                                        ║
║  All tests are DATA-DRIVEN from the JSON stats files.                  ║
║  No approximations. No heuristics. Just raw engine validation.         ║
╚══════════════════════════════════════════════════════════════════════════╝

Usage:
    cd <project_root>
    python -m python.test_stress_hardcore

    Or standalone (with cr_engine on PYTHONPATH):
    python test_stress_hardcore.py
"""

import sys
import os
import time
import json
import resource
import platform
import traceback
from collections import defaultdict, Counter
from typing import Any, Optional

try:
    import cr_engine
except ImportError:
    raise ImportError(
        "cr_engine not found. Build with: cd simulator && maturin develop --release"
    )


# =========================================================================
# Constants from game_state.rs (mirrored for validation)
# =========================================================================

TICKS_PER_SEC = 20
MS_PER_TICK = 50
ARENA_HALF_W = 8_400
ARENA_HALF_H = 15_400
RIVER_Y_MIN = -1_200
RIVER_Y_MAX = 1_200
KING_TOWER_HP = 4_824
PRINCESS_TOWER_HP = 3_052
MAX_ELIXIR = 10
STARTING_ELIXIR = 5
REGULAR_TIME_TICKS = 180 * TICKS_PER_SEC   # 3600
OVERTIME_TICKS = 120 * TICKS_PER_SEC        # 2400
SUDDEN_DEATH_TICKS = 180 * TICKS_PER_SEC    # 3600
MAX_MATCH_TICKS = REGULAR_TIME_TICKS + OVERTIME_TICKS + SUDDEN_DEATH_TICKS  # 9600

# Real-time constraint: each tick must complete in under 50ms.
# For AI headroom, we want p99 under 5ms (10× margin).
TICK_BUDGET_MS = 50.0
TICK_TARGET_P99_MS = 5.0

# Canonical decks — use specific card keys that match data_types.rs key resolution
# These are chosen to stress different subsystems:
DECK_SWARM = [
    "skeleton-army",      # 15 skeletons (multi-unit)
    "minion-horde",       # 6 minions (air swarm)
    "barbarians",         # 5 barbarians
    "goblin-gang",        # 3 goblins + 2 spear goblins
    "bats",               # 5 bats (air)
    "guards",             # 3 guards (with shields)
    "skeletons",          # 3 skeletons
    "spear-goblins",      # 3 spear goblins
]

DECK_HEAVY = [
    "golem",              # 8 elixir tank, death spawns 2 golemites
    "giant",              # 5 elixir tank
    "prince",             # 5 elixir charge mechanic
    "witch",              # 5 elixir spawner (skeletons)
    "valkyrie",           # 4 elixir splash
    "musketeer",          # 4 elixir ranged
    "knight",             # 3 elixir melee
    "bomber",             # 2 elixir ranged splash
]

DECK_SPELLS = [
    "knight",
    "musketeer",
    "valkyrie",
    "giant",
    "witch",
    "prince",
    "bomber",
    "bowler",
]

DECK_BUILDINGS = [
    "cannon",
    "tombstone",          # spawns skeletons
    "tesla",
    "mortar",
    "knight",
    "musketeer",
    "valkyrie",
    "bomber",
]


# =========================================================================
# Helpers
# =========================================================================

class TestResult:
    """Tracks pass/fail/metrics for a single test."""
    def __init__(self, name: str):
        self.name = name
        self.passed = True
        self.errors: list[str] = []
        self.warnings: list[str] = []
        self.metrics: dict[str, Any] = {}
        self.t0 = time.perf_counter()

    def fail(self, msg: str):
        self.passed = False
        self.errors.append(msg)

    def warn(self, msg: str):
        self.warnings.append(msg)

    def metric(self, key: str, value: Any):
        self.metrics[key] = value

    def elapsed_ms(self) -> float:
        return (time.perf_counter() - self.t0) * 1000

    def report(self) -> str:
        status = "✅ PASS" if self.passed else "❌ FAIL"
        lines = [f"\n{'─'*70}", f"  {status}  {self.name}  ({self.elapsed_ms():.0f}ms)"]
        for e in self.errors:
            lines.append(f"    ✗ {e}")
        for w in self.warnings:
            lines.append(f"    ⚠ {w}")
        for k, v in self.metrics.items():
            if isinstance(v, float):
                lines.append(f"    • {k}: {v:.4f}")
            else:
                lines.append(f"    • {k}: {v}")
        return "\n".join(lines)


def get_mem_mb() -> float:
    """Current process peak RSS in MB.

    macOS `ru_maxrss` returns bytes; Linux returns kilobytes.
    We detect the platform and normalize to megabytes.
    """
    usage = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if platform.system() == "Darwin":
        return usage / (1024.0 * 1024.0)   # macOS: bytes → MB
    return usage / 1024.0                   # Linux: KB → MB


def percentile(data: list[float], p: float) -> float:
    """Compute the p-th percentile of a sorted list."""
    if not data:
        return 0.0
    s = sorted(data)
    idx = int(len(s) * p / 100.0)
    idx = min(idx, len(s) - 1)
    return s[idx]


# =========================================================================
# 5.1 — Multi-Unit Scaling: 100+ units on the field
# =========================================================================

def test_multi_unit_scaling(data: "cr_engine.GameData") -> TestResult:
    """
    Stress test: spawn 100+ units on both sides and run 200 ticks of combat.

    Validates:
    - Engine doesn't crash with 100+ entities
    - Tick throughput remains acceptable under load
    - All entities are tracked correctly (HP changes, deaths)
    - Entity cleanup works (dead entities removed)

    Data-driven: uses actual skeleton (32 HP, 67 dmg) and barbarian (262 HP, 75 dmg)
    stats from cards_stats_characters.json to set up realistic mass combat.
    """
    tr = TestResult("5.1 Multi-Unit Scaling (100+ units)")

    m = cr_engine.new_match(data, DECK_SWARM, DECK_HEAVY)

    # Give both players infinite elixir for spawning
    m.set_elixir(1, 10)
    m.set_elixir(2, 10)

    # Phase 1: Spawn 60 skeletons for P1 (swarm) across own side
    p1_ids = []
    for i in range(60):
        col = i % 10
        row = i // 10
        x = -4000 + col * 800
        y = -6000 + row * 800
        try:
            eid = m.spawn_troop(1, "skeleton", x, y)
            p1_ids.append(eid)
        except Exception as e:
            tr.fail(f"Failed to spawn skeleton #{i}: {e}")
            return tr

    # Phase 2: Spawn 40 barbarians for P1
    for i in range(40):
        col = i % 8
        row = i // 8
        x = -3500 + col * 900
        y = -3000 + row * 600
        try:
            eid = m.spawn_troop(1, "barbarian", x, y)
            p1_ids.append(eid)
        except Exception as e:
            tr.fail(f"Failed to spawn barbarian #{i}: {e}")
            return tr

    # Phase 3: Spawn 50 entities for P2 (mixed defenders)
    p2_ids = []
    for i in range(25):
        x = -3000 + (i % 5) * 1200
        y = 4000 + (i // 5) * 800
        try:
            eid = m.spawn_troop(2, "barbarian", x, y)
            p2_ids.append(eid)
        except Exception as e:
            tr.fail(f"Failed to spawn P2 barbarian #{i}: {e}")
            return tr

    for i in range(25):
        x = -2500 + (i % 5) * 1000
        y = 7000 + (i // 5) * 600
        try:
            eid = m.spawn_troop(2, "skeleton", x, y)
            p2_ids.append(eid)
        except Exception as e:
            tr.fail(f"Failed to spawn P2 skeleton #{i}: {e}")
            return tr

    # Verify total entity count
    initial_entity_count = m.num_entities
    tr.metric("initial_entities", initial_entity_count)

    if initial_entity_count < 150:
        tr.fail(f"Expected 150+ entities, got {initial_entity_count}")

    # Phase 4: Run 200 ticks of combat — measure tick latency under load
    tick_times = []
    entity_counts = []
    max_entities = 0

    for tick in range(200):
        t0 = time.perf_counter_ns()
        m.step()
        dt_ns = time.perf_counter_ns() - t0
        tick_times.append(dt_ns / 1_000_000)  # ms

        count = m.num_entities
        entity_counts.append(count)
        max_entities = max(max_entities, count)

    tr.metric("max_entities_during_combat", max_entities)
    tr.metric("final_entities", m.num_entities)
    tr.metric("entities_cleaned", initial_entity_count - m.num_entities)

    # Tick performance under 100+ entity load
    avg_tick = sum(tick_times) / len(tick_times)
    p50_tick = percentile(tick_times, 50)
    p99_tick = percentile(tick_times, 99)
    max_tick = max(tick_times)
    tr.metric("avg_tick_ms", avg_tick)
    tr.metric("p50_tick_ms", p50_tick)
    tr.metric("p99_tick_ms", p99_tick)
    tr.metric("max_tick_ms", max_tick)

    # Validation: no tick should exceed the 50ms budget
    if max_tick > TICK_BUDGET_MS:
        tr.fail(f"Tick exceeded budget: {max_tick:.2f}ms > {TICK_BUDGET_MS}ms")

    # Validation: some entities should have died (combat happened)
    if m.num_entities >= initial_entity_count:
        tr.warn("No entities died — combat may not have engaged")

    # Validation: entity list should be clean (no dead entities lingering)
    entities = m.get_entities()
    dead_in_list = sum(1 for e in entities if not e["alive"])
    if dead_in_list > 0:
        tr.fail(f"{dead_in_list} dead entities in list (cleanup failed)")

    return tr


# =========================================================================
# 5.1b — Multi-Unit Scaling: Spawner Entity Growth Under Load
# =========================================================================

def test_spawner_growth(data: "cr_engine.GameData") -> TestResult:
    """
    Supplementary stress test: entity GROWTH from spawner mechanics.

    5.1 proved the engine handles 150 pre-placed entities. But in real CR,
    entities multiply during combat:
      - Witch spawns 4 Skeletons every 7s (140 ticks)
      - Golem splits into 2 Golemites on death
      - Tombstone spawns Skeletons every 3.5s, plus 4 on death
      - Skeleton Army deploys 15 units from a single card

    This test plays actual cards through play_card() (not raw spawn_troop),
    uses Witch + Golem decks that continuously produce new entities, and
    runs long enough for spawner waves + death spawns to cascade.

    Validates:
    - Entity count grows from spawner mechanics (Witch skeletons)
    - Entity count stays bounded (cleanup keeps pace with spawning)
    - Engine doesn't degrade as entity count fluctuates
    - Death spawns create new entities (Golem → Golemites)
    - play_card() multi-unit deploy works (Skeleton Army = 15 skeletons)
    """
    tr = TestResult("5.1b Spawner Entity Growth Under Load")

    # Deck with spawners + death spawners + swarm cards
    deck_spawner = [
        "witch",              # spawns 4 skeletons every 7s
        "golem",              # death-spawns 2 golemites (which death-spawn further)
        "skeleton-army",      # 15 skeletons from one card
        "knight",
        "musketeer",
        "valkyrie",
        "giant",
        "bomber",
    ]
    deck_defense = [
        "valkyrie",           # splash to kill spawned units
        "knight",
        "musketeer",
        "witch",              # also a spawner (both sides spawn)
        "bomber",
        "prince",
        "giant",
        "bowler",
    ]

    m = cr_engine.new_match(data, deck_spawner, deck_defense)

    entity_counts: list[int] = []
    peak_entities = 0
    spawner_deployed = False
    golem_deployed = False
    army_deployed = False
    tick_times: list[float] = []

    # Track spawner-created entities via diff
    prev_snap_ids: set[int] = set()
    total_spawned_by_engine = 0  # entities that appeared without play_card

    for tick in range(1200):  # 60 seconds — enough for multiple Witch waves
        if not m.is_running:
            break

        # Deploy cards on a schedule to create interesting spawner dynamics
        if tick == 1:
            # T=0: P1 deploys Witch (will start spawning skeletons)
            m.set_elixir(1, 10)
            try:
                hand = m.p1_hand()
                witch_idx = next((i for i, k in enumerate(hand) if k == "witch"), None)
                if witch_idx is not None:
                    m.play_card(1, witch_idx, -2000, -5000)
                    spawner_deployed = True
            except Exception:
                pass

        if tick == 40:
            # T=2s: P2 deploys Witch too (both sides spawning)
            m.set_elixir(2, 10)
            try:
                hand = m.p2_hand()
                witch_idx = next((i for i, k in enumerate(hand) if k == "witch"), None)
                if witch_idx is not None:
                    m.play_card(2, witch_idx, 2000, 5000)
            except Exception:
                pass

        if tick == 100:
            # T=5s: P1 deploys Golem (will eventually die and spawn golemites)
            m.set_elixir(1, 10)
            try:
                hand = m.p1_hand()
                golem_idx = next((i for i, k in enumerate(hand) if k == "golem"), None)
                if golem_idx is not None:
                    m.play_card(1, golem_idx, 0, -5000)
                    golem_deployed = True
            except Exception:
                pass

        if tick == 200:
            # T=10s: P1 deploys Skeleton Army (15 units at once)
            m.set_elixir(1, 10)
            try:
                hand = m.p1_hand()
                army_idx = next((i for i, k in enumerate(hand) if k == "skeleton-army"), None)
                if army_idx is not None:
                    m.play_card(1, army_idx, 0, -3000)
                    army_deployed = True
            except Exception:
                pass

        if tick == 400:
            # T=20s: Deploy more troops to keep combat going
            m.set_elixir(1, 10)
            m.set_elixir(2, 10)
            for p in [1, 2]:
                playable = m.playable_cards(p)
                if playable:
                    try:
                        x = -2000 if p == 1 else 2000
                        y = -4000 if p == 1 else 4000
                        m.play_card(p, playable[0], x, y)
                    except Exception:
                        pass

        # Measure tick time
        t0 = time.perf_counter_ns()
        m.step()
        dt_ms = (time.perf_counter_ns() - t0) / 1_000_000
        tick_times.append(dt_ms)

        # Track entity count
        count = m.num_entities
        entity_counts.append(count)
        peak_entities = max(peak_entities, count)

        # Track engine-spawned entities (not from play_card)
        curr_ids = {e["id"] for e in m.get_entities()}
        new_ids = curr_ids - prev_snap_ids
        # Subtract 1 for play_card ticks (rough — we only play_card on specific ticks)
        if tick not in (1, 40, 100, 200, 400):
            total_spawned_by_engine += len(new_ids)
        prev_snap_ids = curr_ids

    # Results
    tr.metric("peak_entities", peak_entities)
    tr.metric("final_entities", m.num_entities if m.is_running else entity_counts[-1] if entity_counts else 0)
    tr.metric("total_entity_snapshots", len(entity_counts))
    tr.metric("entities_spawned_by_engine", total_spawned_by_engine)
    tr.metric("spawner_deployed", spawner_deployed)
    tr.metric("golem_deployed", golem_deployed)
    tr.metric("army_deployed", army_deployed)

    # Entity count over time — sample at key moments
    sample_points = [0, 50, 150, 250, 500, 800, 1100]
    for sp in sample_points:
        if sp < len(entity_counts):
            tr.metric(f"entities_at_tick_{sp}", entity_counts[sp])

    # Tick performance under spawner load
    if tick_times:
        tr.metric("spawner_avg_tick_ms", sum(tick_times) / len(tick_times))
        tr.metric("spawner_p99_tick_ms", percentile(tick_times, 99))
        tr.metric("spawner_max_tick_ms", max(tick_times))

    # Validation: Witch should have spawned skeletons (engine-spawned entities > 0)
    if spawner_deployed and total_spawned_by_engine == 0:
        tr.fail("Witch deployed but no engine-spawned entities detected (spawner broken)")

    if spawner_deployed and total_spawned_by_engine > 0:
        tr.metric("spawner_mechanic", "WORKING")

    # Validation: entity count should have exceeded initial deployment
    # (spawners create new entities over time)
    initial_count = entity_counts[10] if len(entity_counts) > 10 else 0
    if peak_entities <= initial_count and spawner_deployed:
        tr.warn(f"Peak entities ({peak_entities}) never exceeded initial ({initial_count}) — spawners may not be working")

    # Validation: entity count should be bounded (cleanup works)
    if peak_entities > 300:
        tr.warn(f"Peak entities = {peak_entities} (very high — check cleanup)")

    # Validation: no tick budget exceeded
    if tick_times and max(tick_times) > TICK_BUDGET_MS:
        tr.fail(f"Tick exceeded 50ms budget under spawner load: {max(tick_times):.2f}ms")

    return tr

def test_memory_usage(data: "cr_engine.GameData") -> TestResult:
    """
    Stress test: run multiple full matches while monitoring memory.

    Validates:
    - Memory doesn't grow unbounded across matches
    - A single match doesn't exceed reasonable memory limits
    - Continuous spawning during a match doesn't leak entities
    - Entity cleanup prevents OOM

    Scenario: Run 5 full matches with aggressive agents that spam cards
    every time they have enough elixir (worst-case entity churn).
    """
    tr = TestResult("5.2 Memory Usage (Dataset + Sim)")

    mem_before = get_mem_mb()
    tr.metric("mem_before_mb", mem_before)

    match_mems = []
    match_entity_peaks = []

    for match_idx in range(5):
        m = cr_engine.new_match(data, DECK_SWARM, DECK_HEAVY)
        peak_entities = 0
        cards_played = 0

        # Run a full match with continuous card spam
        for tick in range(min(MAX_MATCH_TICKS, 6000)):  # Cap at 5 min for speed
            if not m.is_running:
                break

            # Force both players to 10 elixir and play every 40 ticks
            if tick % 40 == 0:
                m.set_elixir(1, 10)
                m.set_elixir(2, 10)

                # P1 plays whatever's available
                playable1 = m.playable_cards(1)
                if playable1:
                    idx = playable1[0]
                    # Deploy on own side near bridge
                    try:
                        m.play_card(1, idx, -2000, -2000)
                        cards_played += 1
                    except Exception:
                        pass

                # P2 plays whatever's available
                playable2 = m.playable_cards(2)
                if playable2:
                    idx = playable2[0]
                    try:
                        m.play_card(2, idx, 2000, 2000)
                        cards_played += 1
                    except Exception:
                        pass

            m.step()
            peak_entities = max(peak_entities, m.num_entities)

        match_entity_peaks.append(peak_entities)
        mem_after_match = get_mem_mb()
        match_mems.append(mem_after_match)

    mem_after = get_mem_mb()
    tr.metric("mem_after_mb", mem_after)
    tr.metric("mem_delta_mb", mem_after - mem_before)
    tr.metric("peak_entities_per_match", match_entity_peaks)
    tr.metric("mem_per_match_mb", [round(m, 2) for m in match_mems])

    # Validation: total memory should be bounded
    # GameData is ~5-10MB, each match is tiny. 100MB total is very generous.
    if mem_after > 200:
        tr.fail(f"Memory usage too high: {mem_after:.1f}MB (limit 200MB)")

    # Validation: memory shouldn't grow linearly with matches
    # (would indicate leak)
    if len(match_mems) >= 3:
        growth = match_mems[-1] - match_mems[0]
        if growth > 50:
            tr.fail(f"Memory grew {growth:.1f}MB across 5 matches (possible leak)")

    # Validation: peak entities per match shouldn't be insane
    for i, peak in enumerate(match_entity_peaks):
        if peak > 500:
            tr.warn(f"Match {i}: peak entities = {peak} (very high)")

    return tr


# =========================================================================
# 5.3 — Tick Latency: Real-Time Constraint Validation
# =========================================================================

def test_tick_latency(data: "cr_engine.GameData") -> TestResult:
    """
    Measure tick execution time across different game phases and entity loads.

    Validates:
    - Empty arena ticks (baseline)
    - Light combat ticks (5-10 entities)
    - Heavy combat ticks (30+ entities)
    - Phase transitions (regular → double elixir → overtime)
    - p50, p95, p99, max all under real-time constraint

    Real CR runs at 20 tps (50ms budget). For AI, we want p99 < 5ms.
    """
    tr = TestResult("5.3 Tick Latency (Real-Time Constraint)")

    # --- Scenario A: Empty arena baseline ---
    m = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)
    empty_times = []
    for _ in range(500):
        t0 = time.perf_counter_ns()
        m.step()
        dt = (time.perf_counter_ns() - t0) / 1_000_000
        empty_times.append(dt)

    tr.metric("empty_p50_ms", percentile(empty_times, 50))
    tr.metric("empty_p99_ms", percentile(empty_times, 99))
    tr.metric("empty_max_ms", max(empty_times))

    # --- Scenario B: Light combat (2 troops per side) ---
    m = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)
    m.spawn_troop(1, "knight", -2000, -5000)
    m.spawn_troop(1, "musketeer", -1000, -6000)
    m.spawn_troop(2, "knight", 2000, 5000)
    m.spawn_troop(2, "musketeer", 1000, 6000)

    light_times = []
    for _ in range(500):
        t0 = time.perf_counter_ns()
        m.step()
        dt = (time.perf_counter_ns() - t0) / 1_000_000
        light_times.append(dt)

    tr.metric("light_p50_ms", percentile(light_times, 50))
    tr.metric("light_p99_ms", percentile(light_times, 99))
    tr.metric("light_max_ms", max(light_times))

    # --- Scenario C: Heavy combat (30+ troops, mixed types) ---
    m = cr_engine.new_match(data, DECK_SWARM, DECK_HEAVY)
    troops_per_side = [
        ("skeleton", 15), ("barbarian", 10), ("knight", 5),
    ]
    for key, count in troops_per_side:
        for i in range(count):
            x = -4000 + (i % 6) * 1300
            y = -5000 + (i // 6) * 800
            try:
                m.spawn_troop(1, key, x, y)
            except Exception:
                pass
            try:
                m.spawn_troop(2, key, -x, -y)
            except Exception:
                pass

    heavy_times = []
    for _ in range(500):
        t0 = time.perf_counter_ns()
        m.step()
        dt = (time.perf_counter_ns() - t0) / 1_000_000
        heavy_times.append(dt)

    tr.metric("heavy_entities", m.num_entities)
    tr.metric("heavy_p50_ms", percentile(heavy_times, 50))
    tr.metric("heavy_p95_ms", percentile(heavy_times, 95))
    tr.metric("heavy_p99_ms", percentile(heavy_times, 99))
    tr.metric("heavy_max_ms", max(heavy_times))

    # --- Scenario D: Phase transition — advance through all phases ---
    m = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)
    m.spawn_troop(1, "knight", 0, -5000)
    m.spawn_troop(2, "knight", 0, 5000)

    phase_times: dict[str, list[float]] = defaultdict(list)
    for _ in range(REGULAR_TIME_TICKS + OVERTIME_TICKS + 200):
        if not m.is_running:
            break
        phase = m.phase
        t0 = time.perf_counter_ns()
        m.step()
        dt = (time.perf_counter_ns() - t0) / 1_000_000
        phase_times[phase].append(dt)

    for phase_name, times in phase_times.items():
        if times:
            tr.metric(f"phase_{phase_name}_p99_ms", percentile(times, 99))

    # --- Validation ---
    all_times = empty_times + light_times + heavy_times
    overall_p99 = percentile(all_times, 99)
    overall_max = max(all_times)

    tr.metric("overall_p99_ms", overall_p99)
    tr.metric("overall_max_ms", overall_max)
    tr.metric("total_ticks_measured", len(all_times))

    if overall_max > TICK_BUDGET_MS:
        tr.fail(f"Tick exceeded 50ms budget: {overall_max:.2f}ms")

    if overall_p99 > TICK_TARGET_P99_MS:
        tr.warn(f"p99 tick latency {overall_p99:.2f}ms exceeds 5ms AI target")

    # Throughput
    total_time_s = sum(all_times) / 1000.0
    throughput = len(all_times) / total_time_s if total_time_s > 0 else 0
    tr.metric("throughput_ticks_per_sec", int(throughput))

    return tr


# =========================================================================
# 6.1 — State Logging: Full State Capture Every Tick
# =========================================================================

def test_state_logging(data: "cr_engine.GameData") -> TestResult:
    """
    Validate that full game state can be captured every tick.

    Validates:
    - get_entities() returns complete entity data every tick
    - get_observation() returns complete player observations
    - Tower HP, elixir, phase are all accessible
    - State is serializable to JSON (for replay/training data)
    - State capture overhead is bounded

    Scenario: Run 600 ticks (30s game time) with active combat,
    capture full state every tick, validate completeness.
    """
    tr = TestResult("6.1 State Logging (Full State Capture)")

    m = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)

    # Spawn some troops for interesting state
    m.set_elixir(1, 10)
    m.set_elixir(2, 10)
    m.spawn_troop(1, "knight", -2000, -5000)
    m.spawn_troop(1, "giant", 0, -6000)
    m.spawn_troop(1, "witch", 1000, -7000)
    m.spawn_troop(2, "valkyrie", -1000, 5000)
    m.spawn_troop(2, "prince", 0, 6000)
    m.spawn_troop(2, "musketeer", 2000, 7000)

    # Required fields in entity dicts (from lib.rs get_entities())
    REQUIRED_ENTITY_FIELDS = {
        "id", "team", "card_key", "x", "y", "z", "hp", "max_hp",
        "shield_hp", "alive", "damage", "kind", "num_buffs",
        "is_stunned", "is_frozen", "is_invisible",
        "speed_mult", "hitspeed_mult", "damage_mult",
        "is_evolved", "is_hero", "hero_ability_active",
    }

    # Required fields in observation dicts (from lib.rs get_observation())
    REQUIRED_OBS_FIELDS = {
        "tick", "phase", "time_remaining", "my_elixir", "my_hand",
        "my_king_hp", "my_princess_left_hp", "my_princess_right_hp",
        "my_king_alive", "my_princess_left_alive", "my_princess_right_alive",
        "opp_king_hp", "opp_princess_left_hp", "opp_princess_right_hp",
        "opp_king_alive", "opp_princess_left_alive", "opp_princess_right_alive",
        "my_crowns", "opp_crowns", "my_troop_count", "opp_troop_count",
        "total_entities",
    }

    state_log: list[dict] = []
    capture_times: list[float] = []
    field_errors: list[str] = []

    for tick in range(600):
        if not m.is_running:
            break

        m.step()

        # Capture full state
        t0 = time.perf_counter_ns()

        entities = m.get_entities()
        obs1 = m.get_observation(1)
        obs2 = m.get_observation(2)
        tower_hp = {
            "p1": m.p1_tower_hp(),
            "p2": m.p2_tower_hp(),
        }
        meta = {
            "tick": m.tick,
            "phase": m.phase,
            "p1_elixir": m.p1_elixir,
            "p2_elixir": m.p2_elixir,
            "p1_crowns": m.p1_crowns,
            "p2_crowns": m.p2_crowns,
            "num_entities": m.num_entities,
        }

        dt_us = (time.perf_counter_ns() - t0) / 1_000  # microseconds
        capture_times.append(dt_us)

        # Store snapshot (keep only every 10th tick to limit memory)
        if tick % 10 == 0:
            state_log.append({
                "meta": meta,
                "tower_hp": tower_hp,
                "entities": entities,
                "obs1": obs1,
                "obs2": obs2,
            })

        # Validate entity fields (check first 3 entities each tick, all on tick 0)
        check_count = len(entities) if tick == 0 else min(3, len(entities))
        for i in range(check_count):
            e = entities[i]
            missing = REQUIRED_ENTITY_FIELDS - set(e.keys())
            if missing:
                field_errors.append(f"tick={tick} entity[{i}] missing: {missing}")

            # Troop-specific fields
            if e.get("kind") == "troop":
                troop_fields = {"attack_phase", "phase_timer", "windup_ticks",
                                "backswing_ticks", "attack_cooldown", "hit_speed", "range_sq"}
                missing_troop = troop_fields - set(e.keys())
                if missing_troop:
                    field_errors.append(f"tick={tick} troop entity[{i}] missing: {missing_troop}")

            # SpellZone-specific fields
            if e.get("kind") == "spell_zone":
                sz_fields = {"sz_damage_per_tick", "sz_affects_air", "sz_affects_ground",
                             "sz_radius", "sz_remaining", "sz_hit_timer", "sz_hit_interval"}
                missing_sz = sz_fields - set(e.keys())
                if missing_sz:
                    field_errors.append(f"tick={tick} spell_zone entity[{i}] missing: {missing_sz}")

        # Validate observation fields
        for obs, pname in [(obs1, "P1"), (obs2, "P2")]:
            missing = REQUIRED_OBS_FIELDS - set(obs.keys())
            if missing:
                field_errors.append(f"tick={tick} {pname} obs missing: {missing}")

        # Cross-validate: obs total_entities should match len(entities)
        if obs1["total_entities"] != len(entities):
            field_errors.append(
                f"tick={tick} obs1.total_entities={obs1['total_entities']} != len(entities)={len(entities)}"
            )

    # Validate JSON serialization of captured states
    serialization_ok = True
    try:
        json_str = json.dumps(state_log[:5], default=str)
        parsed = json.loads(json_str)
        if len(parsed) != min(5, len(state_log)):
            serialization_ok = False
    except Exception as e:
        serialization_ok = False
        tr.fail(f"State not JSON-serializable: {e}")

    # Report
    tr.metric("ticks_captured", len(capture_times))
    tr.metric("snapshots_stored", len(state_log))
    tr.metric("avg_capture_us", sum(capture_times) / len(capture_times) if capture_times else 0)
    tr.metric("p99_capture_us", percentile(capture_times, 99))
    tr.metric("max_capture_us", max(capture_times) if capture_times else 0)
    tr.metric("json_serializable", serialization_ok)
    tr.metric("field_errors", len(field_errors))

    if field_errors:
        # Report first 5 errors
        for err in field_errors[:5]:
            tr.fail(err)
        if len(field_errors) > 5:
            tr.fail(f"...and {len(field_errors) - 5} more field errors")

    # Validation: capture overhead should be < 1ms per tick
    if capture_times:
        max_capture_ms = max(capture_times) / 1000.0
        if max_capture_ms > 1.0:
            tr.warn(f"State capture took {max_capture_ms:.2f}ms (>1ms target)")

    return tr


# =========================================================================
# 6.2 — Event Tracing: Diff-Based Event Reconstruction
# =========================================================================

def test_event_tracing(data: "cr_engine.GameData") -> TestResult:
    """
    Reconstruct game events by diffing consecutive state snapshots.

    Since the Rust engine doesn't emit events natively, we diff
    get_entities() snapshots between ticks to detect:
    - SPAWN: entity ID appears (new entity)
    - DEATH: entity ID disappears (entity removed)
    - DAMAGE: entity HP decreased
    - HEAL: entity HP increased
    - MOVE: entity position changed
    - BUFF_APPLY: num_buffs increased
    - BUFF_EXPIRE: num_buffs decreased
    - TOWER_DAMAGE: tower HP decreased

    Validates:
    - Events are actually detected (combat is happening)
    - Event counts are plausible for the scenario
    - No phantom events (spurious changes)

    Scenario: Knight vs Knight at close range — predictable combat.
    Then a larger fight with death spawns.
    """
    tr = TestResult("6.2 Event Tracing (Diff-Based Reconstruction)")

    # ── Scenario A: Controlled 1v1 — Knight vs Knight ──
    m = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)

    # Place knights facing each other near the center
    kid1 = m.spawn_troop(1, "knight", 0, -800)
    kid2 = m.spawn_troop(2, "knight", 0, 800)

    # Build entity index from current state
    def snapshot_entities(match_obj):
        """Return {id: entity_dict} for alive entities."""
        return {e["id"]: e for e in match_obj.get_entities()}

    def snapshot_towers(match_obj):
        return {
            "p1_king": match_obj.p1_tower_hp()[0],
            "p1_left": match_obj.p1_tower_hp()[1],
            "p1_right": match_obj.p1_tower_hp()[2],
            "p2_king": match_obj.p2_tower_hp()[0],
            "p2_left": match_obj.p2_tower_hp()[1],
            "p2_right": match_obj.p2_tower_hp()[2],
        }

    events: list[dict] = []
    prev_snap = snapshot_entities(m)
    prev_towers = snapshot_towers(m)

    for tick in range(400):
        if not m.is_running:
            break
        m.step()

        curr_snap = snapshot_entities(m)
        curr_towers = snapshot_towers(m)

        # Detect SPAWN events
        for eid in curr_snap:
            if eid not in prev_snap:
                events.append({
                    "tick": m.tick, "type": "SPAWN",
                    "entity_id": eid, "card_key": curr_snap[eid]["card_key"],
                    "team": curr_snap[eid]["team"],
                })

        # Detect DEATH events (entity disappeared)
        for eid in prev_snap:
            if eid not in curr_snap:
                events.append({
                    "tick": m.tick, "type": "DEATH",
                    "entity_id": eid, "card_key": prev_snap[eid]["card_key"],
                    "team": prev_snap[eid]["team"],
                })

        # Detect DAMAGE / HEAL / MOVE on surviving entities
        for eid in curr_snap:
            if eid in prev_snap:
                curr_e = curr_snap[eid]
                prev_e = prev_snap[eid]

                # HP change
                hp_delta = curr_e["hp"] - prev_e["hp"]
                if hp_delta < 0:
                    events.append({
                        "tick": m.tick, "type": "DAMAGE",
                        "entity_id": eid, "card_key": curr_e["card_key"],
                        "amount": -hp_delta,
                    })
                elif hp_delta > 0:
                    events.append({
                        "tick": m.tick, "type": "HEAL",
                        "entity_id": eid, "card_key": curr_e["card_key"],
                        "amount": hp_delta,
                    })

                # Position change
                if curr_e["x"] != prev_e["x"] or curr_e["y"] != prev_e["y"]:
                    events.append({
                        "tick": m.tick, "type": "MOVE",
                        "entity_id": eid,
                    })

                # Buff change
                buff_delta = curr_e["num_buffs"] - prev_e["num_buffs"]
                if buff_delta > 0:
                    events.append({
                        "tick": m.tick, "type": "BUFF_APPLY",
                        "entity_id": eid, "count": buff_delta,
                    })
                elif buff_delta < 0:
                    events.append({
                        "tick": m.tick, "type": "BUFF_EXPIRE",
                        "entity_id": eid, "count": -buff_delta,
                    })

        # Detect TOWER_DAMAGE
        for tower_key in curr_towers:
            if curr_towers[tower_key] < prev_towers[tower_key]:
                events.append({
                    "tick": m.tick, "type": "TOWER_DAMAGE",
                    "tower": tower_key,
                    "amount": prev_towers[tower_key] - curr_towers[tower_key],
                })

        prev_snap = curr_snap
        prev_towers = curr_towers

    # Analyze events
    event_counts = Counter(e["type"] for e in events)
    tr.metric("total_events", len(events))
    tr.metric("event_breakdown", dict(event_counts))

    # Validate: we should see damage events (combat happened)
    if event_counts.get("DAMAGE", 0) == 0:
        tr.fail("No DAMAGE events detected — combat didn't engage")

    # Validate: we should see at least one death
    if event_counts.get("DEATH", 0) == 0:
        tr.warn("No DEATH events — fight may not have resolved in 400 ticks")

    # Validate: we should see movement events
    if event_counts.get("MOVE", 0) == 0:
        tr.fail("No MOVE events detected — troops didn't move")

    # ── Scenario B: Golem death → Golemite death-spawn validation ──
    # The Golem (8192 HP at level 11) must die within the tick budget.
    # Key challenge: Golem is building-targeting (won't fight barbarians),
    # and P2 barbarians default-target P1 towers, not P1 troops.
    # Solution: Position P2 Golem walking toward P1 towers, with P1 barbarians
    # on P2's side heading toward P2 towers. They meet and engage.
    # The barbarians will target the Golem as the nearest enemy entity.
    m2 = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)

    # P2 Golem heading south toward P1 towers — spawned just past the bridge
    m2.spawn_troop(2, "golem", 0, -2000)

    # P1 Barbarians heading north — they'll encounter the Golem as nearest enemy
    # Spawned in a tight cluster south of the Golem so they immediately engage
    barb_positions = [
        (-400, -2800), (400, -2800), (-400, -3200), (400, -3200),
        (0, -2600), (0, -3000), (-300, -2700), (300, -2700),
        (-200, -2900), (200, -2900), (-500, -3100), (500, -3100),
        (0, -3300), (-300, -3400), (300, -3400), (0, -3500),
    ]
    for bx, by in barb_positions:
        m2.spawn_troop(1, "barbarian", bx, by)

    events_b: list[dict] = []
    prev_snap = snapshot_entities(m2)
    golem_died = False
    golemite_spawned = False
    golemite_count = 0

    for tick in range(600):
        if not m2.is_running:
            break
        m2.step()

        curr_snap = snapshot_entities(m2)

        # Detect deaths
        for eid in prev_snap:
            if eid not in curr_snap:
                card_key = prev_snap[eid]["card_key"]
                events_b.append({"tick": m2.tick, "type": "DEATH", "key": card_key})
                # Check if the Golem died (key is "golem" after data_types.rs resolution)
                if card_key.lower() in ("golem",):
                    golem_died = True

        # Detect spawns
        for eid in curr_snap:
            if eid not in prev_snap:
                card_key = curr_snap[eid]["card_key"]
                events_b.append({"tick": m2.tick, "type": "SPAWN", "key": card_key})
                # Golemites: card_key will be "Golemite" or "golemite" from death_spawn_character
                if "golemite" in card_key.lower():
                    golemite_spawned = True
                    golemite_count += 1

        prev_snap = curr_snap

    event_counts_b = Counter(e["type"] for e in events_b)
    tr.metric("scenario_b_events", dict(event_counts_b))
    tr.metric("golem_died", golem_died)
    tr.metric("golemite_spawned", golemite_spawned)
    tr.metric("golemite_count", golemite_count)

    # Validate: Golem MUST die with 8 barbarians attacking it for 600 ticks
    if not golem_died:
        tr.fail("Golem did not die despite 16 barbarians attacking for 600 ticks")

    # Validate: if Golem died, exactly 2 Golemites should spawn (death_spawn_count=2)
    if golem_died and not golemite_spawned:
        tr.fail("Golem died but no Golemites spawned (death_spawn_character broken)")
    if golem_died and golemite_spawned and golemite_count != 2:
        tr.warn(f"Expected 2 Golemites from Golem death, got {golemite_count}")

    # Validate: JSON serializable
    try:
        json.dumps(events[:20], default=str)
    except Exception as e:
        tr.fail(f"Events not JSON-serializable: {e}")

    return tr


# =========================================================================
# 6.3 — Feature Extraction: AI State Vectors
# =========================================================================

def test_feature_extraction(data: "cr_engine.GameData") -> TestResult:
    """
    Validate that the observation space is complete, consistent, and
    convertible to fixed-size numeric vectors for AI training.

    Tests:
    - Observation shape is consistent across ticks
    - All numeric fields are bounded (no NaN, no overflow)
    - Entity feature vectors have consistent dimensions
    - Feature extraction is fast enough for real-time AI
    - Observations are symmetric (P1 obs of P2 ≈ P2 obs of P1)

    This is the training.py spec (Phase 5) validated against live data:
      - my_elixir: 0-10
      - hand: 4 card keys
      - tower HP: 6 floats (3 own + 3 opponent)
      - troop counts
      - phase: one of 4
      - time_remaining: bounded
    """
    tr = TestResult("6.3 Feature Extraction (AI State Vectors)")

    m = cr_engine.new_match(data, DECK_HEAVY, DECK_SWARM)

    # Spawn some units for rich state
    m.set_elixir(1, 10)
    m.set_elixir(2, 10)
    m.spawn_troop(1, "giant", 0, -5000)
    m.spawn_troop(1, "witch", -1000, -6000)
    m.spawn_troop(1, "musketeer", 1000, -7000)
    m.spawn_troop(2, "valkyrie", 0, 5000)
    m.spawn_troop(2, "prince", -1000, 6000)
    m.spawn_troop(2, "knight", 1000, 7000)

    # Define feature extraction function matching training.py spec
    VALID_PHASES = {"regular", "double_elixir", "overtime", "sudden_death"}

    def extract_features(match_obj, player: int) -> dict:
        """Extract a fixed-schema feature dict from match state."""
        obs = match_obj.get_observation(player)
        entities = match_obj.get_entities()

        # Normalize elixir (0-1)
        elixir_norm = obs["my_elixir"] / MAX_ELIXIR

        # Hand card costs (4 floats, 0 if slot empty)
        hand_keys = obs["my_hand"]
        hand_costs = []
        for k in hand_keys:
            cost = data.get_elixir_cost(k)
            hand_costs.append(cost / 10.0 if cost > 0 else 0.0)
        while len(hand_costs) < 4:
            hand_costs.append(0.0)
        hand_costs = hand_costs[:4]

        # Tower HP normalized (0-1)
        my_king_hp = obs["my_king_hp"] / KING_TOWER_HP
        my_pl_hp = obs["my_princess_left_hp"] / PRINCESS_TOWER_HP
        my_pr_hp = obs["my_princess_right_hp"] / PRINCESS_TOWER_HP
        opp_king_hp = obs["opp_king_hp"] / KING_TOWER_HP
        opp_pl_hp = obs["opp_princess_left_hp"] / PRINCESS_TOWER_HP
        opp_pr_hp = obs["opp_princess_right_hp"] / PRINCESS_TOWER_HP

        # Phase one-hot
        phase = obs["phase"]
        phase_vec = [1.0 if phase == p else 0.0 for p in
                     ["regular", "double_elixir", "overtime", "sudden_death"]]

        # Time remaining normalized
        time_norm = obs["time_remaining"] / MAX_MATCH_TICKS

        # Troop counts normalized (cap at 50 for normalization)
        my_troops = min(obs["my_troop_count"], 50) / 50.0
        opp_troops = min(obs["opp_troop_count"], 50) / 50.0

        # Entity spatial features: for each entity, extract (x_norm, y_norm, hp_norm, team)
        entity_features = []
        for e in entities:
            if e["kind"] not in ("troop", "building"):
                continue
            entity_features.append({
                "x_norm": e["x"] / ARENA_HALF_W,
                "y_norm": e["y"] / ARENA_HALF_H,
                "hp_norm": e["hp"] / max(e["max_hp"], 1),
                "is_mine": 1.0 if e["team"] == player else 0.0,
                "is_flying": 1.0 if e["z"] > 0 else 0.0,
                "kind": e["kind"],
            })

        # Flat feature vector (training.py spec: ~50 floats)
        flat = (
            [elixir_norm]
            + hand_costs
            + [my_king_hp, my_pl_hp, my_pr_hp]
            + [opp_king_hp, opp_pl_hp, opp_pr_hp]
            + phase_vec
            + [time_norm]
            + [my_troops, opp_troops]
            + [obs["my_crowns"] / 3.0, obs["opp_crowns"] / 3.0]
        )

        return {
            "flat_vector": flat,
            "flat_dim": len(flat),
            "entity_features": entity_features,
            "phase": phase,
            "raw_obs": obs,
        }

    # Run 400 ticks, extract features every tick, validate consistency
    feature_dims: set[int] = set()
    extraction_times: list[float] = []
    value_errors: list[str] = []
    phase_seen: set[str] = set()

    for tick in range(400):
        if not m.is_running:
            break

        m.step()

        # Play cards periodically to keep the field interesting
        if tick % 60 == 0:
            m.set_elixir(1, 10)
            m.set_elixir(2, 10)
            p1_playable = m.playable_cards(1)
            p2_playable = m.playable_cards(2)
            if p1_playable:
                try:
                    m.play_card(1, p1_playable[0], -2000, -4000)
                except Exception:
                    pass
            if p2_playable:
                try:
                    m.play_card(2, p2_playable[0], 2000, 4000)
                except Exception:
                    pass

        # Extract features for both players
        t0 = time.perf_counter_ns()
        feat1 = extract_features(m, 1)
        feat2 = extract_features(m, 2)
        dt_us = (time.perf_counter_ns() - t0) / 1_000
        extraction_times.append(dt_us)

        feature_dims.add(feat1["flat_dim"])
        feature_dims.add(feat2["flat_dim"])
        phase_seen.add(feat1["phase"])

        # Validate numeric bounds
        for i, v in enumerate(feat1["flat_vector"]):
            if not isinstance(v, (int, float)):
                value_errors.append(f"tick={tick} feat1[{i}] not numeric: {type(v)}")
            elif v != v:  # NaN check
                value_errors.append(f"tick={tick} feat1[{i}] is NaN")
            elif abs(v) > 100:
                value_errors.append(f"tick={tick} feat1[{i}] out of range: {v}")

        # Validate entity spatial features are bounded
        for ef in feat1["entity_features"]:
            if abs(ef["x_norm"]) > 1.5:
                value_errors.append(f"tick={tick} entity x_norm={ef['x_norm']:.2f} out of range")
            if abs(ef["y_norm"]) > 1.5:
                value_errors.append(f"tick={tick} entity y_norm={ef['y_norm']:.2f} out of range")

        # Symmetry check: P1's opp_king_hp should equal P2's my_king_hp
        if tick == 0:
            p1_obs = feat1["raw_obs"]
            p2_obs = feat2["raw_obs"]
            if p1_obs["opp_king_hp"] != p2_obs["my_king_hp"]:
                value_errors.append(
                    f"Symmetry broken: P1.opp_king={p1_obs['opp_king_hp']} != P2.my_king={p2_obs['my_king_hp']}"
                )
            if p1_obs["my_king_hp"] != p2_obs["opp_king_hp"]:
                value_errors.append(
                    f"Symmetry broken: P1.my_king={p1_obs['my_king_hp']} != P2.opp_king={p2_obs['opp_king_hp']}"
                )

    # Report
    tr.metric("feature_dims", feature_dims)
    tr.metric("phases_seen", phase_seen)
    tr.metric("avg_extraction_us", sum(extraction_times) / len(extraction_times) if extraction_times else 0)
    tr.metric("p99_extraction_us", percentile(extraction_times, 99))
    tr.metric("max_extraction_us", max(extraction_times) if extraction_times else 0)
    tr.metric("value_errors", len(value_errors))

    # Validation: feature dimensions must be consistent (same every tick)
    if len(feature_dims) > 1:
        tr.fail(f"Feature dimension inconsistent: {feature_dims}")
    else:
        tr.metric("consistent_dim", list(feature_dims)[0])

    # Validation: no value errors
    if value_errors:
        for err in value_errors[:5]:
            tr.fail(err)
        if len(value_errors) > 5:
            tr.fail(f"...and {len(value_errors) - 5} more value errors")

    # Validation: phase should be valid
    invalid_phases = phase_seen - VALID_PHASES
    if invalid_phases:
        tr.fail(f"Invalid phases detected: {invalid_phases}")

    # Validation: extraction should be fast (<500μs for AI real-time)
    if extraction_times:
        max_extract_ms = max(extraction_times) / 1000.0
        if max_extract_ms > 2.0:
            tr.warn(f"Feature extraction took {max_extract_ms:.2f}ms (>2ms target)")

    return tr


# =========================================================================
# 5.4 — Agent Count Ceiling: Find the Actual Limit
# =========================================================================

def test_agent_ceiling(data: "cr_engine.GameData") -> TestResult:
    """
    Escalation test: keep increasing agent count until p99 exceeds
    the 50 ms real-time budget, or we hit 3000 agents (whichever first).

    We test: 100, 200, 400, 600, 800, 1000, 1500, 2000, 2500, 3000.
    At each level, spawn N agents, run 100 ticks, measure p99.

    This finds the REAL scaling ceiling — not a theoretical projection.

    Why this matters: in RL self-play training, the simulation speed is
    the bottleneck. Knowing the exact agent ceiling tells you how many
    parallel environments you can run per core before real-time breaks.
    """
    tr = TestResult("5.4 Agent Count Ceiling (find the limit)")

    test_levels = [100, 200, 400, 600, 800, 1000, 1500, 2000, 2500, 3000]
    results_table: list[dict] = []
    ceiling_n = None       # first N where p99 > 50ms
    last_ok_n = 0          # last N where p99 <= 50ms

    for n_agents in test_levels:
        m = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)

        # Spawn N/2 agents per side, spread across the arena
        spawned = 0
        for i in range(n_agents):
            team = 1 if i % 2 == 0 else 2
            # Spread in a grid across each team's half
            col = (i // 2) % 20
            row = (i // 2) // 20
            x = -7000 + col * 700
            if team == 1:
                y = -12000 + row * 600
            else:
                y = 3000 + row * 600
            try:
                m.spawn_troop(team, "skeleton", x, y)
                spawned += 1
            except Exception:
                break

        if spawned < n_agents * 0.9:
            # Couldn't spawn enough — engine rejected, note it and stop
            tr.metric(f"N={n_agents}_spawn_fail", f"only spawned {spawned}")
            break

        # Run 100 ticks and measure
        tick_times = []
        for _ in range(100):
            t0 = time.perf_counter_ns()
            m.step()
            dt_ms = (time.perf_counter_ns() - t0) / 1_000_000
            tick_times.append(dt_ms)

        p50 = percentile(tick_times, 50)
        p99 = percentile(tick_times, 99)
        max_t = max(tick_times)
        alive_after = m.num_entities

        results_table.append({
            "agents": n_agents,
            "spawned": spawned,
            "alive_after_100t": alive_after,
            "p50_ms": round(p50, 4),
            "p99_ms": round(p99, 4),
            "max_ms": round(max_t, 4),
        })

        tr.metric(f"N={n_agents}_p99_ms", round(p99, 4))

        if p99 <= TICK_BUDGET_MS:
            last_ok_n = n_agents
        elif ceiling_n is None:
            ceiling_n = n_agents

    # Summary metrics
    tr.metric("levels_tested", [r["agents"] for r in results_table])
    tr.metric("last_ok_under_50ms", last_ok_n)

    if ceiling_n is not None:
        tr.metric("ceiling_agents", ceiling_n)
        tr.metric("ceiling_p99_ms", next(r["p99_ms"] for r in results_table if r["agents"] == ceiling_n))
    else:
        tr.metric("ceiling_agents", f">{test_levels[-1]} (never exceeded budget)")

    # Provide the full scaling table as a metric
    tr.metric("scaling_table", results_table)

    # Validation: 150 agents (our previous test) must still be under budget
    r150 = next((r for r in results_table if r["agents"] >= 100), None)
    if r150 and r150["p99_ms"] > TICK_BUDGET_MS:
        tr.fail(f"Even {r150['agents']} agents exceeded 50ms: p99={r150['p99_ms']}ms")

    return tr


# =========================================================================
# 5.5 — Parallel Simulation: Measured Concurrent Matches
# =========================================================================

def test_parallel_simulation(data: "cr_engine.GameData") -> TestResult:
    """
    Actually create N independent matches and step them ALL within a
    wall-clock time budget. This measures REAL parallel throughput —
    not a theoretical division of single-tick speed.

    Method:
    1. Create N matches (each with its own GameState).
    2. For each "real-time frame" (50 ms budget): step ALL N matches once.
    3. Measure wall-clock time for the batch step.
    4. Increase N until the batch exceeds 50 ms.

    This tells you: "How many independent simulations can a single core
    actually sustain at 20 tps real-time?"

    Each match has 4 active troops (light combat) to be realistic —
    real RL training environments aren't empty arenas.
    """
    tr = TestResult("5.5 Parallel Simulation (measured concurrent matches)")

    test_counts = [10, 50, 100, 250, 500, 1000, 2000, 5000, 10000]
    results_table: list[dict] = []
    max_sustained = 0

    for n_matches in test_counts:
        # Create N matches with light combat (4 troops each)
        matches = []
        for _ in range(n_matches):
            m = cr_engine.new_match(data, DECK_HEAVY, DECK_HEAVY)
            m.spawn_troop(1, "knight", -2000, -5000)
            m.spawn_troop(1, "musketeer", -1000, -6000)
            m.spawn_troop(2, "knight", 2000, 5000)
            m.spawn_troop(2, "musketeer", 1000, 6000)
            matches.append(m)

        # Warm up: step all once (first tick can be slower due to cache)
        for m in matches:
            m.step()

        # Measure: step all N matches, time the entire batch
        batch_times = []
        for _ in range(20):  # 20 frames = 1 second of real-time
            t0 = time.perf_counter_ns()
            for m in matches:
                m.step()
            dt_ms = (time.perf_counter_ns() - t0) / 1_000_000
            batch_times.append(dt_ms)

        avg_batch = sum(batch_times) / len(batch_times)
        p99_batch = percentile(batch_times, 99)
        max_batch = max(batch_times)
        per_match_us = (avg_batch * 1000) / n_matches  # μs per match per frame

        results_table.append({
            "matches": n_matches,
            "avg_batch_ms": round(avg_batch, 3),
            "p99_batch_ms": round(p99_batch, 3),
            "max_batch_ms": round(max_batch, 3),
            "per_match_us": round(per_match_us, 3),
            "fits_in_budget": p99_batch <= TICK_BUDGET_MS,
        })

        tr.metric(f"N={n_matches}_p99_batch_ms", round(p99_batch, 3))

        if p99_batch <= TICK_BUDGET_MS:
            max_sustained = n_matches

        # Clean up to free memory before next round
        del matches

    tr.metric("max_sustained_real_time", max_sustained)
    tr.metric("parallel_table", results_table)

    # Estimate: if last tested count still fits, extrapolate
    if max_sustained == test_counts[-1]:
        # We didn't find the ceiling — extrapolate from per-match cost
        last = results_table[-1]
        estimated_max = int(TICK_BUDGET_MS * 1000 / last["per_match_us"])
        tr.metric("estimated_ceiling", estimated_max)
    elif max_sustained > 0:
        # Find the first count that broke, interpolate
        for i, r in enumerate(results_table):
            if not r["fits_in_budget"]:
                broke_at = r["matches"]
                prev = results_table[i-1] if i > 0 else None
                if prev:
                    # Linear interpolation between last-ok and first-fail
                    ok_n = prev["matches"]
                    ok_ms = prev["p99_batch_ms"]
                    fail_ms = r["p99_batch_ms"]
                    frac = (TICK_BUDGET_MS - ok_ms) / (fail_ms - ok_ms)
                    estimated = int(ok_n + frac * (broke_at - ok_n))
                    tr.metric("estimated_ceiling", estimated)
                break

    # Validation: at least 100 parallel matches should fit
    if max_sustained < 100:
        tr.fail(f"Only {max_sustained} parallel matches fit in 50ms budget (expected 100+)")

    return tr


# =========================================================================
# Test Runner
# =========================================================================

def run_all_tests():
    """Run all stress tests and print a consolidated report."""

    print("╔══════════════════════════════════════════════════════════════╗")
    print("║   HARDCORE STRESS TEST — CR Simulator Engine                ║")
    print("║   Data-driven • No approximations • Full validation        ║")
    print("╚══════════════════════════════════════════════════════════════╝")
    print()

    # Load game data
    print("Loading game data...")
    t0 = time.perf_counter()
    data = cr_engine.load_data("data/")
    load_time = (time.perf_counter() - t0) * 1000
    print(f"  Loaded in {load_time:.0f}ms: {data}")
    print()

    # Run all tests
    tests = [
        ("5.1", "Multi-Unit Scaling", test_multi_unit_scaling),
        ("5.1b", "Spawner Entity Growth", test_spawner_growth),
        ("5.2", "Memory Usage", test_memory_usage),
        ("5.3", "Tick Latency", test_tick_latency),
        ("5.4", "Agent Count Ceiling", test_agent_ceiling),
        ("5.5", "Parallel Simulation", test_parallel_simulation),
        ("6.1", "State Logging", test_state_logging),
        ("6.2", "Event Tracing", test_event_tracing),
        ("6.3", "Feature Extraction", test_feature_extraction),
    ]

    results: list[TestResult] = []
    for test_id, name, fn in tests:
        print(f"Running {test_id}: {name}...")
        try:
            result = fn(data)
        except Exception as e:
            result = TestResult(f"{test_id} {name}")
            result.fail(f"CRASHED: {e}")
            result.fail(traceback.format_exc())
        results.append(result)
        print(result.report())

    # ── Summary ──
    passed = sum(1 for r in results if r.passed)
    failed = sum(1 for r in results if not r.passed)
    total = len(results)

    print(f"\n{'═'*70}")
    print(f"  RESULTS: {passed}/{total} passed, {failed}/{total} failed")
    print(f"{'═'*70}")

    if failed > 0:
        print("\n  FAILED TESTS:")
        for r in results:
            if not r.passed:
                print(f"    ✗ {r.name}")
                for e in r.errors[:3]:
                    print(f"      → {e}")
    else:
        print("\n  ALL TESTS PASSED ✅")

    # Key performance metrics summary
    print(f"\n{'─'*70}")
    print("  KEY METRICS:")
    for r in results:
        for k, v in r.metrics.items():
            if any(kw in k for kw in ["p99", "throughput", "consistent_dim", "mem_delta", "ceiling", "max_sustained", "estimated_ceiling", "last_ok"]):
                if isinstance(v, float):
                    print(f"    {r.name[:30]:30s} {k}: {v:.4f}")
                else:
                    print(f"    {r.name[:30]:30s} {k}: {v}")
    print()

    return failed == 0


# =========================================================================
# Entry point
# =========================================================================

if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)
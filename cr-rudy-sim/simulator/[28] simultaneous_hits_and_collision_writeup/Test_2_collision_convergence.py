"""
TEST 2 — N-Body Collision Convergence
=======================================
Writeup claim (§3):
  - N troops spawned at the exact same position converge to stable
    equilibrium positions with min pairwise distance ≈ 2 × collision_radius.
  - 5 Knights converge in ~10 ticks, 10 Knights in ~16 ticks.

Setup:
  - Run A: Spawn 5 P1 Knights at (0, -5000)
  - Run B: Spawn 10 P1 Knights at (0, -5000)
  - Track min pairwise distance each tick until convergence
  - Knight collision_radius = 500, so equilibrium ≈ 1000

What to look for in the replay viewer:
  - Knights start as a single stacked blob
  - They rapidly fan out in the first ~10 ticks
  - Then stabilize into a fixed formation
  - The 10-Knight version takes visibly longer to settle

Console output tracks min pairwise distance converging to ~998.
"""

import sys, json, time, math
sys.path.insert(0, ".")

from python.data_loader import load_game_data
from python.replay_recorder import _snapshot_tick, save_replay

import cr_engine

data = load_game_data("data/")

FILLER_DECK = ["knight", "archer", "giant", "musketeer",
               "valkyrie", "bomber", "prince", "babydragon"]


def min_pairwise_dist(entities, ids):
    """Compute minimum pairwise distance among a set of entity IDs."""
    positions = []
    for eid in ids:
        e = next((e for e in entities if e["id"] == eid and e["alive"]), None)
        if e:
            positions.append((e["x"], e["y"]))

    if len(positions) < 2:
        return -1, len(positions)

    min_dist = float("inf")
    for i in range(len(positions)):
        for j in range(i + 1, len(positions)):
            dx = positions[i][0] - positions[j][0]
            dy = positions[i][1] - positions[j][1]
            d = math.isqrt(dx * dx + dy * dy)
            if d < min_dist:
                min_dist = d

    return min_dist, len(positions)


def count_unique_positions(entities, ids):
    """Count how many unique (x,y) positions exist."""
    positions = set()
    for eid in ids:
        e = next((e for e in entities if e["id"] == eid and e["alive"]), None)
        if e:
            positions.add((e["x"], e["y"]))
    return len(positions)


# ═══════════════════════════════════════════════════════════
# RUN A: 5 Knights
# ═══════════════════════════════════════════════════════════
print("=" * 60)
print("  RUN A: 5 Knights at same position")
print("=" * 60)

match_a = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)
match_a.set_elixir(1, 10)
match_a.set_elixir(2, 10)

ids_a = []
for i in range(5):
    kid = match_a.spawn_troop(1, "knight", 0, -5000, 11, False)
    ids_a.append(kid)
print(f"  Spawned 5 Knights (ids={ids_a}) at (0, -5000)")

TICKS = 30
frames_a = [_snapshot_tick(match_a)]
converged_tick_a = None

print(f"\n{'tick':>5s}  {'min_dist':>8s}  {'unique_pos':>10s}  {'status'}")
print("-" * 45)

for tick in range(1, TICKS + 1):
    match_a.step()
    frames_a.append(_snapshot_tick(match_a))

    entities = match_a.get_entities()
    md, alive = min_pairwise_dist(entities, ids_a)
    uniq = count_unique_positions(entities, ids_a)

    status = ""
    if md >= 950 and converged_tick_a is None:
        converged_tick_a = tick
        status = "← CONVERGED"

    print(f"{tick:5d}  {md:8d}  {uniq:10d}  {status}")

print(f"\n  Converged at tick: {converged_tick_a}")
entities = match_a.get_entities()
md_final, _ = min_pairwise_dist(entities, ids_a)
print(f"  Final min pairwise dist: {md_final}")
print(f"  Expected equilibrium: ~1000 (2 × collision_radius)")


# ═══════════════════════════════════════════════════════════
# RUN B: 10 Knights
# ═══════════════════════════════════════════════════════════
print(f"\n{'=' * 60}")
print("  RUN B: 10 Knights at same position")
print("=" * 60)

match_b = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)
match_b.set_elixir(1, 10)
match_b.set_elixir(2, 10)

ids_b = []
for i in range(10):
    kid = match_b.spawn_troop(1, "knight", 0, -5000, 11, False)
    ids_b.append(kid)
print(f"  Spawned 10 Knights (ids={ids_b[:5]}...{ids_b[-1]}) at (0, -5000)")

TICKS_B = 40
frames_b = [_snapshot_tick(match_b)]
converged_tick_b = None

print(f"\n{'tick':>5s}  {'min_dist':>8s}  {'unique_pos':>10s}  {'status'}")
print("-" * 45)

for tick in range(1, TICKS_B + 1):
    match_b.step()
    frames_b.append(_snapshot_tick(match_b))

    entities = match_b.get_entities()
    md, alive = min_pairwise_dist(entities, ids_b)
    uniq = count_unique_positions(entities, ids_b)

    status = ""
    if md >= 950 and converged_tick_b is None:
        converged_tick_b = tick
        status = "← CONVERGED"

    print(f"{tick:5d}  {md:8d}  {uniq:10d}  {status}")

print(f"\n  Converged at tick: {converged_tick_b}")
entities = match_b.get_entities()
md_final_b, _ = min_pairwise_dist(entities, ids_b)
print(f"  Final min pairwise dist: {md_final_b}")


# ═══════════════════════════════════════════════════════════
# VERDICT
# ═══════════════════════════════════════════════════════════
print(f"\n{'=' * 60}")
print("  VERDICT")
print("=" * 60)

a_pass = md_final is not None and md_final >= 900
b_pass = md_final_b is not None and md_final_b >= 900
scaling = converged_tick_b is not None and converged_tick_a is not None and converged_tick_b > converged_tick_a

print(f"  N=5 equilibrium:  {md_final} iu {'✓' if a_pass else '✗'} (expect ~1000)")
print(f"  N=10 equilibrium: {md_final_b} iu {'✓' if b_pass else '✗'} (expect ~1000)")
print(f"  N=5 converged:    tick {converged_tick_a}")
print(f"  N=10 converged:   tick {converged_tick_b} {'✓ (slower than N=5)' if scaling else ''}")
print(f"\n  Overall: {'PASS ✓' if a_pass and b_pass else 'FAIL ✗'}")


# ═══════════════════════════════════════════════════════════
# SAVE REPLAY (5-Knight version)
# ═══════════════════════════════════════════════════════════
total = len(frames_a) - 1
replay = {
    "version": 1,
    "recorded_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    "deck1": FILLER_DECK,
    "deck2": FILLER_DECK,
    "sample_rate": 1,
    "total_ticks": total,
    "result": {
        "winner": "draw",
        "p1_crowns": 0, "p2_crowns": 0,
        "ticks": total,
        "seconds": total / 20.0,
    },
    "events": [
        {"tick": 0, "player": 1, "action": "spawn", "card": "knight",
         "x": 0, "y": -5000, "note": "5 Knights at same position"},
    ],
    "frames": frames_a,
}
save_replay(replay, "replay_test2_collision_convergence.json", compress=False)
print(f"\nReplay saved → replay_test2_collision_convergence.json")
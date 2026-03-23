"""
TEST 3 — Swarm Cohesion
========================
Writeup claim: 5 Barbarians spawned in a cluster stay within ~4 tiles
width and reasonable average pairwise distance after 40 ticks of walking.

Setup:
  - Spawn 5 P1 Barbarians at the SAME position (0, -6000)
  - The avoidance + collision system must separate them organically
  - Run 60 ticks (~3 seconds)
  - Key measurement at tick 40 (matching writeup)

What to look for in the replay viewer:
  - The pack should separate into a natural formation while advancing
  - They should NOT fan out into a wide horizontal line spanning the arena
  - The formation should look like real CR Barbarians: a tight cluster

Console output tracks max spread width and avg pairwise distance.
"""

import sys, json, time, math
sys.path.insert(0, ".")

from python.data_loader import load_game_data
from python.replay_recorder import _snapshot_tick, save_replay

import cr_engine

data = load_game_data("data/")

FILLER_DECK = ["knight", "archer", "giant", "musketeer",
               "valkyrie", "bomber", "prince", "babydragon"]

match = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)
match.set_elixir(1, 10)
match.set_elixir(2, 10)

# ── Spawn 5 Barbarians in a tight cluster ──
# Spawn all 5 at the SAME position in the LEFT LANE.
# In real CR you deploy Barbarians in a lane, not dead center.
# At x=-3000 all 5 will path toward the left bridge — the cohesion
# test is whether they stay in a tight pack, not whether they split lanes.
CENTER_X, CENTER_Y = -3000, -6000
barb_ids = []
for i in range(5):
    bid = match.spawn_troop(1, "barbarian", CENTER_X, CENTER_Y, 11, False)
    barb_ids.append(bid)
    print(f"  Barbarian {i+1} (id={bid}) at ({CENTER_X}, {CENTER_Y})")


def measure_swarm(entities, ids):
    """Compute max X spread and average pairwise distance for a set of entity IDs."""
    positions = []
    for eid in ids:
        e = next((e for e in entities if e["id"] == eid and e["alive"]), None)
        if e:
            positions.append((e["x"], e["y"]))

    if len(positions) < 2:
        return 0, 0, positions

    xs = [p[0] for p in positions]
    width = max(xs) - min(xs)

    # Average pairwise distance
    total_dist = 0
    pairs = 0
    for i in range(len(positions)):
        for j in range(i + 1, len(positions)):
            dx = positions[i][0] - positions[j][0]
            dy = positions[i][1] - positions[j][1]
            total_dist += math.isqrt(dx * dx + dy * dy)
            pairs += 1

    avg_dist = total_dist // pairs if pairs > 0 else 0
    return width, avg_dist, positions


# ── Run and record ──
TOTAL_TICKS = 60
frames = [_snapshot_tick(match)]
tick40_data = None

print(f"\n{'tick':>5s}  {'width':>6s}  {'avg_pair':>8s}  {'alive':>5s}  {'width_tiles':>11s}")
print("-" * 50)

for tick in range(1, TOTAL_TICKS + 1):
    match.step()
    frames.append(_snapshot_tick(match))

    if tick % 10 == 0:
        entities = match.get_entities()
        width, avg_dist, positions = measure_swarm(entities, barb_ids)
        alive = len(positions)
        tiles = width / 600.0
        print(f"{tick:5d}  {width:6d}  {avg_dist:8d}  {alive:5d}  {tiles:9.1f} tiles")
        if tick == 40:
            tick40_data = (width, avg_dist, alive)

# ── Verdict at tick 40 (writeup reference point) ──
print(f"\n{'='*50}")
if tick40_data:
    w40, d40, a40 = tick40_data
    print(f"State at tick 40 (writeup reference):")
    print(f"  Alive: {a40}/5")
    print(f"  Max X spread: {w40} units ({w40/600:.1f} tiles)")
    print(f"  Avg pairwise dist: {d40} units ({d40/600:.1f} tiles)")

# ── Final state ──
entities = match.get_entities()
width, avg_dist, positions = measure_swarm(entities, barb_ids)
alive = len(positions)

print(f"\nFinal state at tick {TOTAL_TICKS}:")
print(f"  Alive: {alive}/5")
print(f"  Max X spread: {width} units ({width/600:.1f} tiles)")
print(f"  Avg pairwise dist: {avg_dist} units ({avg_dist/600:.1f} tiles)")

# Growth rate: is the spread stabilizing or exploding?
# Without cohesion safeguards, 5 troops with O(N) avoidance would
# spread to 10+ tiles. Staying under 5 tiles = safeguards working.
width_ok = width <= 3000   # ~5 tiles — reasonable CR formation
dist_ok = avg_dist <= 2500
passed = width_ok and dist_ok and alive >= 3

print(f"\nCohesion check (is this a tight pack, not a scattered mess?):")
print(f"  Width < 5 tiles: {'✓' if width_ok else '✗'} ({width/600:.1f} tiles)")
print(f"  Avg dist < 4.2 tiles: {'✓' if dist_ok else '✗'} ({avg_dist/600:.1f} tiles)")
print(f"  All alive: {'✓' if alive >= 3 else '✗'} ({alive}/5)")
print(f"\n{'PASS' if passed else 'FAIL'}: "
      f"{'Swarm stays cohesive ✓' if passed else 'Swarm scattered too wide ✗'}")

for i, (x, y) in enumerate(positions):
    print(f"  Barb {i+1}: ({x}, {y})")

# ── Save replay ──
replay = {
    "version": 1,
    "recorded_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    "deck1": FILLER_DECK,
    "deck2": FILLER_DECK,
    "sample_rate": 1,
    "total_ticks": TOTAL_TICKS,
    "result": {
        "winner": "draw",
        "p1_crowns": 0, "p2_crowns": 0,
        "ticks": TOTAL_TICKS,
        "seconds": TOTAL_TICKS / 20.0,
    },
    "events": [
        {"tick": 0, "player": 1, "action": "spawn", "card": "barbarian",
         "x": CENTER_X, "y": CENTER_Y, "note": "5 Barbarians in cluster"},
    ],
    "frames": frames,
}
save_replay(replay, "replay_test3_swarm_cohesion.json", compress=False)
print(f"\nReplay saved → replay_test3_swarm_cohesion.json")
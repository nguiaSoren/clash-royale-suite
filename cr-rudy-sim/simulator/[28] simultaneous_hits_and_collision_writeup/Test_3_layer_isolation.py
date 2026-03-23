"""
TEST 3 — Layer Isolation (flying ignores ground)
==================================================
Writeup claim (§4):
  - Ground entities only collide with ground entities
  - Flying entities pass through ground entities and buildings
  - A Knight and a Mega Minion at the same (x,y) do NOT push each other

Setup:
  - Spawn 1 P1 Knight (ground) and 1 P1 Mega Minion (flying) at
    the exact same position (0, -5000)
  - Also spawn 2 P1 Knights at the same position as a control —
    these SHOULD push each other
  - Run 30 ticks
  - Measure: Knight+MegaMinion should stay overlapped (dist ≈ 0)
    while Knight+Knight should separate (dist > 0)

What to look for in the replay viewer:
  - The Knight and Mega Minion walk in the same direction, overlapping
  - The two control Knights visibly separate
  - This contrast proves layer isolation

Console output tracks distances for both pairs.
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

# ── Cross-layer pair: Knight + Mega Minion at same position ──
SPAWN_X, SPAWN_Y = -2000, -5000
knight_id = match.spawn_troop(1, "knight", SPAWN_X, SPAWN_Y, 11, False)
mm_id = match.spawn_troop(1, "megaminion", SPAWN_X, SPAWN_Y, 11, False)
print(f"Cross-layer pair (should NOT collide):")
print(f"  Knight (id={knight_id}, ground) at ({SPAWN_X}, {SPAWN_Y})")
print(f"  Mega Minion (id={mm_id}, flying) at ({SPAWN_X}, {SPAWN_Y})")

# ── Same-layer control: two Knights at same position ──
CTRL_X, CTRL_Y = 2000, -5000
ctrl_a = match.spawn_troop(1, "knight", CTRL_X, CTRL_Y, 11, False)
ctrl_b = match.spawn_troop(1, "knight", CTRL_X, CTRL_Y, 11, False)
print(f"\nSame-layer control (SHOULD collide):")
print(f"  Knight A (id={ctrl_a}, ground) at ({CTRL_X}, {CTRL_Y})")
print(f"  Knight B (id={ctrl_b}, ground) at ({CTRL_X}, {CTRL_Y})")


def dist_between(entities, id1, id2):
    e1 = next((e for e in entities if e["id"] == id1 and e["alive"]), None)
    e2 = next((e for e in entities if e["id"] == id2 and e["alive"]), None)
    if not e1 or not e2:
        return -1
    dx = e1["x"] - e2["x"]
    dy = e1["y"] - e2["y"]
    return math.isqrt(dx * dx + dy * dy)


# ── Run and record ──
TOTAL_TICKS = 40
frames = [_snapshot_tick(match)]

print(f"\n{'tick':>5s}  {'cross_dist':>10s}  {'ctrl_dist':>10s}  {'note'}")
print("-" * 50)

for tick in range(1, TOTAL_TICKS + 1):
    match.step()
    frames.append(_snapshot_tick(match))

    if tick % 5 == 0 or tick <= 3:
        entities = match.get_entities()
        cross_d = dist_between(entities, knight_id, mm_id)
        ctrl_d = dist_between(entities, ctrl_a, ctrl_b)

        note = ""
        if tick >= 15:
            if cross_d <= 200 and ctrl_d > 500:
                note = "← layer isolation working"

        print(f"{tick:5d}  {cross_d:10d}  {ctrl_d:10d}  {note}")


# ── Verdict ──
entities = match.get_entities()
final_cross = dist_between(entities, knight_id, mm_id)
final_ctrl = dist_between(entities, ctrl_a, ctrl_b)

print(f"\n{'=' * 50}")
print(f"RESULTS at tick {TOTAL_TICKS}:")
print(f"  Cross-layer (Knight + MegaMinion): dist = {final_cross}")
print(f"  Same-layer (Knight + Knight):      dist = {final_ctrl}")

# Cross-layer should have small distance (they just walk independently,
# may diverge slightly due to different speeds/targets but NOT from collision)
# Same-layer should have large distance (collision pushed them apart)
cross_ok = final_cross < 2000  # may diverge from different speed, but no collision push
ctrl_ok = final_ctrl > 500     # collision separation
contrast = final_ctrl > final_cross + 300  # control clearly more separated

print(f"\n  Cross-layer no collision push: {'✓' if cross_ok else '✗'} (dist={final_cross})")
print(f"  Same-layer collision works:    {'✓' if ctrl_ok else '✗'} (dist={final_ctrl})")
print(f"  Clear contrast:                {'✓' if contrast else '✗'} (ctrl {final_ctrl - final_cross} more separated)")
print(f"\n  Overall: {'PASS ✓' if cross_ok and ctrl_ok and contrast else 'FAIL ✗'}")


# ── Save replay ──
total = len(frames) - 1
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
         "x": SPAWN_X, "y": SPAWN_Y, "note": "ground — cross-layer test"},
        {"tick": 0, "player": 1, "action": "spawn", "card": "megaminion",
         "x": SPAWN_X, "y": SPAWN_Y, "note": "flying — cross-layer test"},
        {"tick": 0, "player": 1, "action": "spawn", "card": "knight",
         "x": CTRL_X, "y": CTRL_Y, "note": "ground control A"},
        {"tick": 0, "player": 1, "action": "spawn", "card": "knight",
         "x": CTRL_X, "y": CTRL_Y, "note": "ground control B"},
    ],
    "frames": frames,
}
save_replay(replay, "replay_test3_layer_isolation.json", compress=False)
print(f"\nReplay saved → replay_test3_layer_isolation.json")
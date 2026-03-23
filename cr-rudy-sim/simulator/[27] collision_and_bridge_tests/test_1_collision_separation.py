"""
TEST 1 — Collision Separation
=============================
Writeup claim: Two same-team troops spawned at the identical position
separate into side-by-side formation instead of permanently stacking.

Setup:
  - Spawn 2 P1 Knights at the exact same position (0, -5000)
  - No P2 troops (DoNothing agent won't interfere)
  - Run 80 ticks (~4 seconds)

What to look for in the replay viewer:
  - The two Knights should visually fan out laterally (side by side)
  - They should NOT walk as a single overlapping unit

Console output prints positions every 10 ticks so you can verify
dist > 0 even without the viewer.
"""

import sys, json, time, math
sys.path.insert(0, ".")

from python.data_loader import load_game_data
from python.replay_recorder import _snapshot_tick, save_replay

# ── Setup ──
data = load_game_data("data/")

# Dummy decks (we won't play from hand — we spawn directly)
import cr_engine
FILLER_DECK = ["knight", "archer", "giant", "musketeer",
               "valkyrie", "bomber", "prince", "babydragon"]

match = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)

# Give plenty of elixir so engine doesn't end weirdly
match.set_elixir(1, 10)
match.set_elixir(2, 10)

# ── Spawn two Knights at the EXACT same position ──
SPAWN_X, SPAWN_Y = 0, -5000
id_a = match.spawn_troop(1, "knight", SPAWN_X, SPAWN_Y, 11, False)
id_b = match.spawn_troop(1, "knight", SPAWN_X, SPAWN_Y, 11, False)
print(f"Spawned Knight A (id={id_a}) and Knight B (id={id_b}) at ({SPAWN_X}, {SPAWN_Y})")

# ── Run and record ──
TOTAL_TICKS = 80
frames = [_snapshot_tick(match)]
print(f"\n{'tick':>5s}  {'A_x':>7s} {'A_y':>7s}  {'B_x':>7s} {'B_y':>7s}  {'dist':>6s}")
print("-" * 52)

for tick in range(1, TOTAL_TICKS + 1):
    match.step()
    frames.append(_snapshot_tick(match))

    if tick % 10 == 0 or tick == 1:
        entities = match.get_entities()
        a = next((e for e in entities if e["id"] == id_a), None)
        b = next((e for e in entities if e["id"] == id_b), None)
        if a and b:
            dx = a["x"] - b["x"]
            dy = a["y"] - b["y"]
            dist = math.isqrt(dx * dx + dy * dy)
            print(f"{tick:5d}  {a['x']:7d} {a['y']:7d}  {b['x']:7d} {b['y']:7d}  {dist:6d}")

# ── Final verdict ──
entities = match.get_entities()
a = next((e for e in entities if e["id"] == id_a), None)
b = next((e for e in entities if e["id"] == id_b), None)
if a and b:
    dx = a["x"] - b["x"]
    dy = a["y"] - b["y"]
    final_dist = math.isqrt(dx * dx + dy * dy)
    print(f"\n{'PASS' if final_dist > 200 else 'FAIL'}: final distance = {final_dist} "
          f"(need > 200 for meaningful separation)")

# ── Save replay for viewer ──
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
        {"tick": 0, "player": 1, "action": "spawn", "card": "knight",
         "x": SPAWN_X, "y": SPAWN_Y, "note": "Knight A"},
        {"tick": 0, "player": 1, "action": "spawn", "card": "knight",
         "x": SPAWN_X, "y": SPAWN_Y, "note": "Knight B"},
    ],
    "frames": frames,
}
save_replay(replay, "replay_test1_collision.json", compress=False)
print(f"\nReplay saved → replay_test1_collision.json")
print("Open cr_replay_viewer and drag in the file to visualize.")
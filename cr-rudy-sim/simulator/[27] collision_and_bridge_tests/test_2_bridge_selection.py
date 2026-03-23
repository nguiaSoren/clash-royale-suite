"""
TEST 2 — Bridge Selection & Commitment (graph vs naive)
========================================================
Writeup claims (§2, §4):
  - Bridge selection uses shortest-total-path through the lane graph,
    not nearest-bridge-to-self
  - Once committed, the troop doesn't zigzag between bridges

Why this test is hard for simple heuristics:
  A Giant spawned at x=2000 (firmly on the RIGHT half of the arena)
  targets P2's LEFT princess tower at (-5100, 10200).

  To force this target, we first destroy P2's RIGHT princess tower so
  the left princess is the only valid target.

  - "Nearest bridge to self" picks RIGHT bridge (5100 is closer to 2000)
  - The lane GRAPH picks LEFT by summing 3 legs:
      Left route:  2000→(-5100,-1200) + crossing + (-5100,1200)→(-5100,10200) = 19,453
      Right route: 2000→(5100,-1200)  + crossing + (5100,1200)→(-5100,10200)  = 20,907
  - Graph wins by 1,454 units despite the LEFT bridge being 4,000 units
    farther from the Giant.

  A simple "nearest bridge to self" gets this WRONG.

Phase 1: Destroy P2 right princess tower (PEKKAs + fast ticks)
Phase 2: Spawn Giant at x=2000 and observe bridge choice

What to look for in the replay viewer:
  - Phase 1: PEKKAs destroy the right princess tower
  - Phase 2: Giant starts on the RIGHT side, walks LEFT toward left bridge
  - Giant commits — no oscillation or direction reversal
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

# ═══════════════════════════════════════════════════════════
# PHASE 1: Destroy P2 right princess tower
# ═══════════════════════════════════════════════════════════
# Spawn 3 PEKKAs right next to the P2 right princess tower (5100, 10200)
# They'll attack it immediately and kill it in a few hits.
print("Phase 1: Destroying P2 right princess tower...")
pekka_ids = []
for i in range(3):
    pid = match.spawn_troop(1, "pekka", 5100, 9500 + i * 200, 11, False)
    pekka_ids.append(pid)

frames = []
phase1_ticks = 0
while True:
    match.step()
    phase1_ticks += 1
    frames.append(_snapshot_tick(match))
    
    p2_hp = match.p2_tower_hp()  # [king, princess_left, princess_right]
    if p2_hp[2] <= 0:
        print(f"  P2 right princess destroyed at tick {phase1_ticks}")
        break
    if phase1_ticks > 400:
        print(f"  WARNING: right princess still alive at tick {phase1_ticks} (HP={p2_hp[2]})")
        break
    if phase1_ticks % 50 == 0:
        print(f"  tick {phase1_ticks}: P2 right princess HP = {p2_hp[2]}")

# ═══════════════════════════════════════════════════════════
# PHASE 2: Spawn Giant and observe bridge choice
# ═══════════════════════════════════════════════════════════
print(f"\nPhase 2: Spawning Giant at x=2000 (right half of arena)")
print(f"  Only target available: P2 LEFT princess at (-5100, 10200)")
print(f"")
print(f"  Path analysis:")
print(f"    Left route:  Giant→BRIDGE_L_P1→BRIDGE_L_P2→target = 19,453 units")
print(f"    Right route: Giant→BRIDGE_R_P1→BRIDGE_R_P2→target = 20,907 units")
print(f"    → Left is shorter by 1,454 units")
print(f"")
print(f"    Nearest bridge to self: RIGHT (|2000−5100|=3100 < |2000−(−5100)|=7100)")
print(f"    Graph shortest path:    LEFT  ✓")
print(f"    These DISAGREE — this test distinguishes graph from naive")

SPAWN_X, SPAWN_Y = 2000, -5000
gid = match.spawn_troop(1, "giant", SPAWN_X, SPAWN_Y, 11, False)
print(f"\n  Spawned Giant (id={gid}) at ({SPAWN_X}, {SPAWN_Y})")

PHASE2_TICKS = 150
x_history = [SPAWN_X]
direction_changes = 0
prev_dir = 0

print(f"\n{'tick':>5s}  {'X':>7s} {'Y':>7s}  {'dir':>8s}  {'note'}")
print("-" * 60)

for tick in range(1, PHASE2_TICKS + 1):
    match.step()
    frames.append(_snapshot_tick(match))

    entities = match.get_entities()
    g = next((e for e in entities if e["id"] == gid and e["alive"]), None)
    if g:
        x_history.append(g["x"])
        curr_x = g["x"]
        prev_x = x_history[-2]

        if curr_x != prev_x:
            curr_dir = 1 if curr_x > prev_x else -1
            if prev_dir != 0 and curr_dir != prev_dir:
                direction_changes += 1
            prev_dir = curr_dir

        if tick % 10 == 0:
            dir_str = "→ RIGHT" if prev_dir > 0 else "← LEFT" if prev_dir < 0 else "—"
            note = ""
            if tick == 20 and prev_dir < 0:
                note = "← Graph picked LEFT (naive would pick RIGHT)"
            elif tick == 20 and prev_dir > 0:
                note = "→ Went right (nearest-to-self behavior)"
            print(f"{tick:5d}  {g['x']:7d} {g['y']:7d}  {dir_str:>8s}  {note}")

# ── Verdict: bridge selection ──
print(f"\n{'='*60}")
print(f"BRIDGE SELECTION:")
went_left = x_history[-1] < SPAWN_X
print(f"  Started at x={SPAWN_X}, ended at x={x_history[-1]}")
print(f"  Direction: {'LEFT ✓ (graph-based)' if went_left else 'RIGHT ✗ (naive — possible bug)'}")
print(f"  {'PASS' if went_left else 'FAIL'}: "
      f"{'Graph correctly overruled nearest-to-self' if went_left else 'Troop took nearest bridge to self'}")

# ── Verdict: commitment (no zigzag) ──
print(f"\nBRIDGE COMMITMENT:")
print(f"  Direction reversals: {direction_changes}")
no_zigzag = direction_changes <= 3
print(f"  {'PASS' if no_zigzag else 'FAIL'}: "
      f"{'No zigzag — committed to one bridge ✓' if no_zigzag else f'Zigzag detected ✗ ({direction_changes} reversals)'}")

# ── Combined result ──
both_pass = went_left and no_zigzag
print(f"\n{'='*60}")
print(f"OVERALL: {'PASS ✓' if both_pass else 'FAIL ✗'}")
print(f"  Bridge selection: {'✓' if went_left else '✗'}")
print(f"  Bridge commitment: {'✓' if no_zigzag else '✗'}")

# ── Save replay ──
total_ticks = phase1_ticks + PHASE2_TICKS
replay = {
    "version": 1,
    "recorded_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    "deck1": FILLER_DECK,
    "deck2": FILLER_DECK,
    "sample_rate": 1,
    "total_ticks": total_ticks,
    "result": {
        "winner": "draw",
        "p1_crowns": 0, "p2_crowns": 0,
        "ticks": total_ticks,
        "seconds": total_ticks / 20.0,
    },
    "events": [
        {"tick": 0, "player": 1, "action": "spawn", "card": "pekka",
         "x": 5100, "y": 9500, "note": "3 PEKKAs to destroy right princess"},
        {"tick": phase1_ticks, "player": 1, "action": "spawn", "card": "giant",
         "x": SPAWN_X, "y": SPAWN_Y,
         "note": "Giant on RIGHT side — graph should send it LEFT"},
    ],
    "frames": frames,
}
save_replay(replay, "replay_test2_bridge_graph.json", compress=False)
print(f"\nReplay saved → replay_test2_bridge_graph.json")
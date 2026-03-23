"""
TEST 1 — Simultaneous Hit Resolution (melee + ranged)
======================================================
Writeup claims (§1, §2):
  - Two identical melee troops (Knights) facing each other at equal
    distance both deal damage on the exact same tick. No first-mover
    advantage from entity iteration order.
  - Two identical ranged troops (Musketeers) fire projectiles on the
    same tick and both take damage symmetrically.

Setup — Part A (melee):
  - Spawn P1 Knight at (-800, 0) and P2 Knight at (800, 0)
  - They walk toward each other, enter range on the same tick,
    start windup on the same tick, deal damage on the same tick
  - Track HP of both every tick around the first hit

Setup — Part B (ranged):
  - Spawn P1 Musketeer at (-4000, 0) and P2 Musketeer at (4000, 0)
  - Within range immediately (range=6000, dist=8000... need closer)
  - Actually: range 6000, so spawn at (-3000, 0) and (3000, 0) = dist 6000

What to look for in the replay viewer:
  - Part A: Both Knights' HP bars drop at the exact same moment
  - Part B: Both Musketeers' HP bars drop identically in lockstep
  - Both die on the same tick (mutual kill)

Console output is the definitive proof: tick-by-tick HP for both.
"""

import sys, json, time, math
sys.path.insert(0, ".")

from python.data_loader import load_game_data
from python.replay_recorder import _snapshot_tick, save_replay

import cr_engine

data = load_game_data("data/")

FILLER_DECK = ["knight", "archer", "giant", "musketeer",
               "valkyrie", "bomber", "prince", "babydragon"]

# ═══════════════════════════════════════════════════════════
# PART A: Melee trade (two Knights)
# ═══════════════════════════════════════════════════════════
print("=" * 60)
print("  PART A: Melee simultaneous trade (Knight vs Knight)")
print("=" * 60)

match_a = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)
match_a.set_elixir(1, 10)
match_a.set_elixir(2, 10)

# Spawn on opposite sides of the river, equidistant from center
# River is y=-1200 to y=1200. Place them in their own territory.
ka = match_a.spawn_troop(1, "knight", 0, -2000, 11, False)
kb = match_a.spawn_troop(2, "knight", 0, 2000, 11, False)
print(f"  P1 Knight (id={ka}) at (0, -2000)")
print(f"  P2 Knight (id={kb}) at (0, 2000)")
print(f"  Distance: 4000 (they'll walk toward each other and meet at the bridge)")

TICKS_A = 200
frames_a = [_snapshot_tick(match_a)]

first_hit_a = None
first_hit_b = None
prev_hp_a = None
prev_hp_b = None

print(f"\n{'tick':>5s}  {'A_HP':>6s} {'B_HP':>6s}  {'A_phase':>12s} {'B_phase':>12s}  {'event'}")
print("-" * 70)

for tick in range(1, TICKS_A + 1):
    match_a.step()
    frames_a.append(_snapshot_tick(match_a))

    entities = match_a.get_entities()
    a = next((e for e in entities if e["id"] == ka), None)
    b = next((e for e in entities if e["id"] == kb), None)

    if not a or not b:
        continue

    hp_a = a["hp"] if a["alive"] else 0
    hp_b = b["hp"] if b["alive"] else 0
    phase_a = a.get("attack_phase", "idle")
    phase_b = b.get("attack_phase", "idle")

    event = ""
    if prev_hp_a is not None:
        if hp_a < prev_hp_a and first_hit_a is None:
            first_hit_a = tick
            event += f"A takes {prev_hp_a - hp_a} dmg! "
        if hp_b < prev_hp_b and first_hit_b is None:
            first_hit_b = tick
            event += f"B takes {prev_hp_b - hp_b} dmg! "
        if hp_a < prev_hp_a and first_hit_a == tick - 0:  # already set
            pass
        if hp_a < prev_hp_a and first_hit_a != tick:
            event += f"A: -{prev_hp_a - hp_a} "
        if hp_b < prev_hp_b and first_hit_b != tick:
            event += f"B: -{prev_hp_b - hp_b} "

    # Print every tick around combat, or every 5 ticks otherwise
    is_combat = phase_a != "idle" or phase_b != "idle" or event
    if is_combat or tick % 10 == 0 or tick <= 2:
        print(f"{tick:5d}  {hp_a:6d} {hp_b:6d}  {phase_a:>12s} {phase_b:>12s}  {event}")

    prev_hp_a = hp_a
    prev_hp_b = hp_b

    if not a["alive"] and not b["alive"]:
        print(f"\n  Both dead at tick {tick}")
        break

print(f"\n  First hit on A: tick {first_hit_a}")
print(f"  First hit on B: tick {first_hit_b}")
if first_hit_a and first_hit_b:
    diff = abs(first_hit_a - first_hit_b)
    print(f"  Tick difference: {diff}")
    print(f"  {'PASS' if diff == 0 else 'FAIL'}: "
          f"{'Both hit on same tick ✓' if diff == 0 else f'Hit {diff} ticks apart ✗'}")


# ═══════════════════════════════════════════════════════════
# PART B: Ranged trade (two Musketeers)
# ═══════════════════════════════════════════════════════════
print(f"\n{'=' * 60}")
print("  PART B: Ranged simultaneous trade (Musketeer vs Musketeer)")
print("=" * 60)

match_b = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)
match_b.set_elixir(1, 10)
match_b.set_elixir(2, 10)

# Musketeers have range=6000. Spawn just within range.
ma = match_b.spawn_troop(1, "musketeer", -2500, 0, 11, False)
mb = match_b.spawn_troop(2, "musketeer", 2500, 0, 11, False)
print(f"  P1 Musketeer (id={ma}) at (-2500, 0)")
print(f"  P2 Musketeer (id={mb}) at (2500, 0)")
print(f"  Distance: 5000 (range: 6000 — within range immediately)")

TICKS_B = 150
frames_b = [_snapshot_tick(match_b)]

first_hit_ma = None
first_hit_mb = None
prev_hp_ma = None
prev_hp_mb = None

print(f"\n{'tick':>5s}  {'A_HP':>6s} {'B_HP':>6s}  {'A_phase':>12s} {'B_phase':>12s}  {'event'}")
print("-" * 70)

for tick in range(1, TICKS_B + 1):
    match_b.step()
    frames_b.append(_snapshot_tick(match_b))

    entities = match_b.get_entities()
    a = next((e for e in entities if e["id"] == ma), None)
    b = next((e for e in entities if e["id"] == mb), None)

    if not a or not b:
        continue

    hp_a = a["hp"] if a["alive"] else 0
    hp_b = b["hp"] if b["alive"] else 0
    phase_a = a.get("attack_phase", "idle")
    phase_b = b.get("attack_phase", "idle")

    event = ""
    if prev_hp_ma is not None:
        if hp_a < prev_hp_ma:
            if first_hit_ma is None:
                first_hit_ma = tick
            event += f"A: -{prev_hp_ma - hp_a} "
        if hp_b < prev_hp_mb:
            if first_hit_mb is None:
                first_hit_mb = tick
            event += f"B: -{prev_hp_mb - hp_b} "

    is_combat = phase_a != "idle" or phase_b != "idle" or event
    if is_combat or tick % 10 == 0 or tick <= 2:
        print(f"{tick:5d}  {hp_a:6d} {hp_b:6d}  {phase_a:>12s} {phase_b:>12s}  {event}")

    prev_hp_ma = hp_a
    prev_hp_mb = hp_b

    if not a["alive"] and not b["alive"]:
        print(f"\n  Both dead at tick {tick}")
        break

print(f"\n  First hit on A: tick {first_hit_ma}")
print(f"  First hit on B: tick {first_hit_mb}")
if first_hit_ma and first_hit_mb:
    diff = abs(first_hit_ma - first_hit_mb)
    print(f"  Tick difference: {diff}")
    print(f"  {'PASS' if diff == 0 else 'FAIL'}: "
          f"{'Both hit on same tick ✓' if diff == 0 else f'Hit {diff} ticks apart ✗'}")


# ═══════════════════════════════════════════════════════════
# COMBINED VERDICT
# ═══════════════════════════════════════════════════════════
print(f"\n{'=' * 60}")
print("  COMBINED VERDICT")
print("=" * 60)
melee_pass = first_hit_a is not None and first_hit_b is not None and first_hit_a == first_hit_b
ranged_pass = first_hit_ma is not None and first_hit_mb is not None and first_hit_ma == first_hit_mb
print(f"  Melee trade:  {'PASS ✓' if melee_pass else 'FAIL ✗'}")
print(f"  Ranged trade: {'PASS ✓' if ranged_pass else 'FAIL ✗'}")
print(f"  Overall:      {'PASS ✓' if melee_pass and ranged_pass else 'FAIL ✗'}")


# ═══════════════════════════════════════════════════════════
# SAVE REPLAY (melee portion — most visually compelling)
# ═══════════════════════════════════════════════════════════
total_a = len(frames_a) - 1
replay = {
    "version": 1,
    "recorded_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    "deck1": FILLER_DECK,
    "deck2": FILLER_DECK,
    "sample_rate": 1,
    "total_ticks": total_a,
    "result": {
        "winner": "draw",
        "p1_crowns": 0, "p2_crowns": 0,
        "ticks": total_a,
        "seconds": total_a / 20.0,
    },
    "events": [
        {"tick": 0, "player": 1, "action": "spawn", "card": "knight",
         "x": 0, "y": -2000, "note": "P1 Knight"},
        {"tick": 0, "player": 2, "action": "spawn", "card": "knight",
         "x": 0, "y": 2000, "note": "P2 Knight"},
    ],
    "frames": frames_a,
}
save_replay(replay, "replay_test1_simultaneous_hit.json", compress=False)
print(f"\nReplay saved → replay_test1_simultaneous_hit.json")
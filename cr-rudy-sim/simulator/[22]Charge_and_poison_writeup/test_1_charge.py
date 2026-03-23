"""
TEST 1 — Charge Mechanics (Prince)
====================================
Writeup claims (§1):
  - Speed jumps from 28 to 57 u/tick after walking 300 distance units
  - Charge hit deals exactly 2× damage (627 → 1254 at level 11)
  - Charge activates with no enemies on the map (distance-based)
  - Step function, not gradual ramp

Setup — Part A (charge activation, no enemies):
  - Spawn P1 Prince at (-3000, -8000), heading toward bridge
  - NO enemies on the map
  - Track speed (displacement per tick) every tick
  - Confirm speed jumps from ~28 to ~57 after ~11 ticks of movement

Setup — Part B (charge damage):
  - Spawn P1 Prince far from P2 Golem (8192 HP target)
  - Prince walks, charges, then hits the Golem
  - Track Golem HP — first hit should be 1254 (2× 627)

What to look for in the replay viewer:
  - Part A: Prince visibly accelerates after a short walk
  - Part B: Prince slams into the Golem at high speed, big HP drop
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
# PART A: Charge activation with no enemies
# ═══════════════════════════════════════════════════════════
print("=" * 60)
print("  PART A: Charge activation (no enemies)")
print("=" * 60)

match_a = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)
match_a.set_elixir(1, 10)
match_a.set_elixir(2, 10)

# Prince in P1 territory heading north toward bridge
SPAWN_X, SPAWN_Y = 0, -8000
pid = match_a.spawn_troop(1, "prince", SPAWN_X, SPAWN_Y, 11, False)
print(f"  Spawned Prince (id={pid}) at ({SPAWN_X}, {SPAWN_Y})")
print(f"  No enemies — charge should still activate via distance walked")

TICKS_A = 60
frames_a = [_snapshot_tick(match_a)]

prev_x, prev_y = SPAWN_X, SPAWN_Y
charge_tick = None
cumulative_dist = 0

print(f"\n{'tick':>5s}  {'X':>7s} {'Y':>7s}  {'speed':>6s}  {'cum_dist':>8s}  {'note'}")
print("-" * 60)

for tick in range(1, TICKS_A + 1):
    match_a.step()
    frames_a.append(_snapshot_tick(match_a))

    entities = match_a.get_entities()
    p = next((e for e in entities if e["id"] == pid and e["alive"]), None)
    if not p:
        continue

    dx = p["x"] - prev_x
    dy = p["y"] - prev_y
    speed = math.isqrt(dx * dx + dy * dy)
    cumulative_dist += speed

    note = ""
    if speed > 40 and charge_tick is None:
        charge_tick = tick
        note = "← CHARGE ACTIVATED (speed > 40)"
    elif speed > 0 and speed <= 40 and charge_tick is None:
        note = f"base speed"

    if tick <= 35 or note:
        print(f"{tick:5d}  {p['x']:7d} {p['y']:7d}  {speed:6d}  {cumulative_dist:8d}  {note}")

    prev_x, prev_y = p["x"], p["y"]

print(f"\n  Charge activated at tick: {charge_tick}")
print(f"  Cumulative distance at charge: ~{cumulative_dist}")
if charge_tick:
    print(f"  PASS: Charge activates via distance alone (no enemies needed) ✓")
else:
    print(f"  FAIL: Charge never activated ✗")


# ═══════════════════════════════════════════════════════════
# PART B: Charge damage (2× normal)
# ═══════════════════════════════════════════════════════════
print(f"\n{'=' * 60}")
print("  PART B: Charge damage (Prince vs Golem)")
print("=" * 60)

match_b = cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)
match_b.set_elixir(1, 10)
match_b.set_elixir(2, 10)

# Prince far south, Golem far north — Prince needs to walk enough to charge
# before reaching the Golem
prince_id = match_b.spawn_troop(1, "prince", 0, -8000, 11, False)
# Golem as a big HP target — placed on P2 side near bridge
# Golem is building-targeting so it won't attack the Prince
golem_id = match_b.spawn_troop(2, "golem", 0, -3000, 11, False)

print(f"  Prince (id={prince_id}) at (0, -8000)")
print(f"  Golem (id={golem_id}) at (0, -3000) — 8192 HP target")
print(f"  Distance: 5000 units. Prince needs 300 to charge → will be charged on impact.")
print(f"  Expected charge damage: 627 × 2 = 1254")

TICKS_B = 250
frames_b = [_snapshot_tick(match_b)]

first_hit_tick = None
first_hit_dmg = None
prev_golem_hp = 8192

print(f"\n{'tick':>5s}  {'golem_HP':>9s}  {'event'}")
print("-" * 40)

for tick in range(1, TICKS_B + 1):
    match_b.step()
    frames_b.append(_snapshot_tick(match_b))

    entities = match_b.get_entities()
    g = next((e for e in entities if e["id"] == golem_id), None)
    if not g:
        continue

    golem_hp = g["hp"] if g["alive"] else 0

    if golem_hp < prev_golem_hp and first_hit_tick is None:
        first_hit_tick = tick
        first_hit_dmg = prev_golem_hp - golem_hp
        print(f"{tick:5d}  {golem_hp:9d}  ★ FIRST HIT: −{first_hit_dmg} HP")
    elif golem_hp < prev_golem_hp:
        dmg = prev_golem_hp - golem_hp
        print(f"{tick:5d}  {golem_hp:9d}  hit: −{dmg} HP")

    if tick % 20 == 0 and golem_hp == prev_golem_hp:
        print(f"{tick:5d}  {golem_hp:9d}")

    prev_golem_hp = golem_hp

print(f"\n  First hit at tick: {first_hit_tick}")
print(f"  First hit damage: {first_hit_dmg}")
if first_hit_dmg:
    is_charge = first_hit_dmg > 1000  # charge damage should be ~1254
    expected_charge = 1254
    print(f"  Expected charge damage: {expected_charge}")
    print(f"  {'PASS' if first_hit_dmg == expected_charge else 'CLOSE' if abs(first_hit_dmg - expected_charge) < 50 else 'FAIL'}: "
          f"first hit = {first_hit_dmg} {'(exact 2× ✓)' if first_hit_dmg == expected_charge else ''}")


# ═══════════════════════════════════════════════════════════
# COMBINED VERDICT
# ═══════════════════════════════════════════════════════════
print(f"\n{'=' * 60}")
print("  COMBINED VERDICT")
print("=" * 60)
charge_activation_pass = charge_tick is not None
charge_dmg_pass = first_hit_dmg is not None and first_hit_dmg >= 1200
print(f"  Charge activation (no enemies): {'PASS ✓' if charge_activation_pass else 'FAIL ✗'}")
print(f"  Charge damage (2× normal):      {'PASS ✓' if charge_dmg_pass else 'FAIL ✗'}")
print(f"  Overall: {'PASS ✓' if charge_activation_pass and charge_dmg_pass else 'FAIL ✗'}")


# ═══════════════════════════════════════════════════════════
# SAVE REPLAYS (both parts)
# ═══════════════════════════════════════════════════════════

# Part A: Prince charging in empty lane (no enemies)
total_a = len(frames_a) - 1
replay_a = {
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
        {"tick": 0, "player": 1, "action": "spawn", "card": "prince",
         "x": SPAWN_X, "y": SPAWN_Y, "note": "Prince alone — charge via distance only"},
    ],
    "frames": frames_a,
}
save_replay(replay_a, "replay_test1a_charge_activation.json", compress=False)
print(f"Replay saved → replay_test1a_charge_activation.json")

# Part B: Prince charges into Golem (2× damage)
total_b = len(frames_b) - 1
replay_b = {
    "version": 1,
    "recorded_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    "deck1": FILLER_DECK,
    "deck2": FILLER_DECK,
    "sample_rate": 1,
    "total_ticks": total_b,
    "result": {
        "winner": "draw",
        "p1_crowns": 0, "p2_crowns": 0,
        "ticks": total_b,
        "seconds": total_b / 20.0,
    },
    "events": [
        {"tick": 0, "player": 1, "action": "spawn", "card": "prince",
         "x": 0, "y": -8000, "note": "Prince — will charge before reaching Golem"},
        {"tick": 0, "player": 2, "action": "spawn", "card": "golem",
         "x": 0, "y": -3000, "note": "Golem — 8192 HP target"},
    ],
    "frames": frames_b,
}
save_replay(replay_b, "replay_test1b_charge_damage.json", compress=False)
print(f"Replay saved → replay_test1b_charge_damage.json")
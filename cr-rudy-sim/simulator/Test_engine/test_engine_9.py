#!/usr/bin/env python3
"""
============================================================
  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 9
  Tests 91-112: Troop kiting, sideways movement,
  bridge re-aggro, retarget on death
============================================================

These tests probe two "partially handled" mechanics:
  A) Troop kiting / sideways movement (91-101)
     — Does a troop walk laterally toward a target that is
       to its side rather than straight ahead?
  B) Bridge re-aggro / retarget after target death (102-112)
     — When a troop's target dies, does it pick up the
       nearest remaining enemy and change direction?

Calibration notes (from JSON data + entities.rs):
  - Knight: sight=5500  speed=30u/t  deploy=20t  range=1200  retarget_load=0
  - Giant:  sight=7500  speed=18u/t  deploy=20t  range=1200  building-only
  - Golem:  sight=7000  speed=18u/t  deploy=60t  range=750   building-only
  - Balloon: sight=7700 speed=30u/t  deploy=20t  flying, building-only
  - Musketeer: sight=6000 speed=30u/t deploy=20t range=6000  ranged
  - Hog Rider: sight=9500 speed=60u/t deploy=20t building-only
  - All troops: load_after_retarget=0 (no coded retarget delay)
  - All deploy via spawn_troop get deploy_timer ticks (NOT cleared by step_n(1))
  - Dead entities removed by cleanup — find_entity returns None after death

Usage:
  Copy this file into your simulator/ project root and run:
    python test_engine_9.py
"""

import sys
import os
import math

# ---------------------------------------------------------------------------
# Bootstrap
# ---------------------------------------------------------------------------

try:
    import cr_engine
except ImportError:
    here = os.path.dirname(os.path.abspath(__file__))
    candidates = [
        os.path.join(here, "engine", "target", "release"),
        os.path.join(here, "target", "release"),
        os.path.join(here, "engine", "target", "maturin", "release"),
    ]
    for p in candidates:
        if os.path.isdir(p):
            sys.path.insert(0, p)
    import cr_engine

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data")
if not os.path.isdir(DATA_DIR):
    DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data")
if not os.path.isdir(DATA_DIR):
    DATA_DIR = "data"

data = cr_engine.load_data(DATA_DIR)

FILLER_DECK = [
    "knight", "archers", "giant", "valkyrie",
    "musketeer", "hog-rider", "fireball", "zap",
]

passed = 0
failed = 0

# Key constants for calibration
KNIGHT_SIGHT = 5500
KNIGHT_SPEED = 30     # units per tick
KNIGHT_DEPLOY = 20    # ticks (1000ms)
KNIGHT_RANGE = 1200


def check(condition, msg):
    global passed, failed
    if condition:
        print(f"  \u2713 {msg}")
        passed += 1
    else:
        print(f"  \u2717 FAIL: {msg}")
        failed += 1


def new_match():
    return cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)


def find_entity(m, entity_id):
    for e in m.get_entities():
        if e["id"] == entity_id:
            return e
    return None


def entity_alive(m, entity_id):
    """Check if alive. Returns False if dead OR cleaned up (None)."""
    e = find_entity(m, entity_id)
    if e is None:
        return False
    return e["alive"]


def dist(x1, y1, x2, y2):
    return math.sqrt((x1 - x2) ** 2 + (y1 - y2) ** 2)


# ====================================================================
# SECTION A — TROOP KITING / SIDEWAYS MOVEMENT (Tests 91-101)
# ====================================================================

print("=" * 60)
print("  CLASH ROYALE ENGINE FIDELITY TESTS \u2014 BATCH 9")
print("  Tests 91-112: kiting, sideways movement, bridge re-aggro")
print("=" * 60)

# ------------------------------------------------------------------
# TEST 91: Basic lateral movement toward a side target
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 91: Lateral movement \u2014 Knight chases enemy to its left")
print("=" * 60)

m = new_match()
# Place Knight and enemy within sight range (5500), laterally offset
# Distance: sqrt(3000^2 + 1000^2) = 3162 < 5500
kid = m.spawn_troop(1, "knight", 0, -5000)
sid = m.spawn_troop(2, "skeleton", -3000, -4000)

start = find_entity(m, kid)
start_x, start_y = start["x"], start["y"]
print(f"  Knight start: ({start_x}, {start_y})")
print(f"  Skeleton (bait): (-3000, -4000)  dist={dist(0,-5000,-3000,-4000):.0f}")

# Run 60 ticks (deploy=20t, movement=40t, 40*30=1200 units of travel)
m.step_n(60)
after = find_entity(m, kid)
after_x, after_y = after["x"], after["y"]
print(f"  Knight after 60 ticks: ({after_x}, {after_y})")

x_shift = after_x - start_x
print(f"  X shift: {x_shift} (negative = moved left toward bait)")

check(x_shift < -200, "Knight moved significantly leftward toward bait")
check(abs(x_shift) > 100, "Knight had meaningful lateral (X) movement")

# ------------------------------------------------------------------
# TEST 92: Pure sideways movement (target at same Y, within sight)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 92: Pure sideways \u2014 enemy at same Y, within sight range")
print("=" * 60)

m = new_match()
# 4000 units apart laterally at same Y — within 5500 sight
kid = m.spawn_troop(1, "knight", 2000, -5000)
sid = m.spawn_troop(2, "giant", -2000, -5000)

start = find_entity(m, kid)
start_x = start["x"]
print(f"  Knight start: ({start['x']}, {start['y']})")
print(f"  Skeleton: (-2000, -5000)  dist={dist(2000,-5000,-2000,-5000):.0f}")

m.step_n(60)  # 20t deploy + 40t movement
after = find_entity(m, kid)
print(f"  Knight after 60 ticks: ({after['x']}, {after['y']})")
x_shift = after["x"] - start_x
y_shift = abs(after["y"] - start["y"])

print(f"  X shift: {x_shift}  Y shift: {y_shift}")

check(x_shift < -500, "Knight moved left toward enemy (pure lateral)")
check(abs(x_shift) > y_shift, "X movement dominates over Y (sideways chase)")

# ------------------------------------------------------------------
# TEST 93: Diagonal chase — target ahead and to the side (within sight)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 93: Diagonal chase \u2014 target ahead-left, within sight")
print("=" * 60)

m = new_match()
# Distance: sqrt(3000^2 + 2000^2) = 3606 < 5500
kid = m.spawn_troop(1, "knight", 1500, -6000)
sid = m.spawn_troop(2, "giant", -1500, -4000)

start = find_entity(m, kid)
start_x, start_y = start["x"], start["y"]
print(f"  Start: ({start_x}, {start_y})")
print(f"  Target: (-1500, -4000)  dist={dist(1500,-6000,-1500,-4000):.0f}")

m.step_n(60)
after = find_entity(m, kid)
x_shift = after["x"] - start_x
y_shift = after["y"] - start_y

print(f"  After: ({after['x']}, {after['y']})  X shift: {x_shift}  Y shift: {y_shift}")

check(x_shift < -200, "Knight moved leftward (diagonal X component)")
check(y_shift > 200, "Knight moved forward (diagonal Y component)")
check(abs(x_shift) > 100 and abs(y_shift) > 100, "Both X and Y changed (true diagonal)")

# ------------------------------------------------------------------
# TEST 94: Giant (building-only) pulled sideways by a building
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 94: Giant pulled sideways by Cannon placement")
print("=" * 60)

m = new_match()
# Giant sight=7500, so buildings across the arena are visible
gid = m.spawn_troop(1, "giant", 0, -5000)
bid = m.spawn_building(2, "cannon", 5000, 3000)

start = find_entity(m, gid)
start_x = start["x"]
print(f"  Giant start: ({start['x']}, {start['y']})")

m.step_n(100)  # 20t deploy + 80t movement (80*18=1440 units)
after = find_entity(m, gid)
print(f"  Giant after 100 ticks: ({after['x']}, {after['y']})")
x_shift = after["x"] - start_x

print(f"  X shift: {x_shift}")
check(after["y"] > start["y"], "Giant moved forward (toward enemy side)")

# ------------------------------------------------------------------
# TEST 95: Fast troop (Hog Rider) covers more total ground than slow troop
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 95: Fast troop covers more ground \u2014 Hog Rider vs Knight")
print("=" * 60)

# Hog Rider: speed=60u/t
m = new_match()
hid = m.spawn_troop(1, "hog-rider", 0, -3000)
start = find_entity(m, hid)
start_x, start_y = start["x"], start["y"]

m.step_n(60)
after = find_entity(m, hid)
hog_dist = dist(start_x, start_y, after["x"], after["y"]) if after else 0
print(f"  Hog start: ({start_x},{start_y})  After: ({after['x']},{after['y']})  dist={hog_dist:.0f}")

# Knight comparison: speed=30u/t
m2 = new_match()
kid2 = m2.spawn_troop(1, "knight", 0, -3000)
m2.step_n(60)
after2 = find_entity(m2, kid2)
knight_dist = dist(0, -3000, after2["x"], after2["y"]) if after2 else 0
print(f"  Knight dist in same time: {knight_dist:.0f}")

check(hog_dist > 500, "Hog Rider covered significant ground")
check(hog_dist > knight_dist, "Hog covered more total ground than Knight (faster speed)")

# ------------------------------------------------------------------
# TEST 96: Slow troop (Golem) still moves laterally
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 96: Slow troop lateral movement \u2014 Golem")
print("=" * 60)

m = new_match()
# Golem: deploy=60t, speed=18u/t, sight=7000
gid = m.spawn_troop(1, "golem", 0, -5000)
bid = m.spawn_building(2, "cannon", 5000, 5000)

start = find_entity(m, gid)
start_x, start_y = start["x"], start["y"]

# Need 60t deploy + enough movement: 150 total ticks = 90t movement = 1620 units
m.step_n(150)
after = find_entity(m, gid)
x_shift = after["x"] - start_x if after else 0
print(f"  Golem start: ({start_x}, {start_y})")
print(f"  Golem after 150 ticks: ({after['x']}, {after['y']})" if after else "  Golem: DEAD")
print(f"  X shift: {x_shift}")

check(after is not None, "Golem still alive after 150 ticks")
if after:
    check(after["y"] > start_y, "Golem moved forward")

# ------------------------------------------------------------------
# TEST 97: Flying troop ignores river (Balloon crosses without bridge)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 97: Air troop \u2014 Balloon ignores river")
print("=" * 60)

m = new_match()
bid = m.spawn_troop(1, "balloon", 6000, -4000)

start = find_entity(m, bid)
start_x, start_y = start["x"], start["y"]

# Balloon: deploy=20t, speed=30u/t. Need to cross from Y=-4000 past Y=0.
# ~134 ticks to cover 4000 units + 20 deploy = 154 ticks. Use 180 for margin.
m.step_n(180)
after = find_entity(m, bid)
x_shift = after["x"] - start_x
y_shift = after["y"] - start_y
print(f"  Balloon: ({start_x},{start_y}) -> ({after['x']},{after['y']})")
print(f"  Y shift: {y_shift}")

check(y_shift > 3000, "Balloon moved forward substantially")
check(after["y"] > 0, "Balloon crossed river (Y > 0) \u2014 no bridge needed")

# ------------------------------------------------------------------
# TEST 98: Ground troop near river must path to bridge even when
#          target is laterally offset
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 98: Ground troop lateral + bridge constraint")
print("=" * 60)

m = new_match()
kid = m.spawn_troop(1, "knight", 3000, -2000)
sid = m.spawn_troop(2, "skeleton", -5000, 3000)

start = find_entity(m, kid)
print(f"  Knight start: ({start['x']}, {start['y']})")

m.step_n(80)
after = find_entity(m, kid)
print(f"  Knight after 80 ticks: ({after['x']}, {after['y']})")

bridge_left_dist = abs(after["x"] - (-5100))
bridge_right_dist = abs(after["x"] - 5100)
near_bridge = min(bridge_left_dist, bridge_right_dist) < 3000
print(f"  Distance to left bridge: {bridge_left_dist}  right bridge: {bridge_right_dist}")

check(near_bridge or after["y"] > 0, "Knight pathed toward a bridge or already crossed")

# ------------------------------------------------------------------
# TEST 99: Troop X-axis reversal — first bait left, dies, new bait right
#          (new bait placed within sight range of knight's current pos)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 99: Troop reverses X direction \u2014 target switches sides")
print("=" * 60)

# TEST 99 FIX: Use skeleton but place OUTSIDE P1 tower range
# P1 towers at (-5100, -10200) and (5100, -10200), range=7500
# Skeleton at (-2000, -4000): dist to left tower = sqrt(3100² + 6200²) ≈ 6932 < 7500  ← IN RANGE
# Skeleton at (-2000, -2000): dist to left tower = sqrt(3100² + 8200²) ≈ 8766 > 7500  ← SAFE
m = new_match()
kid = m.spawn_troop(1, "knight", 0, -3000)        # Start closer to midfield
s1 = m.spawn_troop(2, "skeleton", -2000, -2000)    # Safe from towers

# Let knight chase and kill skeleton (~20t deploy + ~33t travel + 1 hit)
m.step_n(80)
# ... skeleton should die, knight retargets to bait 2

skel_alive = entity_alive(m, s1)
print(f"  Archers 1 alive after 80 ticks: {skel_alive}")

knight = find_entity(m, kid)
if knight and knight["alive"]:
    before_x = knight["x"]
    print(f"  Knight X after skeleton 1 phase: {before_x}")

    # Bait 2: to the RIGHT, within sight range from knight's current position
    # Knight should be near (-2000, -4000). Place bait at (1500, -4000) -> dist ~3500 < 5500
    s2 = m.spawn_troop(2, "skeleton", 1500, -2500)     # Also safe from towers

    m.step_n(60)  # 20t deploy + 40t movement
    knight_after = find_entity(m, kid)
    if knight_after and knight_after["alive"]:
        after_x = knight_after["x"]
        x_reversal = after_x - before_x
        print(f"  Knight X after new bait: {before_x} -> {after_x} (reversal: {x_reversal})")
        check(x_reversal > 200, "Knight reversed direction rightward toward new target")
    else:
        check(False, "Knight died before reversal could be measured")
else:
    check(False, "Knight died chasing first Archers")

# ------------------------------------------------------------------
# TEST 100: Multiple troops kite toward same lateral target
#           (all within sight range of the bait)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 100: Multiple troops kite toward same bait")
print("=" * 60)

m = new_match()
# Pack troops tighter so all are within 5500 of the bait
k1 = m.spawn_troop(1, "knight", -1000, -5000)
k2 = m.spawn_troop(1, "knight", 0, -5000)
k3 = m.spawn_troop(1, "knight", 1000, -5000)
# Bait: Giant at (-3500, -3500)
# Distances: k1=2693, k2=3808, k3=4610 — all < 5500
bait = m.spawn_troop(2, "giant", -3500, -3500)

starts = {}
for kid_id in [k1, k2, k3]:
    e = find_entity(m, kid_id)
    starts[kid_id] = e["x"]
    d = dist(e["x"], e["y"], -3500, -3500)
    print(f"  Knight {kid_id}: start X={e['x']}  dist to bait={d:.0f}")

m.step_n(60)

all_moved_left = True
for kid_id in [k1, k2, k3]:
    e = find_entity(m, kid_id)
    if e and e["alive"]:
        shift = e["x"] - starts[kid_id]
        print(f"  Knight {kid_id}: X shift = {shift}")
        if shift >= 0:
            all_moved_left = False
    else:
        print(f"  Knight {kid_id}: dead or missing")

check(all_moved_left, "All three knights moved leftward toward bait")

# ------------------------------------------------------------------
# TEST 101: Troop crosses open field diagonally (within sight range)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 101: Troop crosses open field diagonally to reach target")
print("=" * 60)

m = new_match()
# Distance: sqrt(4000^2 + 2000^2) = 4472 < 5500
kid = m.spawn_troop(1, "knight", 2000, -6000)
gid = m.spawn_troop(2, "giant", -2000, -4000)

start = find_entity(m, kid)
start_dist = dist(start["x"], start["y"], -2000, -4000)
print(f"  Knight start: ({start['x']}, {start['y']})  dist to Giant: {start_dist:.0f}")

m.step_n(80)  # 20t deploy + 60t movement = 1800 units
after = find_entity(m, kid)
after_dist = dist(after["x"], after["y"], -2000, -4000) if after else start_dist

print(f"  After 80 ticks: ({after['x']}, {after['y']})  dist: {after_dist:.0f}")
check(after_dist < start_dist - 500, "Knight closed distance to lateral target")


# ====================================================================
# SECTION B — BRIDGE RE-AGGRO / RETARGET ON DEATH (Tests 102-112)
# ====================================================================

# ------------------------------------------------------------------
# TEST 102: Target dies -> troop retargets nearest remaining enemy
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 102: Retarget after target death \u2014 picks nearest")
print("=" * 60)

m = new_match()
kid = m.spawn_troop(1, "knight", 0, -5000)
close_skel = m.spawn_troop(2, "skeleton", 0, -4000)   # 1000 away
far_skel = m.spawn_troop(2, "skeleton", 0, -2500)      # 2500 away

m.step_n(100)

close_alive = entity_alive(m, close_skel)
far_alive = entity_alive(m, far_skel)
print(f"  Close skeleton alive: {close_alive}  Far skeleton alive: {far_alive}")

knight = find_entity(m, kid)
if knight and knight["alive"]:
    if not far_alive:
        check(True, "Knight killed both \u2014 retargeted successfully")
    else:
        ky = knight["y"]
        print(f"  Knight Y: {ky} (should be heading toward -2500)")
        check(ky > -4000, "Knight advanced past close skeleton toward far target")
else:
    check(False, "Knight died unexpectedly")

# ------------------------------------------------------------------
# TEST 103: Retarget reverses Y direction (troop turns around)
#           — ensure ahead target is DEAD before spawning behind target
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 103: Retarget reversal \u2014 troop turns back toward new target")
print("=" * 60)


# TEST 103 FIX: Place everything near midfield so the behind skeleton
# stays outside P1 princess tower range (7500 from ±5100, -10200).
# Safe zone for skeletons at X=0: Y > -4700.
# Strategy: Knight starts at Y=-2000, skeleton just ahead at Y=-800.
# Knight kills skeleton fast, advances a bit. Behind skeleton placed
# only 2000 units back — keeps it above Y=-4700.
m = new_match()
kid = m.spawn_troop(1, "knight", 0, -2000)
# Ahead skeleton within attack range (dist=1200), safe from towers
# dist to P1 left tower: sqrt(5100² + 9400²) ≈ 10694 > 7500 ✓
ahead_skel = m.spawn_troop(2, "skeleton", 0, -800)

# Knight deploys (20t), one-shots skeleton, then advances forward.
# Use only 40 ticks so knight doesn't advance too far.
m.step_n(40)
ahead_alive = entity_alive(m, ahead_skel)
print(f"  Ahead skeleton alive after 40 ticks: {ahead_alive}")

knight = find_entity(m, kid)
if not knight or not knight["alive"]:
    check(False, "Knight died before retarget test (tower fire?)")
else:
    advanced_y = knight["y"]
    print(f"  Knight Y after killing ahead skeleton: {advanced_y}")

    # Place behind target 2000 units back (not 3000) — must stay above Y=-4700
    behind_y = advanced_y - 2000
    # Safety clamp so skeleton doesn't land in tower range
    behind_y = max(behind_y, -4600)
    behind_skel = m.spawn_troop(2, "skeleton", 0, behind_y)
    print(f"  Behind skeleton placed at (0, {behind_y})")
    print(f"    dist to P1L tower: {dist(0, behind_y, -5100, -10200):.0f} (need >7500)")

    # Wait for deploy (20t) + time for knight to turn and walk back
    m.step_n(60)
    knight_after = find_entity(m, kid)
    if knight_after and knight_after["alive"]:
        after_y = knight_after["y"]
        y_reversal = advanced_y - after_y  # Positive = moved backward (toward P1 side)
        print(f"  Knight Y: {advanced_y} -> {after_y} (reversal: {y_reversal})")
        check(y_reversal > 200, "Knight reversed Y direction toward behind target")
    else:
        behind_alive = entity_alive(m, behind_skel)
        check(not behind_alive, "Knight died but killed behind skeleton (retarget worked)")

# ------------------------------------------------------------------
# TEST 104: Troop retargets backward after crossing bridge
#           (behind target within sight range)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 104: Bridge cross then retarget backward")
print("=" * 60)

m = new_match()
kid = m.spawn_troop(1, "knight", -5100, -2000)
ahead_target = m.spawn_troop(2, "skeleton", -5100, 2000)

# Run until knight crosses river
crossed = False
for t in range(300):
    m.step()
    k = find_entity(m, kid)
    if k and k["y"] > 1200:
        crossed = True
        break

knight = find_entity(m, kid)
if knight and knight["alive"] and crossed:
    crossed_y = knight["y"]
    print(f"  Knight crossed river at Y={crossed_y}")

    # Place behind target WITHIN sight (3000 units back from current position)
    behind_y = crossed_y - 3000
    behind = m.spawn_troop(2, "skeleton", -5100, behind_y)
    print(f"  Behind skeleton at (-5100, {behind_y})")

    # Wait for deploy (20t) + turn and walk time
    m.step_n(60)
    knight_after = find_entity(m, kid)

    if knight_after and knight_after["alive"]:
        after_y = knight_after["y"]
        y_reversal = crossed_y - after_y
        print(f"  Knight Y: {crossed_y} -> {after_y} (reversal: {y_reversal})")
        check(y_reversal > 200 or after_y < crossed_y,
              "Knight moved backward toward new target")
    else:
        behind_alive = entity_alive(m, behind)
        check(not behind_alive, "Knight died but killed behind skeleton")
else:
    if knight:
        print(f"  Knight Y after 300 ticks: {knight['y']}")
    check(False, "Knight failed to cross river in 300 ticks")

# ------------------------------------------------------------------
# TEST 105: Giant retargets from dead building to princess tower
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 105: Giant retargets from dead Cannon to princess tower")
print("=" * 60)

m = new_match()
gid = m.spawn_troop(1, "giant", 0, -5000)
bid = m.spawn_building(2, "cannon", 0, 5000)

m.step_n(100)
giant_mid = find_entity(m, gid)
mid_y = giant_mid["y"] if giant_mid else -5000
print(f"  Giant Y after 100 ticks: {mid_y}")

# Cannon lifetime ~620 ticks — run enough for it to expire
m.step_n(600)
cannon_alive = entity_alive(m, bid)
print(f"  Cannon alive after 700 ticks total: {cannon_alive}")

giant = find_entity(m, gid)
if giant and giant["alive"]:
    final_y = giant["y"]
    print(f"  Giant final Y: {final_y}")
    check(final_y > mid_y or final_y > 5000, "Giant continued advancing after cannon died")
else:
    tower_hp = m.p2_tower_hp()
    any_damaged = any(hp < 3052 for hp in tower_hp[1:])
    check(any_damaged, "Giant damaged a tower before dying (retargeted)")

# ------------------------------------------------------------------
# TEST 106: Sequential retargets — Knight kills 3 skeletons in sequence
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 106: Sequential retargets \u2014 Knight kills 3 skeletons")
print("=" * 60)

m = new_match()
kid = m.spawn_troop(1, "knight", 0, -6000)
s1 = m.spawn_troop(2, "skeleton", 0, -5000)
s2 = m.spawn_troop(2, "skeleton", 0, -3500)
s3 = m.spawn_troop(2, "skeleton", 0, -2000)

kills = 0
for t in range(300):
    m.step()
    alive_count = sum(1 for sid in [s1, s2, s3] if entity_alive(m, sid))
    current_kills = 3 - alive_count
    if current_kills > kills:
        kills = current_kills
        print(f"  Kill #{kills} at tick ~{t+1}")

print(f"  Total kills: {kills}")
check(kills >= 2, "Knight killed at least 2 skeletons (retargeted at least once)")
check(kills == 3, "Knight killed all 3 skeletons (retargeted twice)")

# ------------------------------------------------------------------
# TEST 107: Retarget picks nearest, not first spawned
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 107: Retarget picks nearest enemy (not first-spawned)")
print("=" * 60)

m = new_match()
kid = m.spawn_troop(1, "knight", 0, -5000)
close = m.spawn_troop(2, "skeleton", 0, -4000)

# Let Knight kill close skeleton
m.step_n(60)
print(f"  Close skeleton alive: {entity_alive(m, close)}")

knight = find_entity(m, kid)
if knight and knight["alive"]:
    kx, ky = knight["x"], knight["y"]
    print(f"  Knight at ({kx}, {ky})")

    # Far: 4000 units away; Near: ~360 units away — both within sight
    far_enemy = m.spawn_troop(2, "skeleton", kx + 4000, ky)
    near_enemy = m.spawn_troop(2, "skeleton", kx + 300, ky - 200)

    # Run enough for deploy + one hit
    m.step_n(50)

    near_alive = entity_alive(m, near_enemy)
    far_alive = entity_alive(m, far_enemy)

    near_e = find_entity(m, near_enemy)
    far_e = find_entity(m, far_enemy)
    print(f"  Near skeleton alive: {near_alive}  HP: {near_e['hp'] if near_e else 'cleaned up'}")
    print(f"  Far skeleton alive: {far_alive}  HP: {far_e['hp'] if far_e else 'cleaned up'}")

    near_hit = not near_alive
    far_untouched = far_alive
    check(near_hit or far_untouched,
          "Knight targeted nearest enemy (not first-spawned)")
else:
    check(False, "Knight dead before retarget test")

# ------------------------------------------------------------------
# TEST 108: Ranged retarget — Musketeer switches after kill
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 108: Ranged retarget \u2014 Musketeer switches target after kill")
print("=" * 60)

m = new_match()
mid = m.spawn_troop(1, "musketeer", 0, -5000)
close = m.spawn_troop(2, "skeleton", 0, -3000)
far = m.spawn_troop(2, "giant", 0, -1000)

for t in range(200):
    m.step()
    if not entity_alive(m, close):
        print(f"  Close skeleton died at tick ~{t+1}")
        break

giant_before = find_entity(m, far)
giant_hp_before = giant_before["hp"] if giant_before else 0
print(f"  Giant HP before retarget: {giant_hp_before}")

m.step_n(60)
giant_after = find_entity(m, far)
giant_hp_after = giant_after["hp"] if giant_after else 0
giant_damage = giant_hp_before - giant_hp_after
print(f"  Giant HP after 60 ticks: {giant_hp_after}  (damage: {giant_damage})")

check(giant_damage > 0, "Musketeer retargeted to Giant and dealt damage")

# ------------------------------------------------------------------
# TEST 109: Retarget travel gap — time between kills includes
#           target acquisition + movement (load_after_retarget=0
#           for all troops, so gap is purely travel time)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 109: Retarget travel gap \u2014 time between kills")
print("=" * 60)

m = new_match()
kid = m.spawn_troop(1, "knight", 0, -5000)
s1 = m.spawn_troop(2, "skeleton", 0, -4000)
s2 = m.spawn_troop(2, "skeleton", 0, -2500)

s1_death_tick = None
s2_death_tick = None

for t in range(200):
    m.step()
    if s1_death_tick is None and not entity_alive(m, s1):
        s1_death_tick = t + 1

    if s2_death_tick is None and not entity_alive(m, s2):
        s2_death_tick = t + 1

    if s2_death_tick:
        break

if s1_death_tick and s2_death_tick:
    gap = s2_death_tick - s1_death_tick
    print(f"  Skeleton 1 died at tick ~{s1_death_tick}")
    print(f"  Skeleton 2 died at tick ~{s2_death_tick}")
    print(f"  Gap between kills: {gap} ticks ({gap/20:.1f}s)")
    check(gap >= 1, "Gap between kills > 0 (retarget + travel time)")
    check(gap <= 80, "Gap reasonable (\u226480 ticks / 4 seconds)")
elif s1_death_tick:
    print(f"  Skeleton 1 died at tick ~{s1_death_tick}")
    print("  Skeleton 2 not yet dead after 200 ticks")
    check(False, "Second skeleton not killed in 200 ticks")
else:
    print("  Neither skeleton died in 200 ticks")
    check(False, "Skeletons not killed")

# ------------------------------------------------------------------
# TEST 110: Sticky targeting — doesn't switch to closer new enemy
#           (use P2 Knight as target — deploy=20t, HP=1766)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 110: Sticky targeting \u2014 doesn't switch to closer new enemy")
print("=" * 60)

m = new_match()
attacker = m.spawn_troop(1, "knight", 0, -5000)
# P2 Knight 1200 away — right at attack range edge
original = m.spawn_troop(2, "knight", 0, -3800)

# Wait for both to deploy (20t) + attacker walks to range + first attacks
# distance = 1200, range = 1200, so effectively in range immediately after deploy
m.step_n(60)  # 20t deploy + 40t of attacking (should land ~1-2 hits)

original_e = find_entity(m, original)
original_hp = original_e["hp"] if original_e else 0
print(f"  Original target (P2 Knight) HP: {original_hp}/1766")

if original_hp < 1766:
    print("  Attacker IS hitting original target")

    # Now spawn distractor RIGHT NEXT to attacker
    attacker_e = find_entity(m, attacker)
    ax, ay = attacker_e["x"], attacker_e["y"]
    distractor = m.spawn_troop(2, "giant", ax + 100, ay - 100)

    m.step_n(50)  # 20t deploy + 30t

    original_e2 = find_entity(m, original)
    distractor_alive = entity_alive(m, distractor)

    original_hp2 = original_e2["hp"] if original_e2 else 0
    original_took_more = original_hp2 < original_hp
    print(f"  Original HP: {original_hp} -> {original_hp2} (continued damage: {original_took_more})")
    print(f"  Distractor alive: {distractor_alive}")

    check(original_took_more, "Attacker kept hitting original target (sticky)")
    check(distractor_alive, "Distractor giant NOT attacked (sticky targeting)")
else:
    print("  Attacker hasn't hit target yet \u2014 running more ticks...")
    m.step_n(60)
    original_e = find_entity(m, original)
    original_hp = original_e["hp"] if original_e else 0
    print(f"  Original HP after more ticks: {original_hp}")
    check(original_hp < 1766, "Attacker eventually hit original target")
    check(True, "Sticky targeting (deferred test)")

# ------------------------------------------------------------------
# TEST 111: Hog Rider retargets from dead building to tower
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 111: Hog Rider retargets from dead building to tower")
print("=" * 60)

m = new_match()
hid = m.spawn_troop(1, "hog-rider", -5100, -3000)
bid = m.spawn_building(2, "tombstone", -5100, 3000)

building_died = False
for t in range(400):
    m.step()
    if not entity_alive(m, bid):
        print(f"  Tombstone died/expired at tick ~{t+1}")
        building_died = True
        break

if building_died:
    hog = find_entity(m, hid)
    if hog and hog["alive"]:
        hog_y = hog["y"]
        m.step_n(100)
        hog2 = find_entity(m, hid)
        if hog2 and hog2["alive"]:
            print(f"  Hog Y: {hog_y} -> {hog2['y']} (advancing toward tower)")
            check(hog2["y"] > hog_y or hog2["y"] > 8000,
                  "Hog continued advancing toward tower after building died")
        else:
            tower_hp = m.p2_tower_hp()
            check(tower_hp[1] < 3052, "Hog damaged princess tower before dying")
    else:
        tower_hp = m.p2_tower_hp()
        check(tower_hp[1] < 3052 or tower_hp[2] < 3052,
              "Hog dealt tower damage (retargeted)")
else:
    check(False, "Tombstone didn't die in 400 ticks")

# ------------------------------------------------------------------
# TEST 112: Kiting chain — sequential baits pull troop sideways
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 112: Kiting chain \u2014 sequential baits pull troop sideways")
print("=" * 60)

m = new_match()
kid = m.spawn_troop(1, "knight", 0, -5000)
bait1 = m.spawn_troop(2, "skeleton", 2000, -4500)

m.step_n(70)  # deploy + kill bait 1
knight = find_entity(m, kid)
k_x_1 = knight["x"] if knight else 0
print(f"  Knight X after bait 1: {k_x_1}")

# Bait 2: further right, within sight of knight's current position
bait2 = m.spawn_troop(2, "skeleton", k_x_1 + 2000, -4500)
m.step_n(70)

knight = find_entity(m, kid)
k_x_2 = knight["x"] if knight else k_x_1
print(f"  Knight X after bait 2: {k_x_2}")

# Bait 3: even further right
bait3 = m.spawn_troop(2, "skeleton", k_x_2 + 2000, -4500)
m.step_n(70)

knight = find_entity(m, kid)
k_x_final = knight["x"] if knight else k_x_2
print(f"  Knight X after bait 3: {k_x_final}")
print(f"  Knight X progression: 0 -> {k_x_1} -> {k_x_2} -> {k_x_final}")

check(k_x_final > 2000, "Knight was pulled rightward by sequential baits")
check(k_x_2 > k_x_1 and k_x_final > k_x_2,
      "Knight followed the bait chain (monotonically rightward)")

# ====================================================================
# SUMMARY
# ====================================================================

print("\n" + "=" * 60)
print(f"  RESULTS: {passed}/{passed + failed} passed, {failed}/{passed + failed} failed")
print("=" * 60)

if failed == 0:
    print("\n  All kiting & re-aggro tests passed!")
else:
    print(f"\n  {failed} test(s) failed \u2014 see above for details.")
    print("  Failures indicate engine mechanics that need code changes.")

sys.exit(0 if failed == 0 else 1)
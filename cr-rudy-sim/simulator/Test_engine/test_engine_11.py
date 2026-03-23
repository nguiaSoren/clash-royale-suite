#!/usr/bin/env python3
"""
============================================================
  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 11
  Tests 141-150: River jumping, Hog Rider pathing,
  Royal Hogs, Ram Rider, flying vs ground river behavior
============================================================

Tests that river-jumping troops (Hog Rider, Royal Hogs, Ram Rider)
cross the river directly without routing to a bridge, while normal
ground troops must use bridges, and flying troops ignore it entirely.

Calibration:
  - River Y band: -1200 to +1200
  - Bridges at X = ±5100, half-width 1200
  - Hog Rider: speed=60u/t, building-only, can_jump_river=true
  - Royal Hogs: speed=60u/t, building-only, can_jump_river=true (via "royal-hogs" key)
  - Knight: speed=30u/t, can_jump_river=false (must use bridge)
  - Balloon: flying, ignores river entirely
"""

import sys
import os
import math

try:
    import cr_engine
except ImportError:
    here = os.path.dirname(os.path.abspath(__file__))
    for p in [
        os.path.join(here, "engine", "target", "release"),
        os.path.join(here, "target", "release"),
        os.path.join(here, "engine", "target", "maturin", "release"),
    ]:
        if os.path.isdir(p):
            sys.path.insert(0, p)
    import cr_engine

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


def check(condition, msg):
    global passed, failed
    if condition:
        print(f"  ✓ {msg}")
        passed += 1
    else:
        print(f"  ✗ FAIL: {msg}")
        failed += 1


def new_match():
    return cr_engine.new_match(data, FILLER_DECK, FILLER_DECK)


def find_entity(m, entity_id):
    for e in m.get_entities():
        if e["id"] == entity_id:
            return e
    return None


def dist(x1, y1, x2, y2):
    return math.sqrt((x1 - x2) ** 2 + (y1 - y2) ** 2)


print("=" * 60)
print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 11")
print("  Tests 141-150: River jumping & crossing mechanics")
print("=" * 60)


# ------------------------------------------------------------------
# TEST 141: Hog Rider crosses river without bridge
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 141: Hog Rider jumps river (no bridge needed)")
print("=" * 60)

m = new_match()
# Place Hog at center, far from both bridges (X=0, bridges at ±5100)
hog_id = m.spawn_troop(1, "hog-rider", 0, -2000)

# Run until Hog crosses river (Y > 1200)
crossed = False
for t in range(200):
    m.step()
    h = find_entity(m, hog_id)
    if h and h["y"] > 1200:
        crossed = True
        print(f"  Hog crossed river at tick {t+1}: ({h['x']}, {h['y']})")
        break

check(crossed, "Hog Rider crossed river")

if crossed:
    h = find_entity(m, hog_id)
    # Hog should NOT be near a bridge (X should be near 0, not ±5100)
    dist_to_left_bridge = abs(h["x"] - (-5100))
    dist_to_right_bridge = abs(h["x"] - 5100)
    print(f"  Hog X={h['x']}  dist to left bridge: {dist_to_left_bridge}  right: {dist_to_right_bridge}")
    check(min(dist_to_left_bridge, dist_to_right_bridge) > 2000,
          "Hog crossed at center (not via bridge)")


# ------------------------------------------------------------------
# TEST 142: Knight CANNOT cross river without bridge
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 142: Knight must use bridge (can't jump river)")
print("=" * 60)

m = new_match()
# Place Knight at center — should path to a bridge
kid = m.spawn_troop(1, "knight", 0, -2000)

m.step_n(60)  # 20t deploy + 40t movement

knight = find_entity(m, kid)
if knight and knight["alive"]:
    # Knight should be heading toward a bridge (X shifting toward ±5100)
    x_shift = abs(knight["x"])
    print(f"  Knight after 60 ticks: ({knight['x']}, {knight['y']})")
    print(f"  |X shift| from center: {x_shift}")
    check(x_shift > 500, "Knight moved laterally toward bridge (not straight)")
    check(knight["y"] < 1200, "Knight hasn't crossed river yet (needs bridge)")
else:
    check(False, "Knight died")


# ------------------------------------------------------------------
# TEST 143: Hog Rider goes straight toward enemy tower (no lateral drift)
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 143: Hog Rider paths straight (no bridge detour)")
print("=" * 60)

m = new_match()
# Place Hog at center heading toward P2 left princess tower
hog_id = m.spawn_troop(1, "hog-rider", 0, -3000)

m.step_n(40)  # deploy + 20t movement

hog = find_entity(m, hog_id)
if hog and hog["alive"]:
    # Hog should move mostly in Y direction, minimal X drift
    # (it's heading toward nearest building, probably a tower)
    y_progress = hog["y"] - (-3000)
    x_drift = abs(hog["x"])
    print(f"  Hog at ({hog['x']}, {hog['y']})")
    print(f"  Y progress: {y_progress}  X drift: {x_drift}")
    check(y_progress > 500, "Hog made forward progress")
    # Hog's target is a building, likely a princess tower. It should go
    # mostly forward, not sideways to a bridge.
    check(y_progress > x_drift, "Hog moved more forward than sideways (direct path)")
else:
    check(False, "Hog died")


# ------------------------------------------------------------------
# TEST 144: Hog Rider placed off-center still jumps river
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 144: Off-center Hog still jumps river (no bridge routing)")
print("=" * 60)

m = new_match()
# Place Hog between center and bridge — should still jump, not detour to bridge
hog_id = m.spawn_troop(1, "hog-rider", 2000, -2000)

crossed = False
for t in range(200):
    m.step()
    h = find_entity(m, hog_id)
    if h and h["y"] > 1200:
        crossed = True
        print(f"  Hog crossed at tick {t+1}: ({h['x']}, {h['y']})")
        break

check(crossed, "Off-center Hog crossed river")
if crossed:
    h = find_entity(m, hog_id)
    # Should NOT be at bridge X (5100) — should have crossed near X=2000
    check(abs(h["x"] - 5100) > 1000, "Hog crossed near its start X (not at bridge)")


# ------------------------------------------------------------------
# TEST 145: Flying troop (Balloon) crosses river at any X
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 145: Balloon ignores river (flying)")
print("=" * 60)

m = new_match()
balloon_id = m.spawn_troop(1, "balloon", 0, -3000)

# Balloon speed=30u/t (not 60 like Hog). From Y=-3000 to Y=1200 = 4200 units.
# After deploy (20t), needs 4200/30 = 140 ticks. Give 200 total.
m.step_n(200)

b = find_entity(m, balloon_id)
if b and b["alive"]:
    print(f"  Balloon at ({b['x']}, {b['y']})")
    check(b["y"] > 0, "Balloon crossed river (flying)")
    check(abs(b["x"]) < 3000, "Balloon stayed near center X (no bridge routing)")
else:
    check(False, "Balloon died")


# ------------------------------------------------------------------
# TEST 146: Giant (building-only) still uses bridge
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 146: Giant uses bridge (building-only but can't jump)")
print("=" * 60)

m = new_match()
giant_id = m.spawn_troop(1, "giant", 0, -2000)

m.step_n(80)

giant = find_entity(m, giant_id)
if giant and giant["alive"]:
    x_shift = abs(giant["x"])
    print(f"  Giant after 80 ticks: ({giant['x']}, {giant['y']})")
    check(x_shift > 500 or giant["y"] < -500,
          "Giant heading toward bridge (not jumping river)")
else:
    check(False, "Giant died")


# ------------------------------------------------------------------
# TEST 147: Hog Rider reaches tower faster than bridge-pathing Knight
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 147: Hog arrives at tower faster than Knight (river jump advantage)")
print("=" * 60)

m1 = new_match()
m2 = new_match()

# Same start: center of P1 side
hog_id = m1.spawn_troop(1, "hog-rider", 0, -3000)
kid = m2.spawn_troop(1, "knight", 0, -3000)

# Run both for same duration
for _ in range(150):
    m1.step()
    m2.step()

hog = find_entity(m1, hog_id)
knight = find_entity(m2, kid)

if hog and knight:
    hog_y = hog["y"] if hog["alive"] else 99999
    knight_y = knight["y"] if knight["alive"] else -99999
    print(f"  Hog Y after 150t: {hog_y}")
    print(f"  Knight Y after 150t: {knight_y}")
    # Hog should be much further forward (jumped river + faster speed)
    check(hog_y > knight_y + 1000,
          "Hog significantly ahead of Knight (river jump + speed advantage)")
else:
    check(False, "Entity missing")


# ------------------------------------------------------------------
# TEST 148: Royal Hogs jump river
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 148: Royal Hogs jump river (4 hogs, no bridge)")
print("=" * 60)

try:
    hog_deck = [
        "royal-hogs", "knight", "archers", "giant",
        "valkyrie", "musketeer", "fireball", "zap",
    ]
    m = cr_engine.new_match(data, hog_deck, FILLER_DECK)

    # Build up elixir for Royal Hogs (5 elixir)
    m.step_n(20)  # start with 5 + a bit

    # Play Royal Hogs at center
    m.play_card(1, 0, 0, -2000)

    # Find the spawned hogs
    m.step_n(100)

    hogs = [e for e in m.get_entities()
            if e["alive"] and e["team"] == 1 and "hog" in e.get("card_key", "").lower()]

    if hogs:
        crossed = sum(1 for h in hogs if h["y"] > 1200)
        print(f"  Royal Hogs alive: {len(hogs)}  crossed river: {crossed}")
        check(crossed > 0, "At least one Royal Hog crossed river")
        # Check they didn't path to bridge
        center_hogs = sum(1 for h in hogs if abs(h["x"]) < 3000)
        check(center_hogs > 0, "Royal Hogs stayed near center (jumped, not bridged)")
    else:
        print("  No hogs found — checking if they spawned at all")
        all_troops = [e for e in m.get_entities() if e["alive"] and e["team"] == 1]
        print(f"  P1 troops: {[e['card_key'] for e in all_troops]}")
        check(False, "Royal Hogs not found after play_card")
except Exception as e:
    print(f"  Royal Hogs test failed: {e}")
    check(False, f"Could not play Royal Hogs: {e}")


# ------------------------------------------------------------------
# TEST 149: Hog Rider river jump + building pull combo
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 149: Hog jumps river then gets pulled by building")
print("=" * 60)

m = new_match()
hog_id = m.spawn_troop(1, "hog-rider", 0, -2000)

# Let Hog cross river
for t in range(200):
    m.step()
    h = find_entity(m, hog_id)
    if h and h["y"] > 2000:
        break

hog = find_entity(m, hog_id)
if hog and hog["alive"] and hog["y"] > 1200:
    print(f"  Hog crossed river at ({hog['x']}, {hog['y']})")

    # Place Cannon to pull Hog
    cannon_id = m.spawn_building(2, "cannon", 3000, hog["y"] + 1000)
    m.step_n(30)

    hog2 = find_entity(m, hog_id)
    if hog2 and hog2["alive"]:
        x_shift = hog2["x"] - hog["x"]
        print(f"  Hog X shift after Cannon placed: {x_shift}")
        check(x_shift > 200, "Hog pulled toward Cannon after river jump")
    else:
        check(True, "Hog died (reached a building)")
else:
    check(False, "Hog didn't cross river")


# ------------------------------------------------------------------
# TEST 150: Ground troop comparison — same start, bridge vs jump
# ------------------------------------------------------------------
print("\n" + "=" * 60)
print("TEST 150: Side-by-side — Hog jumps while Knight bridges")
print("=" * 60)

m = new_match()
# Both start at center
hog_id = m.spawn_troop(1, "hog-rider", 0, -2500)
kid = m.spawn_troop(1, "knight", 100, -2500)  # Slight offset for collision

m.step_n(80)  # deploy + 60t movement

hog = find_entity(m, hog_id)
knight = find_entity(m, kid)

if hog and knight and hog["alive"] and knight["alive"]:
    hog_y = hog["y"]
    knight_y = knight["y"]
    print(f"  Hog: ({hog['x']}, {hog['y']})")
    print(f"  Knight: ({knight['x']}, {knight['y']})")

    # Hog should have more Y progress (jumps river, goes straight)
    # Knight must detour to bridge (less Y progress in same time)
    check(hog_y > knight_y,
          "Hog further forward than Knight (river jump advantage)")
else:
    check(False, "Entity died")


# ====================================================================
# SUMMARY
# ====================================================================

print("\n" + "=" * 60)
print(f"  RESULTS: {passed}/{passed + failed} passed, {failed}/{passed + failed} failed")
print("=" * 60)

if failed == 0:
    print("\n  All river jump tests passed!")
else:
    print(f"\n  {failed} test(s) failed — see above for details.")

sys.exit(0 if failed == 0 else 1)
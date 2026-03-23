#!/usr/bin/env python3
"""
PUSHBACK FIDELITY TEST v2
Measures pure pushback displacement by tracking the single-tick jump at impact.

Method: Track target position every tick. Normal walking = ~12-30u/tick.
Pushback produces a single-tick jump of hundreds of units.
The largest single-tick displacement IS the pushback impulse.

Data values:
  LogProjectileRolling:  pushback=700
  SnowballSpell:         pushback=1800
  FireballSpell:         pushback=1000
"""
import sys, os

try:
    import cr_engine
except ImportError:
    for p in ["engine/target/release", "target/release"]:
        if os.path.isdir(p):
            sys.path.insert(0, p)
    import cr_engine

DATA_DIR = "data"
for d in [os.path.join(os.path.dirname(os.path.abspath(__file__)), "data"), "data"]:
    if os.path.isdir(d):
        DATA_DIR = d
        break

data = cr_engine.load_data(DATA_DIR)
DUMMY_DECK = ["knight"] * 8
PASS = 0
FAIL = 0


def check(name, cond, detail=""):
    global PASS, FAIL
    if cond:
        PASS += 1
        print(f"  ✓ {name}")
    else:
        FAIL += 1
        print(f"  ✗ {name}  {detail}")


def find(m, eid):
    for e in m.get_entities():
        if e["id"] == eid:
            return e
    return None


def step_n(m, n):
    for _ in range(n):
        m.step()


card_keys = {c["key"] for c in data.list_cards()}
LOG_KEY = next((c for c in ["the-log", "log"] if c in card_keys), None)
SNOWBALL_KEY = next((c for c in ["giant-snowball", "snowball"] if c in card_keys), None)
FIREBALL_KEY = next((c for c in ["fireball"] if c in card_keys), None)


def measure_max_single_tick_jump(spell_key, target_card, target_x, target_y, spell_x, spell_y):
    deck = [spell_key] + ["knight"] * 7
    m = cr_engine.new_match(data, deck, DUMMY_DECK)
    step_n(m, 20)

    tid = m.spawn_troop(2, target_card, target_x, target_y)
    step_n(m, 60)

    e = find(m, tid)
    if not e or not e["alive"]:
        return None

    prev_x, prev_y = e["x"], e["y"]
    hp0 = e["hp"]

    m.play_card(1, 0, spell_x, spell_y)

    max_jump = 0
    max_jump_tick = -1
    jump_dx = 0
    jump_dy = 0
    hp_lost = 0

    for t in range(60):
        m.step()
        e = find(m, tid)
        if not e or not e["alive"]:
            hp_lost = hp0
            break
        dx = e["x"] - prev_x
        dy = e["y"] - prev_y
        jump = (dx * dx + dy * dy) ** 0.5
        if jump > max_jump:
            max_jump = jump
            max_jump_tick = t
            jump_dx = dx
            jump_dy = dy
        hp_lost = hp0 - e["hp"]
        prev_x, prev_y = e["x"], e["y"]

    return (max_jump, max_jump_tick, jump_dx, jump_dy, hp_lost)


print("=" * 70)
print("  PUSHBACK FIDELITY TEST v2")
print("  Measuring single-tick displacement impulse vs data values")
print("=" * 70)

tests = [
    ("LOG", LOG_KEY, 700, "knight", 0, 5000, 0, 4000),
    ("SNOWBALL", SNOWBALL_KEY, 1800, "knight", 0, 5000, 0, 5000),
    ("FIREBALL", FIREBALL_KEY, 1000, "knight", 0, 5000, 0, 5000),
]

for label, key, expected, target, tx, ty, sx, sy in tests:
    print(f"\n" + "-" * 60)
    print(f"{label}: expected pushback={expected}")
    print("-" * 60)

    if not key:
        check(f"{label} key", False, "not found")
        continue

    result = measure_max_single_tick_jump(key, target, tx, ty, sx, sy)
    if not result:
        check(f"{label} setup", False, "target not found")
        continue

    impulse, tick, dx, dy, hp_lost = result
    print(f"  Max single-tick jump: {impulse:.0f}u at tick {tick} (dx={dx}, dy={dy:+d})")
    print(f"  Expected: {expected}u")
    print(f"  HP lost: {hp_lost}")
    error_pct = abs(impulse - expected) / expected * 100

    check(f"{label} dealt damage", hp_lost > 0, f"hp_lost={hp_lost}")
    check(f"{label} impulse within 25% of {expected} (actual={impulse:.0f}, err={error_pct:.0f}%)",
          expected * 0.75 <= impulse <= expected * 1.25,
          f"impulse={impulse:.0f}, err={error_pct:.0f}%")
    check(f"{label} impulse within 10% of {expected} (actual={impulse:.0f}, err={error_pct:.0f}%)",
          expected * 0.90 <= impulse <= expected * 1.10,
          f"impulse={impulse:.0f}, err={error_pct:.0f}%")

print("\n" + "=" * 70)
print(f"  RESULTS: {PASS}/{PASS + FAIL} passed, {FAIL}/{PASS + FAIL} failed")
print("=" * 70)
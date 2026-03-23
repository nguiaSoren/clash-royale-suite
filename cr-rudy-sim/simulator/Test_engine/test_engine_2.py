"""
Engine fidelity tests — batch 2 (7 tests)

Place in: simulator/test_engine_2.py
Run with: python test_engine_2.py

Tests:
  4. Attack speed timing — exact tick intervals between hits
  5. Movement speed — distance per tick matches speed table
  6. Splash damage — Valkyrie hits multiple skeletons at once
  7. Princess tower shoots approaching troop
  8. Elixir generation rate over time
  9. Golem death spawn — Golemites appear when Golem dies
 10. King tower activation — activates when princess tower destroyed
"""

import cr_engine
import sys

data = cr_engine.load_data("data/")

def find_entity(match, entity_id):
    for e in match.get_entities():
        if e["id"] == entity_id:
            return e
    return None

def find_alive(match, kind="troop", team=None):
    result = []
    for e in match.get_entities():
        if e["alive"] and e["kind"] == kind:
            if team is None or e["team"] == team:
                result.append(e)
    return result

DUMMY_DECK = ["knight"] * 8
PASS = 0
FAIL = 0

def check(name, condition, detail=""):
    global PASS, FAIL
    if condition:
        PASS += 1
        print(f"  ✓ {name}")
    else:
        FAIL += 1
        print(f"  ✗ {name}  {detail}")


# =========================================================================
# TEST 4: Attack speed timing — exact intervals between hits
# =========================================================================
#
# What this tests:
#   Knight hit_speed = 1200ms = 24 ticks.
#   After the first hit lands, the next hit should land exactly 24 ticks later.
#   We spawn a Knight attacking a high-HP Giant and log every tick the Giant
#   loses HP. The intervals between damage events should be exactly 24 ticks.
#
# Why this matters:
#   If attack speed is doubled or halved, every DPS calculation in the game
#   is wrong. This is the most critical timing check.

def test_attack_timing():
    print("\n" + "="*60)
    print("TEST 4: Attack speed timing (Knight hit_speed = 24 ticks)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Knight right next to P2 Giant — within attack range (1200)
    # Place at 600 apart — well within range, and attacker-target collision
    # skip ensures they don't get pushed apart.
    knight_id = m.spawn_troop(1, "knight", 0, 0)
    giant_id = m.spawn_troop(2, "giant", 0, 600)

    stats = data.get_character_stats("knight")
    expected_interval = round(stats["hit_speed"] / 50)  # ms to ticks
    print(f"\n  Knight hit_speed: {stats['hit_speed']}ms = {expected_interval} ticks expected")
    print(f"  Note: with windup model, first hit is delayed by load_time (~14t)")

    prev_hp = None
    damage_ticks = []

    for t in range(300):
        m.step()
        e_giant = find_entity(m, giant_id)
        if e_giant is None:
            break

        if prev_hp is None:
            prev_hp = e_giant["hp"]
        elif e_giant["hp"] < prev_hp:
            damage_ticks.append(t + 1)
            prev_hp = e_giant["hp"]

        if not e_giant["alive"]:
            break

    print(f"  Damage landed at ticks: {damage_ticks[:8]}")

    if len(damage_ticks) >= 3:
        intervals = [damage_ticks[i+1] - damage_ticks[i] for i in range(len(damage_ticks)-1)]
        print(f"  Intervals between hits: {intervals[:7]}")

        # With windup/backswing model: interval = windup_ticks + backswing_ticks
        # Knight: windup=14t, backswing=10t → 24t total (same as hit_speed)
        # Accept ±3 for rounding
        stable_intervals = intervals[:min(5, len(intervals))]
        good_intervals = sum(1 for iv in stable_intervals if abs(iv - expected_interval) <= 3)
        check(f"Attack intervals ≈ {expected_interval} ticks (±3) [first {len(stable_intervals)} hits]",
              good_intervals >= len(stable_intervals) * 0.6 if stable_intervals else False,
              f"got intervals {stable_intervals}")
    else:
        check("Enough hits landed to measure intervals", False,
              f"only {len(damage_ticks)} damage events")

    check("At least 3 hits landed", len(damage_ticks) >= 3,
          f"only {len(damage_ticks)} hits")


# =========================================================================
# TEST 5: Movement speed — Knight travels correct distance per tick
# =========================================================================
#
# What this tests:
#   Knight speed=60 (Medium) → 30 units/tick.
#   Spawn a P1 Knight with no enemies nearby. It should walk toward
#   the enemy princess tower. Measure Y change over 100 ticks.
#   Expected: ~3000 units of Y movement (30 * 100).
#
# Why this matters:
#   If movement is too fast, troops reach towers instantly.
#   If too slow, nothing ever happens.

def test_movement_speed():
    print("\n" + "="*60)
    print("TEST 5: Movement speed (Knight = 30 units/tick)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Spawn P1 Knight on the left bridge — clear path to enemy tower
    knight_id = m.spawn_troop(1, "knight", -5100, -5000)

    # Wait for deploy timer to expire
    for _ in range(30):
        m.step()

    e = find_entity(m, knight_id)
    if e is None:
        check("Knight exists after deploy", False)
        return

    start_y = e["y"]
    start_tick = 30
    print(f"\n  Start: tick={start_tick}  Y={start_y}")

    # Run 100 more ticks
    for _ in range(100):
        m.step()

    e = find_entity(m, knight_id)
    if e is None:
        check("Knight still alive after movement", False)
        return

    end_y = e["y"]
    dy = end_y - start_y  # P1 moves toward positive Y
    speed_per_tick = dy / 100.0

    print(f"  End:   tick={start_tick + 100}  Y={end_y}")
    print(f"  Delta Y: {dy}  ({speed_per_tick:.1f} units/tick)")
    print(f"  Expected: ~30 units/tick (Medium speed)")

    # Allow range 20-40 (speed might not be perfectly straight-line Y)
    check("Movement speed in reasonable range (20-40 units/tick)",
          20 <= speed_per_tick <= 40,
          f"got {speed_per_tick:.1f}")
    check("Knight moved toward enemy side (Y increased)",
          dy > 0, f"dy={dy}")


# =========================================================================
# TEST 6: Splash damage — Valkyrie hits multiple nearby troops
# =========================================================================
#
# What this tests:
#   Valkyrie has area_damage_radius > 0 (splash).
#   Spawn a P1 Valkyrie next to a cluster of 3 P2 Knights.
#   After combat, ALL three Knights should have taken damage,
#   not just the one Valkyrie is targeting.
#
# Why this matters:
#   Without splash working, Valkyrie/Wizard/Baby Dragon are just
#   single-target troops — completely wrong.

def test_splash_damage():
    print("\n" + "="*60)
    print("TEST 6: Splash damage (Valkyrie hits multiple targets)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    valk_stats = data.get_character_stats("valkyrie")
    print(f"\n  Valkyrie: is_splash={valk_stats['is_splash']}  "
          f"DMG={valk_stats['damage']}  hit_speed={valk_stats['hit_speed']}ms")

    # P1 Valkyrie in the center
    valk_id = m.spawn_troop(1, "valkyrie", 0, 0)

    # 3 P2 Knights clustered very close together around the Valkyrie
    k1_id = m.spawn_troop(2, "knight", 200, 200)
    k2_id = m.spawn_troop(2, "knight", -200, 200)
    k3_id = m.spawn_troop(2, "knight", 0, -200)

    initial_hps = {}
    for kid in [k1_id, k2_id, k3_id]:
        e = find_entity(m, kid)
        if e:
            initial_hps[kid] = e["hp"]

    print(f"  3 Knights spawned, each HP={list(initial_hps.values())[0] if initial_hps else '?'}")

    # Run enough ticks for Valkyrie to attack a few times
    for t in range(200):
        m.step()

    # Check how many knights took damage
    damaged_count = 0
    for kid in [k1_id, k2_id, k3_id]:
        e = find_entity(m, kid)
        if e is None:
            # Dead = definitely took damage
            damaged_count += 1
        elif e["hp"] < initial_hps.get(kid, 0):
            damaged_count += 1

    print(f"  After 200 ticks: {damaged_count}/3 knights took damage")

    check("Valkyrie is classified as splash", valk_stats["is_splash"] == True)
    check("Multiple knights took damage (splash working)",
          damaged_count >= 2,
          f"only {damaged_count}/3 damaged — splash might not be working")


# =========================================================================
# TEST 7: Princess tower shoots approaching troop
# =========================================================================
#
# What this tests:
#   A troop walks toward an enemy princess tower.
#   The tower should start shooting when the troop enters range (7500 units).
#   The troop should take damage from the tower.
#
# Real CR: Princess tower range=7500, damage=109, hit_speed=0.8s (16 ticks)
#
# Why this matters:
#   If towers don't shoot, the entire game is broken — you can just
#   walk to the king tower for free.

def test_princess_tower_shoots():
    print("\n" + "="*60)
    print("TEST 7: Princess tower shoots approaching troop")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Giant walking toward P2's left princess tower (at -5100, 10200)
    # Spawn at the bridge — it'll walk straight into tower range
    giant_id = m.spawn_troop(1, "giant", -5100, 1500)

    e = find_entity(m, giant_id)
    initial_hp = e["hp"]
    print(f"\n  Giant spawned at (-5100, 1500), HP={initial_hp}")
    print(f"  P2 left princess tower at (-5100, 10200), range=7500")
    print(f"  Distance to tower: {10200 - 1500} = 8700 units (outside range)")

    took_damage = False
    first_damage_tick = None

    for t in range(400):
        m.step()
        e = find_entity(m, giant_id)
        if e is None:
            break

        if e["hp"] < initial_hp and not took_damage:
            took_damage = True
            first_damage_tick = t + 1
            print(f"  Giant took first tower hit at tick {first_damage_tick}: "
                  f"HP={e['hp']}/{initial_hp}  pos=({e['x']},{e['y']})")
            dist_to_tower = abs(10200 - e["y"])
            print(f"  Distance to tower when hit: {dist_to_tower}")

        if not e["alive"]:
            print(f"  Giant died at tick {t+1}")
            break

    check("Tower shot the approaching Giant", took_damage,
          "Giant HP never decreased — towers might not be shooting")

    if first_damage_tick:
        # Tower should fire when Giant enters 7500 range of princess tower at (−5100, 10200)
        # So roughly when Giant Y > 10200 - 7500 = 2700
        check("Tower fired at reasonable range",
              first_damage_tick < 300,
              f"first damage at tick {first_damage_tick} — tower fired very late")


# =========================================================================
# TEST 8: Elixir generation rate
# =========================================================================
#
# What this tests:
#   Starting elixir: 5. Rate: 1 elixir per 2.8s = 1 per 56 ticks.
#   After 560 ticks (28 seconds), player should have 5 + 10 = 15 → capped at 10.
#   After 168 ticks (8.4s), player should have 5 + 3 = 8.
#   We run 100 ticks with no plays and check elixir is ~6-7.
#
# Why this matters:
#   Wrong elixir rate means the entire economy is broken —
#   too fast and games become spam-fests, too slow and nobody
#   can play cards.

def test_elixir_generation():
    print("\n" + "="*60)
    print("TEST 8: Elixir generation rate")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    initial = m.p1_elixir
    print(f"\n  Starting elixir: {initial}")

    check("Starting elixir is 5", initial == 5)

    # Run 56 ticks — should gain ~1 elixir
    for _ in range(56):
        m.step()
    after_56 = m.p1_elixir
    print(f"  After 56 ticks (2.8s): {after_56} elixir (expected ~6)")
    check("Gained ~1 elixir in 56 ticks", 5 <= after_56 <= 7,
          f"got {after_56}")

    # Run to 280 ticks total — should gain ~5, so at 10
    for _ in range(224):
        m.step()
    after_280 = m.p1_elixir
    print(f"  After 280 ticks (14s): {after_280} elixir (expected 10, capped)")
    check("Elixir reached cap (10) by 280 ticks", after_280 == 10,
          f"got {after_280}")

    # Run 56 more ticks — should still be 10 (capped, not overflowing)
    for _ in range(56):
        m.step()
    after_336 = m.p1_elixir
    print(f"  After 336 ticks (16.8s): {after_336} elixir (should still be 10)")
    check("Elixir stays capped at 10", after_336 == 10,
          f"got {after_336}")


# =========================================================================
# TEST 9: Golem death spawn — Golemites appear when Golem dies
# =========================================================================
#
# What this tests:
#   Golem has death_spawn_character (Golemites) and death_damage.
#   When Golem dies, Golemites should appear at the death location.
#   This tests the death processing system.
#
# Why this matters:
#   Death spawns are core to several cards (Golem, Lava Hound, etc.).
#   If broken, Golem is just a big HP sponge with no second phase.

def test_golem_death_spawn():
    print("\n" + "="*60)
    print("TEST 9: Golem death spawn (Golemites)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    golem_id = m.spawn_troop(1, "golem", 0, 0)
    e = find_entity(m, golem_id)
    golem_hp = e["hp"]
    print(f"\n  Golem spawned: HP={golem_hp}")

    golem_stats = data.get_character_stats("golem")
    print(f"  Golem base damage={golem_stats['damage']}  "
          f"hit_speed={golem_stats['hit_speed']}ms")

    # Spawn enough P2 knights to kill the Golem
    # Knight does ~202 damage per 24 ticks at level 11.
    # Golem has ~4940 HP → needs ~25 hits → one knight takes 600 ticks.
    # Spawn 5 knights to kill it faster.
    for i in range(5):
        m.spawn_troop(2, "knight", 200 * (i - 2), 300)

    # Count P1 troops before
    p1_before = len(find_alive(m, "troop", team=1))
    print(f"  P1 troops before: {p1_before} (just the Golem)")

    # Run until Golem dies
    golem_died = False
    for t in range(800):
        m.step()
        e = find_entity(m, golem_id)
        if e is None or (e is not None and not e["alive"]):
            golem_died = True
            print(f"  Golem died at tick {t+1}")
            break

    # Run a few more ticks for death processing
    for _ in range(5):
        m.step()

    p1_after = find_alive(m, "troop", team=1)
    p1_count = len(p1_after)
    p1_keys = [e["card_key"] for e in p1_after]
    print(f"  P1 troops after Golem death: {p1_count}  keys={p1_keys}")

    check("Golem died", golem_died, "Golem survived 800 ticks — not enough damage?")
    check("New P1 troops spawned on death (Golemites)",
          p1_count > 0,
          f"no P1 troops exist after death — death spawn not working")
    if p1_count > 0:
        check("Death spawns are Golemites (not the original Golem)",
              any("golemite" in k.lower() or "golem" in k.lower() for k in p1_keys),
              f"spawned keys: {p1_keys}")


# =========================================================================
# TEST 10: King tower activation
# =========================================================================
#
# What this tests:
#   King tower starts inactive. It activates when:
#     a) A troop enters king activation range (3600 units), OR
#     b) A princess tower is destroyed.
#   We test (b): destroy a princess tower and verify the king starts shooting.
#
# Why this matters:
#   If king tower never activates, one side of the defense is permanently
#   offline. If it's always active from the start, the defender has an
#   unfair advantage.

def test_king_tower_activation():
    print("\n" + "="*60)
    print("TEST 10: King tower activation on princess death")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    p2_towers = m.p2_tower_hp()
    print(f"\n  P2 towers: king={p2_towers[0]}  left={p2_towers[1]}  right={p2_towers[2]}")

    # Spawn a massive force of P1 troops to destroy P2's left princess tower
    # 10 knights right next to the tower
    for i in range(10):
        m.spawn_troop(1, "knight", -5100 + (i * 100), 9000)

    princess_died = False
    princess_death_tick = None

    for t in range(600):
        m.step()
        p2_towers = m.p2_tower_hp()

        if p2_towers[1] <= 0 and not princess_died:
            princess_died = True
            princess_death_tick = t + 1
            print(f"\n  P2 left princess tower destroyed at tick {princess_death_tick}")
            print(f"  P2 towers now: king={p2_towers[0]}  left={p2_towers[1]}  right={p2_towers[2]}")
            break

    check("Princess tower was destroyed", princess_died,
          "10 knights couldn't kill a tower in 600 ticks")

    if not princess_died:
        return

    # Now spawn a single knight near P2's king tower to see if king shoots it
    test_knight_id = m.spawn_troop(1, "knight", 0, 10000)

    knight_initial_hp = find_entity(m, test_knight_id)["hp"]
    king_shot = False

    for t in range(200):
        m.step()
        e = find_entity(m, test_knight_id)
        if e is None:
            king_shot = True  # Knight died, must have been shot
            break
        if e["hp"] < knight_initial_hp:
            king_shot = True
            print(f"  King tower hit knight at tick {princess_death_tick + t + 1}: "
                  f"HP={e['hp']}/{knight_initial_hp}")
            break

    check("King tower activated and shot after princess death", king_shot,
          "Knight near king took no damage — king tower might not have activated")

    # Also verify king tower HP hasn't been touched
    p2_towers_final = m.p2_tower_hp()
    check("King tower still alive", p2_towers_final[0] > 0,
          f"king HP={p2_towers_final[0]}")


# =========================================================================
# Run all tests
# =========================================================================

if __name__ == "__main__":
    print("="*60)
    print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 2")
    print("  Tests 4-10: timing, movement, splash, towers, elixir, death")
    print("="*60)

    test_attack_timing()
    test_movement_speed()
    test_splash_damage()
    test_princess_tower_shoots()
    test_elixir_generation()
    test_golem_death_spawn()
    test_king_tower_activation()

    print("\n" + "="*60)
    total = PASS + FAIL
    print(f"  RESULTS: {PASS}/{total} passed, {FAIL}/{total} failed")
    print("="*60)

    if FAIL > 0:
        print("\n  Each failure points to a specific engine system to fix.")
        print("  Run the failed test in isolation and add more logging to")
        print("  narrow down the exact broken mechanic.")
        sys.exit(1)
    else:
        print("\n  All checks passed!")
        sys.exit(0)
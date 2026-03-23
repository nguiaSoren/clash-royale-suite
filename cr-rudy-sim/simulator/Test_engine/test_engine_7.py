"""
Engine fidelity tests — batch 7 (buildings + poison)

Place in: simulator/test_engine_7.py
Run with: python test_engine_7.py

Tests 53-66: hardcore building mechanics + poison spell
"""

import cr_engine
import sys

data = cr_engine.load_data("data/")

def find_entity(match, entity_id):
    for e in match.get_entities():
        if e["id"] == entity_id:
            return e
    return None

def find_alive(match, kind="troop", team=None, card_key=None):
    result = []
    for e in match.get_entities():
        if e["alive"] and e["kind"] == kind:
            if team is not None and e["team"] != team:
                continue
            if card_key is not None and e["card_key"] != card_key:
                continue
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
# TEST 53: Tesla attacks air AND ground
# =========================================================================
# Tesla: attacks_air=true, attacks_ground=true, damage=90, range=5500.
# It should shoot both a ground Knight and a flying Balloon.

def test_tesla_attacks_air_and_ground():
    print("\n" + "="*60)
    print("TEST 53: Tesla attacks air AND ground")
    print("="*60)

    # Test ground targeting
    m1 = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    tesla_id = m1.spawn_building(1, "tesla", 0, -5000)
    knight_id = m1.spawn_troop(2, "knight", 0, -1000)
    for _ in range(30):
        m1.step()
    knight_hp_before = find_entity(m1, knight_id)["hp"]
    for _ in range(100):
        m1.step()
    e_knight = find_entity(m1, knight_id)
    ground_damaged = e_knight is None or (e_knight and e_knight["hp"] < knight_hp_before)

    # Test air targeting
    m2 = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    tesla2 = m2.spawn_building(1, "tesla", 0, -5000)
    balloon_id = m2.spawn_troop(2, "balloon", 0, -1000)
    for _ in range(30):
        m2.step()
    balloon_hp_before = find_entity(m2, balloon_id)["hp"]
    for _ in range(100):
        m2.step()
    e_balloon = find_entity(m2, balloon_id)
    air_damaged = e_balloon is None or (e_balloon and e_balloon["hp"] < balloon_hp_before)

    print(f"\n  Ground (Knight) damaged by Tesla: {ground_damaged}")
    print(f"  Air (Balloon) damaged by Tesla: {air_damaged}")

    check("Tesla damaged ground troop", ground_damaged)
    check("Tesla damaged air troop", air_damaged)


# =========================================================================
# TEST 54: Cannon only attacks ground (not air)
# =========================================================================
# Cannon: attacks_air=false, attacks_ground=true.

def test_cannon_ground_only():
    print("\n" + "="*60)
    print("TEST 54: Cannon only attacks ground (not air)")
    print("="*60)

    # Cannon: range=5500, attacks_air=false, attacks_ground=true.
    # We place Cannon and targets close together, and use a SHORT window so
    # the Balloon can't drift into P1's princess tower range (7500 at y=-10200).
    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    cannon_id = m.spawn_building(1, "cannon", 0, -5000)
    # Place targets within cannon range (dist ~2000) but far from princess towers
    balloon_id = m.spawn_troop(2, "balloon", 0, -3000)
    knight_id = m.spawn_troop(2, "knight", 300, -3000)

    # Let deploy timers expire
    for _ in range(30):
        m.step()
    balloon_hp = find_entity(m, balloon_id)["hp"]
    knight_hp = find_entity(m, knight_id)["hp"]

    # Run only 40 ticks — enough for ~2 cannon attacks but the Balloon
    # (speed ~30/tick → ~1200 units moved) stays well outside princess range.
    for _ in range(40):
        m.step()

    e_balloon = find_entity(m, balloon_id)
    e_knight = find_entity(m, knight_id)

    balloon_damaged = e_balloon is None or (e_balloon and e_balloon["hp"] < balloon_hp)
    knight_damaged = e_knight is None or (e_knight and e_knight["hp"] < knight_hp)

    print(f"\n  Balloon damaged by Cannon: {balloon_damaged}")
    print(f"  Knight damaged by Cannon: {knight_damaged}")

    check("Cannon damaged ground troop (Knight)", knight_damaged)
    check("Cannon did NOT damage air troop (Balloon)", not balloon_damaged,
          "Cannon hit Balloon — attacks_air should be false")


# =========================================================================
# TEST 55: Goblin Hut spawns Spear Goblins periodically
# =========================================================================
# Goblin Hut: spawn_character=SpearGoblin, spawn_number=3, spawn_interval=500ms

def test_goblin_hut_spawner():
    print("\n" + "="*60)
    print("TEST 55: Goblin Hut spawns Spear Goblins")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    hut_id = m.spawn_building(1, "goblin-hut", 0, -5000)

    e = find_entity(m, hut_id)
    if e is None:
        check("Goblin Hut spawned", False, "not in building data")
        return

    print(f"\n  Goblin Hut spawned: HP={e['hp']}")

    # Track total unique spear goblin entity IDs ever seen (not just alive).
    # Earlier waves walk into tower fire and die, so alive count at any single
    # tick may only show one wave's worth.
    seen_ids = set()
    for t in range(500):
        m.step()
        for ent in m.get_entities():
            if ent["team"] == 1 and ent["kind"] == "troop" and "speargoblin" in ent["card_key"].lower():
                seen_ids.add(ent["id"])

    spear_count = len(seen_ids)
    # Also count currently alive for display
    alive_now = sum(1 for ent in m.get_entities()
                    if ent["alive"] and ent["team"] == 1
                    and "speargoblin" in ent["card_key"].lower())
    print(f"  Spear Goblins total ever spawned: {spear_count}")
    print(f"  Spear Goblins alive at tick 500: {alive_now}")

    check("Goblin Hut spawned Spear Goblins", spear_count > 0,
          "No spear goblins found")
    check("Multiple waves spawned (> 3 total)", spear_count > 3,
          f"only {spear_count} — might be single wave")


# =========================================================================
# TEST 56: Goblin Cage death spawn — releases Goblin Brawler
# =========================================================================
# Goblin Cage: death_spawn_character=GoblinBrawler, death_spawn_count=1

def test_goblin_cage_death_spawn():
    print("\n" + "="*60)
    print("TEST 56: Goblin Cage death → Goblin Brawler")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    cage_id = m.spawn_building(1, "goblin-cage", 0, -5000)

    # Spawn enemies to destroy the cage
    for i in range(4):
        m.spawn_troop(2, "knight", (i - 2) * 200, -4500)

    cage_died = False
    for t in range(600):
        m.step()
        e = find_entity(m, cage_id)
        if e is None or (e and not e["alive"]):
            cage_died = True
            print(f"\n  Goblin Cage destroyed at tick {t+1}")
            break

    # Let death spawn process
    for _ in range(5):
        m.step()

    p1_troops = find_alive(m, "troop", team=1)
    brawler_count = sum(1 for t in p1_troops if "goblinbrawler" in t["card_key"].lower())
    print(f"  P1 troops after cage death: {len(p1_troops)}  keys={[t['card_key'] for t in p1_troops]}")

    check("Goblin Cage was destroyed", cage_died)
    check("Goblin Brawler spawned on death", brawler_count > 0,
          f"no brawler found, troops={[t['card_key'] for t in p1_troops]}")


# =========================================================================
# TEST 57: Building lifetime — all buildings expire
# =========================================================================
# Cannon: life_time=30000ms = 600 ticks. Should die on its own.

def test_cannon_lifetime():
    print("\n" + "="*60)
    print("TEST 57: Cannon expires after lifetime (30s)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    cannon_id = m.spawn_building(1, "cannon", 0, -5000)

    alive_at = {}
    for t in range(700):
        m.step()
        e = find_entity(m, cannon_id)
        if (t + 1) % 200 == 0:
            is_alive = e is not None and e["alive"] if e else False
            alive_at[t + 1] = is_alive
            print(f"  tick {t+1}: alive={is_alive}")
        if e is None or (e and not e["alive"]):
            print(f"  Cannon expired at tick {t+1} ({(t+1)/20:.1f}s)")
            break

    expired = e is None or (e is not None and not e["alive"])
    check("Cannon expired", expired, "Still alive after 700 ticks")
    if expired:
        lifetime = (t + 1) / 20.0
        check("Lifetime ~30s (25-35s)", 25 <= lifetime <= 35,
              f"lasted {lifetime:.1f}s")


# =========================================================================
# TEST 58: Barbarian Hut death spawn — releases Barbarian
# =========================================================================
# Barbarian Hut: death_spawn_character=Barbarian, death_spawn_count=1

def test_barbarian_hut_death_spawn():
    print("\n" + "="*60)
    print("TEST 58: Barbarian Hut death → spawns Barbarian")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    hut_id = m.spawn_building(1, "barbarian-hut", 0, -5000)

    # Kill it with many troops
    for i in range(6):
        m.spawn_troop(2, "knight", (i - 3) * 200, -4500)

    hut_died = False
    for t in range(800):
        m.step()
        e = find_entity(m, hut_id)
        if e is None or (e and not e["alive"]):
            hut_died = True
            print(f"\n  Barbarian Hut destroyed at tick {t+1}")
            break

    for _ in range(5):
        m.step()

    p1_troops = find_alive(m, "troop", team=1)
    barb_count = sum(1 for t in p1_troops if "barbarian" in t["card_key"].lower())
    print(f"  P1 troops after hut death: {len(p1_troops)}  barbarians={barb_count}")

    check("Barbarian Hut was destroyed", hut_died)
    # Barbs from spawner waves + death spawn should exist
    check("Barbarians present (from spawn waves or death)", barb_count > 0,
          "No barbarians found at all")


# =========================================================================
# TEST 59: Inferno Tower deals damage (melee building)
# =========================================================================
# Inferno Tower: damage=20, hit_speed=400ms, range=6000, attacks_air=true

def test_inferno_tower_deals_damage():
    print("\n" + "="*60)
    print("TEST 59: Inferno Tower deals damage")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    inferno_id = m.spawn_building(1, "inferno-tower", 0, -5000)
    golem_id = m.spawn_troop(2, "golem", 0, -1000)

    for _ in range(50):
        m.step()

    golem_hp_before = find_entity(m, golem_id)["hp"]

    for _ in range(200):
        m.step()

    e_golem = find_entity(m, golem_id)
    golem_hp_after = e_golem["hp"] if e_golem else 0
    damage_dealt = golem_hp_before - golem_hp_after

    print(f"\n  Golem HP: {golem_hp_before} → {golem_hp_after} (damage={damage_dealt})")

    check("Inferno Tower dealt damage to Golem", damage_dealt > 0,
          "0 damage — Inferno Tower might not be attacking")
    check("Significant damage dealt (> 500 in 200 ticks)",
          damage_dealt > 500,
          f"only {damage_dealt} damage")


# =========================================================================
# TEST 60: X-Bow long range attacks enemy tower
# =========================================================================
# X-Bow: range=11500, attacks_ground=true, hits tower from far away.

def test_xbow_attacks_tower():
    print("\n" + "="*60)
    print("TEST 60: X-Bow attacks enemy tower from long range")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    # Place X-Bow at bridge — should reach P2 princess tower
    xbow_id = m.spawn_building(1, "x-bow", -5100, -1000)

    for _ in range(50):
        m.step()

    p2_towers_before = m.p2_tower_hp()
    princess_hp_before = p2_towers_before[1]  # left princess

    for _ in range(300):
        m.step()

    p2_towers_after = m.p2_tower_hp()
    princess_hp_after = p2_towers_after[1]
    tower_damage = princess_hp_before - princess_hp_after

    print(f"\n  P2 left princess HP: {princess_hp_before} → {princess_hp_after}")
    print(f"  X-Bow damage to tower: {tower_damage}")

    check("X-Bow damaged enemy princess tower", tower_damage > 0,
          "0 damage — X-Bow might not reach or not target towers")


# =========================================================================
# TEST 61: Building takes damage from troops
# =========================================================================
# A P2 Knight should attack and damage a P1 Tesla.

def test_building_takes_damage():
    print("\n" + "="*60)
    print("TEST 61: Building takes damage from enemy troops")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    tesla_id = m.spawn_building(1, "tesla", 0, 0)
    knight_id = m.spawn_troop(2, "knight", 0, 500)

    for _ in range(30):
        m.step()

    tesla_hp_before = find_entity(m, tesla_id)["hp"]

    for _ in range(100):
        m.step()

    e_tesla = find_entity(m, tesla_id)
    tesla_hp_after = e_tesla["hp"] if e_tesla else 0
    damage = tesla_hp_before - tesla_hp_after

    print(f"\n  Tesla HP: {tesla_hp_before} → {tesla_hp_after} (damage={damage})")

    check("Knight damaged the Tesla building", damage > 0,
          "0 damage — troops might not be able to target buildings")


# =========================================================================
# TEST 62: Multiple buildings coexist without interference
# =========================================================================
# Spawn Tesla + Cannon on same side. Both should function independently.

def test_multiple_buildings():
    print("\n" + "="*60)
    print("TEST 62: Multiple buildings coexist")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    tesla_id = m.spawn_building(1, "tesla", -2000, -5000)
    cannon_id = m.spawn_building(1, "cannon", 2000, -5000)

    # Send enemies toward both
    m.spawn_troop(2, "knight", -2000, 0)
    m.spawn_troop(2, "knight", 2000, 0)

    for _ in range(200):
        m.step()

    e_tesla = find_entity(m, tesla_id)
    e_cannon = find_entity(m, cannon_id)

    tesla_alive = e_tesla is not None and e_tesla["alive"]
    cannon_alive = e_cannon is not None and e_cannon["alive"]

    print(f"\n  Tesla alive: {tesla_alive}")
    print(f"  Cannon alive: {cannon_alive}")

    # Both should still exist (they have 30s lifetime)
    check("Tesla still alive after 200 ticks", tesla_alive)
    check("Cannon still alive after 200 ticks", cannon_alive)

    # Check that the approaching Knights took damage from buildings
    p2_troops = find_alive(m, "troop", team=2)
    knights_damaged = sum(1 for t in p2_troops if t["hp"] < 1766)
    total_p2 = len(p2_troops)
    print(f"  P2 Knights remaining: {total_p2}  damaged: {knights_damaged}")

    check("At least one P2 Knight took building damage",
          knights_damaged > 0 or total_p2 < 2,
          "No knights damaged — buildings might not be attacking")


# =========================================================================
# TEST 63: Tombstone death spawn — releases 4 skeletons on death
# =========================================================================
# Tombstone: death_spawn_character=Skeleton, death_spawn_count=4

def test_tombstone_death_spawn():
    print("\n" + "="*60)
    print("TEST 63: Tombstone death → 4 skeletons")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    ts_id = m.spawn_building(1, "tombstone", 0, -5000)

    # Kill it quickly
    for i in range(4):
        m.spawn_troop(2, "knight", (i - 2) * 200, -4500)

    ts_died = False
    for t in range(400):
        m.step()
        e = find_entity(m, ts_id)
        if e is None or (e and not e["alive"]):
            ts_died = True
            print(f"\n  Tombstone destroyed at tick {t+1}")
            break

    for _ in range(5):
        m.step()

    p1_troops = find_alive(m, "troop", team=1)
    skel_count = sum(1 for t in p1_troops if "skeleton" in t["card_key"].lower())
    print(f"  P1 troops after tombstone death: {len(p1_troops)}  skeletons={skel_count}")

    check("Tombstone was destroyed", ts_died)
    # Death spawn = 4 skeletons, plus some from periodic spawning
    check("Skeletons present after death", skel_count > 0,
          "No skeletons found — death spawn might not work for buildings")


# =========================================================================
# TEST 64: Building with ranged attack creates projectiles
# =========================================================================
# Cannon fires projectiles (TowerCannonball). Check projectile entity exists.

def test_building_projectile():
    print("\n" + "="*60)
    print("TEST 64: Building ranged attack creates projectile")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    cannon_id = m.spawn_building(1, "cannon", 0, -5000)
    # Knight within cannon range
    knight_id = m.spawn_troop(2, "knight", 0, -1000)

    projectile_seen = False
    for t in range(100):
        m.step()
        projs = [e for e in m.get_entities() if e["kind"] == "projectile" and e["team"] == 1]
        if projs and not projectile_seen:
            projectile_seen = True
            print(f"\n  Cannon projectile spawned at tick {t+1}")
            break

    check("Cannon created a projectile entity", projectile_seen,
          "No projectile — cannon might be melee or not attacking")


# =========================================================================
# TEST 65: Poison spell applies DOT via buff
# =========================================================================
# Poison: damage=0 in spell, but buff has damage_per_second=57.
# The buff system should apply damage_per_second as periodic damage.
# Poison also has hit_speed=250ms = 5 ticks between zone ticks.

def test_poison_dot_via_buff():
    print("\n" + "="*60)
    print("TEST 65: Poison applies damage over time")
    print("="*60)

    try:
        m = cr_engine.new_match(data, ["poison"] * 8, DUMMY_DECK)
        golem_id = m.spawn_troop(2, "golem", 0, 0)
        for _ in range(100):
            m.step()

        hp_before = find_entity(m, golem_id)["hp"]
        print(f"\n  Golem HP before Poison: {hp_before}")

        # Play Poison on the Golem
        m.play_card(1, 0, 0, 0)

        # Track HP over Poison duration (8000ms = 160 ticks)
        hp_log = {}
        for t in range(200):
            m.step()
            e = find_entity(m, golem_id)
            if e and (t + 1) % 40 == 0:
                hp_log[t + 1] = e["hp"]

        for tick, hp in sorted(hp_log.items()):
            print(f"  tick {tick}: HP={hp}  (damage so far: {hp_before - hp})")

        total_damage = hp_before - list(hp_log.values())[-1] if hp_log else 0
        print(f"  Total Poison damage: {total_damage}")

        check("Poison dealt damage over time", total_damage > 0,
              f"total_damage={total_damage} — Poison buff damage_per_second might not be implemented")

        # Check progressive damage
        damages = [hp_before - hp for hp in hp_log.values()]
        if len(damages) >= 3:
            progressive = damages[0] < damages[1] < damages[2]
            check("Poison damage was progressive", progressive,
                  f"damages at intervals: {damages[:4]}")
    except Exception as e:
        print(f"  Poison test failed: {e}")
        check("Poison spell deployable", False, str(e))


# =========================================================================
# TEST 66: Poison slows enemy movement
# =========================================================================
# Poison buff: speed_multiplier=-15 → 15% slow.
# Compare movement with and without Poison.

def test_poison_slows():
    print("\n" + "="*60)
    print("TEST 66: Poison slows enemy movement")
    print("="*60)

    try:
        # Normal movement
        m_a = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
        k_a = m_a.spawn_troop(2, "knight", 0, 5000)
        for _ in range(50):
            m_a.step()
        y_start_a = find_entity(m_a, k_a)["y"]
        for _ in range(60):
            m_a.step()
        y_end_a = find_entity(m_a, k_a)["y"]
        normal_move = abs(y_end_a - y_start_a)

        # Poisoned movement
        m_b = cr_engine.new_match(data, ["poison"] * 8, DUMMY_DECK)
        k_b = m_b.spawn_troop(2, "knight", 0, 5000)
        for _ in range(50):
            m_b.step()
        y_start_b = find_entity(m_b, k_b)["y"]

        # Deploy Poison on the Knight
        m_b.play_card(1, 0, 0, y_start_b)

        for _ in range(5):
            m_b.step()  # let poison apply

        y_after_poison = find_entity(m_b, k_b)["y"]
        for _ in range(60):
            m_b.step()
        y_end_b = find_entity(m_b, k_b)["y"]
        poisoned_move = abs(y_end_b - y_after_poison)

        print(f"\n  Normal movement in 60 ticks: {normal_move}")
        print(f"  Poisoned movement in 60 ticks: {poisoned_move}")

        check("Knight moved normally (> 500)", normal_move > 500,
              f"only {normal_move}")

        if normal_move > 0 and poisoned_move > 0:
            ratio = poisoned_move / normal_move
            print(f"  Slow ratio: {ratio:.2f} (expected ~0.85 for -15% slow)")

            check("Poison slowed movement (ratio < 1.0)",
                  poisoned_move < normal_move,
                  f"poisoned={poisoned_move} >= normal={normal_move}")
        else:
            check("Both movements measurable", False,
                  f"normal={normal_move} poisoned={poisoned_move}")
    except Exception as e:
        print(f"  Poison test failed: {e}")
        check("Poison spell deployable", False, str(e))


# =========================================================================
# Run all tests
# =========================================================================

if __name__ == "__main__":
    print("="*60)
    print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 7")
    print("  Tests 53-66: buildings + poison")
    print("="*60)

    test_tesla_attacks_air_and_ground()
    test_cannon_ground_only()
    test_goblin_hut_spawner()
    test_goblin_cage_death_spawn()
    test_cannon_lifetime()
    test_barbarian_hut_death_spawn()
    test_inferno_tower_deals_damage()
    test_xbow_attacks_tower()
    test_building_takes_damage()
    test_multiple_buildings()
    test_tombstone_death_spawn()
    test_building_projectile()
    test_poison_dot_via_buff()
    test_poison_slows()

    print("\n" + "="*60)
    total = PASS + FAIL
    print(f"  RESULTS: {PASS}/{total} passed, {FAIL}/{total} failed")
    print("="*60)

    if FAIL > 0:
        print("\n  Building and Poison failures point to specific subsystems.")
        sys.exit(1)
    else:
        print("\n  All building & poison tests passed!")
        sys.exit(0)
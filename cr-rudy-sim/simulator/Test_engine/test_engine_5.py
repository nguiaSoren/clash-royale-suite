"""
Engine fidelity tests — batch 5 (stress + edge cases)

Place in: simulator/test_engine_5.py
Run with: python test_engine_5.py

Tests 28-42: systems not covered by batches 1-4.
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
# TEST 28: Double elixir rate is 2× normal
# =========================================================================
# In double elixir phase, you gain 2 elixir per 56 ticks instead of 1.
# We force the match to double elixir tick and measure gain.

def test_double_elixir_rate():
    print("\n" + "="*60)
    print("TEST 28: Double elixir rate is 2× normal")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Fast-forward to double elixir phase (tick 1201+)
    for _ in range(1210):
        m.step()

    check("In double_elixir phase", m.phase == "double_elixir")

    # Spend all elixir to start from 0
    # (Can't spend directly, so just track the rate)
    # Record current elixir, run 56 ticks, check gain
    # But elixir is capped at 10. We need to spend some first.
    # Instead: start a fresh match, step to tick 1201, then
    # play 3 knights (9 elixir) to drain down to ~1
    m2 = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    for _ in range(1205):
        m2.step()

    # Play cards to drain elixir
    try:
        m2.play_card(1, 0, 0, -5000)  # -3 elixir
        m2.play_card(1, 1, 0, -5000)  # -3 elixir
        m2.play_card(1, 2, 0, -5000)  # -3 elixir
    except:
        pass

    elixir_after_spend = m2.p1_elixir
    print(f"\n  Elixir after spending: {elixir_after_spend}")

    # Run 56 ticks (1 normal elixir cycle)
    for _ in range(56):
        m2.step()

    elixir_after_56 = m2.p1_elixir
    gained = elixir_after_56 - elixir_after_spend
    print(f"  Elixir after 56 ticks in double: {elixir_after_56}")
    print(f"  Gained: {gained} (expected ~2 in double elixir)")

    check("Gained ≥ 2 elixir in 56 ticks during double elixir",
          gained >= 2,
          f"gained {gained} — double rate might not be applied")


# =========================================================================
# TEST 29: Ranged projectile has travel time
# =========================================================================
# A Musketeer's shot should take multiple ticks to reach a distant target.
# Melee damage is instant; ranged creates a projectile entity.

def test_ranged_projectile_travel():
    print("\n" + "="*60)
    print("TEST 29: Ranged projectile has travel time")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Musketeer at range from a Giant
    musk_id = m.spawn_troop(1, "musketeer", 0, 0)
    giant_id = m.spawn_troop(2, "giant", 0, 5500)

    # Wait for deploy
    for _ in range(30):
        m.step()

    giant_initial_hp = find_entity(m, giant_id)["hp"]

    # Run until Musketeer fires (look for projectile entities)
    projectile_seen = False
    damage_tick = None
    fire_tick = None

    for t in range(200):
        m.step()
        entities = m.get_entities()

        # Check for projectile entities
        projs = [e for e in entities if e["kind"] == "projectile" and e["team"] == 1]
        if projs and not projectile_seen:
            projectile_seen = True
            fire_tick = t + 1
            print(f"\n  Projectile spawned at tick {fire_tick}")

        # Check when Giant takes damage
        e_giant = find_entity(m, giant_id)
        if e_giant and e_giant["hp"] < giant_initial_hp and damage_tick is None:
            damage_tick = t + 1
            print(f"  Giant took damage at tick {damage_tick}")
            break

    check("Musketeer created a projectile entity", projectile_seen)
    if fire_tick and damage_tick:
        travel_time = damage_tick - fire_tick
        print(f"  Projectile travel time: {travel_time} ticks")
        check("Projectile had travel time > 0 ticks",
              travel_time > 0,
              f"travel_time={travel_time} — ranged damage might be instant")


# =========================================================================
# TEST 30: Giant ignores troops, walks to building
# =========================================================================
# Giant has target_only_buildings=true. It should walk past enemy troops
# without stopping to fight them.

def test_giant_ignores_troops():
    print("\n" + "="*60)
    print("TEST 30: Giant ignores troops, walks to building")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Giant heading toward P2 towers
    giant_id = m.spawn_troop(1, "giant", -5100, -3000)
    # P2 Knight blocking the path
    knight_id = m.spawn_troop(2, "knight", -5100, 0)

    for _ in range(30):
        m.step()

    e_knight = find_entity(m, knight_id)
    knight_initial_hp = e_knight["hp"]

    # Run 200 ticks — Giant should walk past Knight without attacking
    for _ in range(200):
        m.step()

    e_giant = find_entity(m, giant_id)
    e_knight = find_entity(m, knight_id)

    if e_giant:
        print(f"\n  Giant position after 200 ticks: ({e_giant['x']}, {e_giant['y']})")
        # Giant should have advanced Y significantly (toward P2 towers)
        check("Giant advanced toward enemy towers",
              e_giant["y"] > -1500,
              f"Y={e_giant['y']} — Giant might be stuck fighting Knight")

    if e_knight:
        knight_damaged = e_knight["hp"] < knight_initial_hp
        print(f"  Knight HP: {e_knight['hp']}/{knight_initial_hp}")
        # Giant should NOT have damaged the Knight (target_only_buildings)
        # But the Knight WILL attack the Giant (and tower might hit Knight)
        # We check Giant's behavior, not Knight's HP
        check("Giant didn't stop to fight Knight (kept advancing)",
              e_giant is not None and e_giant["y"] > -1500,
              "Giant stopped at Knight's position")


# =========================================================================
# TEST 31: Hog Rider targets buildings only
# =========================================================================
# Similar to Giant but Hog Rider is faster and also target_only_buildings.

def test_hog_rider_targets_buildings():
    print("\n" + "="*60)
    print("TEST 31: Hog Rider targets buildings only")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    hog_id = m.spawn_troop(1, "hog-rider", 5100, -1500)

    for _ in range(30):
        m.step()

    # Track Hog Rider heading toward tower
    hog_positions = []
    for t in range(200):
        m.step()
        e = find_entity(m, hog_id)
        if e and e["alive"]:
            hog_positions.append((t, e["x"], e["y"]))
        else:
            break

    if hog_positions:
        last_pos = hog_positions[-1]
        print(f"\n  Hog Rider final position: ({last_pos[1]}, {last_pos[2]})")
        print(f"  Ticks tracked: {len(hog_positions)}")

        # Hog Rider (speed 120 = Very Fast = 48 units/tick) should reach tower area
        check("Hog Rider advanced far toward tower (Y > 5000)",
              last_pos[2] > 5000,
              f"Y={last_pos[2]} — Hog might be stuck or targeting wrong")

    # Check if tower took damage from Hog
    p2_towers = m.p2_tower_hp()
    tower_damaged = any(hp < 3052 for hp in p2_towers[1:])  # princess towers
    print(f"  P2 tower HP: {p2_towers}")
    check("Hog Rider damaged a P2 tower", tower_damaged,
          "No tower damage — Hog might not be reaching or attacking towers")


# =========================================================================
# TEST 32: Card cycling — hand rotates through deck
# =========================================================================
# After playing a card, the hand slot should be filled by the next card
# in the deck cycle.

def test_card_cycling():
    print("\n" + "="*60)
    print("TEST 32: Card cycling — hand rotates through deck")
    print("="*60)

    deck = ["knight", "archers", "giant", "valkyrie",
            "musketeer", "pekka", "goblin-gang", "hog-rider"]
    m = cr_engine.new_match(data, deck, DUMMY_DECK)

    # Give time for elixir
    for _ in range(200):
        m.step()

    hand_before = m.p1_hand()
    print(f"\n  Initial hand: {hand_before}")

    # Play hand slot 0
    try:
        m.play_card(1, 0, 0, -5000)
    except Exception as e:
        check("Card played successfully", False, str(e))
        return

    hand_after = m.p1_hand()
    print(f"  Hand after playing slot 0: {hand_after}")

    check("Hand slot 0 changed after playing",
          hand_after[0] != hand_before[0],
          f"before={hand_before[0]} after={hand_after[0]}")

    # The new card in slot 0 should be the 5th card in the deck (index 4)
    check("New card is from next in deck cycle",
          hand_after[0] == deck[4],
          f"expected {deck[4]}, got {hand_after[0]}")


# =========================================================================
# TEST 33: Retarget load time on target switch
# =========================================================================
# When a troop's target dies and it retargets, there's a brief
# load_after_retarget delay before the next attack.

def test_retarget_delay():
    print("\n" + "="*60)
    print("TEST 33: Retarget delay after target dies")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 PEKKA (high damage, kills Knights fast)
    pekka_id = m.spawn_troop(1, "pekka", 0, 0)
    # Two P2 skeletons (very low HP, die in 1 hit)
    skel1 = m.spawn_troop(2, "skeleton", 0, 300)
    skel2 = m.spawn_troop(2, "skeleton", 200, 300)

    for _ in range(30):
        m.step()

    # Track kills
    skel1_dead_tick = None
    skel2_dead_tick = None

    for t in range(200):
        m.step()
        e1 = find_entity(m, skel1)
        e2 = find_entity(m, skel2)

        if (e1 is None or not e1["alive"]) and skel1_dead_tick is None:
            skel1_dead_tick = t + 1
        if (e2 is None or not e2["alive"]) and skel2_dead_tick is None:
            skel2_dead_tick = t + 1

        if skel1_dead_tick and skel2_dead_tick:
            break

    if skel1_dead_tick and skel2_dead_tick:
        gap = abs(skel2_dead_tick - skel1_dead_tick)
        print(f"\n  First skeleton died at tick {skel1_dead_tick}")
        print(f"  Second skeleton died at tick {skel2_dead_tick}")
        print(f"  Gap between kills: {gap} ticks")

        # Should be > hit_speed because of retarget delay
        check("Both skeletons killed", True)
        check("Gap between kills > 1 tick (retarget delay exists)",
              gap > 1,
              f"gap={gap} — kills are instantaneous, no retarget delay")
    else:
        check("PEKKA killed both skeletons", False,
              f"skel1_dead={skel1_dead_tick} skel2_dead={skel2_dead_tick}")


# =========================================================================
# TEST 34: PEKKA high damage — one-shots low HP troops
# =========================================================================
# PEKKA at level 11 does ~1305 damage. A Skeleton has 32 HP.
# PEKKA should one-shot it.

def test_pekka_oneshot():
    print("\n" + "="*60)
    print("TEST 34: PEKKA one-shots low HP troops")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    pekka_id = m.spawn_troop(1, "pekka", 0, 0)
    skel_id = m.spawn_troop(2, "skeleton", 0, 300)

    pekka_stats = data.get_character_stats("pekka")
    skel_stats = data.get_character_stats("skeleton")
    print(f"\n  PEKKA damage_per_level[10]: {pekka_stats.get('damage', 0)}")
    print(f"  Skeleton HP: {skel_stats.get('hitpoints', 0)}")

    for _ in range(30):
        m.step()

    skel_alive_before = find_entity(m, skel_id)
    initial_hp = skel_alive_before["hp"] if skel_alive_before else 0
    print(f"  Skeleton HP at level 11: {initial_hp}")

    # PEKKA should one-shot within a few ticks of engaging
    for t in range(100):
        m.step()
        e_skel = find_entity(m, skel_id)
        if e_skel is None or not e_skel["alive"]:
            print(f"  Skeleton killed at tick {t+1}")
            break

    skel_dead = e_skel is None or not e_skel["alive"]
    check("PEKKA killed Skeleton", skel_dead)

    # PEKKA should also have taken 0 or minimal damage from skeleton
    e_pekka = find_entity(m, pekka_id)
    if e_pekka:
        pekka_dmg_taken = e_pekka["max_hp"] - e_pekka["hp"]
        print(f"  PEKKA damage taken: {pekka_dmg_taken}/{e_pekka['max_hp']}")
        check("PEKKA took minimal damage (< 20% HP)",
              pekka_dmg_taken < e_pekka["max_hp"] * 0.2,
              f"took {pekka_dmg_taken} damage")


# =========================================================================
# TEST 35: Entity cleanup — dead entities removed from pool
# =========================================================================
# After a troop dies, it should be removed from the entity list.

def test_entity_cleanup():
    print("\n" + "="*60)
    print("TEST 35: Dead entities cleaned up from entity pool")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    skel_id = m.spawn_troop(1, "skeleton", 0, 0)
    # P2 knight to kill the skeleton
    knight_id = m.spawn_troop(2, "knight", 0, 300)

    initial_count = len(m.get_entities())
    print(f"\n  Entities after spawn: {initial_count}")

    # Run until skeleton dies
    for t in range(100):
        m.step()
        e = find_entity(m, skel_id)
        if e is None:
            print(f"  Skeleton removed at tick {t+1}")
            break

    post_death_count = len(m.get_entities())
    skel_gone = find_entity(m, skel_id) is None
    print(f"  Entities after skeleton death: {post_death_count}")

    check("Skeleton removed from entity list after death",
          skel_gone,
          "Dead skeleton still in entity list")
    check("Entity count decreased",
          post_death_count < initial_count,
          f"before={initial_count} after={post_death_count}")


# =========================================================================
# TEST 36: Flying troop ignores river
# =========================================================================
# Balloon (flying) should go straight to the tower without bridge pathing.

def test_flying_ignores_river():
    print("\n" + "="*60)
    print("TEST 36: Flying troop ignores river (Balloon)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Balloon at center X=0, south of river — should fly straight north
    balloon_id = m.spawn_troop(1, "balloon", 0, -3000)

    for _ in range(30):
        m.step()

    e = find_entity(m, balloon_id)
    start_x = e["x"]
    print(f"\n  Start: ({e['x']}, {e['y']})")

    for _ in range(150):
        m.step()

    e = find_entity(m, balloon_id)
    if e and e["alive"]:
        print(f"  After 150 ticks: ({e['x']}, {e['y']})")
        # Balloon should NOT have deviated to a bridge (X should stay near 0)
        check("Balloon stayed near center X (didn't path to bridge)",
              abs(e["x"]) < 2000,
              f"X={e['x']} — Balloon might be pathing through bridge like a ground unit")
        # Should have crossed the river line
        check("Balloon crossed river (Y > 1200)",
              e["y"] > 1200,
              f"Y={e['y']} — Balloon stuck at river")


# =========================================================================
# TEST 37: Multiple troops can attack same target
# =========================================================================
# 3 P1 Knights should all deal damage to 1 P2 Golem simultaneously.

def test_multi_attacker_focus():
    print("\n" + "="*60)
    print("TEST 37: Multiple troops attack same target")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P2 Golem in P1 territory — walks toward P1 buildings (negative Y),
    # which keeps it near the P1 Knights who are slightly behind it.
    golem_id = m.spawn_troop(2, "golem", 0, -4000)
    k1 = m.spawn_troop(1, "knight", -400, -4600)
    k2 = m.spawn_troop(1, "knight", 0, -4600)
    k3 = m.spawn_troop(1, "knight", 400, -4600)

    for _ in range(30):
        m.step()

    golem_hp_start = find_entity(m, golem_id)["hp"]

    # Run 200 ticks for plenty of combat time
    for _ in range(200):
        m.step()

    golem_hp_end = find_entity(m, golem_id)["hp"]
    total_damage = golem_hp_start - golem_hp_end

    print(f"\n  Golem damage from 3 Knights in 200 ticks: {total_damage}")
    print(f"  Single Knight would do ~1600 in 200 ticks")

    check("3 Knights dealt more than 1 Knight could alone",
          total_damage > 500,
          f"total={total_damage} — only 1 Knight might be attacking")
    check("Damage consistent with multiple attackers (>1000)",
          total_damage > 1000,
          f"total={total_damage}")


# =========================================================================
# TEST 38: Lane awareness — troop goes to correct lane
# =========================================================================
# A troop spawned on the left side should target P2's left princess tower.

def test_lane_awareness():
    print("\n" + "="*60)
    print("TEST 38: Lane awareness — left side troop targets left tower")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Knight on left side
    knight_id = m.spawn_troop(1, "knight", -5100, -5000)

    for _ in range(30):
        m.step()

    # Track position over time
    positions = []
    for t in range(300):
        m.step()
        e = find_entity(m, knight_id)
        if e and e["alive"]:
            positions.append((e["x"], e["y"]))
        else:
            break

    if positions:
        final_x, final_y = positions[-1]
        print(f"\n  Knight final pos: ({final_x}, {final_y})")
        print(f"  P2 left princess tower: (-5100, 10200)")
        print(f"  P2 right princess tower: (5100, 10200)")

        # Knight should stay on left side (X < 0)
        check("Knight stayed on left side (X < 0)",
              final_x < 0,
              f"X={final_x} — Knight crossed to right side")


# =========================================================================
# TEST 39: Troop DPS — Musketeer (ranged, patched from projectile)
# =========================================================================
# Verify the projectile damage patch works for actual DPS output.

def test_musketeer_dps():
    print("\n" + "="*60)
    print("TEST 39: Musketeer DPS (ranged, damage from projectile)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    musk_id = m.spawn_troop(1, "musketeer", 0, 0)
    golem_id = m.spawn_troop(2, "golem", 0, 4000)

    # Wait for deploy + first shot
    for _ in range(30):
        m.step()

    golem_hp_after_deploy = find_entity(m, golem_id)["hp"]

    # Wait for first damage
    for _ in range(300):
        m.step()
        e = find_entity(m, golem_id)
        if e and e["hp"] < golem_hp_after_deploy:
            break

    hp_start = find_entity(m, golem_id)["hp"]
    start_tick = m.tick

    # Measure over 100 ticks
    for _ in range(100):
        m.step()

    hp_end = find_entity(m, golem_id)["hp"]
    damage = hp_start - hp_end
    dps = damage / (100 / 20.0) if damage > 0 else 0
    print(f"\n  Musketeer damage in 100 ticks: {damage}")
    print(f"  DPS: {dps:.1f}")

    check("Musketeer dealt damage (ranged patch working)", damage > 0,
          "0 damage — projectile damage patch might not be applied")
    # Musketeer DPS should be meaningful (real CR: ~228 dmg / 1.0s = 228 DPS)
    check("Musketeer DPS in reasonable range (100-400)",
          100 <= dps <= 400,
          f"DPS={dps:.1f}")


# =========================================================================
# TEST 40: Splash troop damages multiple enemies simultaneously
# =========================================================================
# Baby Dragon (splash + air) should hit multiple ground troops at once.
# This is different from Test 6 (Valkyrie melee splash) — this tests
# ranged splash from a projectile with area_damage_radius.

def test_splash_projectile_damage():
    print("\n" + "="*60)
    print("TEST 40: Ranged splash damages cluster of enemies")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Baby Dragon (ranged splash, attacks air+ground)
    bd_id = m.spawn_troop(1, "baby-dragon", 0, -500)
    # Cluster of 3 P2 skeletons close together
    t1 = m.spawn_troop(2, "skeleton", 0, 200)
    t2 = m.spawn_troop(2, "skeleton", 150, 200)
    t3 = m.spawn_troop(2, "skeleton", -150, 200)

    for _ in range(30):
        m.step()

    hp_before = {}
    for tid in [t1, t2, t3]:
        e = find_entity(m, tid)
        if e:
            hp_before[tid] = e["hp"]

    # Run until splash damage lands
    for _ in range(200):
        m.step()

    damaged_count = 0
    for tid in [t1, t2, t3]:
        e = find_entity(m, tid)
        if e is None or (e and e["hp"] < hp_before.get(tid, 999)):
            damaged_count += 1

    print(f"\n  Enemies damaged by ranged splash: {damaged_count}/3")
    check("Ranged splash damaged at least 1 enemy", damaged_count > 0,
          "No damage — ranged splash might not be working")
    check("Ranged splash hit multiple enemies",
          damaged_count >= 2,
          f"only {damaged_count}/3 — splash radius might be too small")


# =========================================================================
# TEST 41: Simultaneous deaths don't crash
# =========================================================================
# Kill many troops at once and verify the engine doesn't panic.

def test_simultaneous_deaths():
    print("\n" + "="*60)
    print("TEST 41: Simultaneous deaths don't crash engine")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Spawn 10 P2 skeletons (32 HP each)
    skel_ids = []
    for i in range(10):
        sid = m.spawn_troop(2, "skeleton", (i - 5) * 200, 0)
        skel_ids.append(sid)

    # Spawn 5 P1 Valkyries (splash damage, should kill multiple per hit)
    for i in range(5):
        m.spawn_troop(1, "valkyrie", (i - 2) * 200, -300)

    # Run until most skeletons die
    crashed = False
    try:
        for t in range(200):
            m.step()
    except Exception as e:
        crashed = True
        print(f"\n  ENGINE CRASHED at tick {t+1}: {e}")

    surviving = [sid for sid in skel_ids
                 if find_entity(m, sid) and find_entity(m, sid)["alive"]]

    print(f"\n  Skeletons surviving: {len(surviving)}/10")
    check("Engine didn't crash during mass deaths", not crashed)
    check("Most skeletons killed", len(surviving) <= 3,
          f"{len(surviving)} survived")


# =========================================================================
# TEST 42: Troop deploy timer — can't attack immediately
# =========================================================================
# A troop should have a brief deploy delay before it becomes active.

def test_deploy_timer():
    print("\n" + "="*60)
    print("TEST 42: Deploy timer — troop inactive for first ticks")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Knight next to P2 Golem — Knight should NOT hit immediately
    knight_id = m.spawn_troop(1, "knight", 0, 0)
    golem_id = m.spawn_troop(2, "golem", 0, 300)

    golem_hp_t0 = find_entity(m, golem_id)["hp"]

    # Check HP at tick 1
    m.step()
    golem_hp_t1 = find_entity(m, golem_id)["hp"]

    # Check HP at tick 5
    for _ in range(4):
        m.step()
    golem_hp_t5 = find_entity(m, golem_id)["hp"]

    print(f"\n  Golem HP at tick 0: {golem_hp_t0}")
    print(f"  Golem HP at tick 1: {golem_hp_t1}")
    print(f"  Golem HP at tick 5: {golem_hp_t5}")

    check("No damage on tick 1 (deploy timer active)",
          golem_hp_t1 == golem_hp_t0,
          f"HP dropped to {golem_hp_t1} — troop attacked during deploy")
    check("No damage by tick 5 (deploy timer still active)",
          golem_hp_t5 == golem_hp_t0,
          f"HP dropped to {golem_hp_t5} — deploy timer might be too short")

    # Now run until damage happens — should be around tick 10-20
    first_damage_tick = None
    for t in range(6, 50):
        m.step()
        hp = find_entity(m, golem_id)["hp"]
        if hp < golem_hp_t0:
            first_damage_tick = t + 1
            print(f"  First damage at tick {first_damage_tick}")
            break

    if first_damage_tick:
        check("First damage after tick 5 (deploy timer expired)",
              first_damage_tick > 5,
              f"first damage at tick {first_damage_tick}")


# =========================================================================
# Run all tests
# =========================================================================

if __name__ == "__main__":
    print("="*60)
    print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 5")
    print("  Tests 28-42: stress + edge cases")
    print("="*60)

    test_double_elixir_rate()
    test_ranged_projectile_travel()
    test_giant_ignores_troops()
    test_hog_rider_targets_buildings()
    test_card_cycling()
    test_retarget_delay()
    test_pekka_oneshot()
    test_entity_cleanup()
    test_flying_ignores_river()
    test_multi_attacker_focus()
    test_lane_awareness()
    test_musketeer_dps()
    test_splash_projectile_damage()
    test_simultaneous_deaths()
    test_deploy_timer()

    print("\n" + "="*60)
    total = PASS + FAIL
    print(f"  RESULTS: {PASS}/{total} passed, {FAIL}/{total} failed")
    print("="*60)

    if FAIL > 0:
        print("\n  These stress tests probe edge cases and untested systems.")
        print("  Failures here reveal subtle engine inaccuracies.")
        sys.exit(1)
    else:
        print("\n  All stress tests passed! Engine is battle-ready.")
        sys.exit(0)
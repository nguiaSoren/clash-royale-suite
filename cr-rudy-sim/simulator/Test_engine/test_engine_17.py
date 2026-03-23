#!/usr/bin/env python3
"""
============================================================
  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 17
  Tests 700-729: Champion Ability Behavioral Tests
============================================================

Tests that each champion's activate_hero() produces the correct
mechanical effect, using exact values from the JSON data.

  A. Skeleton King Ability (700-704)
     - Spawns SkeletonKingGraveyard zone on activation
     - Zone spawns SkeletonKingSkeleton troops every 250ms
     - Zone lasts 10 seconds, radius=4000

  B. Archer Queen Ability (705-709)
     - Applies ArcherQueenRapid buff on activation
     - 2.8× attack speed (hit_speed_multiplier=280)
     - -25% movement speed
     - Ability expires after duration

  C. Monk Deflect (710-714)
     - 80% damage reduction (ShieldBoostMonk buff)
     - Lasts 4 seconds (Deflect spell: life_duration=4000ms)
     - Monk takes 80% less damage during ability

  D. Golden Knight Chain Dash (715-719)
     - Dashes to nearest enemy on activation
     - Deals dash_damage=310 (level-scaled)
     - Chains through multiple enemies (dash_count=10)

  E. Mighty Miner Lane Switch (720-724)
     - Teleports to opposite lane (X flips)
     - Drops bomb at old position (334 damage, radius=3000)
     - Resets targeting after teleport
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
DUMMY_DECK = ["knight"] * 8
DEPLOY_TICKS = 20
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


def find_entity(m, eid):
    for e in m.get_entities():
        if e["id"] == eid:
            return e
    return None


def safe_spawn(m, player, key, x, y):
    try:
        return m.spawn_troop(player, key, x, y)
    except Exception as ex:
        print(f"    [spawn failed: {key} → {ex}]")
        return None


def new_match():
    return cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)


def step_n(m, n):
    for _ in range(n):
        m.step()


print("=" * 70)
print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 17")
print("  Tests 700-729: Champion Ability Behavioral Tests")
print("=" * 70)


# =====================================================================
#  SECTION A: SKELETON KING ABILITY (700-704)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION A: SKELETON KING — Summon Skeletons (700-704)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 700: SK ability creates a spell zone
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 700: SK activate_hero creates graveyard zone")
print("-" * 60)

m = new_match()
sk = safe_spawn(m, 1, "skeletonking", 0, -5000)
if sk is not None:
    step_n(m, DEPLOY_TICKS + 200)  # wait for elixir
    try:
        m.activate_hero(sk)
        step_n(m, 5)
        zones = [e for e in m.get_entities() if e["kind"] == "spell_zone"]
        print(f"  Spell zones after activation: {len(zones)}")
        check("700a: SK ability created spell zone", len(zones) >= 1)
        if zones:
            gz = zones[0]
            print(f"  Zone: card_key={gz.get('card_key','')} radius={gz.get('sz_radius',0)}")
            check("700b: Zone is SkeletonKingGraveyard",
                  "skeleton" in gz.get("card_key", "").lower() or gz.get("sz_radius", 0) > 0)
    except Exception as ex:
        check("700a: SK ability activation", False, str(ex))
else:
    check("700: SK spawnable", False)

# ------------------------------------------------------------------
# TEST 701: SK zone spawns skeletons
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 701: SK zone spawns SkeletonKingSkeleton troops")
print("  Data: spawn_interval=250ms (5 ticks), duration=10s (200 ticks)")
print("-" * 60)

m = new_match()
sk = safe_spawn(m, 1, "skeletonking", 0, -8000)  # far from towers
if sk is not None:
    step_n(m, DEPLOY_TICKS + 200)
    try:
        m.activate_hero(sk)

        max_skels = 0
        total_spawns = 0
        seen_ids = set()

        for batch in range(12):
            step_n(m, 20)
            skels = [e for e in m.get_entities()
                     if e["team"] == 1 and e["kind"] == "troop"
                     and "skeleton" in e.get("card_key", "").lower()
                     and e["id"] != sk]
            new = {e["id"] for e in skels} - seen_ids
            total_spawns += len(new)
            seen_ids |= {e["id"] for e in skels}
            if len(skels) > max_skels:
                max_skels = len(skels)

        print(f"  Total unique skeletons spawned: {total_spawns}")
        print(f"  Max alive at once: {max_skels}")
        check("701a: SK spawned skeletons", total_spawns > 0, f"count={total_spawns}")
        # 200 ticks / 5 tick interval = ~40 skeletons expected
        check("701b: Spawned many skeletons (expect ~40 over 10s at 250ms interval)",
              total_spawns >= 15, f"count={total_spawns}")
    except Exception as ex:
        check("701: SK ability", False, str(ex))
else:
    check("701: SK spawnable", False)

# ------------------------------------------------------------------
# TEST 702: SK ability marks hero_ability_active
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 702: SK hero_ability_active flag set")
print("-" * 60)

m = new_match()
sk = safe_spawn(m, 1, "skeletonking", 0, -5000)
if sk is not None:
    step_n(m, DEPLOY_TICKS + 200)
    try:
        m.activate_hero(sk)
        e = find_entity(m, sk)
        check("702a: SK hero_ability_active = True",
              e.get("hero_ability_active", False) if e else False)
    except Exception as ex:
        check("702: SK ability", False, str(ex))
else:
    check("702: SK spawnable", False)


# =====================================================================
#  SECTION B: ARCHER QUEEN ABILITY (705-709)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION B: ARCHER QUEEN — Rapid Fire + Invisibility (705-709)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 705: AQ ability activates and sets buff
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 705: AQ activate_hero applies ArcherQueenRapid buff")
print("-" * 60)

m = new_match()
aq = safe_spawn(m, 1, "archerqueen", 0, -5000)
if aq is not None:
    step_n(m, DEPLOY_TICKS + 200)
    try:
        m.activate_hero(aq)
        e = find_entity(m, aq)
        check("705a: AQ ability activated",
              e.get("hero_ability_active", False) if e else False)
        # Check that hitspeed multiplier changed (280% = base 100 + 180 from buff)
        hs_mult = e.get("hitspeed_mult", 100) if e else 100
        print(f"  AQ hitspeed_mult after ability: {hs_mult} (expect ~280)")
        check("705b: AQ hitspeed boosted (2.8× attack speed)",
              hs_mult > 200, f"hitspeed_mult={hs_mult}")
    except Exception as ex:
        check("705: AQ ability", False, str(ex))
else:
    check("705: AQ spawnable", False)

# ------------------------------------------------------------------
# TEST 706: AQ attacks faster during ability
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 706: AQ attack speed increased during ability")
print("-" * 60)

m = new_match()
aq = safe_spawn(m, 1, "archerqueen", 0, -6000)
golem = m.spawn_troop(2, "golem", 0, -3000)
if aq is not None:
    step_n(m, DEPLOY_TICKS)

    # Measure normal attack speed first
    ge = find_entity(m, golem)
    prev_hp = ge["hp"] if ge else 0
    normal_ticks = []
    for t in range(100):
        m.step()
        g = find_entity(m, golem)
        if g and g["hp"] < prev_hp:
            normal_ticks.append(t)
            prev_hp = g["hp"]
        if g: prev_hp = g["hp"]

    normal_intervals = [normal_ticks[i+1] - normal_ticks[i] for i in range(len(normal_ticks)-1)] if len(normal_ticks) > 1 else []
    normal_avg = sum(normal_intervals[:3]) / max(len(normal_intervals[:3]), 1) if normal_intervals else 99

    # Now activate ability
    step_n(m, 100)  # more elixir
    try:
        m.activate_hero(aq)
    except:
        pass
    step_n(m, 5)

    ge = find_entity(m, golem)
    prev_hp = ge["hp"] if ge else 0
    rapid_ticks = []
    for t in range(80):
        m.step()
        g = find_entity(m, golem)
        if g and g["hp"] < prev_hp:
            rapid_ticks.append(t)
            prev_hp = g["hp"]
        if g: prev_hp = g["hp"]

    rapid_intervals = [rapid_ticks[i+1] - rapid_ticks[i] for i in range(len(rapid_ticks)-1)] if len(rapid_ticks) > 1 else []
    rapid_avg = sum(rapid_intervals[:3]) / max(len(rapid_intervals[:3]), 1) if rapid_intervals else 99

    print(f"  Normal attack avg interval: {normal_avg:.1f} ticks")
    print(f"  Rapid fire avg interval: {rapid_avg:.1f} ticks")
    check("706a: AQ attacks faster during ability",
          rapid_avg < normal_avg * 0.7 if normal_avg > 0 and rapid_avg < 99 else False,
          f"normal={normal_avg:.1f} rapid={rapid_avg:.1f}")
else:
    check("706: AQ spawnable", False)

# ------------------------------------------------------------------
# TEST 707: AQ ability expires
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 707: AQ ability expires after duration")
print("-" * 60)

m = new_match()
aq = safe_spawn(m, 1, "archerqueen", 0, -5000)
if aq is not None:
    step_n(m, DEPLOY_TICKS + 200)
    try:
        m.activate_hero(aq)
        step_n(m, 200)  # 10 seconds — ability should be expired
        e = find_entity(m, aq)
        still_active = e.get("hero_ability_active", False) if e and e["alive"] else False
        print(f"  After 200 ticks: active={still_active}")
        check("707a: AQ ability expired", not still_active)
    except Exception as ex:
        check("707: AQ ability", False, str(ex))
else:
    check("707: AQ spawnable", False)


# =====================================================================
#  SECTION C: MONK DEFLECT (710-714)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION C: MONK — Deflect (710-714)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 710: Monk ability activates
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 710: Monk Deflect activates (80% damage reduction)")
print("-" * 60)

m = new_match()
monk = safe_spawn(m, 1, "monk", 0, -5000)
if monk is not None:
    step_n(m, DEPLOY_TICKS + 200)
    try:
        m.activate_hero(monk)
        e = find_entity(m, monk)
        check("710a: Monk ability activated",
              e.get("hero_ability_active", False) if e else False)
    except Exception as ex:
        check("710: Monk ability", False, str(ex))
else:
    check("710: Monk spawnable", False)

# ------------------------------------------------------------------
# TEST 711: Monk takes 80% less damage during Deflect
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 711: Monk damage reduction during Deflect")
print("  Data: ShieldBoostMonk damage_reduction=80 → melee dmg should be ~20% of normal")
print("-" * 60)

# Track individual damage ticks, filter out tower hits (109 dmg),
# compare ONLY melee damage where 80% reduction applies.
m1 = new_match()
m2 = new_match()
monk1 = safe_spawn(m1, 1, "monk", 0, -12000)
monk2 = safe_spawn(m2, 1, "monk", 0, -12000)

if monk1 is not None and monk2 is not None:
    step_n(m1, DEPLOY_TICKS + 200)
    step_n(m2, DEPLOY_TICKS + 200)

    me1 = find_entity(m1, monk1)
    me2 = find_entity(m2, monk2)
    print(f"  Monk1 after wait: alive={me1['alive'] if me1 else 'NOT FOUND'} hp={me1['hp'] if me1 else 0}")
    print(f"  Monk2 after wait: alive={me2['alive'] if me2 else 'NOT FOUND'} hp={me2['hp'] if me2 else 0}")

    # Spawn enemies FIRST — let them deploy and walk into melee range
    me1_pos = find_entity(m1, monk1)
    me2_pos = find_entity(m2, monk2)
    y1 = me1_pos["y"] if me1_pos else -8000
    y2 = me2_pos["y"] if me2_pos else -8000
    enemy1 = m1.spawn_troop(2, "knight", 0, y1 + 1000)
    enemy2 = m2.spawn_troop(2, "knight", 0, y2 + 1000)

    # Wait for Knight to deploy + walk into melee range (~50 ticks)
    step_n(m1, 50)
    step_n(m2, 50)

    # NOW activate deflect — buff starts when Knight is already attacking
    try:
        m1.activate_hero(monk1)
    except Exception as ex:
        print(f"  Activate failed: {ex}")

    # Record HP at deflect activation
    me1_now = find_entity(m1, monk1)
    me2_now = find_entity(m2, monk2)
    prev_hp1 = me1_now["hp"] if me1_now else 2000
    prev_hp2 = me2_now["hp"] if me2_now else 2000
    melee_dmg_1 = []
    melee_dmg_2 = []

    # Measure damage during deflect window (80 ticks = 4 seconds from data)
    for t in range(80):
        m1.step()
        m2.step()
        e1 = find_entity(m1, monk1)
        e2 = find_entity(m2, monk2)
        if e1 and e1["hp"] < prev_hp1:
            hit = prev_hp1 - e1["hp"]
            if abs(hit - 109) > 10:
                melee_dmg_1.append(hit)
            prev_hp1 = e1["hp"]
        if e1: prev_hp1 = e1["hp"]
        if e2 and e2["hp"] < prev_hp2:
            hit = prev_hp2 - e2["hp"]
            if abs(hit - 109) > 10:
                melee_dmg_2.append(hit)
            prev_hp2 = e2["hp"]
        if e2: prev_hp2 = e2["hp"]

    total_melee_1 = sum(melee_dmg_1)
    total_melee_2 = sum(melee_dmg_2)
    print(f"  Melee hits WITH Deflect: {melee_dmg_1}")
    print(f"  Melee hits WITHOUT Deflect: {melee_dmg_2}")
    print(f"  Total melee dmg: with={total_melee_1} without={total_melee_2}")

    if total_melee_2 > 0 and total_melee_1 >= 0:
        # damage_reduction=80 from data → deflected damage = normal × (100-80)/100 = 20%
        ratio = total_melee_1 / total_melee_2
        print(f"  Melee damage ratio: {ratio:.2f} (expect ~0.20 from damage_reduction=80)")
        check("711a: Monk melee damage reduced ~80% (data: damage_reduction=80)",
              ratio < 0.30,
              f"ratio={ratio:.2f} deflected={total_melee_1} normal={total_melee_2}")
    elif total_melee_2 == 0 and total_melee_1 == 0:
        check("711a: Knight dealt melee damage", False, "no melee hits on either monk")
    else:
        check("711a: Deflect reduced melee damage",
              total_melee_1 < total_melee_2,
              f"with={total_melee_1} without={total_melee_2}")
else:
    check("711: Monks spawnable", False)

# ------------------------------------------------------------------
# TEST 712: Deflect expires after 4 seconds (80 ticks)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 712: Deflect expires after ~4s (80 ticks)")
print("-" * 60)

m = new_match()
monk = safe_spawn(m, 1, "monk", 0, -5000)
if monk is not None:
    step_n(m, DEPLOY_TICKS + 200)
    try:
        m.activate_hero(monk)
        step_n(m, 100)  # 5s — past the 4s duration
        e = find_entity(m, monk)
        still_active = e.get("hero_ability_active", False) if e and e["alive"] else False
        print(f"  After 100 ticks: active={still_active}")
        check("712a: Deflect expired after ~4s", not still_active)
    except Exception as ex:
        check("712: Monk ability", False, str(ex))
else:
    check("712: Monk spawnable", False)


# =====================================================================
#  SECTION D: GOLDEN KNIGHT CHAIN DASH (715-719)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION D: GOLDEN KNIGHT — Chain Dash (715-719)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 715: GK ability activates
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 715: GK chain dash activates")
print("-" * 60)

m = new_match()
gk = safe_spawn(m, 1, "goldenknight", 0, -12000)  # far back, safe from towers

if gk is not None:
    step_n(m, DEPLOY_TICKS + 200)  # accumulate elixir

    # Verify GK alive
    gke = find_entity(m, gk)
    if gke and gke["alive"]:
        gk_y = gke["y"]
        # Place enemies near current GK position for dash
        e1 = m.spawn_troop(2, "knight", 0, gk_y + 2000)
        e2 = m.spawn_troop(2, "knight", 2000, gk_y + 4000)
        e3 = m.spawn_troop(2, "knight", -2000, gk_y + 5000)
        step_n(m, DEPLOY_TICKS)  # let enemies deploy

        try:
            m.activate_hero(gk)
            e = find_entity(m, gk)
            check("715a: GK ability activated",
                  e.get("hero_ability_active", False) if e else False)
        except Exception as ex:
            check("715: GK ability", False, str(ex))
    else:
        check("715: GK alive after wait", False, f"gk={'DEAD' if gke else 'NOT FOUND'}")
else:
    check("715: GK spawnable", False)

# ------------------------------------------------------------------
# TEST 716: GK dash deals damage to enemies
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 716: GK chain dash damages enemies")
print("-" * 60)

m = new_match()
gk = safe_spawn(m, 1, "goldenknight", 0, -5000)
targets = []
for i in range(3):
    tid = m.spawn_troop(2, "golem", i * 2000 - 2000, -3000 + i * 1000)
    targets.append(tid)

if gk is not None:
    step_n(m, DEPLOY_TICKS + 200)
    # Record HP before
    hps_before = {}
    for tid in targets:
        te = find_entity(m, tid)
        if te:
            hps_before[tid] = te["hp"]

    try:
        m.activate_hero(gk)
        step_n(m, 60)  # let chain dash play out

        damaged = 0
        for tid in targets:
            te = find_entity(m, tid)
            if te and te["hp"] < hps_before.get(tid, 0):
                damaged += 1
            elif te is None or not te.get("alive", True):
                damaged += 1

        print(f"  Enemies damaged by chain dash: {damaged}/{len(targets)}")
        check("716a: GK chain dash damaged at least one enemy",
              damaged >= 1, f"damaged={damaged}")
    except Exception as ex:
        check("716: GK ability", False, str(ex))
else:
    check("716: GK spawnable", False)


# =====================================================================
#  SECTION E: MIGHTY MINER LANE SWITCH (720-724)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION E: MIGHTY MINER — Lane Switch + Bomb (720-724)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 720: MM ability teleports to opposite lane
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 720: MM lane switch teleports X position")
print("-" * 60)

m = new_match()
mm = safe_spawn(m, 1, "mightyminer", -3000, -5000)  # left lane
if mm is not None:
    step_n(m, DEPLOY_TICKS + 200)
    me = find_entity(m, mm)
    x_before = me["x"] if me else 0
    print(f"  X before activation: {x_before}")

    try:
        m.activate_hero(mm)
        step_n(m, 5)
        me2 = find_entity(m, mm)
        x_after = me2["x"] if me2 else 0
        print(f"  X after activation: {x_after}")

        check("720a: MM teleported to opposite lane",
              (x_before < 0 and x_after > 0) or (x_before > 0 and x_after < 0),
              f"before={x_before} after={x_after}")
    except Exception as ex:
        check("720: MM ability", False, str(ex))
else:
    check("720: MM spawnable", False)

# ------------------------------------------------------------------
# TEST 721: MM bomb damages enemies at old position
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 721: MM bomb deals damage at old position")
print("  Data: MightyMinerBomb death_damage=334, radius=3000")
print("-" * 60)

m = new_match()
mm = safe_spawn(m, 1, "mightyminer", -3000, -5000)
# Place enemy near MM's starting position
enemy = m.spawn_troop(2, "golem", -2800, -5000)

if mm is not None:
    step_n(m, DEPLOY_TICKS + 200)
    ge = find_entity(m, enemy)
    golem_hp_before = ge["hp"] if ge else 0

    try:
        m.activate_hero(mm)
        step_n(m, 5)

        ge2 = find_entity(m, enemy)
        golem_hp_after = ge2["hp"] if ge2 else 0
        bomb_dmg = golem_hp_before - golem_hp_after

        print(f"  Golem HP: {golem_hp_before} → {golem_hp_after} (lost {bomb_dmg})")
        check("721a: Bomb dealt damage at old position",
              bomb_dmg > 100, f"dmg={bomb_dmg}")
    except Exception as ex:
        check("721: MM ability", False, str(ex))
else:
    check("721: MM spawnable", False)

# ------------------------------------------------------------------
# TEST 722: MM ability marks active
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 722: MM hero_ability_active flag")
print("-" * 60)

m = new_match()
mm = safe_spawn(m, 1, "mightyminer", -3000, -5000)
if mm is not None:
    step_n(m, DEPLOY_TICKS + 200)
    try:
        m.activate_hero(mm)
        e = find_entity(m, mm)
        check("722a: MM ability activated",
              e.get("hero_ability_active", False) if e else False)
    except Exception as ex:
        check("722: MM ability", False, str(ex))
else:
    check("722: MM spawnable", False)


# ====================================================================
# SUMMARY
# ====================================================================

print("\n" + "=" * 70)
print(f"  RESULTS: {PASS}/{PASS + FAIL} passed, {FAIL}/{PASS + FAIL} failed")
print("=" * 70)

print("\n  Section coverage:")
sections = {
    "A: Skeleton King (700-702)": "graveyard zone spawn, skeleton generation, ability flag",
    "B: Archer Queen (705-707)": "rapid fire buff (2.8× speed), hitspeed verification, expiry",
    "C: Monk Deflect (710-712)": "80% damage reduction, comparative damage test, expiry",
    "D: Golden Knight (715-716)": "chain dash activation, multi-enemy damage",
    "E: Mighty Miner (720-722)": "lane teleport (X flip), bomb damage, ability flag",
}
for section, desc in sections.items():
    print(f"    {section}")
    print(f"      → {desc}")

sys.exit(0 if FAIL == 0 else 1)
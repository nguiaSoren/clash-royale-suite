#!/usr/bin/env python3
"""
============================================================
  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 20
  Tests 950-999: Targeting, Stealth, Multi-Projectile, Physics
============================================================

All values from JSON data files. No heuristics.

  A. TARGETING — FISHERMAN HOOK PULL (950-959)
     Fisherman: key=fisherman, range=1200, hit_speed=1300ms, damage=160(lv1),
       projectile_special=FishermanProjectile, special_range=7000, special_min_range=3500
     FishermanProjectile: speed=800, homing=True, drag_back_as_attractor=True,
       drag_back_speed=850, drag_self_speed=450, drag_margin=200

  B. STEALTH — ROYAL GHOST INVISIBILITY (960-969)
     Ghost: speed=90, hit_speed=1800ms, damage=216(lv1), area_damage_radius=1000
       buff_when_not_attacking=Invisibility, buff_when_not_attacking_time=1800ms

  C. MULTI-PROJECTILE (970-979)
     Hunter: multiple_projectiles=10, HunterProjectile damage=53(lv1), scatter=Line
     Princess: multiple_projectiles=5, PrincessProjectile damage=140(lv1), radius=2000

  D. PHYSICS (980-999)
     LogProjectileRolling: pushback=700, pushback_all=True
     SnowballSpell: pushback=1800, target_buff=IceWizardSlowDown (speed=-35%)
     FireballSpell: pushback=1000
     Tornado buff: attract_percentage=360, push_speed_factor=100, radius=5500
"""

import sys, os

try:
    import cr_engine
except ImportError:
    here = os.path.dirname(os.path.abspath(__file__))
    for p in [os.path.join(here, "engine", "target", "release"),
              os.path.join(here, "target", "release")]:
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


def find_all(m, team=None, kind=None, card_key_contains=None):
    result = []
    for e in m.get_entities():
        if not e["alive"]:
            continue
        if team is not None and e["team"] != team:
            continue
        if kind is not None and e["kind"] != kind:
            continue
        if card_key_contains and card_key_contains.lower() not in e.get("card_key", "").lower():
            continue
        result.append(e)
    return result


def new_match(d1=None, d2=None):
    return cr_engine.new_match(data, d1 or DUMMY_DECK, d2 or DUMMY_DECK)


def step_n(m, n):
    for _ in range(n):
        m.step()


def safe_spawn(m, player, key, x, y):
    try:
        return m.spawn_troop(player, key, x, y)
    except Exception as ex:
        print(f"    [spawn failed: {key} → {ex}]")
        return None


def dist_to(entity, x, y):
    dx = entity["x"] - x
    dy = entity["y"] - y
    return (dx * dx + dy * dy) ** 0.5


def dist_between(e1, e2):
    dx = e1["x"] - e2["x"]
    dy = e1["y"] - e2["y"]
    return (dx * dx + dy * dy) ** 0.5


# ── Resolve card keys ──
card_list = data.list_cards()
card_keys_available = {c["key"] for c in card_list}

LOG_KEY = next((c for c in ["the-log", "log"] if c in card_keys_available), None)
SNOWBALL_KEY = next((c for c in ["giant-snowball", "snowball"] if c in card_keys_available), None)
FIREBALL_KEY = next((c for c in ["fireball"] if c in card_keys_available), None)
TORNADO_KEY = next((c for c in ["tornado"] if c in card_keys_available), None)

print(f"  Card keys: log={LOG_KEY}, snowball={SNOWBALL_KEY}, fireball={FIREBALL_KEY}, tornado={TORNADO_KEY}")

print("=" * 70)
print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 20")
print("  Tests 950-999: Targeting, Stealth, Multi-Projectile, Physics")
print("=" * 70)


# =====================================================================
#  SECTION A: FISHERMAN HOOK PULL (950-959)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION A: FISHERMAN HOOK PULL (950-959)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 950: Fisherman melee attack
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 950: Fisherman melee attack")
print("  Data: range=1200, hit_speed=1300ms, damage=160(lv1)")
print("-" * 60)

m = new_match()
fisher = safe_spawn(m, 1, "fisherman", 0, -6000)
enemy = safe_spawn(m, 2, "golem", 0, -5800)

if fisher is not None:
    step_n(m, DEPLOY_TICKS)
    fe = find_entity(m, fisher)
    ee = find_entity(m, enemy)
    hp_before = ee["hp"] if ee else 0
    step_n(m, 80)
    ee2 = find_entity(m, enemy)
    dmg = hp_before - (ee2["hp"] if ee2 and ee2["alive"] else 0)
    print(f"  Fisherman alive: {fe['alive'] if fe else 'N/A'},  Golem HP lost: {dmg}")
    check("950a: Fisherman spawned", fe is not None and fe["alive"])
    check("950b: Fisherman dealt melee damage", dmg > 0, f"dmg={dmg}")
else:
    check("950: Fisherman spawnable", False)

# ------------------------------------------------------------------
# TEST 951: Fisherman hook drag displaces enemy
# Track Golem position tick by tick. If hook works, there should be a
# sudden large Y jump when the hook projectile impacts and drags the
# Golem toward the Fisherman.
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 951: Fisherman hook drags enemy (detect sudden displacement)")
print("  Data: drag_back_speed=850, drag_margin=200")
print("  Method: Track Golem Y tick by tick, detect jump > 500u")
print("-" * 60)

m = new_match()
fisher = safe_spawn(m, 1, "fisherman", 0, -10000)
enemy = safe_spawn(m, 2, "golem", 0, -4000)  # 6000u away, well within special_range=7000

if fisher is not None and enemy is not None:
    step_n(m, DEPLOY_TICKS)
    ee = find_entity(m, enemy)
    prev_y = ee["y"] if ee else -4000
    max_jump = 0
    jump_tick = -1

    for t in range(200):
        m.step()
        g = find_entity(m, enemy)
        if g and g["alive"]:
            dy = abs(g["y"] - prev_y)
            if dy > max_jump:
                max_jump = dy
                jump_tick = t
            prev_y = g["y"]

    print(f"  Largest single-tick Y jump: {max_jump}u at tick {jump_tick}")
    print(f"  Golem final Y: {prev_y}")

    # Normal Golem walking: 18u/tick. A drag-back would produce a jump of hundreds/thousands.
    # With drag_back: Golem should jump ~5000u in a single tick toward Fisherman.
    check("951a: Hook drag displaced Golem (jump > 500u in single tick)",
          max_jump > 500,
          f"max_jump={max_jump}u — hook drag not producing displacement")
else:
    check("951: Spawnable", False)

# ------------------------------------------------------------------
# TEST 952: No hook at close range
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 952: No hook below min range (3500)")
print("-" * 60)

m = new_match()
fisher = safe_spawn(m, 1, "fisherman", 0, -10000)
enemy = safe_spawn(m, 2, "golem", 0, -8200)

if fisher is not None and enemy is not None:
    step_n(m, DEPLOY_TICKS)
    projs_seen = False
    for _ in range(100):
        m.step()
        if find_all(m, team=1, kind="projectile"):
            projs_seen = True
            break
    check("952a: No hook projectile at close range", not projs_seen, "Projectile found")
else:
    check("952: Spawnable", False)

# ------------------------------------------------------------------
# TEST 953: Hook projectile spawn
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 953: Fisherman fires hook projectile at distant enemy")
print("-" * 60)

m = new_match()
fisher = safe_spawn(m, 1, "fisherman", 0, -10000)
enemy = safe_spawn(m, 2, "golem", 0, -4000)

if fisher is not None and enemy is not None:
    step_n(m, DEPLOY_TICKS)
    found_proj = False
    for _ in range(200):
        m.step()
        projs = find_all(m, team=1, kind="projectile")
        if projs:
            found_proj = True
            print(f"  Projectile: '{projs[0].get('card_key','')}' at ({projs[0]['x']}, {projs[0]['y']})")
            break
    check("953a: Hook projectile spawned (ENGINE GAP: projectile_special not implemented)",
          found_proj, "No projectile in 200 ticks")
else:
    check("953: Spawnable", False)


# =====================================================================
#  SECTION B: ROYAL GHOST INVISIBILITY (960-969)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION B: ROYAL GHOST INVISIBILITY (960-969)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 960: Ghost attacks
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 960: Royal Ghost deals area damage")
print("  Data: damage=216(lv1), area_damage_radius=1000")
print("-" * 60)

m = new_match()
ghost = safe_spawn(m, 1, "royal-ghost", 0, -6000)
enemy = safe_spawn(m, 2, "golem", 0, -5800)

if ghost is not None:
    step_n(m, DEPLOY_TICKS)
    ge = find_entity(m, ghost)
    ee = find_entity(m, enemy)
    hp0 = ee["hp"] if ee else 0
    step_n(m, 60)
    ee2 = find_entity(m, enemy)
    dmg = hp0 - (ee2["hp"] if ee2 and ee2["alive"] else 0)
    print(f"  Ghost card_key={ge.get('card_key','') if ge else '?'},  Golem HP lost: {dmg}")
    check("960a: Royal Ghost spawned", ge is not None and ge["alive"])
    check("960b: Ghost dealt damage", dmg > 0, f"dmg={dmg}")
else:
    check("960: Spawnable", False)

# ------------------------------------------------------------------
# TEST 961-963: Invisibility (all expected to fail — not implemented)
# ------------------------------------------------------------------
for test_id, desc in [
    ("961", "Ghost gains invisibility buff when idle"),
    ("962", "Enemies ignore invisible Ghost"),
    ("963", "Ghost loses invisibility on attack"),
]:
    print(f"\n" + "-" * 60)
    print(f"TEST {test_id}: {desc}")
    print(f"  Data: buff_when_not_attacking=Invisibility, time=1800ms")
    print("-" * 60)

m = new_match()
ghost = safe_spawn(m, 1, "royal-ghost", 0, -12000)
if ghost is not None:
    step_n(m, DEPLOY_TICKS + 80)
    ge = find_entity(m, ghost)
    num_buffs = ge.get("num_buffs", 0) if ge else 0
    print(f"  Ghost buffs after 4s idle: {num_buffs}")
    check("961a: Ghost has invisibility buff (ENGINE GAP: buff_when_not_attacking not implemented)",
          num_buffs > 0, f"num_buffs={num_buffs}")

    # 962: spawn enemy near invisible ghost
    enemy = m.spawn_troop(2, "knight", ge["x"] if ge else 0, (ge["y"] if ge else -12000) + 200)
    ghost_hp = ge["hp"] if ge else 0
    step_n(m, DEPLOY_TICKS + 60)
    ge2 = find_entity(m, ghost)
    hp_lost = ghost_hp - (ge2["hp"] if ge2 else 0)
    if num_buffs > 0:
        check("962a: Invisible Ghost not targeted", hp_lost == 0, f"hp_lost={hp_lost}")
    else:
        check("962a: Prerequisite failed (no invisibility)", False, "buff_when_not_attacking not implemented")

    # 963: check if buffs cleared after attacking
    ge3 = find_entity(m, ghost)
    buffs_now = ge3.get("num_buffs", 0) if ge3 else 0
    if num_buffs > 0:
        check("963a: Ghost lost invisibility on attack", buffs_now < num_buffs,
              f"before={num_buffs} after={buffs_now}")
    else:
        check("963a: Prerequisite failed (no invisibility)", False, "buff_when_not_attacking not implemented")
else:
    check("961: Spawnable", False)
    check("962: Spawnable", False)
    check("963: Spawnable", False)


# =====================================================================
#  SECTION C: MULTI-PROJECTILE (970-979)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION C: MULTI-PROJECTILE — Hunter & Princess (970-979)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 970: Hunter 10-bullet volley
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 970: Hunter fires 10 projectiles per attack")
print("  Data: multiple_projectiles=10, HunterProjectile damage=53(lv1)")
print("-" * 60)

m = new_match()
hunter = safe_spawn(m, 1, "hunter", 0, -6000)
enemy = safe_spawn(m, 2, "golem", 0, -3000)

if hunter is not None:
    step_n(m, DEPLOY_TICKS)
    max_projs = 0
    total_ids = set()
    for t in range(200):
        m.step()
        projs = find_all(m, team=1, kind="projectile")
        max_projs = max(max_projs, len(projs))
        for p in projs:
            total_ids.add(p["id"])
    print(f"  Max simultaneous: {max_projs},  Total unique: {len(total_ids)}")
    check("970a: Hunter fired projectiles", len(total_ids) >= 1, f"total={len(total_ids)}")
    check("970b: 10 per volley (ENGINE GAP: multiple_projectiles hardcoded to 1)",
          max_projs >= 10, f"max={max_projs}")
else:
    check("970: Spawnable", False)

# ------------------------------------------------------------------
# TEST 971: Hunter distance-based damage
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 971: Hunter damage scales with distance")
print("  Data: 10 × 53 = 530 total at point blank; scatter=Line spreads at range")
print("-" * 60)

m_close = new_match()
h_close = safe_spawn(m_close, 1, "hunter", 0, -10000)
e_close = safe_spawn(m_close, 2, "golem", 0, -9800)

m_far = new_match()
h_far = safe_spawn(m_far, 1, "hunter", 0, -10000)
e_far = safe_spawn(m_far, 2, "golem", 0, -6500)

if h_close is not None and h_far is not None:
    step_n(m_close, DEPLOY_TICKS)
    step_n(m_far, DEPLOY_TICKS)
    hp_c0 = find_entity(m_close, e_close)["hp"]
    hp_f0 = find_entity(m_far, e_far)["hp"]
    step_n(m_close, 80)
    step_n(m_far, 80)
    close_dmg = hp_c0 - (find_entity(m_close, e_close) or {}).get("hp", 0)
    far_dmg = hp_f0 - (find_entity(m_far, e_far) or {}).get("hp", 0)
    print(f"  Close (200u): {close_dmg},  Far (3500u): {far_dmg}")
    check("971a: Dealt damage close", close_dmg > 0, f"dmg={close_dmg}")
    check("971b: Dealt damage far", far_dmg > 0, f"dmg={far_dmg}")
    if close_dmg > 0 and far_dmg > 0:
        check("971c: Close > far (ENGINE GAP: scatter not implemented)",
              close_dmg > far_dmg * 1.15, f"close={close_dmg} far={far_dmg}")
    else:
        check("971c: Both dealt damage", False, f"close={close_dmg} far={far_dmg}")
else:
    check("971: Spawnable", False)

# ------------------------------------------------------------------
# TEST 972: Princess 5-arrow volley
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 972: Princess fires 5 projectiles per attack")
print("  Data: multiple_projectiles=5")
print("-" * 60)

m = new_match()
princess = safe_spawn(m, 1, "princess", 0, -10000)
enemy = safe_spawn(m, 2, "golem", 0, -2000)

if princess is not None:
    step_n(m, DEPLOY_TICKS)
    max_projs = 0
    total_ids = set()
    for t in range(200):
        m.step()
        projs = find_all(m, team=1, kind="projectile")
        max_projs = max(max_projs, len(projs))
        for p in projs:
            total_ids.add(p["id"])
    print(f"  Max simultaneous: {max_projs},  Total unique: {len(total_ids)}")
    check("972a: Princess fired projectiles", len(total_ids) >= 1, f"total={len(total_ids)}")
    check("972b: 5 per volley (ENGINE GAP: multiple_projectiles hardcoded to 1)",
          max_projs >= 5, f"max={max_projs}")
else:
    check("972: Spawnable", False)

# ------------------------------------------------------------------
# TEST 973: Princess AoE splash
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 973: Princess splash hits clustered enemies")
print("  Data: PrincessProjectile radius=2000")
print("-" * 60)

m = new_match()
princess = safe_spawn(m, 1, "princess", 0, -10000)
e1 = m.spawn_troop(2, "knight", 0, -2000)
e2 = m.spawn_troop(2, "knight", 500, -2000)
e3 = m.spawn_troop(2, "knight", -500, -2000)

if princess is not None:
    step_n(m, DEPLOY_TICKS)
    hp0 = {eid: find_entity(m, eid)["hp"] for eid in [e1, e2, e3] if find_entity(m, eid)}
    step_n(m, 200)
    damaged = sum(1 for eid in [e1, e2, e3]
                  if eid in hp0 and (
                      find_entity(m, eid) is None  # dead = definitely damaged
                      or hp0[eid] - find_entity(m, eid)["hp"] > 0))
    print(f"  Enemies damaged: {damaged}/3")
    check("973a: ≥1 enemy hit", damaged >= 1, f"damaged={damaged}")
    check("973b: All 3 hit by splash", damaged >= 3, f"damaged={damaged}/3")
else:
    check("973: Spawnable", False)


# =====================================================================
#  SECTION D: PHYSICS (980-999)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION D: PHYSICS — Knockback & Tornado Pull (980-999)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 980: Log pushback (with control comparison)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 980: Log pushback displaces troops beyond natural walk")
print("  Data: LogProjectileRolling pushback=700, pushback_all=True")
print("  Method: Compare Golem Y with Log vs without Log (control)")
print("-" * 60)

if LOG_KEY:
    log_deck = [LOG_KEY] + ["knight"] * 7
    TICKS = 120
    try:
        # Control: Golem walks with no Log
        m_ctrl = new_match(DUMMY_DECK, DUMMY_DECK)
        step_n(m_ctrl, 20)
        g_ctrl = safe_spawn(m_ctrl, 2, "golem", 0, -5000)
        step_n(m_ctrl, DEPLOY_TICKS)
        g_ctrl_y0 = find_entity(m_ctrl, g_ctrl)["y"]
        step_n(m_ctrl, TICKS)
        g_ctrl_y1 = find_entity(m_ctrl, g_ctrl)["y"] if find_entity(m_ctrl, g_ctrl) else g_ctrl_y0

        # Test: Golem hit by Log
        m = new_match(log_deck, DUMMY_DECK)
        step_n(m, 20)
        g_test = safe_spawn(m, 2, "golem", 0, -5000)
        step_n(m, DEPLOY_TICKS)
        g_test_y0 = find_entity(m, g_test)["y"]
        g_test_hp0 = find_entity(m, g_test)["hp"]
        m.play_card(1, 0, 0, -6000)
        step_n(m, TICKS)
        g_test_e = find_entity(m, g_test)
        g_test_y1 = g_test_e["y"] if g_test_e else g_test_y0
        g_test_dmg = g_test_hp0 - (g_test_e["hp"] if g_test_e and g_test_e["alive"] else 0)

        ctrl_displacement = g_ctrl_y1 - g_ctrl_y0
        test_displacement = g_test_y1 - g_test_y0

        # Pushback pushes Golem AWAY from impact (toward P2 side = positive Y).
        # The pushed Golem should end up further NORTH (more positive Y) than control.
        # Measure: test_y1 - ctrl_y1 > 0 means pushed Golem is north of control.
        pushback_effect = g_test_y1 - g_ctrl_y1

        print(f"  Control Y: {g_ctrl_y0} → {g_ctrl_y1} (Δ={ctrl_displacement})")
        print(f"  With Log Y: {g_test_y0} → {g_test_y1} (Δ={test_displacement})")
        print(f"  Pushback effect (test_y - ctrl_y): {pushback_effect}")
        print(f"  Golem damage: {g_test_dmg}")

        check("980a: Log dealt damage", g_test_dmg > 100, f"dmg={g_test_dmg}")
        # Log pushback=700 should displace Golem ~700u north of where control Golem ends up.
        # Log is rolling so pushback is in roll direction (P1 forward = positive Y).
        check("980b: Log pushed Golem (displaced >400u from control position)",
              pushback_effect > 400, f"pushback_effect={pushback_effect}")
    except Exception as ex:
        check("980: Log playable", False, str(ex))
else:
    check("980: Log card key not found", False)

# ------------------------------------------------------------------
# TEST 981: Snowball pushback + slow (with control)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 981: Snowball pushback + slow debuff")
print("  Data: pushback=1800, IceWizardSlowDown speed=-35%")
print("-" * 60)

if SNOWBALL_KEY:
    snow_deck = [SNOWBALL_KEY] + ["knight"] * 7
    TICKS = 80
    try:
        # Control
        m_ctrl = new_match(DUMMY_DECK, DUMMY_DECK)
        step_n(m_ctrl, 20)
        g_ctrl = safe_spawn(m_ctrl, 2, "golem", 0, -5000)
        step_n(m_ctrl, DEPLOY_TICKS)
        g_ctrl_y0 = find_entity(m_ctrl, g_ctrl)["y"]
        step_n(m_ctrl, TICKS)
        g_ctrl_y1 = find_entity(m_ctrl, g_ctrl)["y"] if find_entity(m_ctrl, g_ctrl) else g_ctrl_y0

        # Test
        m = new_match(snow_deck, DUMMY_DECK)
        step_n(m, 20)
        g_test = safe_spawn(m, 2, "golem", 0, -5000)
        step_n(m, DEPLOY_TICKS)
        g_test_y0 = find_entity(m, g_test)["y"]
        g_test_hp0 = find_entity(m, g_test)["hp"]
        m.play_card(1, 0, 0, -5000)

        # Check speed_mult shortly after impact (before buff expires)
        # Snowball buff lasts 50 ticks, arrives in ~5 ticks
        step_n(m, 15)
        g_mid = find_entity(m, g_test)
        speed_mult_mid = g_mid.get("speed_mult", 100) if g_mid else 100

        step_n(m, TICKS - 15)
        g_test_e = find_entity(m, g_test)
        g_test_y1 = g_test_e["y"] if g_test_e else g_test_y0
        g_test_dmg = g_test_hp0 - (g_test_e["hp"] if g_test_e and g_test_e["alive"] else 0)
        speed_mult = g_test_e.get("speed_mult", 100) if g_test_e else 100

        # Pushback pushes Golem north (positive Y). Pushed Golem ends up further north.
        pushback_effect = g_test_y1 - g_ctrl_y1

        print(f"  Control final Y: {g_ctrl_y1},  With Snowball final Y: {g_test_y1}")
        print(f"  Pushback effect (test_y - ctrl_y): {pushback_effect}")
        print(f"  Golem damage: {g_test_dmg}")
        print(f"  speed_mult at t=15: {speed_mult_mid}, at t=80: {speed_mult}")

        check("981a: Snowball dealt damage", g_test_dmg > 0, f"dmg={g_test_dmg}")
        check("981b: Snowball pushed Golem (displaced >400u from control)",
              pushback_effect > 400, f"pushback_effect={pushback_effect}")
        check("981c: Snowball applied slow (speed_mult < 100 shortly after impact)",
              speed_mult_mid < 100, f"speed_mult_at_t15={speed_mult_mid}")
    except Exception as ex:
        check("981: Snowball playable", False, str(ex))
else:
    check("981: Snowball key not found", False)

# ------------------------------------------------------------------
# TEST 982: Fireball knockback (with control)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 982: Fireball knockback")
print("  Data: pushback=1000")
print("-" * 60)

if FIREBALL_KEY:
    fb_deck = [FIREBALL_KEY] + ["knight"] * 7
    TICKS = 80
    try:
        # Control
        m_ctrl = new_match(DUMMY_DECK, DUMMY_DECK)
        step_n(m_ctrl, 20)
        g_ctrl = safe_spawn(m_ctrl, 2, "golem", 0, -5000)
        step_n(m_ctrl, DEPLOY_TICKS)
        g_ctrl_y0 = find_entity(m_ctrl, g_ctrl)["y"]
        step_n(m_ctrl, TICKS)
        g_ctrl_y1 = find_entity(m_ctrl, g_ctrl)["y"] if find_entity(m_ctrl, g_ctrl) else g_ctrl_y0

        # Test
        m = new_match(fb_deck, DUMMY_DECK)
        step_n(m, 20)
        g_test = safe_spawn(m, 2, "golem", 0, -5000)
        step_n(m, DEPLOY_TICKS)
        g_test_y0 = find_entity(m, g_test)["y"]
        g_test_hp0 = find_entity(m, g_test)["hp"]
        m.play_card(1, 0, 0, -5000)
        step_n(m, TICKS)
        g_test_e = find_entity(m, g_test)
        g_test_y1 = g_test_e["y"] if g_test_e else g_test_y0
        g_test_dmg = g_test_hp0 - (g_test_e["hp"] if g_test_e and g_test_e["alive"] else 0)

        # Pushback pushes Golem north. Pushed Golem ends up further north than control.
        pushback_effect = g_test_y1 - g_ctrl_y1

        print(f"  Control final Y: {g_ctrl_y1},  With Fireball final Y: {g_test_y1}")
        print(f"  Pushback effect: {pushback_effect},  Golem damage: {g_test_dmg}")

        check("982a: Fireball dealt damage", g_test_dmg > 200, f"dmg={g_test_dmg}")
        check("982b: Fireball knockback (displaced >300u from control)",
              pushback_effect > 300, f"pushback_effect={pushback_effect}")
    except Exception as ex:
        check("982: Fireball playable", False, str(ex))
else:
    check("982: Fireball key not found", False)

# ------------------------------------------------------------------
# TEST 985: Tornado pull (corrected placement)
# Place P1 troops (walk toward P2 = positive Y = AWAY from tornado center).
# Tornado at center. If tornado pulls, troops move AGAINST their walk direction.
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 985: Tornado pulls enemies toward center")
print("  Data: attract_percentage=360, radius=5500, ~21 ticks")
print("  Method: P2 enemies ABOVE tornado center — they walk away, tornado pulls back")
print("-" * 60)

if TORNADO_KEY:
    tornado_deck = [TORNADO_KEY] + ["knight"] * 7
    try:
        tornado_x, tornado_y = 0, -5000

        # Control: how far do P2 troops walk in 40 ticks WITHOUT tornado?
        m_ctrl = new_match(DUMMY_DECK, DUMMY_DECK)
        step_n(m_ctrl, 20)
        c1 = safe_spawn(m_ctrl, 2, "knight", -3000, -3000)  # North of center
        step_n(m_ctrl, DEPLOY_TICKS)
        c1s = find_entity(m_ctrl, c1)
        c1_d0 = dist_to(c1s, tornado_x, tornado_y) if c1s else 0
        step_n(m_ctrl, 40)
        c1a = find_entity(m_ctrl, c1)
        c1_d1 = dist_to(c1a, tornado_x, tornado_y) if c1a and c1a["alive"] else c1_d0
        ctrl_drift = c1_d1 - c1_d0  # Positive = moved away from center (natural walk)
        print(f"  Control: Knight drifted {ctrl_drift:.0f}u from center in 40t (natural walk)")

        # Test: same troops, with tornado
        m = new_match(tornado_deck, DUMMY_DECK)
        step_n(m, 20)

        # Place P2 troops NORTH of tornado center (Y > tornado_y = -5000)
        # P2 troops walk toward P1 side = negative Y = toward tornado center
        # But also place some at Y = -3000 (2000u north of center = within radius 5500)
        e1 = safe_spawn(m, 2, "knight", -3000, -3000)
        e2 = safe_spawn(m, 2, "knight", 3000, -3000)
        e3 = safe_spawn(m, 2, "knight", 0, -8000)  # SOUTH of center — walks away from center
        step_n(m, DEPLOY_TICKS)

        starts = {}
        for label, eid in [("E1", e1), ("E2", e2), ("E3-south", e3)]:
            e = find_entity(m, eid) if eid else None
            if e:
                d = dist_to(e, tornado_x, tornado_y)
                starts[label] = (e["x"], e["y"], d)
                print(f"  {label} start: ({e['x']}, {e['y']}) dist={d:.0f}")

        m.play_card(1, 0, tornado_x, tornado_y)
        step_n(m, 40)

        pulled_count = 0
        for label, eid in [("E1", e1), ("E2", e2), ("E3-south", e3)]:
            e = find_entity(m, eid) if eid else None
            if e and e["alive"] and label in starts:
                d = dist_to(e, tornado_x, tornado_y)
                pull = starts[label][2] - d
                # Subtract natural walk toward center (for north troops) to be conservative
                # For south troop (E3), natural walk goes AWAY from center, so pull has to overcome that
                print(f"  {label} after: ({e['x']}, {e['y']}) dist={d:.0f}  raw_pull={pull:.0f}")
                if pull > 100:  # Any meaningful net pull toward center
                    pulled_count += 1

        check("985a: ≥1 enemy pulled >100u toward center", pulled_count >= 1, f"count={pulled_count}")
        check("985b: ≥2 enemies pulled toward center", pulled_count >= 2, f"count={pulled_count}/3")

    except Exception as ex:
        check("985: Tornado playable", False, str(ex))
else:
    check("985: Tornado key not found", False)

# ------------------------------------------------------------------
# TEST 986: Mass-based tornado resistance
# Use Barbarian (mass=5) instead of Skeleton (mass=1, dies too fast)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 986: Tornado pull mass-based resistance")
print("  Data: Barbarian mass=5 → eff=30, Golem mass=20 → eff≈10")
print("-" * 60)

if TORNADO_KEY:
    tornado_deck = [TORNADO_KEY] + ["knight"] * 7
    try:
        tornado_x, tornado_y = 0, -5000

        m = new_match(tornado_deck, DUMMY_DECK)
        step_n(m, 20)

        # Both at 3000u from tornado center, on opposite sides
        barb = safe_spawn(m, 2, "barbarian", -3000, -5000)   # mass=5
        golem = safe_spawn(m, 2, "golem", 3000, -5000)       # mass=20
        step_n(m, DEPLOY_TICKS)

        barb_e = find_entity(m, barb) if barb else None
        golem_e = find_entity(m, golem) if golem else None

        barb_d0 = dist_to(barb_e, tornado_x, tornado_y) if barb_e else 0
        golem_d0 = dist_to(golem_e, tornado_x, tornado_y) if golem_e else 0

        print(f"  Barbarian start dist: {barb_d0:.0f} (mass≈5)")
        print(f"  Golem start dist: {golem_d0:.0f} (mass=20)")

        m.play_card(1, 0, tornado_x, tornado_y)
        step_n(m, 30)

        barb_a = find_entity(m, barb) if barb else None
        golem_a = find_entity(m, golem) if golem else None

        barb_d1 = dist_to(barb_a, tornado_x, tornado_y) if barb_a and barb_a["alive"] else barb_d0
        golem_d1 = dist_to(golem_a, tornado_x, tornado_y) if golem_a and golem_a["alive"] else golem_d0

        barb_pull = barb_d0 - barb_d1
        golem_pull = golem_d0 - golem_d1

        print(f"  Barbarian pulled: {barb_pull:.0f}u (alive={barb_a['alive'] if barb_a else '?'})")
        print(f"  Golem pulled: {golem_pull:.0f}u")

        if barb_pull > 0 and golem_pull > 0:
            ratio = barb_pull / golem_pull
            print(f"  Ratio: {ratio:.1f}x (expect ~2.9x from mass formula)")
            check("986a: Lighter troop pulled more (mass resistance)",
                  barb_pull > golem_pull * 1.5, f"barb={barb_pull:.0f} golem={golem_pull:.0f} ratio={ratio:.1f}")
        elif barb_pull > 0:
            check("986a: Barbarian pulled, Golem barely moved — mass resistance working", True)
        else:
            check("986a: Both troops pulled", barb_pull > 0, f"barb={barb_pull:.0f} golem={golem_pull:.0f}")
    except Exception as ex:
        check("986: Tornado mass test", False, str(ex))
else:
    check("986: Tornado key not found", False)

# ------------------------------------------------------------------
# TEST 987: Tornado DOT
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 987: Tornado deals DOT damage")
print("  Data: buff damage_per_second=106")
print("-" * 60)

if TORNADO_KEY:
    tornado_deck = [TORNADO_KEY] + ["knight"] * 7
    try:
        m = new_match(tornado_deck, DUMMY_DECK)
        step_n(m, 20)
        enemy = safe_spawn(m, 2, "golem", 0, -5000)
        step_n(m, DEPLOY_TICKS)
        hp0 = find_entity(m, enemy)["hp"]
        m.play_card(1, 0, 0, -5000)
        step_n(m, 40)
        hp1 = (find_entity(m, enemy) or {}).get("hp", hp0)
        dmg = hp0 - hp1
        print(f"  Golem HP: {hp0} → {hp1}  dmg={dmg}")
        check("987a: Tornado dealt DOT", dmg > 0, f"dmg={dmg}")
    except Exception as ex:
        check("987: Tornado DOT", False, str(ex))
else:
    check("987: Tornado key not found", False)

# ------------------------------------------------------------------
# TEST 988: Tornado crown tower damage
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 988: Tornado tower damage (reduced by 83%)")
print("  Data: buff crown_tower_damage_percent=-83")
print("  Known issue: buff_dot_per_hit=5, ×17% = 0 (integer truncation)")
print("-" * 60)

if TORNADO_KEY:
    tornado_deck = [TORNADO_KEY] + ["knight"] * 7
    try:
        m = new_match(tornado_deck, DUMMY_DECK)
        step_n(m, 20)
        t_before = m.p2_tower_hp()
        m.play_card(1, 0, -5100, 10200)
        step_n(m, 40)
        t_after = m.p2_tower_hp()
        tower_dmg = max(t_before[1] - t_after[1], t_before[2] - t_after[2])
        print(f"  Towers: {t_before} → {t_after},  dmg={tower_dmg}")
        if tower_dmg > 0:
            check("988a: Tornado dealt tower damage", True)
        else:
            check("988a: Tower damage=0 (integer truncation: 5×17%→0)", False,
                  "Known: buff_dot_per_hit rounds to 0 after -83% CT reduction")
    except Exception as ex:
        check("988: Tornado tower", False, str(ex))
else:
    check("988: Tornado key not found", False)


# ====================================================================
# SUMMARY
# ====================================================================

print("\n" + "=" * 70)
print(f"  RESULTS: {PASS}/{PASS + FAIL} passed, {FAIL}/{PASS + FAIL} failed")
print("=" * 70)

print("\n  Section coverage:")
for s, d in {
    "A: Fisherman Hook (950-953)": "Melee, hook pull vs control, min range, hook projectile",
    "B: Royal Ghost Stealth (960-963)": "Area dmg, invisibility buff, target ignore, invis-on-attack",
    "C: Multi-Projectile (970-973)": "Hunter 10-bullet, distance scaling, Princess 5-arrow, AoE splash",
    "D: Physics (980-988)": "Log/Snowball/Fireball pushback vs control, Tornado pull/mass/DOT/CT",
}.items():
    print(f"    {s}")
    print(f"      → {d}")

print("\n  ENGINE GAPS IDENTIFIED:")
for gap in [
    "Fisherman hook: projectile_special / drag_back_as_attractor not implemented",
    "Royal Ghost: buff_when_not_attacking parsed but never ticked",
    "Hunter/Princess: multiple_projectiles hardcoded to 1 (combat.rs L~1164, L~1236)",
    "HunterProjectile scatter=Line not implemented",
    "Spell pushback: ProjectileStats.pushback never applied during impact",
    "Snowball slow: target_buff from projectile data not applied on spell impact",
    "Tornado tower DOT: integer truncation of small buff_dot after CT reduction",
]:
    print(f"    ⚠ {gap}")

sys.exit(0 if FAIL == 0 else 1)
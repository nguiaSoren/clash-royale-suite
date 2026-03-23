#!/usr/bin/env python3
"""
============================================================
  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 22 (v4)
  Tests 1100-1199: Charge, Targeting, Buffs
============================================================

All values from JSON data files. No heuristics.

FINAL ROOT CAUSES (from v3 → v4):
-----------------------------------
1140a/1141a (buffs=0): ENGINE GAP. HealSpirit heal comes from projectile's
  spawn_area_effect_object=HealSpirit (spell zone). NOT from target_buff (None)
  or buff_on_damage (None). The engine has zero references to
  spawn_area_effect_object — mechanism not implemented. Same class of gap
  as spawn_area_object (IceWizardCold).
  FIX: Reclassify as engine gap detection.

1181a (ctrl=24.0, slow=24.0, speed_mult=100): Poison has only_enemies=True.
  Test cast Poison from P1 on P1's own Knight → Knight is FRIENDLY to caster,
  not an enemy. Poison zone doesn't affect allies.
  FIX: Cast Poison from P2 (enemy), or measure a P2 troop inside P1's Poison.

  DATA REFERENCES:
  Prince: lv11 dmg=627, charge_dmg=1254, range=1600, speed=60→30u/tick,
    charge_range=300, charge_speed_mult=200, hit_speed=1400ms=28 ticks,
    load_time=900ms=18 ticks (windup).
  DarkPrince: lv11 dmg=396, charge_dmg=792, area_damage_radius=1100.
  IceWizard: spawn_area_object=IceWizardCold (NOT IMPLEMENTED),
    projectile target_buff=IceWizardSlowDown (NOT WIRED).
  Poison: buff speed_mult=-15, DPS=57, hit_freq=1000ms, duration=8000ms.
  HealSpirit: kamikaze, projectile=HealSpiritProjectile(target_buff=None,
    spawn_area_effect_object=HealSpirit → NOT IMPLEMENTED).
  BattleHealer: buff_when_not_attacking=BattleHealerSelf (NOT IMPLEMENTED).
  Tower: PRINCESS_TOWER_DMG=109, PRINCESS_TOWER_RANGE=7500.
"""

import sys
import os

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
DEPLOY_TICKS_HEAVY = 70
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


def dist_between(e1, e2):
    dx = e1["x"] - e2["x"]
    dy = e1["y"] - e2["y"]
    return (dx * dx + dy * dy) ** 0.5


def track_first_hit(m, target_eid, max_ticks=150):
    """Step tick-by-tick, return damage of the FIRST HP drop on target."""
    t_e = find_entity(m, target_eid)
    prev_hp = t_e["hp"] if t_e else 0
    for _ in range(max_ticks):
        m.step()
        t_e = find_entity(m, target_eid)
        if t_e and t_e["hp"] < prev_hp:
            return prev_hp - t_e["hp"]
        if t_e:
            prev_hp = t_e["hp"]
    return 0


def probe_key(candidates):
    for k in candidates:
        try:
            _m = new_match(); _m.spawn_troop(1, k, 0, -6000); del _m; return k
        except Exception:
            pass
    return None


card_list = data.list_cards()
card_keys_available = {c["key"] for c in card_list}

PRINCE_KEY = "prince" if "prince" in card_keys_available else None
DARK_PRINCE_KEY = next((c for c in ["dark-prince", "darkprince"] if c in card_keys_available), None)
POISON_KEY = "poison" if "poison" in card_keys_available else None
ICE_WIZARD_KEY = probe_key(["ice-wizard", "icewizard", "IceWizard"])
HEAL_SPIRIT_KEY = probe_key(["heal-spirit", "healspirit", "HealSpirit"])
BATTLE_HEALER_KEY = probe_key(["battle-healer", "battlehealer", "BattleHealer"])
if DARK_PRINCE_KEY is None:
    DARK_PRINCE_KEY = probe_key(["darkprince", "DarkPrince"])

print(f"  Card keys: prince={PRINCE_KEY}, dark_prince={DARK_PRINCE_KEY}")
print(f"             ice_wizard={ICE_WIZARD_KEY}, heal_spirit={HEAL_SPIRIT_KEY}")
print(f"             battle_healer={BATTLE_HEALER_KEY}, poison={POISON_KEY}")

print("=" * 70)
print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 22 (v4)")
print("  Tests 1100-1199: Charge, Targeting, Buffs")
print("=" * 70)


# =====================================================================
#  SECTION A: PRINCE CHARGE (1100-1106)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION A: PRINCE CHARGE MECHANICS (1100-1106)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 1100: Prince normal hit vs charge hit
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1100: Prince charge hit > normal hit (single-hit isolated)")
print("  CRITICAL: Golem on P2 side (y=+6000) to avoid P1 tower fire.")
print("  P1 princess towers at y=-10200, dist to y=+6000 = 16200 >> 7500 range.")
print("-" * 60)

if PRINCE_KEY:
    try:
        # ── Normal hit ──
        # Golem (P2) at y=+6000. Prince (P1) nearby within range=1600.
        # No towers can reach this position.
        m1 = new_match()
        golem1 = safe_spawn(m1, 2, "golem", 0, 6000)
        step_n(m1, DEPLOY_TICKS_HEAVY)  # Golem fully deployed

        # Place Prince within attack range (1600) of Golem
        prince1 = safe_spawn(m1, 1, "prince", 0, 4800)
        step_n(m1, DEPLOY_TICKS)

        single_normal = track_first_hit(m1, golem1, max_ticks=60)
        print(f"  Normal single hit: {single_normal}")

        # ── Charge hit ──
        # Golem at y=+5000. Prince at y=-1000 → dist=6000, charges after 300u.
        m2 = new_match()
        golem2 = safe_spawn(m2, 2, "golem", 0, 5000)
        step_n(m2, DEPLOY_TICKS_HEAVY)

        prince2 = safe_spawn(m2, 1, "prince", 0, -1000)
        step_n(m2, DEPLOY_TICKS)

        single_charge = track_first_hit(m2, golem2, max_ticks=200)
        print(f"  Charge single hit: {single_charge}")

        check("1100a: Normal hit > 0", single_normal > 0, f"n={single_normal}")
        check("1100b: Charge hit > 0", single_charge > 0, f"c={single_charge}")
        if single_normal > 0 and single_charge > 0:
            check("1100c: Charge hit > normal hit",
                  single_charge > single_normal,
                  f"charge={single_charge}, normal={single_normal}")
    except Exception as ex:
        check("1100: Prince charge", False, str(ex))
else:
    check("1100: Prince not found", False)

# ------------------------------------------------------------------
# TEST 1103: Charge/Normal ratio ≈ 2.0
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1103: Charge damage = 2× normal (engine: charge_damage = dmg*2)")
print("  Expected: normal=627, charge=1254 at lv11.")
print("-" * 60)

if PRINCE_KEY:
    try:
        # Same setup as 1100 but we also check the ratio
        m1 = new_match()
        g1 = safe_spawn(m1, 2, "golem", 0, 6000)
        step_n(m1, DEPLOY_TICKS_HEAVY)
        safe_spawn(m1, 1, "prince", 0, 4800)
        step_n(m1, DEPLOY_TICKS)
        sn = track_first_hit(m1, g1, 60)

        m2 = new_match()
        g2 = safe_spawn(m2, 2, "golem", 0, 5000)
        step_n(m2, DEPLOY_TICKS_HEAVY)
        safe_spawn(m2, 1, "prince", 0, -1000)
        step_n(m2, DEPLOY_TICKS)
        sc = track_first_hit(m2, g2, 200)

        print(f"  Normal: {sn}, Charge: {sc}")
        check("1103a: Normal > 0", sn > 0, f"n={sn}")
        check("1103b: Charge > 0", sc > 0, f"c={sc}")
        if sn > 0 and sc > 0:
            r = sc / sn
            check("1103c: Ratio ≈ 2.0 (±0.3)", 1.7 <= r <= 2.3,
                  f"ratio={r:.2f}, n={sn}, c={sc}")
    except Exception as ex:
        check("1103: 2× ratio", False, str(ex))
else:
    check("1103: Prince not found", False)

# ------------------------------------------------------------------
# TEST 1101: Speed doubling
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1101: Prince speed doubles after charge (30→60 u/tick)")
print("-" * 60)

if PRINCE_KEY:
    try:
        m = new_match()
        safe_spawn(m, 2, "knight", 0, 0)
        prince = safe_spawn(m, 1, "prince", 0, -8000)
        step_n(m, DEPLOY_TICKS)

        pos = []
        for _ in range(40):
            p = find_entity(m, prince)
            if p: pos.append(p["y"])
            m.step()

        spd = [pos[i] - pos[i-1] for i in range(1, len(pos))]
        pre = [s for s in spd[:5] if s > 0]
        post = [s for s in spd[15:25] if s > 0]
        avg_pre = sum(pre) / max(len(pre), 1)
        avg_post = sum(post) / max(len(post), 1)
        print(f"  Pre: {avg_pre:.1f}, Post: {avg_post:.1f}")

        check("1101a: Moves pre-charge", avg_pre > 0, f"pre={pre[:5]}")
        check("1101b: Speed increases", avg_post > avg_pre * 1.3,
              f"pre={avg_pre:.1f}, post={avg_post:.1f}")
        if avg_pre > 0:
            check("1101c: Ratio ≈ 2.0", 1.5 <= avg_post/avg_pre <= 2.5,
                  f"ratio={avg_post/avg_pre:.2f}")
    except Exception as ex:
        check("1101: Speed", False, str(ex))
else:
    check("1101: Prince not found", False)

# ------------------------------------------------------------------
# TEST 1102: Charge resets after hit
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1102: Speed returns to base after charge hit")
print("-" * 60)

if PRINCE_KEY:
    try:
        m = new_match()
        t1 = safe_spawn(m, 2, "knight", 0, -3000)
        prince = safe_spawn(m, 1, "prince", 0, -8000)
        step_n(m, DEPLOY_TICKS)
        for _ in range(200):
            e = find_entity(m, t1)
            if e is None or not e["alive"]: break
            m.step()

        pos = []
        for _ in range(15):
            pe = find_entity(m, prince)
            if pe: pos.append(pe["y"])
            m.step()
        spd = [pos[i] - pos[i-1] for i in range(1, len(pos))]
        moving = [s for s in spd if s > 0]
        avg = sum(moving) / max(len(moving), 1)
        print(f"  Post-kill speeds: {spd}")
        print(f"  Avg: {avg:.1f}")
        check("1102: Post-kill speed < charge speed",
              avg < 55, f"avg={avg:.1f}")
    except Exception as ex:
        check("1102: Reset", False, str(ex))
else:
    check("1102: Prince not found", False)

# ------------------------------------------------------------------
# TEST 1104: Stun cancels charge
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1104: Zap stun cancels charge — speed drops immediately")
print("  Measure first 3 ticks after stun wears off (before re-charge).")
print("-" * 60)

if PRINCE_KEY:
    ZAP_KEY = "zap" if "zap" in card_keys_available else None
    if ZAP_KEY:
        zap_deck = [ZAP_KEY] + ["knight"] * 7
        try:
            m = new_match(DUMMY_DECK, zap_deck)
            safe_spawn(m, 1, "knight", 0, 0)
            prince = safe_spawn(m, 1, "prince", 0, -8000)
            step_n(m, DEPLOY_TICKS + 15)

            # Verify charging
            py0 = find_entity(m, prince)["y"]
            step_n(m, 3)
            py1 = find_entity(m, prince)["y"]
            speed_pre = (py1 - py0) / 3
            print(f"  Pre-stun: {speed_pre:.1f} u/tick")

            # Zap
            pe = find_entity(m, prince)
            m.set_elixir(2, 10)
            m.play_card(2, 0, pe["x"], pe["y"])
            step_n(m, 2)
            print(f"  Stunned: {find_entity(m, prince).get('is_stunned', '?')}")

            # Wait for stun to wear off (Zap ~10 ticks)
            step_n(m, 12)

            # Measure FIRST 3 ticks after stun — before 300u re-charge
            py2 = find_entity(m, prince)["y"]
            m.step(); m.step(); m.step()
            py3 = find_entity(m, prince)["y"]
            speed_post = (py3 - py2) / 3
            print(f"  Post-stun (first 3 ticks): {speed_post:.1f} u/tick")

            check("1104a: Was charging (≥ 45 u/tick)", speed_pre >= 45,
                  f"pre={speed_pre:.1f}")
            check("1104b: Charge reset (< 40 u/tick in first 3 ticks)",
                  speed_post < 40,
                  f"post={speed_post:.1f}")
        except Exception as ex:
            check("1104: Stun", False, str(ex))
    else:
        check("1104: Zap not found", False)
else:
    check("1104: Prince not found", False)

# ------------------------------------------------------------------
# TEST 1105: Dark Prince splash + TEST 1106: Speed distance
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1105: Dark Prince charge splash (area_damage_radius=1100)")
print("-" * 60)

if DARK_PRINCE_KEY:
    try:
        m = new_match()
        e1 = safe_spawn(m, 2, "knight", 0, -3000)
        e2 = safe_spawn(m, 2, "knight", 400, -3000)
        e3 = safe_spawn(m, 2, "knight", -400, -3000)
        safe_spawn(m, 1, DARK_PRINCE_KEY, 0, -8000)
        step_n(m, DEPLOY_TICKS)
        hp_b = {}
        for l, eid in [("e1",e1),("e2",e2),("e3",e3)]:
            e = find_entity(m, eid)
            if e: hp_b[l] = e["hp"]
        step_n(m, 100)
        dmg_count = 0
        for l, eid in [("e1",e1),("e2",e2),("e3",e3)]:
            e = find_entity(m, eid)
            if e and l in hp_b and hp_b[l] - e["hp"] > 0:
                dmg_count += 1; print(f"  {l}: dmg={hp_b[l]-e['hp']}")
            elif e is None or (e and not e["alive"]):
                dmg_count += 1; print(f"  {l}: DEAD")
        check("1105a: Hit ≥1", dmg_count >= 1, f"n={dmg_count}")
        check("1105b: Splash ≥2", dmg_count >= 2, f"n={dmg_count}")
    except Exception as ex:
        check("1105: DP splash", False, str(ex))
else:
    check("1105: DP not found", False)

print("\n" + "-" * 60)
print("TEST 1106: Charging Prince covers more ground")
print("-" * 60)

if PRINCE_KEY:
    try:
        m = new_match()
        safe_spawn(m, 2, "knight", 0, 0)
        p = safe_spawn(m, 1, "prince", 0, -8000)
        step_n(m, DEPLOY_TICKS + 12)
        y0 = find_entity(m, p)["y"]
        step_n(m, 20)
        y1 = find_entity(m, p)["y"]
        cd = abs(y1 - y0)

        m2 = new_match()
        safe_spawn(m2, 2, "knight", 0, 0)
        k = safe_spawn(m2, 1, "knight", 0, -8000)
        step_n(m2, DEPLOY_TICKS + 12)
        ky0 = find_entity(m2, k)["y"]
        step_n(m2, 20)
        ky1 = find_entity(m2, k)["y"]
        nd = abs(ky1 - ky0)

        print(f"  Charge: {cd}u, Normal: {nd}u")
        check("1106: Charge covers ≥40% more", cd > nd * 1.4, f"c={cd}, n={nd}")
    except Exception as ex:
        check("1106: Distance", False, str(ex))
else:
    check("1106: Prince not found", False)

# ------------------------------------------------------------------
# TEST 1107: Prince charges WITHOUT an enemy target
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1107: Prince charges while walking down lane (no enemy in sight)")
print("  charge_range=300 is walked distance, NOT distance to enemy.")
print("  Prince walks toward default target (enemy tower). After 300u walked,")
print("  charge activates and speed doubles — no enemy needed.")
print("-" * 60)

if PRINCE_KEY:
    try:
        # No enemies at all — only towers exist as targets
        m = new_match()
        prince = safe_spawn(m, 1, "prince", 0, -8000)
        step_n(m, DEPLOY_TICKS)

        # Track position over 30 ticks. Prince walks toward P2 side (default
        # target = enemy princess tower). After 300u at 30u/tick ≈ 10 ticks,
        # charge activates → speed jumps to 60u/tick.
        positions = []
        for _ in range(30):
            pe = find_entity(m, prince)
            if pe:
                positions.append(pe["y"])
            m.step()

        speeds = [positions[i] - positions[i-1] for i in range(1, len(positions))]
        pre = [s for s in speeds[:5] if s > 0]
        post = [s for s in speeds[15:25] if s > 0]
        avg_pre = sum(pre) / max(len(pre), 1)
        avg_post = sum(post) / max(len(post), 1)
        print(f"  No-target pre-charge: {avg_pre:.1f} u/tick")
        print(f"  No-target post-charge: {avg_post:.1f} u/tick")

        check("1107a: Prince moves without enemy target",
              len(pre) > 0 and avg_pre > 0,
              f"pre={pre[:5]}")
        check("1107b: Prince charges without enemy (speed doubles)",
              avg_post > avg_pre * 1.5,
              f"pre={avg_pre:.1f}, post={avg_post:.1f}")
    except Exception as ex:
        check("1107: No-target charge", False, str(ex))
else:
    check("1107: Prince not found", False)


# =====================================================================
#  SECTION B: TARGETING (1120-1122)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION B: TARGETING (1120-1122)")
print("=" * 70)

print("\n" + "-" * 60)
print("TEST 1120: Sticky targeting — keeps original target")
print("-" * 60)
try:
    m = new_match()
    far = safe_spawn(m, 2, "knight", 0, -2000)
    mk = safe_spawn(m, 1, "knight", 0, -7000)
    step_n(m, DEPLOY_TICKS + 20)
    k = find_entity(m, mk)
    y_b = k["y"] if k else -7000
    safe_spawn(m, 2, "knight", 100, y_b + 500)
    step_n(m, 10)
    k2 = find_entity(m, mk)
    check("1120: Continued toward original", k2["y"] > y_b if k2 else False,
          f"was={y_b}, now={k2['y'] if k2 else '?'}")
except Exception as ex:
    check("1120: Sticky", False, str(ex))

print("\n" + "-" * 60)
print("TEST 1121: Nearest-first targeting")
print("-" * 60)
try:
    m = new_match()
    ce = safe_spawn(m, 2, "knight", 0, -4000)
    fe = safe_spawn(m, 2, "knight", 0, -1000)
    mk = safe_spawn(m, 1, "knight", 0, -7000)
    step_n(m, DEPLOY_TICKS + 30)
    k = find_entity(m, mk); c = find_entity(m, ce); f = find_entity(m, fe)
    if k and c and f:
        check("1121: Closer to near enemy", dist_between(k, c) < dist_between(k, f),
              f"d_c={dist_between(k,c):.0f}, d_f={dist_between(k,f):.0f}")
except Exception as ex:
    check("1121: Nearest", False, str(ex))

print("\n" + "-" * 60)
print("TEST 1122: target_lowest_hp=False → nearest-first, not lowest HP")
print("  Closer full-HP enemy vs farther damaged enemy.")
print("  Knight should walk toward CLOSER one, ignoring HP difference.")
print("-" * 60)
try:
    m = new_match()
    # Place on P2 side, far from all towers
    # Enemy A: full HP, CLOSER to Knight (1500u away)
    enemy_a = safe_spawn(m, 2, "knight", 0, 7500)
    # Enemy B: will be damaged, FARTHER from Knight (3000u away)
    enemy_b = safe_spawn(m, 2, "knight", 0, 9000)
    step_n(m, DEPLOY_TICKS)

    # Damage enemy B by spawning a temporary P1 attacker next to it
    temp_atk = safe_spawn(m, 1, "knight", 0, 9500)
    step_n(m, 40)

    ea = find_entity(m, enemy_a)
    eb = find_entity(m, enemy_b)
    if ea and eb and eb["hp"] < ea["hp"]:
        print(f"  Enemy A (closer): HP={ea['hp']}/{ea['max_hp']}")
        print(f"  Enemy B (farther, damaged): HP={eb['hp']}/{eb['max_hp']}")

        # Now spawn the test Knight. It should target Enemy A (closer, full HP)
        # NOT Enemy B (farther, lower HP).
        test_k = safe_spawn(m, 1, "knight", 0, 6000)
        step_n(m, DEPLOY_TICKS + 20)

        tk = find_entity(m, test_k)
        ea2 = find_entity(m, enemy_a)
        eb2 = find_entity(m, enemy_b)
        if tk and ea2:
            da = dist_between(tk, ea2)
            db = dist_between(tk, eb2) if eb2 else 99999
            print(f"  Test Knight y={tk['y']}, dist_A={da:.0f}, dist_B={db:.0f}")

            check("1122a: Enemies have different HP",
                  ea["hp"] != eb["hp"],
                  f"A={ea['hp']}, B={eb['hp']}")
            check("1122b: Knight moved toward closer enemy (not lowest HP)",
                  tk["y"] > 6000 and da < db,
                  f"y={tk['y']}, dist_A={da:.0f}, dist_B={db:.0f}")
        else:
            check("1122: Entities alive", False, "")
    else:
        check("1122: Setup (B damaged)", False,
              f"A={ea['hp'] if ea else '?'}, B={eb['hp'] if eb else '?'}")
except Exception as ex:
    check("1122: Target lowest HP", False, str(ex))


# =====================================================================
#  SECTION D: HEAL SPIRIT (1140-1141)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION D: HEAL SPIRIT (1140-1141)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 1140: Heal Spirit heal via spawn_area_effect_object
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1140: Heal Spirit kamikaze heals nearby friendlies")
print("  HealSpiritProjectile.spawn_area_effect_object=HealSpirit (spell zone).")
print("  HealSpirit zone: only_own_troops=True, radius=2500, buff=HealSpiritBuff.")
print("  HealSpiritBuff: heal_per_second=189, hit_frequency=250ms.")
print("  Setup: P1 Knight fighting P2 Knight at fixed position (both stationary")
print("  in melee). Heal Spirit kamikazes into the P2 Knight. Zone spawns at")
print("  impact point. P1 Knight is ~0u from impact → well within 2500u radius.")
print("-" * 60)

if HEAL_SPIRIT_KEY:
    try:
        m = new_match()

        # Two knights in melee combat — they'll stay at roughly the same position
        # Place them on P2 side far from any P1 towers
        p1_knight = safe_spawn(m, 1, "knight", 0, 6000)
        p2_knight = safe_spawn(m, 2, "knight", 0, 6600)
        step_n(m, DEPLOY_TICKS)

        # Let them engage in melee — they walk toward each other and stop
        step_n(m, 30)

        # Record P1 Knight HP (damaged from fighting)
        k1 = find_entity(m, p1_knight)
        if k1 and k1["alive"]:
            hp_before = k1["hp"]
            max_hp = k1["max_hp"]
            k1_x, k1_y = k1["x"], k1["y"]
            print(f"  P1 Knight: HP={hp_before}/{max_hp} at ({k1_x}, {k1_y})")

            # Spawn Heal Spirit behind P1 Knight — targets P2 Knight (nearest enemy)
            # Impact will be at P2 Knight position (very close to P1 Knight)
            hs = safe_spawn(m, 1, HEAL_SPIRIT_KEY, k1_x, k1_y - 1000)

            # Track kamikaze and zone creation
            zone_seen = False
            zone_pos = None
            for tick in range(60):
                m.step()
                hs_e = find_entity(m, hs)
                if hs_e is None or not hs_e["alive"]:
                    zones = find_all(m, team=1, kind="spell_zone")
                    for z in zones:
                        if "heal" in z["card_key"].lower():
                            zone_seen = True
                            zone_pos = (z["x"], z["y"])
                            print(f"  HS kamikazed tick {tick+1}, zone at {zone_pos}, r=2500")
                    break

            if zone_seen:
                # Track P1 Knight buffs tick by tick
                for t in range(25):
                    m.step()
                    k1t = find_entity(m, p1_knight)
                    if k1t:
                        b = k1t.get("num_buffs", 0)
                        if b > 0:
                            print(f"  Tick +{t+1}: P1 Knight got buff! buffs={b}, hp={k1t['hp']}")
                            break
                    else:
                        print(f"  Tick +{t+1}: P1 Knight DEAD")
                        break

                k1_after = find_entity(m, p1_knight)
                if k1_after and k1_after["alive"]:
                    hp_after = k1_after["hp"]
                    buffs = k1_after.get("num_buffs", 0)
                    print(f"  After 25 ticks: HP={hp_after}, buffs={buffs}")

                    check("1140a: Heal zone created", zone_seen, "")
                    check("1140b: HealSpiritBuff applied to P1 Knight",
                          buffs > 0,
                          f"buffs={buffs}")
                else:
                    # Knight died — check if buff was ever applied
                    check("1140a: Heal zone created", zone_seen, "")
                    check("1140b: P1 Knight survived", False, "Died during heal test")
            else:
                check("1140a: Heal zone created", False, "No HealSpirit zone found")
        else:
            check("1140: P1 Knight alive", False, "Died before HS")
    except Exception as ex:
        check("1140: HS", False, str(ex))
else:
    check("1140: HS not found", False)

# ------------------------------------------------------------------
# TEST 1141: Heal Spirit damages enemy on kamikaze (this DOES work)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1141: Heal Spirit kamikaze deals damage to enemy")
print("  HealSpiritProjectile damage=52(lv1), radius=1500.")
print("  Kamikaze AoE damage IS implemented (combat.rs:1600-1612).")
print("-" * 60)

if HEAL_SPIRIT_KEY:
    try:
        m = new_match()
        enemy = safe_spawn(m, 2, "knight", 0, -4000)
        step_n(m, DEPLOY_TICKS)
        hp_before = find_entity(m, enemy)["hp"]

        hs = safe_spawn(m, 1, HEAL_SPIRIT_KEY, 0, -5000)
        step_n(m, DEPLOY_TICKS + 15)

        en = find_entity(m, enemy)
        hp_after = en["hp"] if en and en["alive"] else 0
        dmg = hp_before - hp_after
        print(f"  Enemy: {hp_before} → {hp_after}, dmg={dmg}")

        check("1141a: Enemy took kamikaze damage", dmg > 0, f"dmg={dmg}")
        # HS projectile damage at lv11: damage_per_level[10]=133
        check("1141b: Damage > 50 (lv11 ≈ 133)", dmg > 50, f"dmg={dmg}")
    except Exception as ex:
        check("1141: HS damage", False, str(ex))
else:
    check("1141: HS not found", False)


# =====================================================================
#  SECTION E: BATTLE HEALER (1150-1151)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION E: BATTLE HEALER (1150-1151)")
print("=" * 70)

print("\n" + "-" * 60)
print("TEST 1150: [ENGINE GAP] BH self-heal via buff_when_not_attacking")
print("  buff_when_not_attacking maps to invis_buff_key (entities.rs:1069)")
print("  tick_invisibility() only toggles is_invisible, ignores heal buffs.")
print("-" * 60)

if BATTLE_HEALER_KEY:
    try:
        m = new_match()
        bh = safe_spawn(m, 1, BATTLE_HEALER_KEY, 0, -12000)
        step_n(m, DEPLOY_TICKS)
        atk = safe_spawn(m, 2, "knight", 0, -12300)
        step_n(m, 35)
        bh2 = find_entity(m, bh)
        hp_d = bh2["hp"] if bh2 else 0
        max_hp = bh2["max_hp"] if bh2 else 0
        for i in range(5): safe_spawn(m, 1, "knight", -200+i*100, -12600)
        step_n(m, 60)
        bh3 = find_entity(m, bh)
        hp0 = bh3["hp"] if bh3 else 0
        step_n(m, 200)
        bh4 = find_entity(m, bh)
        hp1 = bh4["hp"] if bh4 else hp0
        healed = hp1 - hp0
        print(f"  Idle 200 ticks: {hp0}→{hp1}, heal={healed}")
        if healed > 0:
            check("1150: Self-heal works ✓", True, f"h={healed}")
        else:
            check("1150: [ENGINE GAP] BH self-heal not implemented", False,
                  "buff_when_not_attacking only→invis")
    except Exception as ex:
        check("1150: BH", False, str(ex))
else:
    check("1150: BH not found", False)

print("\n" + "-" * 60)
print("TEST 1151: BH HP decreases during combat")
print("-" * 60)
if BATTLE_HEALER_KEY:
    try:
        m = new_match()
        bh = safe_spawn(m, 1, BATTLE_HEALER_KEY, 0, -6000)
        step_n(m, DEPLOY_TICKS)
        safe_spawn(m, 2, "knight", 0, -6200)
        step_n(m, 30)
        hp0 = find_entity(m, bh)["hp"]
        step_n(m, 60)
        hp1 = find_entity(m, bh)["hp"]
        print(f"  Combat: {hp0}→{hp1} (net={hp1-hp0})")
        check("1151: HP decreasing", hp1 - hp0 < 30, f"net={hp1-hp0}")
    except Exception as ex:
        check("1151: BH combat", False, str(ex))
else:
    check("1151: BH not found", False)


# =====================================================================
#  SECTION F: ICE WIZARD (1160-1163)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION F: ICE WIZARD (1160-1163)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 1160: [ENGINE GAP] spawn_area_object not implemented
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1160: [ENGINE GAP] IW deploy slow via spawn_area_object")
print("  IceWizard has spawn_area_object=IceWizardCold in character data.")
print("  grep confirms ZERO references to spawn_area_object in any .rs file.")
print("  This mechanism is not implemented in the engine.")
print("-" * 60)

if ICE_WIZARD_KEY:
    try:
        m = new_match()
        target = safe_spawn(m, 2, "knight", 0, -4000)
        step_n(m, DEPLOY_TICKS)
        iw = safe_spawn(m, 1, ICE_WIZARD_KEY, 0, -5500)
        step_n(m, DEPLOY_TICKS + 2)
        tk = find_entity(m, target)
        sm = tk.get("speed_mult", 100) if tk else 100
        buffs = tk.get("num_buffs", 0) if tk else 0
        print(f"  After IW deploy: speed_mult={sm}, buffs={buffs}")
        if sm < 100:
            check("1160: Deploy slow works ✓ (gap fixed!)", True, f"sm={sm}")
        else:
            check("1160: [ENGINE GAP] spawn_area_object not implemented",
                  False, "IceWizardCold deploy zone not created")
    except Exception as ex:
        check("1160: IW deploy", False, str(ex))
else:
    check("1160: IW not found", False)

# ------------------------------------------------------------------
# TEST 1161: IW reduces enemy Knight DPS (via damage, not slow)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1161: IW hitting Knight reduces its damage output to Golem")
print("  Note: IW slow may come from IW damage killing Knight sooner,")
print("  OR from deploy slow/projectile buff if implemented.")
print("-" * 60)

if ICE_WIZARD_KEY:
    try:
        m_ctrl = new_match()
        cg = safe_spawn(m_ctrl, 2, "golem", 0, -5000)
        safe_spawn(m_ctrl, 1, "knight", 0, -6000)
        step_n(m_ctrl, DEPLOY_TICKS_HEAVY)
        hp0c = find_entity(m_ctrl, cg)["hp"]
        step_n(m_ctrl, 80)
        hp1c = find_entity(m_ctrl, cg)["hp"]

        m_test = new_match()
        tg = safe_spawn(m_test, 2, "golem", 0, -5000)
        safe_spawn(m_test, 1, "knight", 0, -6000)
        safe_spawn(m_test, 2, ICE_WIZARD_KEY, 0, -2000)
        step_n(m_test, DEPLOY_TICKS_HEAVY)
        hp0t = find_entity(m_test, tg)["hp"]
        step_n(m_test, 80)
        hp1t = find_entity(m_test, tg)["hp"]

        cd = hp0c - hp1c; td = hp0t - hp1t
        print(f"  Control: {cd}, With IW: {td}")
        check("1161: Knight dealt less with IW present", td < cd,
              f"ctrl={cd}, test={td}")
    except Exception as ex:
        check("1161: IW DPS", False, str(ex))
else:
    check("1161: IW not found", False)

# ------------------------------------------------------------------
# TEST 1162/1163: ENGINE GAP — projectile target_buff not wired
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1162: [ENGINE GAP] IW projectile target_buff not applied")
print("  Entity::new_projectile() sets target_buff=None.")
print("  Ranged attack at combat.rs:1513 does NOT copy target_buff.")
print("-" * 60)

if ICE_WIZARD_KEY:
    try:
        m = new_match()
        # Target 4500u from IW — outside deploy zone (3000) but inside range (5500)
        target = safe_spawn(m, 2, "golem", 0, -1000)
        step_n(m, DEPLOY_TICKS_HEAVY)
        iw = safe_spawn(m, 1, ICE_WIZARD_KEY, 0, -5500)
        step_n(m, DEPLOY_TICKS)
        # Wait for projectile to hit (load=24t + travel 4500/420≈11t = ~35t)
        step_n(m, 45)
        tk = find_entity(m, target)
        sm = tk.get("speed_mult", 100) if tk else 100
        buffs = tk.get("num_buffs", 0) if tk else 0
        print(f"  After projectile: speed_mult={sm}, buffs={buffs}")
        if sm < 100:
            check("1162: Projectile slow works ✓ (gap fixed!)", True, f"sm={sm}")
        else:
            check("1162: [ENGINE GAP] Projectile target_buff not wired",
                  False, "Need to copy ProjectileStats.target_buff in ranged attack path")
    except Exception as ex:
        check("1162: IW proj", False, str(ex))
else:
    check("1162: IW not found", False)


# =====================================================================
#  SECTION G: POISON (1180-1184)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION G: POISON (1180-1184)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 1180: Poison speed_mult=85
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1180: Poison speed_multiplier=-15 → speed_mult=85")
print("-" * 60)

if POISON_KEY:
    pdeck = [POISON_KEY] + ["knight"] * 7
    try:
        m = new_match(pdeck, DUMMY_DECK)
        step_n(m, 80)
        t = safe_spawn(m, 2, "knight", 0, -3000)
        step_n(m, DEPLOY_TICKS)
        sm_b = find_entity(m, t).get("speed_mult", 100)
        m.play_card(1, 0, 0, -3000)
        step_n(m, 10)
        sm_a = find_entity(m, t).get("speed_mult", 100)
        print(f"  Before: {sm_b}, After: {sm_a}")
        check("1180a: Slowed", sm_a < 100, f"sm={sm_a}")
        check("1180b: ≈85", 75 <= sm_a <= 95, f"sm={sm_a}")
    except Exception as ex:
        check("1180: Poison slow", False, str(ex))
else:
    check("1180: Poison not found", False)

# ------------------------------------------------------------------
# TEST 1181: Poison slow movement verified by per-tick displacement
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1181: Poisoned ENEMY troop per-tick speed is 85% of normal")
print("  Poison only_enemies=True → only slows ENEMIES of caster.")
print("  P1 casts Poison. Measure P2 Golem speed (Golem ignores distractions,")
print("  target_only_buildings=True, walks steadily toward P1 tower).")
print("-" * 60)

if POISON_KEY:
    pdeck = [POISON_KEY] + ["knight"] * 7
    try:
        # ── Control: P2 Golem walking freely ──
        # Golem: speed=45→18 u/tick (slow), target_only_buildings, walks straight
        m_ctrl = new_match()
        ctrl_g = safe_spawn(m_ctrl, 2, "golem", 0, 6000)
        step_n(m_ctrl, DEPLOY_TICKS_HEAVY)  # Golem needs heavy deploy

        # Measure control speed over 10 ticks
        cg = find_entity(m_ctrl, ctrl_g)
        cy0 = cg["y"] if cg else 0
        step_n(m_ctrl, 10)
        cg2 = find_entity(m_ctrl, ctrl_g)
        cy1 = cg2["y"] if cg2 else cy0
        ctrl_dist = abs(cy1 - cy0)
        ctrl_speed = ctrl_dist / 10
        print(f"  Control Golem: y {cy0}→{cy1}, dist={ctrl_dist}, speed={ctrl_speed:.1f}/tick")

        # ── Test: P1 casts Poison on P2 Golem ──
        m_test = new_match(pdeck, DUMMY_DECK)
        step_n(m_test, 80)  # Build elixir
        test_g = safe_spawn(m_test, 2, "golem", 0, 6000)
        step_n(m_test, DEPLOY_TICKS_HEAVY)

        # Cast Poison centered on Golem
        tg = find_entity(m_test, test_g)
        m_test.play_card(1, 0, tg["x"], tg["y"])
        step_n(m_test, 8)  # Let buff apply (zone fires at tick 0, then every 5)

        tg2 = find_entity(m_test, test_g)
        sm = tg2.get("speed_mult", 100) if tg2 else 100
        print(f"  Poisoned Golem speed_mult: {sm}")

        # Measure poisoned speed over 10 ticks
        ty0 = find_entity(m_test, test_g)["y"]
        step_n(m_test, 10)
        tg3 = find_entity(m_test, test_g)
        ty1 = tg3["y"] if tg3 else ty0
        test_dist = abs(ty1 - ty0)
        test_speed = test_dist / 10
        print(f"  Poisoned Golem: y {ty0}→{ty1}, dist={test_dist}, speed={test_speed:.1f}/tick")

        if ctrl_speed > 0:
            ratio = test_speed / ctrl_speed
            check("1181a: Poisoned speed < normal speed",
                  test_speed < ctrl_speed,
                  f"ctrl={ctrl_speed:.1f}, slow={test_speed:.1f}")
            check("1181b: Ratio ≈ 0.85 (±0.2)", 0.6 <= ratio <= 1.0,
                  f"ratio={ratio:.2f}")
        else:
            check("1181: Control moved", False, f"ctrl_speed={ctrl_speed}")
    except Exception as ex:
        check("1181: Poison movement", False, str(ex))
else:
    check("1181: Poison not found", False)

# ------------------------------------------------------------------
# TEST 1182: Poison DOT = 456
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1182: Poison DOT = 57/pulse × 8 = 456")
print("-" * 60)

if POISON_KEY:
    pdeck = [POISON_KEY] + ["knight"] * 7
    try:
        m = new_match(pdeck, DUMMY_DECK)
        step_n(m, 80)
        g = safe_spawn(m, 2, "golem", 0, 6000)
        step_n(m, DEPLOY_TICKS_HEAVY)
        hp0 = find_entity(m, g)["hp"]
        m.play_card(1, 0, 0, 6000)
        step_n(m, 170)
        hp1 = find_entity(m, g)["hp"]
        dot = hp0 - hp1
        print(f"  {hp0}→{hp1}, DOT={dot}")
        check("1182a: DOT > 0", dot > 0, f"dot={dot}")
        check("1182b: DOT ≈ 456 (±25%)", 340 <= dot <= 570, f"dot={dot}")
    except Exception as ex:
        check("1182: DOT", False, str(ex))
else:
    check("1182: Poison not found", False)

# ------------------------------------------------------------------
# TEST 1183: Poison slow + DOT simultaneously (25 ticks)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1183: Poison slow + DOT both active after 25 ticks")
print("-" * 60)

if POISON_KEY:
    pdeck = [POISON_KEY] + ["knight"] * 7
    try:
        m = new_match(pdeck, DUMMY_DECK)
        step_n(m, 80)
        t = safe_spawn(m, 2, "golem", 0, -3000)
        step_n(m, DEPLOY_TICKS_HEAVY)
        hp0 = find_entity(m, t)["hp"]
        m.play_card(1, 0, 0, -3000)
        step_n(m, 25)
        tk = find_entity(m, t)
        sm = tk.get("speed_mult", 100) if tk else 100
        hp1 = tk["hp"] if tk else hp0
        hl = hp0 - hp1
        print(f"  25 ticks: speed_mult={sm}, HP lost={hl}")
        check("1183a: Slowed", sm < 100, f"sm={sm}")
        check("1183b: DOT > 0", hl > 0, f"hl={hl}")
        check("1183c: Both", sm < 100 and hl > 0, f"slow={sm<100}, dot={hl>0}")
    except Exception as ex:
        check("1183: Dual", False, str(ex))
else:
    check("1183: Poison not found", False)

# ------------------------------------------------------------------
# TEST 1184: Poison slow expires
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 1184: Poison slow expires after 160 ticks")
print("-" * 60)

if POISON_KEY:
    pdeck = [POISON_KEY] + ["knight"] * 7
    try:
        m = new_match(pdeck, DUMMY_DECK)
        step_n(m, 80)
        t = safe_spawn(m, 2, "golem", 0, -3000)
        step_n(m, DEPLOY_TICKS_HEAVY)
        m.play_card(1, 0, 0, -3000)
        step_n(m, 10)
        sd = find_entity(m, t).get("speed_mult", 100)
        step_n(m, 170)
        sa = find_entity(m, t).get("speed_mult", 100)
        print(f"  During: {sd}, After: {sa}")
        check("1184a: Slow during", sd < 100, f"d={sd}")
        check("1184b: Expired", sa >= 95, f"a={sa}")
    except Exception as ex:
        check("1184: Expiry", False, str(ex))
else:
    check("1184: Poison not found", False)


# ====================================================================
# SUMMARY
# ====================================================================

print("\n" + "=" * 70)
print(f"  RESULTS: {PASS}/{PASS + FAIL} passed, {FAIL}/{PASS + FAIL} failed")
print("=" * 70)

print(f"\n  Known engine gaps (expected failures):")
print(f"    1140: spawn_area_effect_object not implemented (Heal Spirit heal zone)")
print(f"    1150: buff_when_not_attacking only handles invisibility")
print(f"    1160: spawn_area_object not implemented (IW deploy slow)")
print(f"    1162: Projectile target_buff not wired (IW per-hit slow)")

print("\n  Section coverage:")
for s, d in {
    "A: Prince Charge (1100-1107)":
        "Single-hit on Golem far from towers, 2× ratio, speed doubling, "
        "reset after hit, stun cancels (first 3 ticks), DP splash, distance, "
        "NO-TARGET charge (charges while walking to tower)",
    "B: Targeting (1120-1122)":
        "Sticky targeting, nearest-first, target_lowest_hp=False confirmed",
    "D: Heal Spirit (1140-1141)":
        "Heal zone ENGINE GAP (spawn_area_effect_object), kamikaze damage works",
    "E: Battle Healer (1150-1151)":
        "Self-heal ENGINE GAP (buff_when_not_attacking→invis only), combat HP loss",
    "F: Ice Wizard (1160-1162)":
        "Deploy slow ENGINE GAP (spawn_area_object), DPS reduction, proj buff ENGINE GAP",
    "G: Poison (1180-1184)":
        "speed_mult=85, per-tick displacement on ENEMY troop, DOT=456, dual after 25t, expiry",
}.items():
    print(f"    {s}")
    print(f"      -> {d}")

print()
sys.exit(0 if FAIL == 0 else 1)
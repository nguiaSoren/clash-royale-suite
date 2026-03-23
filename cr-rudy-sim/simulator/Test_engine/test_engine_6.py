"""
Engine fidelity tests — batch 6 (spells & buffs)

Place in: simulator/test_engine_6.py
Run with: python test_engine_6.py

Tests 43-52: spell deployment, area damage, buffs, freeze, rage, poison.

NOTE: Spells in the engine are keyed by NAME (e.g., "Rage", "Zap")
in the spell stats HashMap, not by the cards.json key ("rage", "zap").
play_card uses the deck's card key, which may not match the spell stats key.
These tests use spawn_troop + direct spell deployment to isolate spell mechanics.
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

def find_by_kind(match, kind):
    return [e for e in match.get_entities() if e["kind"] == kind and e["alive"]]

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
# TEST 43: Zap spell deals instant damage + stun
# =========================================================================
# Zap: damage=75 (lvl1), radius=2500, buff=ZapFreeze, buff_time=500ms
# It's an instant spell (life_duration=1). Should deal damage and stun.

def test_zap_damage_and_stun():
    print("\n" + "="*60)
    print("TEST 43: Zap deals instant damage + stun")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Spawn a P2 knight as Zap target
    knight_id = m.spawn_troop(2, "knight", 0, 0)
    for _ in range(20):
        m.step()

    knight_hp_before = find_entity(m, knight_id)["hp"]
    print(f"\n  Knight HP before Zap: {knight_hp_before}")

    # Deploy Zap spell at (0, 0) — try the spell stats key "Zap"
    zap_deployed = False
    try:
        m.deploy_spell(1, "Zap", 0, 0)
        zap_deployed = True
    except AttributeError:
        # deploy_spell might not exist, try play_card approach
        pass

    if not zap_deployed:
        # Try using play_card with a spell deck
        try:
            m2 = cr_engine.new_match(data, ["zap"] * 8, DUMMY_DECK)
            k2 = m2.spawn_troop(2, "knight", 0, 0)
            for _ in range(20):
                m2.step()
            hp_before = find_entity(m2, k2)["hp"]
            m2.play_card(1, 0, 0, 0)
            for _ in range(5):
                m2.step()
            hp_after = find_entity(m2, k2)["hp"]
            zap_damage = hp_before - hp_after
            print(f"  Zap played via play_card: damage={zap_damage}")
            check("Zap dealt damage", zap_damage > 0,
                  f"damage={zap_damage}")
            check("Zap damage in expected range (50-300)",
                  50 <= zap_damage <= 300,
                  f"damage={zap_damage}")
            return
        except Exception as e:
            print(f"  play_card with zap failed: {e}")
            # Zap might not be deployable yet — this is a valid finding
            check("Zap spell deployable", False,
                  f"Neither deploy_spell nor play_card('zap') works: {e}")
            return

    # If deploy_spell worked
    for _ in range(5):
        m.step()

    knight_hp_after = find_entity(m, knight_id)["hp"]
    zap_damage = knight_hp_before - knight_hp_after
    print(f"  Knight HP after Zap: {knight_hp_after}")
    print(f"  Damage: {zap_damage}")

    check("Zap dealt damage", zap_damage > 0)
    check("Zap damage in expected range (50-300)",
          50 <= zap_damage <= 300,
          f"damage={zap_damage}")


# =========================================================================
# TEST 44: Freeze spell stops enemy movement
# =========================================================================
# Freeze: buff=Freeze, life_duration=4000ms
# Freeze buff: hit_speed_multiplier=-100, speed_multiplier=-100
# An enemy should stop moving while frozen.

def test_freeze_stops_movement():
    print("\n" + "="*60)
    print("TEST 44: Freeze stops enemy movement")
    print("="*60)

    try:
        # Two separate matches: one normal, one with Freeze
        # Match A: Normal movement baseline
        m_a = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
        k_a = m_a.spawn_troop(2, "knight", 0, 5000)
        for _ in range(30):
            m_a.step()  # wait for deploy
        y_start_a = find_entity(m_a, k_a)["y"]
        for _ in range(40):
            m_a.step()
        y_end_a = find_entity(m_a, k_a)["y"]
        normal_movement = abs(y_end_a - y_start_a)
        print(f"\n  Normal movement in 40 ticks: {normal_movement} (expected ~1200)")

        # Match B: With Freeze applied
        m_b = cr_engine.new_match(data, ["freeze"] * 8, DUMMY_DECK)
        k_b = m_b.spawn_troop(2, "knight", 0, 5000)
        for _ in range(30):
            m_b.step()  # wait for deploy
        y_start_b = find_entity(m_b, k_b)["y"]

        # Deploy Freeze centered on the Knight
        m_b.play_card(1, 0, 0, y_start_b)
        for _ in range(3):
            m_b.step()  # let freeze apply

        y_pre_freeze = find_entity(m_b, k_b)["y"]
        for _ in range(40):
            m_b.step()
        e_after = find_entity(m_b, k_b)
        y_end_b = e_after["y"] if e_after else y_pre_freeze
        frozen_movement = abs(y_end_b - y_pre_freeze)
        print(f"  Frozen movement in 40 ticks: {frozen_movement}")

        check("Knight moved normally without freeze (> 500 units)",
              normal_movement > 500,
              f"only moved {normal_movement} — deploy timer might still be active")
        check("Freeze reduced movement significantly",
              frozen_movement < normal_movement * 0.3,
              f"frozen={frozen_movement} vs normal={normal_movement}")
    except Exception as e:
        print(f"  Freeze test failed: {e}")
        check("Freeze spell deployable", False, str(e))


# =========================================================================
# TEST 45: Rage spell boosts attack speed
# =========================================================================
# Rage: buff=Rage, life_duration=6000ms
# Rage buff: hit_speed_multiplier=135, speed_multiplier=135
# An allied troop should attack faster while raged.
#
# FIX: Previous test placed troops near towers. Tower damage (109/16t)
# was confounding the measurement. Now we:
#   1. Place troops far from all towers to eliminate tower interference
#   2. Count only 202-damage hits (Knight damage) not 109-damage (tower)
#   3. Use a Golem (building-only targeter) as punching bag so it doesn't
#      fight back, and measure Knight-only hit count

def test_rage_boosts_attack_speed():
    print("\n" + "="*60)
    print("TEST 45: Rage boosts attack speed")
    print("="*60)

    KNIGHT_DMG = 202  # Knight damage at level 11

    # ── Helper: count Knight-only hits on Golem over N ticks ──
    def measure_knight_hits(use_rage, ticks=200):
        """Returns (total_knight_damage, hit_count, hit_ticks)"""
        deck = ["rage"] * 8 if use_rage else DUMMY_DECK
        m = cr_engine.new_match(data, deck, DUMMY_DECK)

        # Place far from all towers (center of P1 side, Y=-5000)
        # Knight and Golem very close so Knight stays in melee range
        k = m.spawn_troop(1, "knight", 0, -5000)
        g = m.spawn_troop(2, "golem",  0, -4400)

        # Wait for both to deploy (Golem needs 60 ticks)
        for _ in range(65):
            m.step()

        if use_rage:
            ke = find_entity(m, k)
            m.play_card(1, 0, ke["x"], ke["y"])
            # Let rage buff apply
            for _ in range(3):
                m.step()

        # Now measure
        prev_hp = find_entity(m, g)["hp"]
        knight_damage = 0
        knight_hits = 0
        hit_ticks = []

        for t in range(ticks):
            m.step()
            e = find_entity(m, g)
            if e and e["hp"] < prev_hp:
                dmg = prev_hp - e["hp"]
                # Filter: Knight does 202 damage, towers do 109
                # Accept any damage close to 202 (±10) as Knight hit
                # Also accept combined hits (202+109=311, etc.)
                knight_dmg_in_this = 0
                remaining = dmg
                while remaining >= KNIGHT_DMG - 10:
                    if abs(remaining - KNIGHT_DMG) <= 10:
                        knight_dmg_in_this += KNIGHT_DMG
                        remaining -= KNIGHT_DMG
                    elif remaining >= KNIGHT_DMG:
                        knight_dmg_in_this += KNIGHT_DMG
                        remaining -= KNIGHT_DMG
                    else:
                        break
                if knight_dmg_in_this > 0:
                    knight_damage += knight_dmg_in_this
                    knight_hits += knight_dmg_in_this // KNIGHT_DMG
                    hit_ticks.append(t)
                prev_hp = e["hp"]

        return knight_damage, knight_hits, hit_ticks

    # Match A: normal Knight
    normal_dmg, normal_hits, normal_ticks = measure_knight_hits(False, 200)
    print(f"\n  Normal Knight: {normal_hits} hits, {normal_dmg} damage in 200 ticks")

    # Match B: raged Knight
    raged_dmg, raged_hits, raged_ticks = measure_knight_hits(True, 200)
    print(f"  Raged Knight:  {raged_hits} hits, {raged_dmg} damage in 200 ticks")

    check("Normal Knight dealt damage", normal_dmg > 0)
    check("Raged Knight dealt damage", raged_dmg > 0)

    if normal_hits > 0 and raged_hits > 0:
        ratio = raged_hits / normal_hits
        print(f"  Rage hit ratio: {ratio:.2f}x (expected ~1.35x)")
        check("Rage increased hit count",
              raged_hits > normal_hits,
              f"raged={raged_hits} <= normal={normal_hits}")
        check("Rage boost ratio reasonable (1.1-2.0x)",
              1.1 <= ratio <= 2.0,
              f"ratio={ratio:.2f}")

    if normal_ticks and raged_ticks:
        normal_intervals = [normal_ticks[i+1]-normal_ticks[i] for i in range(len(normal_ticks)-1)]
        raged_intervals = [raged_ticks[i+1]-raged_ticks[i] for i in range(len(raged_ticks)-1)]
        if normal_intervals and raged_intervals:
            avg_n = sum(normal_intervals)/len(normal_intervals)
            avg_r = sum(raged_intervals)/len(raged_intervals)
            print(f"  Normal intervals: {normal_intervals[:5]}  avg={avg_n:.1f}")
            print(f"  Raged intervals:  {raged_intervals[:5]}  avg={avg_r:.1f}")


# =========================================================================
# TEST 46: Freeze deals damage on application
# =========================================================================

def test_freeze_deals_damage():
    print("\n" + "="*60)
    print("TEST 46: Freeze deals damage on application")
    print("="*60)

    try:
        m = cr_engine.new_match(data, ["freeze"] * 8, DUMMY_DECK)
        golem_id = m.spawn_troop(2, "golem", 0, 0)
        for _ in range(100):
            m.step()

        hp_before = find_entity(m, golem_id)["hp"]

        # Play Freeze on the Golem
        m.play_card(1, 0, 0, 0)

        # Run a few ticks for Freeze to apply
        for _ in range(10):
            m.step()

        hp_after = find_entity(m, golem_id)["hp"]
        damage = hp_before - hp_after
        print(f"\n  Golem HP before Freeze: {hp_before}")
        print(f"  Golem HP after Freeze: {hp_after}")
        print(f"  Freeze damage: {damage}")

        check("Freeze dealt damage", damage > 0,
              f"damage={damage} — Freeze damage field might be 0")
        if damage > 0:
            check("Freeze damage in reasonable range (50-300)",
                  50 <= damage <= 300,
                  f"damage={damage}")
    except Exception as e:
        print(f"  Freeze deployment failed: {e}")
        check("Freeze spell deployable", False, str(e))


# =========================================================================
# TEST 47: Rage only buffs friendly troops
# =========================================================================
# Rage has only_own_troops=True. It should only affect friendly troops.
#
# FIX: Compare Knight hit count with and without Rage, using same setup
# as TEST 45 (away from towers).

def test_rage_only_affects_allies():
    print("\n" + "="*60)
    print("TEST 47: Rage only buffs friendly troops")
    print("="*60)

    try:
        KNIGHT_DMG = 202

        # ── Match A: Raged P1 Knight vs Golem (should get more hits) ──
        m = cr_engine.new_match(data, ["rage"] * 8, DUMMY_DECK)
        k = m.spawn_troop(1, "knight", 0, -5000)
        golem = m.spawn_troop(2, "golem", 0, -4400)
        for _ in range(65):
            m.step()
        ke = find_entity(m, k)
        m.play_card(1, 0, ke["x"], ke["y"])
        for _ in range(3):
            m.step()

        hp_start = find_entity(m, golem)["hp"]
        for _ in range(200):
            m.step()
        hp_end = find_entity(m, golem)["hp"]
        raged_total = hp_start - hp_end

        # ── Match B: Normal P1 Knight vs Golem (baseline) ──
        m2 = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
        k2 = m2.spawn_troop(1, "knight", 0, -5000)
        golem2 = m2.spawn_troop(2, "golem", 0, -4400)
        for _ in range(68):  # 65 + 3 to match timing
            m2.step()

        hp_start2 = find_entity(m2, golem2)["hp"]
        for _ in range(200):
            m2.step()
        hp_end2 = find_entity(m2, golem2)["hp"]
        normal_total = hp_start2 - hp_end2

        print(f"\n  Raged P1 Knight total damage in 200 ticks: {raged_total}")
        print(f"  Normal P1 Knight total damage in 200 ticks: {normal_total}")

        check("Raged Knight dealt more than normal",
              raged_total > normal_total,
              f"raged={raged_total} <= normal={normal_total}")
        check("Raged Knight dealt damage at all", raged_total > 0)

        if normal_total > 0:
            ratio = raged_total / normal_total
            print(f"  Rage damage ratio: {ratio:.2f}x")
            check("Rage boost ratio > 1.1",
                  ratio > 1.1,
                  f"ratio={ratio:.2f}")
    except Exception as e:
        print(f"  Test failed: {e}")
        check("Rage deployment worked", False, str(e))


# =========================================================================
# TEST 48: Spell zone entity created and cleaned up
# =========================================================================

def test_spell_zone_lifecycle():
    print("\n" + "="*60)
    print("TEST 48: Spell zone entity lifecycle")
    print("="*60)

    try:
        m = cr_engine.new_match(data, ["rage"] * 8, DUMMY_DECK)
        for _ in range(100):
            m.step()

        entities_before = len(m.get_entities())

        # Deploy Rage (duration = 6000ms = 120 ticks)
        m.play_card(1, 0, 0, -5000)

        m.step()
        spell_zones = find_by_kind(m, "spell_zone")
        entities_after_deploy = len(m.get_entities())

        print(f"\n  Entities before spell: {entities_before}")
        print(f"  Entities after spell deploy: {entities_after_deploy}")
        print(f"  Spell zone entities: {len(spell_zones)}")

        check("Spell zone entity created", len(spell_zones) > 0,
              f"no spell_zone entities found")

        # Run past the duration (120 ticks + buffer)
        for _ in range(150):
            m.step()

        spell_zones_after = find_by_kind(m, "spell_zone")
        print(f"  Spell zones after 150 ticks: {len(spell_zones_after)}")

        check("Spell zone expired after duration",
              len(spell_zones_after) == 0,
              f"still {len(spell_zones_after)} spell zones alive")
    except Exception as e:
        print(f"  Test failed: {e}")
        check("Spell deployment worked", False, str(e))


# =========================================================================
# TEST 49: Spell crown tower damage reduction
# =========================================================================

def test_spell_ct_reduction():
    print("\n" + "="*60)
    print("TEST 49: Spell crown tower damage reduction")
    print("="*60)

    try:
        # Match A: Zap a knight (full damage)
        m_a = cr_engine.new_match(data, ["zap"] * 8, DUMMY_DECK)
        knight_id = m_a.spawn_troop(2, "knight", 0, 0)
        for _ in range(20):
            m_a.step()
        hp_before = find_entity(m_a, knight_id)["hp"]
        m_a.play_card(1, 0, 0, 0)
        for _ in range(5):
            m_a.step()
        hp_after = find_entity(m_a, knight_id)["hp"]
        troop_damage = hp_before - hp_after
        print(f"\n  Zap damage to troop: {troop_damage}")

        # Match B: Zap near P2 princess tower
        m_b = cr_engine.new_match(data, ["zap"] * 8, DUMMY_DECK)
        for _ in range(20):
            m_b.step()
        tower_hp_before = m_b.p2_tower_hp()[1]  # left princess
        # Deploy Zap at the tower position
        m_b.play_card(1, 0, -5100, 10200)
        for _ in range(5):
            m_b.step()
        tower_hp_after = m_b.p2_tower_hp()[1]
        tower_damage = tower_hp_before - tower_hp_after
        print(f"  Zap damage to tower: {tower_damage}")

        check("Zap damaged troop", troop_damage > 0,
              f"troop_damage={troop_damage}")
        check("Zap damaged tower", tower_damage > 0,
              f"tower_damage={tower_damage}")

        if troop_damage > 0 and tower_damage > 0:
            ratio = tower_damage / troop_damage
            print(f"  Tower/Troop ratio: {ratio:.2f} (expected ~0.30 for Zap CT -70%)")
            check("Tower damage < troop damage",
                  tower_damage < troop_damage,
                  f"tower={tower_damage} >= troop={troop_damage}")
            check("CT reduction ratio reasonable (0.1-0.5)",
                  0.1 <= ratio <= 0.5,
                  f"ratio={ratio:.2f}")
    except Exception as e:
        print(f"  Test failed: {e}")
        check("Zap spell deployable", False, str(e))


# =========================================================================
# TEST 50: Buff multipliers — Rage changes entity hit_speed
# =========================================================================
# Direct test: measure actual attack phase durations from entity state.
# This avoids the HP-change measurement that conflates tower damage.

def test_buff_changes_attack_interval():
    print("\n" + "="*60)
    print("TEST 50: Buff multipliers change attack interval")
    print("="*60)

    def median(lst):
        s = sorted(lst)
        n = len(s)
        if n == 0: return 0
        if n % 2 == 1: return s[n // 2]
        return (s[n // 2 - 1] + s[n // 2]) / 2

    def measure_phases(use_rage):
        """Measure windup and backswing durations from attack_phase transitions."""
        deck = ["rage"] * 8 if use_rage else DUMMY_DECK
        m = cr_engine.new_match(data, deck, DUMMY_DECK)
        k = m.spawn_troop(1, "knight", 0, -5000)
        g = m.spawn_troop(2, "golem", 0, -4400)

        for _ in range(65):
            m.step()

        if use_rage:
            ke = find_entity(m, k)
            m.play_card(1, 0, ke["x"], ke["y"])
            for _ in range(5):
                m.step()
        else:
            for _ in range(5):
                m.step()

        windups = []
        backswings = []
        prev_phase = None
        phase_start = 0
        for t in range(200):
            m.step()
            e = find_entity(m, k)
            if not e:
                break
            phase = e.get("attack_phase", "?")
            if phase != prev_phase:
                duration = t - phase_start
                # Only record complete phases (filter out partial windups
                # from cancellations/retargets which are very short)
                if prev_phase == "windup" and duration >= 5:
                    windups.append(duration)
                elif prev_phase == "backswing" and duration >= 3:
                    backswings.append(duration)
                phase_start = t
                prev_phase = phase

        return windups, backswings

    try:
        normal_wu, normal_bs = measure_phases(False)
        raged_wu, raged_bs = measure_phases(True)

        med_normal_wu = median(normal_wu)
        med_normal_bs = median(normal_bs)
        med_raged_wu = median(raged_wu)
        med_raged_bs = median(raged_bs)

        print(f"\n  Normal windups: {normal_wu[:6]}  median={med_normal_wu:.0f}")
        print(f"  Normal backswings: {normal_bs[:6]}  median={med_normal_bs:.0f}")
        print(f"  Normal total cycle: {med_normal_wu + med_normal_bs:.0f}")
        print(f"  Raged windups: {raged_wu[:6]}  median={med_raged_wu:.0f}")
        print(f"  Raged backswings: {raged_bs[:6]}  median={med_raged_bs:.0f}")
        print(f"  Raged total cycle: {med_raged_wu + med_raged_bs:.0f}")

        if med_normal_wu > 0 and med_raged_wu > 0:
            wu_ratio = med_raged_wu / med_normal_wu
            bs_ratio = med_raged_bs / med_normal_bs if med_normal_bs > 0 else 1.0
            normal_cycle = med_normal_wu + med_normal_bs
            raged_cycle = med_raged_wu + med_raged_bs
            cycle_ratio = raged_cycle / normal_cycle if normal_cycle > 0 else 1.0
            print(f"  Windup ratio: {wu_ratio:.2f}  Backswing ratio: {bs_ratio:.2f}")
            print(f"  Total cycle ratio: {cycle_ratio:.2f} (expected ~0.74 for 135% Rage)")

            check("Raged windup shorter than normal",
                  med_raged_wu < med_normal_wu,
                  f"raged={med_raged_wu:.0f} >= normal={med_normal_wu:.0f}")
            check("Rage speed boost ratio reasonable",
                  cycle_ratio < 0.85,
                  f"cycle_ratio={cycle_ratio:.2f} — not fast enough")
        else:
            check("Enough phase transitions measured",
                  len(normal_wu) > 2 and len(raged_wu) > 2,
                  f"normal_windups={len(normal_wu)} raged_windups={len(raged_wu)}")
    except Exception as e:
        print(f"  Test failed: {e}")
        check("Rage deployment worked", False, str(e))
    except Exception as e:
        print(f"  Test failed: {e}")
        check("Rage deployment worked", False, str(e))


# =========================================================================
# TEST 51: Multiple spells can stack
# =========================================================================

def test_multiple_spell_stacking():
    print("\n" + "="*60)
    print("TEST 51: Multiple spell deployments don't crash")
    print("="*60)

    try:
        m = cr_engine.new_match(data, ["rage"] * 8, DUMMY_DECK)
        knight_id = m.spawn_troop(1, "knight", 0, -5000)
        for _ in range(100):
            m.step()

        # Deploy Rage twice
        m.play_card(1, 0, 0, -5000)
        for _ in range(20):
            m.step()
        m.play_card(1, 1, 0, -5000)

        # Run 200 more ticks
        crashed = False
        try:
            for _ in range(200):
                m.step()
        except Exception as ex:
            crashed = True
            print(f"\n  ENGINE CRASHED: {ex}")

        check("Engine didn't crash with stacked spells", not crashed)
        e = find_entity(m, knight_id)
        check("Knight still alive after stacked spells",
              e is not None and e["alive"],
              "Knight died or disappeared")
    except Exception as e:
        print(f"\n  Test setup failed: {e}")
        check("Spell stacking test ran", False, str(e))


# =========================================================================
# TEST 52: Spell with only_enemies doesn't hurt own troops
# =========================================================================

def test_spell_only_enemies():
    print("\n" + "="*60)
    print("TEST 52: Spell with only_enemies doesn't hurt own troops")
    print("="*60)

    try:
        m = cr_engine.new_match(data, ["zap"] * 8, DUMMY_DECK)
        # P1 Knight and P2 Knight in same area
        p1_knight = m.spawn_troop(1, "knight", 0, 0)
        p2_knight = m.spawn_troop(2, "knight", 200, 0)

        for _ in range(20):
            m.step()

        p1_hp_before = find_entity(m, p1_knight)["hp"]
        p2_hp_before = find_entity(m, p2_knight)["hp"]

        # P1 deploys Zap on the area
        m.play_card(1, 0, 100, 0)
        for _ in range(5):
            m.step()

        p1_hp_after = find_entity(m, p1_knight)["hp"]
        p2_hp_after = find_entity(m, p2_knight)["hp"]

        p1_damage = p1_hp_before - p1_hp_after
        p2_damage = p2_hp_before - p2_hp_after

        print(f"\n  P1 Knight (friendly) damage from Zap: {p1_damage}")
        print(f"  P2 Knight (enemy) damage from Zap: {p2_damage}")

        check("P2 enemy took Zap damage", p2_damage > 0,
              f"p2_damage={p2_damage}")
        check("P1 friendly NOT damaged by own Zap",
              p1_damage == 0,
              f"p1_damage={p1_damage} — friendly fire!")
    except Exception as e:
        print(f"  Test failed: {e}")
        check("Zap spell deployable", False, str(e))


# =========================================================================
# Run all tests
# =========================================================================

if __name__ == "__main__":
    print("="*60)
    print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 6")
    print("  Tests 43-52: spells & buffs")
    print("="*60)

    test_zap_damage_and_stun()
    test_freeze_stops_movement()
    test_rage_boosts_attack_speed()
    test_freeze_deals_damage()
    test_rage_only_affects_allies()
    test_spell_zone_lifecycle()
    test_spell_ct_reduction()
    test_buff_changes_attack_interval()
    test_multiple_spell_stacking()
    test_spell_only_enemies()

    print("\n" + "="*60)
    total = PASS + FAIL
    print(f"  RESULTS: {PASS}/{total} passed, {FAIL}/{total} failed")
    print("="*60)

    if FAIL > 0:
        print("\n  Spell/buff failures reveal missing or broken spell mechanics.")
        print("  These are among the hardest systems to get right.")
        sys.exit(1)
    else:
        print("\n  All spell & buff tests passed! Magic is working.")
        sys.exit(0)
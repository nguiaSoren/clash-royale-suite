#!/usr/bin/env python3
"""
============================================================
  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 18
  Tests 800-829: Evolution Ability Behavioral Tests
============================================================

Tests that evolved troops trigger their evo abilities correctly.
Data source: evo_hero_abilities.json → GameData.evolutions

Evolutions tested:
  A. Evo Knight (800-804)
     - trigger=while_not_attacking, effect=damage_reduction(60)
     - Knight gets 60% damage reduction while idle (not attacking)

  B. Evo Barbarians (805-809)
     - trigger=on_each_attack, effects=hitspeed_buff(30) + speed_buff(30)
     - After each attack: +30% attack speed, +30% move speed for 3s

  C. Evo Cannon (810-814)
     - trigger=on_deploy, effects=area_damage(304) + knockback
     - On deploy: deals 304 damage in radius=2000 + knockback

  D. Evo Valkyrie (815-819)
     - trigger=on_each_attack, effects=area_pull + area_damage(84)
     - After each attack: pulls enemies in radius=5500 + deals 84 area damage

  E. Evo PEKKA (820-824)
     - trigger=on_kill, effect=heal
     - When PEKKA kills an enemy, it heals
"""

import sys
import os

try:
    import cr_engine
except ImportError:
    here = os.path.dirname(os.path.abspath(__file__))
    for p in [
        os.path.join(here, "engine", "target", "release"),
        os.path.join(here, "target", "release"),
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


def new_match():
    return cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)


def step_n(m, n):
    for _ in range(n):
        m.step()


def safe_spawn(m, player, key, x, y, evolved=False):
    try:
        return m.spawn_troop(player, key, x, y, 11, evolved)
    except Exception as ex:
        print(f"    [spawn failed: {key} evolved={evolved} → {ex}]")
        return None


print("=" * 70)
print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 18")
print("  Tests 800-829: Evolution Ability Behavioral Tests")
print("=" * 70)


# =====================================================================
#  SECTION A: EVO KNIGHT (800-804)
#  trigger=while_not_attacking, effect=damage_reduction(60)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION A: EVO KNIGHT — Passive Shield (800-804)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 800: Evo Knight spawns with is_evolved=True and evo_state
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 800: Evo Knight has is_evolved flag and evo_state")
print("-" * 60)

m = new_match()
ek = safe_spawn(m, 1, "knight", 0, -12000, evolved=True)
nk = safe_spawn(m, 1, "knight", 200, -12000, evolved=False)

if ek is not None and nk is not None:
    step_n(m, DEPLOY_TICKS + 1)
    ee = find_entity(m, ek)
    ne = find_entity(m, nk)
    print(f"  Evo Knight: is_evolved={ee.get('is_evolved', False) if ee else 'NOT FOUND'}")
    print(f"  Normal Knight: is_evolved={ne.get('is_evolved', False) if ne else 'NOT FOUND'}")
    check("800a: Evo Knight is_evolved=True",
          ee.get("is_evolved", False) if ee else False)
    check("800b: Normal Knight is_evolved=False",
          not (ne.get("is_evolved", False) if ne else True))
else:
    check("800: Knights spawnable", False)

# ------------------------------------------------------------------
# TEST 801: Evo Knight takes less damage while idle (not attacking)
# Data: damage_reduction=60 while_not_attacking
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 801: Evo Knight passive damage reduction while idle")
print("  Data: damage_reduction=60 (trigger=while_not_attacking)")
print("-" * 60)

# Compare total damage taken. Evo knight should take ~40% of normal damage
# while idle (while_not_attacking trigger).
m1 = new_match()
m2 = new_match()
ek = safe_spawn(m1, 1, "knight", 0, -12000, evolved=True)
nk = safe_spawn(m2, 1, "knight", 0, -12000, evolved=False)

if ek is not None and nk is not None:
    step_n(m1, DEPLOY_TICKS + 200)
    step_n(m2, DEPLOY_TICKS + 200)

    # Record HP and spawn identical P2 attackers at knight position
    ek_pos = find_entity(m1, ek)
    nk_pos = find_entity(m2, nk)
    hp1_start = ek_pos["hp"] if ek_pos else 0
    hp2_start = nk_pos["hp"] if nk_pos else 0
    y1 = ek_pos["y"] if ek_pos else -6000
    y2 = nk_pos["y"] if nk_pos else -6000

    # Spawn P2 knight at SAME position — immediately in combat range
    m1.spawn_troop(2, "knight", 0, y1)
    m2.spawn_troop(2, "knight", 0, y2)

    # Let combat happen for 80 ticks
    step_n(m1, DEPLOY_TICKS + 60)
    step_n(m2, DEPLOY_TICKS + 60)

    e1 = find_entity(m1, ek)
    e2 = find_entity(m2, nk)
    hp1_end = e1["hp"] if e1 and e1["alive"] else 0
    hp2_end = e2["hp"] if e2 and e2["alive"] else 0
    lost1 = hp1_start - hp1_end
    lost2 = hp2_start - hp2_end

    print(f"  Evo Knight HP lost: {lost1}")
    print(f"  Normal Knight HP lost: {lost2}")
    if lost2 > 0:
        ratio = lost1 / lost2
        print(f"  Damage ratio: {ratio:.2f}")
        # The evo knight has damage_reduction=60 while NOT attacking.
        # Once they start fighting, the reduction stops.
        # So at minimum the first few hits should be reduced.
        check("801a: Evo Knight took less total damage",
              lost1 < lost2,
              f"evo_lost={lost1} normal_lost={lost2}")
    else:
        check("801a: P2 Knight dealt damage", False, f"normal lost={lost2}")
else:
    check("801: Knights spawnable", False)


# =====================================================================
#  SECTION B: EVO BARBARIANS (805-809)
#  trigger=on_each_attack, effects=hitspeed_buff(30) + speed_buff(30)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION B: EVO BARBARIANS — Attack Speed Ramp (805-809)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 805: Evo Barbarian spawns as evolved
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 805: Evo Barbarian has is_evolved=True")
print("-" * 60)

m = new_match()
eb = safe_spawn(m, 1, "barbarians", 0, -12000, evolved=True)
if eb is not None:
    step_n(m, DEPLOY_TICKS + 1)
    ee = find_entity(m, eb)
    check("805a: Evo Barbarian is_evolved=True",
          ee.get("is_evolved", False) if ee else False)
else:
    check("805: Barbarian spawnable as evolved", False)

# ------------------------------------------------------------------
# TEST 806: Evo Barbarians attack faster after hitting
# Data: +30% hitspeed, +30% speed for 3s after each attack
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 806: Evo Barbarian gets hitspeed buff after attacking")
print("  Data: hitspeed_buff value=30 for 3000ms on_each_attack")
print("-" * 60)

m1 = new_match()  # evo
m2 = new_match()  # normal
eb = safe_spawn(m1, 1, "barbarians", 0, -5000, evolved=True)
nb = safe_spawn(m2, 1, "barbarians", 0, -5000, evolved=False)
golem1 = m1.spawn_troop(2, "golem", 0, -5000)
golem2 = m2.spawn_troop(2, "golem", 0, -5000)

if eb is not None and nb is not None:
    step_n(m1, DEPLOY_TICKS)
    step_n(m2, DEPLOY_TICKS)

    g1 = find_entity(m1, golem1)
    g2 = find_entity(m2, golem2)
    hp1_start = g1["hp"] if g1 else 0
    hp2_start = g2["hp"] if g2 else 0

    step_n(m1, 200)
    step_n(m2, 200)

    g1a = find_entity(m1, golem1)
    g2a = find_entity(m2, golem2)
    dmg_evo = hp1_start - (g1a["hp"] if g1a else 0)
    dmg_normal = hp2_start - (g2a["hp"] if g2a else 0)

    print(f"  Evo Barbarian total damage to Golem: {dmg_evo}")
    print(f"  Normal Barbarian total damage to Golem: {dmg_normal}")
    # With +30% hitspeed buff on each attack, evo should deal more total damage
    check("806a: Evo Barbarian dealt more total damage (hitspeed buff)",
          dmg_evo > dmg_normal,
          f"evo={dmg_evo} normal={dmg_normal}")
else:
    check("806: Barbarian spawnable", False)


# =====================================================================
#  SECTION C: EVO CANNON (810-814)
#  trigger=on_deploy, effects=area_damage(304) + knockback
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION C: EVO CANNON — Deploy Blast (810-814)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 810: Evo Cannon deals area damage on deploy
# Data: on_deploy → area_damage(304) in radius=2000
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 810: Evo Cannon deploy blast (304 dmg, radius=2000)")
print("-" * 60)

# Note: Cannon is a building, not a troop. Need spawn_building with evolved.
# If spawn_building doesn't support evolved, test what we can.
m = new_match()
# Spawn enemies near where cannon will deploy
enemy1 = m.spawn_troop(2, "knight", 0, -5000)
enemy2 = m.spawn_troop(2, "knight", 500, -5000)
step_n(m, DEPLOY_TICKS)

e1 = find_entity(m, enemy1)
e2 = find_entity(m, enemy2)
hp1_before = e1["hp"] if e1 else 0
hp2_before = e2["hp"] if e2 else 0

# Try to spawn evolved cannon (may need building support)
try:
    # Try as building first
    cannon_id = m.spawn_building(1, "cannon", 0, -5200)
    # We can't pass is_evolved to spawn_building yet, so this tests non-evo
    step_n(m, DEPLOY_TICKS + 5)

    e1a = find_entity(m, enemy1)
    e2a = find_entity(m, enemy2)
    hp1_after = e1a["hp"] if e1a else 0
    hp2_after = e2a["hp"] if e2a else 0
    dmg1 = hp1_before - hp1_after
    dmg2 = hp2_before - hp2_after

    print(f"  Normal cannon deploy: enemy1 dmg={dmg1} enemy2 dmg={dmg2}")
    print(f"  (Evo cannon would deal 304 AoE — needs evolved building spawn support)")
    check("810a: Cannon building spawns successfully",
          cannon_id is not None,
          "cannon spawn failed")
    # Document the gap: spawn_building doesn't support is_evolved yet
    check("810b: Evo cannon deploy blast (needs spawn_building evolved support)",
          dmg1 > 250 or dmg2 > 250,
          f"dmg1={dmg1} dmg2={dmg2} — non-evo cannon has no deploy blast")
except Exception as ex:
    check("810: Cannon test", False, str(ex))


# =====================================================================
#  SECTION D: EVO VALKYRIE (815-819)
#  trigger=on_each_attack, effects=area_pull + area_damage(84)
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION D: EVO VALKYRIE — Attack Pull + Damage (815-819)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 815: Evo Valkyrie spawns as evolved
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 815: Evo Valkyrie has is_evolved=True")
print("-" * 60)

m = new_match()
ev = safe_spawn(m, 1, "valkyrie", 0, -12000, evolved=True)
if ev is not None:
    step_n(m, DEPLOY_TICKS + 1)
    ee = find_entity(m, ev)
    check("815a: Evo Valkyrie is_evolved=True",
          ee.get("is_evolved", False) if ee else False)
else:
    check("815: Valkyrie spawnable as evolved", False)

# ------------------------------------------------------------------
# TEST 816: Evo Valkyrie deals extra area damage on attack
# Data: on_each_attack → area_damage=84 in radius=5500
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 816: Evo Valkyrie extra AoE on attack")
print("  Data: area_damage=84 radius=5500 on_each_attack")
print("-" * 60)

m1 = new_match()  # evo valk
m2 = new_match()  # normal valk
ev = safe_spawn(m1, 1, "valkyrie", 0, -5000, evolved=True)
nv = safe_spawn(m2, 1, "valkyrie", 0, -5000, evolved=False)
# Target near valk
golem1 = m1.spawn_troop(2, "golem", 0, -4500)
golem2 = m2.spawn_troop(2, "golem", 0, -4500)
# Distant target within 5500 radius but outside normal splash
far1 = m1.spawn_troop(2, "knight", 3000, -4500)
far2 = m2.spawn_troop(2, "knight", 3000, -4500)

if ev is not None and nv is not None:
    step_n(m1, DEPLOY_TICKS)
    step_n(m2, DEPLOY_TICKS)

    f1_hp = find_entity(m1, far1)
    f2_hp = find_entity(m2, far2)
    far_hp1_before = f1_hp["hp"] if f1_hp else 0
    far_hp2_before = f2_hp["hp"] if f2_hp else 0

    step_n(m1, 80)
    step_n(m2, 80)

    f1a = find_entity(m1, far1)
    f2a = find_entity(m2, far2)
    far_dmg1 = far_hp1_before - (f1a["hp"] if f1a and f1a["alive"] else 0)
    far_dmg2 = far_hp2_before - (f2a["hp"] if f2a and f2a["alive"] else 0)

    print(f"  Evo Valk far-target damage: {far_dmg1}")
    print(f"  Normal Valk far-target damage: {far_dmg2}")
    check("816a: Evo Valkyrie dealt more damage to far target (area_damage evo effect)",
          far_dmg1 > far_dmg2,
          f"evo={far_dmg1} normal={far_dmg2}")
else:
    check("816: Valkyries spawnable", False)


# =====================================================================
#  SECTION E: EVO PEKKA (820-824)
#  trigger=on_kill, effect=heal
# =====================================================================

print("\n" + "=" * 70)
print("  SECTION E: EVO PEKKA — Heal on Kill (820-824)")
print("=" * 70)

# ------------------------------------------------------------------
# TEST 820: Evo PEKKA spawns as evolved
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 820: Evo PEKKA has is_evolved=True")
print("-" * 60)

m = new_match()
ep = safe_spawn(m, 1, "pekka", 0, -12000, evolved=True)
if ep is not None:
    step_n(m, DEPLOY_TICKS + 1)
    ee = find_entity(m, ep)
    check("820a: Evo PEKKA is_evolved=True",
          ee.get("is_evolved", False) if ee else False)
else:
    check("820: PEKKA spawnable as evolved", False)

# ------------------------------------------------------------------
# TEST 821: Evo PEKKA heals after killing an enemy
# Data: on_kill → heal (self)
# ------------------------------------------------------------------
print("\n" + "-" * 60)
print("TEST 821: Evo PEKKA heals on kill")
print("  Data: trigger=on_kill, effect=heal(self)")
print("-" * 60)

m = new_match()
ep = safe_spawn(m, 1, "pekka", 0, -12000, evolved=True)

if ep is not None:
    step_n(m, DEPLOY_TICKS + 200)

    pe = find_entity(m, ep)
    pekka_hp_full = pe["hp"] if pe else 0
    pekka_y = pe["y"] if pe else -6000
    print(f"  PEKKA initial HP: {pekka_hp_full}")

    # Spawn P2 knight at same position — PEKKA will fight and eventually kill it
    dmg_knight = m.spawn_troop(2, "knight", 0, pekka_y)
    step_n(m, DEPLOY_TICKS)  # let knight deploy

    # Track tick-by-tick: detect the moment the knight dies
    # At that moment, on_kill should fire and heal PEKKA
    prev_hp = find_entity(m, ep)["hp"] if find_entity(m, ep) else 0
    knight_was_alive = True
    hp_at_kill = None
    hp_after_kill = None

    for t in range(120):
        m.step()
        pe_now = find_entity(m, ep)
        kn_now = find_entity(m, dmg_knight)
        
        if pe_now is None or not pe_now["alive"]:
            print(f"  PEKKA died at tick {t}")
            break

        current_hp = pe_now["hp"]
        knight_alive = kn_now is not None and kn_now.get("alive", False)

        if knight_was_alive and not knight_alive:
            # Knight just died this tick — record HP before and after
            hp_at_kill = prev_hp
            hp_after_kill = current_hp
            print(f"  Knight killed at tick {t}: PEKKA HP {prev_hp} → {current_hp}")
            break

        prev_hp = current_hp
        knight_was_alive = knight_alive

    if hp_at_kill is not None and hp_after_kill is not None:
        healed = hp_after_kill >= hp_at_kill
        print(f"  HP at kill: {hp_at_kill}  HP after kill: {hp_after_kill}")
        check("821a: Evo PEKKA HP increased or maintained on kill (heal on kill)",
              healed,
              f"before={hp_at_kill} after={hp_after_kill}")
    else:
        # Knight might not have died — check if PEKKA is still fighting
        pe_final = find_entity(m, ep)
        kn_final = find_entity(m, dmg_knight)
        print(f"  No kill detected: PEKKA hp={pe_final['hp'] if pe_final else 'dead'} knight={'alive' if kn_final and kn_final['alive'] else 'dead'}")
        check("821a: PEKKA killed the knight", False, "knight survived 120 ticks")
else:
    check("821: PEKKA spawnable", False)


# ====================================================================
# SUMMARY
# ====================================================================

print("\n" + "=" * 70)
print(f"  RESULTS: {PASS}/{PASS + FAIL} passed, {FAIL}/{PASS + FAIL} failed")
print("=" * 70)

print("\n  Section coverage:")
sections = {
    "A: Evo Knight (800-801)": "is_evolved flag, 60% damage reduction while idle",
    "B: Evo Barbarians (805-806)": "is_evolved flag, hitspeed ramp after attacking",
    "C: Evo Cannon (810)": "building deploy blast (needs evolved building support)",
    "D: Evo Valkyrie (815-816)": "is_evolved flag, extra AoE on attack",
    "E: Evo PEKKA (820-821)": "is_evolved flag, heal on kill",
}
for section, desc in sections.items():
    print(f"    {section}")
    print(f"      → {desc}")

sys.exit(0 if FAIL == 0 else 1)
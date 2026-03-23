"""
Engine fidelity tests — batch 3 (hard + extremely hard)

Place in: simulator/test_engine_3.py
Run with: python test_engine_3.py

Tests:
  11. DPS accuracy — Knight damage per second matches real CR
  12. Ground troop can't hit air — Knight ignores Balloon overhead
  13. River blocks ground troops — must path through bridge
  14. Crown tower damage reduction — Miner does reduced damage to towers
  15. Dark Prince shield absorbs damage before HP
  16. Building lifetime — Tesla expires after its timer runs out
  17. Overtime tiebreaker — tied match goes to overtime, then HP comparison
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
    return [e for e in match.get_entities()
            if e["alive"] and e["kind"] == kind
            and (team is None or e["team"] == team)]

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
# TEST 11: DPS accuracy — Knight total damage over 5 seconds
# =========================================================================
#
# MEDIUM difficulty.
#
# What this tests:
#   Knight: DMG=~202 at lvl11, hit_speed=1200ms, so DPS = 202/1.2 = 168.3
#   Over 5 seconds (100 ticks), Knight should deal ~842 total damage.
#   We measure actual damage dealt to a high-HP target and compare.
#
# Why it matters:
#   If DPS is off, every interaction in the game is wrong. This is more
#   precise than Test 4 — we measure total damage over a known window,
#   not just intervals between hits.

def test_dps_accuracy():
    print("\n" + "="*60)
    print("TEST 11: DPS accuracy (Knight over 5 seconds)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Knight attacks P2 Golem. Place both near the center of the arena
    # far from ALL towers so only Knight damage is measured (no tower fire).
    # P1 princess towers at (±5100, -10200) range 7500 → can't reach Y=0 area
    # P2 princess towers at (±5100, +10200) range 7500 → can't reach Y=0 area
    # Golem (P2) walks toward -Y (P1 side), Knight (P1) walks toward +Y (P2 side).
    # They converge near Y=0 — well out of any tower range.
    knight_id = m.spawn_troop(1, "knight", 0, -1500)
    golem_id = m.spawn_troop(2, "golem", 0, -900)  # 600 apart, near river

    # Wait for deploy + first hit
    for _ in range(60):
        m.step()

    e_golem = find_entity(m, golem_id)
    hp_after_deploy = e_golem["hp"] if e_golem else 0

    # Wait for first contact (should be immediate since they're in range)
    for _ in range(200):
        m.step()
        e_golem = find_entity(m, golem_id)
        if e_golem and e_golem["hp"] < hp_after_deploy:
            break

    # Now measure damage over exactly 100 ticks (5 seconds)
    hp_start = find_entity(m, golem_id)["hp"]
    start_tick = m.tick

    for _ in range(100):
        m.step()

    e_golem = find_entity(m, golem_id)
    hp_end = e_golem["hp"]
    damage_dealt = hp_start - hp_end
    elapsed_ticks = m.tick - start_tick
    dps = damage_dealt / (elapsed_ticks / 20.0)

    # Knight at lvl11: damage ~202, hit_speed 1200ms → DPS ~168
    # Allow 100-250 DPS range (accounts for hit timing alignment)
    print(f"\n  Damage dealt in {elapsed_ticks} ticks: {damage_dealt}")
    print(f"  DPS: {dps:.1f}  (expected ~168 for Knight)")

    check("Knight dealt damage", damage_dealt > 0)
    check("DPS in realistic range (100-250)",
          100 <= dps <= 250,
          f"DPS={dps:.1f} — damage or attack speed is off")
    check("DPS reasonably close to expected 168 (±50%)",
          84 <= dps <= 252,
          f"DPS={dps:.1f}")


# =========================================================================
# TEST 12: Ground troop can't hit air
# =========================================================================
#
# MEDIUM difficulty.
#
# What this tests:
#   Knight (attacks_air=false) should NEVER target or damage a Balloon
#   (flying=true). Even if the Balloon flies directly overhead.
#
# Real CR: A Knight standing under a Balloon literally cannot touch it.
# The Balloon sails past untouched unless you have anti-air.
#
# Why it matters:
#   If ground troops can hit air, the entire air/ground targeting system
#   is broken. Baby Dragon, Balloon, Minions become useless.

def test_ground_cant_hit_air():
    print("\n" + "="*60)
    print("TEST 12: Ground troop can't hit air (Knight vs Balloon)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    knight_stats = data.get_character_stats("knight")
    balloon_stats = data.get_character_stats("balloon")
    print(f"\n  Knight: attacks_air={knight_stats['attacks_air']}")
    print(f"  Balloon: is_flying={balloon_stats['is_flying']}")

    # Place Knight and Balloon at same position — Knight can't reach it
    knight_id = m.spawn_troop(1, "knight", 0, 0)
    balloon_id = m.spawn_troop(2, "balloon", 0, 0)

    e_balloon = find_entity(m, balloon_id)
    initial_hp = e_balloon["hp"]

    # Run 200 ticks — Knight should never damage Balloon
    for _ in range(200):
        m.step()

    e_balloon = find_entity(m, balloon_id)
    balloon_damaged = e_balloon is None or (e_balloon and e_balloon["hp"] < initial_hp)

    # Note: towers might shoot the Balloon, so we only check Knight's contribution
    # by verifying the Knight has attacks_air=false in its stats
    e_knight = find_entity(m, knight_id)

    check("Knight cannot attack air (stats)", knight_stats["attacks_air"] == False)
    check("Balloon is flying (stats)", balloon_stats["is_flying"] == True)

    # The real check: did the balloon take damage ONLY from towers (109 per hit)
    # or did it take extra damage that could only come from Knight?
    if e_balloon:
        tower_max_damage = 200 * 109 // 16  # max tower hits in 200 ticks
        damage_taken = initial_hp - e_balloon["hp"]
        print(f"  Balloon HP: {e_balloon['hp']}/{initial_hp}  (damage taken: {damage_taken})")
        print(f"  Max possible tower damage in 200 ticks: ~{tower_max_damage}")
        # If damage exceeds what towers alone could do, Knight is hitting air
        check("Balloon damage consistent with tower-only (no Knight hits)",
              damage_taken <= tower_max_damage + 50,
              f"damage={damage_taken} exceeds tower-only max {tower_max_damage}")


# =========================================================================
# TEST 13: River blocks ground troops — must use bridge
# =========================================================================
#
# HARD difficulty.
#
# What this tests:
#   A ground troop spawned at X=0 (center, between bridges) cannot walk
#   straight north across the river. It must detour to a bridge (X=±5100).
#   We check that after some ticks, the troop's X has shifted toward a
#   bridge, NOT stayed at X=0.
#
# Real CR: Ground troops ALWAYS path through one of the two bridges.
# If you drop a Knight at center, it walks diagonally to the nearest bridge.
#
# Why it matters:
#   Without river blocking, troops walk straight through the middle,
#   making bridge control irrelevant. Lane strategy breaks completely.

def test_river_blocking():
    print("\n" + "="*60)
    print("TEST 13: River blocks ground troops (bridge pathing)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # P1 Knight at center X=0, south of river
    knight_id = m.spawn_troop(1, "knight", 0, -2000)

    # Wait for deploy
    for _ in range(30):
        m.step()

    e = find_entity(m, knight_id)
    start_x = e["x"]
    start_y = e["y"]
    print(f"\n  Start: ({start_x}, {start_y})")

    # Run 100 ticks — Knight should be heading toward a bridge
    for _ in range(100):
        m.step()

    e = find_entity(m, knight_id)
    mid_x = e["x"]
    mid_y = e["y"]
    print(f"  After 100 ticks: ({mid_x}, {mid_y})")

    # The Knight should have moved its X toward ±5100 (a bridge)
    x_shift = abs(mid_x) - abs(start_x)
    print(f"  X shift toward bridge: {abs(mid_x)} (started at {abs(start_x)})")

    check("Knight moved laterally toward bridge (|X| increased)",
          abs(mid_x) > abs(start_x) + 500,
          f"X barely moved: {start_x} → {mid_x}. River might not be blocking.")

    # Run 200 more ticks — should be on or past the bridge
    for _ in range(200):
        m.step()

    e = find_entity(m, knight_id)
    if e and e["alive"]:
        final_x = e["x"]
        final_y = e["y"]
        print(f"  After 300 ticks: ({final_x}, {final_y})")

        # Should be near a bridge X (±5100 ± 1200)
        near_left = abs(final_x - (-5100)) <= 2500
        near_right = abs(final_x - 5100) <= 2500
        check("Knight near a bridge X position",
              near_left or near_right,
              f"X={final_x}, expected near ±5100")

        # Y should have advanced past the river (Y > -1200 for P1)
        check("Knight crossed or approaching river (Y > -2000)",
              final_y > -2000,
              f"Y={final_y}, still south of river")


# =========================================================================
# TEST 14: Crown tower damage reduction
# =========================================================================
#
# HARD difficulty.
#
# What this tests:
#   Miner has crown_tower_damage_percent (reduced damage vs towers).
#   We compare damage dealt to a TROOP vs damage dealt to a TOWER by
#   running two isolated scenarios and comparing per-hit damage.
#
# Approach:
#   Match A: Miner attacks a high-HP troop (Golem). Measure damage per hit.
#   Match B: Miner attacks a princess tower. Track HP tick-by-tick to
#            isolate individual hits (look for HP drops between ticks).
#   If CT reduction works, tower damage per hit < troop damage per hit.
#
# Why it matters:
#   Without CT reduction, spells and miners melt towers instantly.

def test_crown_tower_reduction():
    print("\n" + "="*60)
    print("TEST 14: Crown tower damage reduction (Miner)")
    print("="*60)

    miner_stats = data.get_character_stats("miner")
    print(f"\n  Miner base damage: {miner_stats['damage']}")
    print(f"  Miner hit_speed: {miner_stats['hit_speed']}ms")

    # ── Match A: Miner vs Golem (troop damage, no CT reduction) ──
    m_a = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    miner_a = m_a.spawn_troop(1, "miner", 0, -5000)
    golem_a = m_a.spawn_troop(2, "golem", 0, -4400)  # Golem walks toward P1 side (toward Miner)

    # Wait for deploy + first contact
    golem_hp_initial = None
    for _ in range(50):
        m_a.step()
    golem_hp_initial = find_entity(m_a, golem_a)["hp"]

    # Collect damage events (tick-by-tick HP drops on the Golem)
    troop_hits = []
    prev_hp = golem_hp_initial
    for t in range(200):
        m_a.step()
        e = find_entity(m_a, golem_a)
        if e is None:
            break
        if e["hp"] < prev_hp:
            hit_dmg = prev_hp - e["hp"]
            troop_hits.append(hit_dmg)
            prev_hp = e["hp"]

    if troop_hits:
        avg_troop_hit = sum(troop_hits) / len(troop_hits)
        print(f"  Miner vs Golem: {len(troop_hits)} hits, damages={troop_hits[:5]}")
        print(f"  Average damage per hit to troop: {avg_troop_hit:.0f}")
    else:
        print(f"  Miner vs Golem: no hits landed")
        avg_troop_hit = 0

    # ── Match B: Miner vs Tower (should have CT reduction) ──
    m_b = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)
    # Spawn Miner right next to P2's left princess tower
    miner_b = m_b.spawn_troop(1, "miner", -5100, 9800)

    # Wait for deploy
    for _ in range(50):
        m_b.step()

    # Track tower HP tick-by-tick to find individual hit damages
    tower_hits = []
    prev_tower_hp = m_b.p2_tower_hp()[1]  # left princess
    for t in range(200):
        m_b.step()
        tower_hp_now = m_b.p2_tower_hp()[1]
        if tower_hp_now < prev_tower_hp:
            hit_dmg = prev_tower_hp - tower_hp_now
            tower_hits.append(hit_dmg)
            prev_tower_hp = tower_hp_now

    if tower_hits:
        avg_tower_hit = sum(tower_hits) / len(tower_hits)
        print(f"  Miner vs Tower: {len(tower_hits)} hits, damages={tower_hits[:5]}")
        print(f"  Average damage per hit to tower: {avg_tower_hit:.0f}")
    else:
        print(f"  Miner vs Tower: no hits landed")
        avg_tower_hit = 0

    # ── Compare ──
    check("Miner dealt damage to troop", len(troop_hits) > 0)
    check("Miner dealt damage to tower", len(tower_hits) > 0)

    if avg_troop_hit > 0 and avg_tower_hit > 0:
        ratio = avg_tower_hit / avg_troop_hit
        print(f"\n  Tower/Troop damage ratio: {ratio:.2f}  (expected ~0.35 for Miner)")

        check("Tower damage < troop damage (CT reduction active)",
              avg_tower_hit < avg_troop_hit,
              f"tower={avg_tower_hit:.0f} >= troop={avg_troop_hit:.0f} — CT reduction not working")

        # Miner CT reduction should make tower hits 25-50% of troop hits
        check("CT reduction ratio reasonable (0.1 - 0.7)",
              0.1 <= ratio <= 0.7,
              f"ratio={ratio:.2f} — expected 0.25-0.50 for Miner")


# =========================================================================
# TEST 15: Dark Prince shield absorbs damage before HP
# =========================================================================
#
# HARD difficulty.
#
# What this tests:
#   Dark Prince has shield_hitpoints > 0. Damage should hit the shield
#   first. Only after shield breaks does HP decrease.
#
# Real CR: Dark Prince's shield absorbs a fixed amount of damage.
# Even a Sparky shot (1320 damage) only breaks the shield — the
# remaining damage does NOT carry through to HP (shield absorbs the
# excess of the hit that breaks it... actually in CR, overflow DOES
# carry through). Let's test the basic mechanic.
#
# Why it matters:
#   Guards, Dark Prince, and any shielded troop depend on this.
#   Without shields, they're just squishy troops with wrong HP.

def test_shield_mechanics():
    print("\n" + "="*60)
    print("TEST 15: Dark Prince shield absorbs damage")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Spawn Dark Prince (P2) — he has a shield
    dp_id = m.spawn_troop(2, "darkprince", 0, 0)

    e_dp = find_entity(m, dp_id)
    if e_dp is None:
        check("Dark Prince spawned", False, "darkprince not in character data")
        return

    initial_hp = e_dp["hp"]
    initial_shield = e_dp["shield_hp"]
    print(f"\n  Dark Prince: HP={initial_hp}  Shield={initial_shield}")

    if initial_shield <= 0:
        check("Dark Prince has shield HP", False, f"shield_hp={initial_shield}")
        return

    check("Dark Prince has shield HP > 0", initial_shield > 0)

    # Spawn a P1 Knight to hit the Dark Prince
    knight_id = m.spawn_troop(1, "knight", 0, -300)

    # Run until Knight damages the Dark Prince
    shield_took_damage = False
    hp_took_damage_before_shield_broke = False

    for t in range(300):
        m.step()
        e_dp = find_entity(m, dp_id)
        if e_dp is None:
            break

        current_shield = e_dp["shield_hp"]
        current_hp = e_dp["hp"]

        if current_shield < initial_shield and not shield_took_damage:
            shield_took_damage = True
            print(f"  Shield first hit at tick {t+1}: shield={current_shield}/{initial_shield}  HP={current_hp}/{initial_hp}")

            # HP should still be full while shield is taking damage
            if current_hp < initial_hp and current_shield > 0:
                hp_took_damage_before_shield_broke = True

        if current_shield <= 0 and shield_took_damage:
            print(f"  Shield broke at tick {t+1}: HP={current_hp}/{initial_hp}")
            break

    check("Shield absorbed damage first", shield_took_damage,
          "Shield HP never decreased")
    check("HP stayed full while shield was up",
          not hp_took_damage_before_shield_broke,
          "HP decreased before shield broke — damage should hit shield first")


# =========================================================================
# TEST 16: Building lifetime — Tesla expires after timer
# =========================================================================
#
# HARD difficulty.
#
# What this tests:
#   Tesla has a lifetime (life_time field in JSON, typically 30-40 seconds).
#   After that many ticks, the building should self-destruct even if
#   it has full HP and no enemies nearby.
#
# Real CR: Every building has a lifetime timer visible as a shrinking HP bar.
# Tesla lasts 35 seconds. After that it dies regardless of remaining HP.
#
# Why it matters:
#   Without lifetime decay, defensive buildings last forever, making
#   them impossibly overpowered. Tesla/Inferno Tower/X-Bow would never
#   disappear.

def test_building_lifetime():
    print("\n" + "="*60)
    print("TEST 16: Building lifetime (Tesla expires)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Spawn a Tesla for P1 — no enemies nearby, just let it tick
    tesla_id = m.spawn_building(1, "tesla", 0, -5000)

    e = find_entity(m, tesla_id)
    if e is None:
        check("Tesla spawned", False, "tesla not in building data")
        return

    initial_hp = e["hp"]
    print(f"\n  Tesla spawned: HP={initial_hp}")

    # Tesla lifetime is typically 30-35 seconds = 600-700 ticks
    # Run for 1000 ticks and check if it dies
    tesla_alive_at = {}
    for t in range(1000):
        m.step()
        e = find_entity(m, tesla_id)

        if t + 1 in [200, 400, 600, 800]:
            alive = e is not None and e["alive"] if e else False
            hp = e["hp"] if e and e["alive"] else 0
            tesla_alive_at[t + 1] = alive
            print(f"  tick {t+1}: alive={alive}  HP={hp}")

        if e is None or (e and not e["alive"]):
            print(f"  Tesla expired at tick {t+1} ({(t+1)/20:.1f}s)")
            break

    expired = e is None or (e is not None and not e["alive"])
    check("Tesla expired within 1000 ticks (50 seconds)",
          expired,
          "Tesla still alive at tick 1000 — lifetime decay not working")

    if expired:
        lifetime_ticks = t + 1
        lifetime_seconds = lifetime_ticks / 20.0
        print(f"  Lifetime: {lifetime_seconds:.1f}s ({lifetime_ticks} ticks)")
        # Tesla should last 30-40 seconds in real CR
        check("Lifetime in reasonable range (20-50 seconds)",
              20 <= lifetime_seconds <= 50,
              f"lasted {lifetime_seconds:.1f}s — expected ~35s")


# =========================================================================
# TEST 17: Overtime tiebreaker — match goes beyond regular time
# =========================================================================
#
# EXTREMELY HARD.
#
# What this tests:
#   1. A match with 0-0 crowns at end of regular time goes to overtime
#   2. Phase transitions: regular → double_elixir → overtime
#   3. If still 0-0 at end of overtime, match ends as draw (or sudden death)
#   4. Match doesn't run forever
#
# This tests the entire match lifecycle without any combat — pure timing.
#
# Real CR timing:
#   Regular: 3 min = 3600 ticks
#   Double elixir: last 1 min of regular = starts at tick 1200
#   Overtime: 2 min = 2400 ticks (triple elixir)
#   Sudden death: 3 min = 3600 ticks
#   Max total: 9600 ticks
#
# Why it matters:
#   If phase transitions are wrong, double/triple elixir kicks in at
#   the wrong time. If overtime resolution is broken, matches never end.

def test_overtime_tiebreaker():
    print("\n" + "="*60)
    print("TEST 17: Overtime tiebreaker (full match lifecycle)")
    print("="*60)

    m = cr_engine.new_match(data, DUMMY_DECK, DUMMY_DECK)

    # Run without playing any cards — 0-0 forever
    phases_seen = set()
    phase_transitions = []

    print(f"\n  Running empty match (no card plays)...")

    # Step through key checkpoints
    checkpoints = {
        1199: "last tick before double elixir",
        1200: "double elixir starts",
        3599: "last tick of regular time",
        3600: "overtime starts",
        5999: "last tick of overtime",
        6000: "sudden death starts",
        9599: "last tick of sudden death",
        9600: "max match ticks",
    }

    last_phase = None
    for t in range(10000):
        if not m.is_running:
            print(f"  Match ended at tick {t} ({t/20:.1f}s)")
            break
        m.step()

        current_phase = m.phase
        if current_phase != last_phase:
            phase_transitions.append((t + 1, current_phase))
            last_phase = current_phase

        phases_seen.add(current_phase)

        if t + 1 in checkpoints:
            print(f"  tick {t+1:5d} ({(t+1)/20:.0f}s): phase={current_phase}  "
                  f"p1_elixir={m.p1_elixir}  [{checkpoints[t+1]}]")

    end_tick = m.tick
    result = m.get_result()
    winner = result["winner"]

    print(f"\n  Final result: {winner} at tick {end_tick} ({end_tick/20:.1f}s)")
    print(f"  Phases seen: {phases_seen}")
    print(f"  Phase transitions: {phase_transitions}")
    print(f"  P1 crowns: {result['p1_crowns']}  P2 crowns: {result['p2_crowns']}")

    check("Match ended (didn't run forever)", not m.is_running)
    check("Saw regular phase", "regular" in phases_seen)
    check("Saw double_elixir phase", "double_elixir" in phases_seen)
    check("Saw overtime phase", "overtime" in phases_seen,
          f"phases seen: {phases_seen}")
    check("Match resolved as draw (0-0 with equal king HP)",
          winner == "draw",
          f"winner={winner} — expected draw with no combat")
    check("Match lasted ~9600 ticks (max match length)",
          9500 <= end_tick <= 9700,
          f"ended at tick {end_tick}")


# =========================================================================
# Run all tests
# =========================================================================

if __name__ == "__main__":
    print("="*60)
    print("  CLASH ROYALE ENGINE FIDELITY TESTS — BATCH 3")
    print("  Tests 11-17: DPS, air/ground, river, CT reduction,")
    print("  shields, building lifetime, overtime")
    print("="*60)

    test_dps_accuracy()
    test_ground_cant_hit_air()
    test_river_blocking()
    test_crown_tower_reduction()
    test_shield_mechanics()
    test_building_lifetime()
    test_overtime_tiebreaker()

    print("\n" + "="*60)
    total = PASS + FAIL
    print(f"  RESULTS: {PASS}/{total} passed, {FAIL}/{total} failed")
    print("="*60)

    if FAIL > 0:
        print("\n  Each failure points to a specific engine system.")
        print("  The hard tests (13-17) stress edge cases that only")
        print("  surface in specific game situations.")
        sys.exit(1)
    else:
        print("\n  All checks passed! Engine is tournament-accurate.")
        sys.exit(0)
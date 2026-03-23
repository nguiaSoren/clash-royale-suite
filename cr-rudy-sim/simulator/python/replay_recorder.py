"""
Replay recorder — captures tick-by-tick match state for visualization.

Hooks into match_runner.run_match() loop and serializes game state
snapshots to a compact JSON replay file.

Usage:
    from replay_recorder import record_match, save_replay, load_replay

    # Record a match (runs it and captures every Nth tick)
    replay = record_match(data, deck1, deck2, agent1, agent2, sample_rate=1)
    save_replay(replay, "replays/match_001.json")

    # Load for playback
    replay = load_replay("replays/match_001.json")
"""

import json
import gzip
import time
from typing import Optional
from pathlib import Path

try:
    import cr_engine
except ImportError:
    raise ImportError(
        "cr_engine not found. Build with: cd simulator && maturin develop --release"
    )


# =========================================================================
# Replay data structures
# =========================================================================

def _snapshot_towers(match_obj, player: int) -> dict:
    """Capture tower state for one player."""
    hp = match_obj.p1_tower_hp() if player == 1 else match_obj.p2_tower_hp()
    obs = match_obj.get_observation(player)
    return {
        "king_hp": hp[0],
        "princess_left_hp": hp[1],
        "princess_right_hp": hp[2],
        "king_alive": obs[f"my_king_alive"],
        "pl_alive": obs[f"my_princess_left_alive"],
        "pr_alive": obs[f"my_princess_right_alive"],
    }


def _snapshot_entities(match_obj) -> list:
    """Capture all alive entities in compact form."""
    entities = match_obj.get_entities()
    result = []
    for e in entities:
        if not e["alive"]:
            continue
        entry = {
            "id": e["id"],
            "t": e["team"],          # 1 or 2
            "k": e["kind"],          # "troop", "building", "projectile", "spell_zone"
            "c": e["card_key"],      # card key
            "x": e["x"],
            "y": e["y"],
            "hp": e["hp"],
            "mhp": e["max_hp"],      # max hp for health bar
            "dmg": e["damage"],
        }
        # Optional fields — only include if non-default to save space
        if e.get("z", 0) > 0:
            entry["z"] = e["z"]      # flying height
        if e.get("shield_hp", 0) > 0:
            entry["shp"] = e["shield_hp"]
        if e.get("is_evolved", False):
            entry["evo"] = True
        if e.get("is_hero", False):
            entry["hero"] = True
        if e.get("hero_ability_active", False):
            entry["hab"] = True
        if e.get("is_stunned", False):
            entry["stun"] = True
        if e.get("is_frozen", False):
            entry["frz"] = True
        if e.get("is_invisible", False):
            entry["inv"] = True
        if e.get("attack_phase", "idle") != "idle":
            entry["atk"] = e["attack_phase"]  # "windup" or "backswing"
        if e.get("charge_ready", False):
            entry["chrg"] = e.get("charge_damage", 0)  # charge damage when ready
        if e["kind"] == "spell_zone":
            entry["r"] = e.get("sz_radius", 0)
            entry["rem"] = e.get("sz_remaining", 0)
        result.append(entry)
    return result


def _snapshot_tick(match_obj) -> dict:
    """Capture full game state for a single tick."""
    return {
        "tick": match_obj.tick,
        "phase": match_obj.phase,
        "p1_elixir": match_obj.p1_elixir,
        "p2_elixir": match_obj.p2_elixir,
        "p1": _snapshot_towers(match_obj, 1),
        "p2": _snapshot_towers(match_obj, 2),
        "entities": _snapshot_entities(match_obj),
    }


# =========================================================================
# Record a match
# =========================================================================

def record_match(
    data,
    deck1: list[str],
    deck2: list[str],
    agent1,
    agent2,
    sample_rate: int = 1,
    max_ticks: Optional[int] = None,
) -> dict:
    """Run a match and record tick-by-tick snapshots.

    Args:
        data: cr_engine.GameData
        deck1, deck2: Player decks (8 card keys each)
        agent1, agent2: Agent instances (from ai_agent.py)
        sample_rate: Record every Nth tick. 1 = every tick (full fidelity),
                     2 = every other tick, etc. Lower = smaller file.
        max_ticks: Override max match ticks.

    Returns:
        Replay dict with metadata + frames array.
    """
    from python.ai_agent import Action

    match_obj = cr_engine.new_match(data, deck1, deck2)
    agent1.on_match_start(match_obj, 1)
    agent2.on_match_start(match_obj, 2)

    frames = []
    events = []  # Card play events for the event log
    tick = 0

    # Capture initial state
    frames.append(_snapshot_tick(match_obj))

    while match_obj.is_running:
        if max_ticks and tick >= max_ticks:
            break

        # Agent decisions
        action1 = agent1.decide(match_obj, 1, data)
        action2 = agent2.decide(match_obj, 2, data)

        # Log card plays as events
        if action1.play:
            obs = match_obj.get_observation(1)
            hand = obs["my_hand"]
            card_key = hand[action1.hand_index] if action1.hand_index < len(hand) else "?"
            events.append({
                "tick": tick,
                "player": 1,
                "action": "play",
                "card": card_key,
                "x": action1.x,
                "y": action1.y,
            })

        if action2.play:
            obs = match_obj.get_observation(2)
            hand = obs["my_hand"]
            card_key = hand[action2.hand_index] if action2.hand_index < len(hand) else "?"
            events.append({
                "tick": tick,
                "player": 2,
                "action": "play",
                "card": card_key,
                "x": action2.x,
                "y": action2.y,
            })

        # Execute actions
        _execute_action(match_obj, 1, action1)
        _execute_action(match_obj, 2, action2)

        # Step
        match_obj.step()
        tick += 1

        # Sample
        if tick % sample_rate == 0:
            frames.append(_snapshot_tick(match_obj))

    # Final frame
    if tick % sample_rate != 0:
        frames.append(_snapshot_tick(match_obj))

    result = match_obj.get_result()

    replay = {
        "version": 1,
        "recorded_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "deck1": deck1,
        "deck2": deck2,
        "sample_rate": sample_rate,
        "total_ticks": tick,
        "result": {
            "winner": result["winner"],
            "p1_crowns": result["p1_crowns"],
            "p2_crowns": result["p2_crowns"],
            "ticks": result["ticks"],
            "seconds": result["seconds"],
        },
        "events": events,
        "frames": frames,
    }

    return replay


def _execute_action(match_obj, player: int, action):
    """Execute an agent action (same as match_runner but without verbose)."""
    if action.activate_hero is not None:
        try:
            match_obj.activate_hero(action.activate_hero)
        except ValueError:
            pass
        return

    if not action.play:
        return

    try:
        match_obj.play_card(player, action.hand_index, action.x, action.y)
    except (ValueError, KeyError):
        pass


# =========================================================================
# Save / Load
# =========================================================================

def save_replay(replay: dict, path: str, compress: bool = True):
    """Save replay to JSON (optionally gzipped).

    Args:
        replay: Replay dict from record_match().
        path: Output file path (.json or .json.gz).
        compress: If True, gzip compress (typically 5-10x smaller).
    """
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)

    json_str = json.dumps(replay, separators=(",", ":"))  # compact

    if compress or path.endswith(".gz"):
        with gzip.open(str(p), "wt", encoding="utf-8") as f:
            f.write(json_str)
    else:
        with open(str(p), "w") as f:
            f.write(json_str)

    size_kb = p.stat().st_size / 1024
    n_frames = len(replay["frames"])
    print(f"Saved replay: {path} ({size_kb:.1f} KB, {n_frames} frames, {replay['total_ticks']} ticks)")


def load_replay(path: str) -> dict:
    """Load a replay from JSON (handles gzip transparently)."""
    p = Path(path)
    if path.endswith(".gz"):
        with gzip.open(str(p), "rt", encoding="utf-8") as f:
            return json.load(f)
    else:
        with open(str(p), "r") as f:
            return json.load(f)


# =========================================================================
# Generate demo replay (for testing viewer without engine)
# =========================================================================

def generate_demo_replay() -> dict:
    """Generate a synthetic demo replay for testing the viewer.

    Creates a plausible-looking match with troops moving, fighting, and
    towers taking damage. No engine required.
    """
    import math
    import random

    frames = []
    events = []
    total_ticks = 2400  # 2 minutes

    # Simulated entities
    next_id = 1
    active_entities = []

    # Tower HP tracking
    p1_king_hp = 4824
    p2_king_hp = 4824
    p1_pl_hp = 3052
    p1_pr_hp = 3052
    p2_pl_hp = 3052
    p2_pr_hp = 3052

    troop_types = [
        ("knight", 1399, 167),
        ("archers", 304, 99),
        ("musketeer", 598, 181),
        ("giant", 3275, 120),
        ("valkyrie", 1654, 120),
        ("hog-rider", 1408, 220),
        ("minions", 252, 84),
        ("baby-dragon", 1152, 100),
    ]

    for tick in range(0, total_ticks, 1):
        # Spawn troops periodically
        if tick % 120 == 60 and tick < 2000:
            # Player 1 deploys
            troop = random.choice(troop_types)
            lane_x = random.choice([-5100, 5100])
            eid = next_id; next_id += 1
            active_entities.append({
                "id": eid, "team": 1, "card_key": troop[0],
                "x": lane_x + random.randint(-400, 400),
                "y": -10000 + random.randint(-1000, 1000),
                "hp": troop[1], "max_hp": troop[1], "damage": troop[2],
                "kind": "troop", "speed": 350 + random.randint(-100, 200),
                "alive": True, "is_flying": troop[0] in ("minions", "baby-dragon"),
            })
            events.append({"tick": tick, "player": 1, "action": "play", "card": troop[0], "x": lane_x, "y": -10000})

        if tick % 120 == 0 and tick < 2000 and tick > 0:
            # Player 2 deploys
            troop = random.choice(troop_types)
            lane_x = random.choice([-5100, 5100])
            eid = next_id; next_id += 1
            active_entities.append({
                "id": eid, "team": 2, "card_key": troop[0],
                "x": lane_x + random.randint(-400, 400),
                "y": 10000 + random.randint(-1000, 1000),
                "hp": troop[1], "max_hp": troop[1], "damage": troop[2],
                "kind": "troop", "speed": 350 + random.randint(-100, 200),
                "alive": True, "is_flying": troop[0] in ("minions", "baby-dragon"),
            })
            events.append({"tick": tick, "player": 2, "action": "play", "card": troop[0], "x": lane_x, "y": 10000})

        # Move troops toward enemy side
        for e in active_entities:
            if not e["alive"]:
                continue
            dy = e["speed"] // 20  # per-tick movement
            if e["team"] == 1:
                e["y"] += dy
                # Damage towers when close
                if e["y"] > 9000:
                    p2_pl_hp = max(0, p2_pl_hp - e["damage"] // 10)
                if e["y"] > 13000:
                    e["alive"] = False
            else:
                e["y"] -= dy
                if e["y"] < -9000:
                    p1_pl_hp = max(0, p1_pl_hp - e["damage"] // 10)
                if e["y"] < -13000:
                    e["alive"] = False

            # Random HP decay (simulating combat)
            if random.random() < 0.02:
                e["hp"] = max(0, e["hp"] - random.randint(30, 150))
                if e["hp"] <= 0:
                    e["alive"] = False

        # Build frame
        phase = "regular" if tick < 1200 else "double_elixir" if tick < 2400 else "overtime"
        elixir_rate = 1 if tick < 1200 else 2
        p1_elixir = min(10, 5 + (tick * elixir_rate) // 56)
        p2_elixir = min(10, 5 + (tick * elixir_rate) // 56)

        if tick % 2 == 0:  # Sample every 2 ticks for demo
            entities_snap = []
            for e in active_entities:
                if not e["alive"]:
                    continue
                entry = {
                    "id": e["id"], "t": e["team"], "k": e["kind"],
                    "c": e["card_key"], "x": e["x"], "y": e["y"],
                    "hp": e["hp"], "mhp": e["max_hp"], "dmg": e["damage"],
                }
                if e.get("is_flying"):
                    entry["z"] = 3000
                entities_snap.append(entry)

            frames.append({
                "tick": tick,
                "phase": phase,
                "p1_elixir": p1_elixir % 11,
                "p2_elixir": p2_elixir % 11,
                "p1": {
                    "king_hp": p1_king_hp, "princess_left_hp": p1_pl_hp,
                    "princess_right_hp": p1_pr_hp,
                    "king_alive": p1_king_hp > 0, "pl_alive": p1_pl_hp > 0, "pr_alive": p1_pr_hp > 0,
                },
                "p2": {
                    "king_hp": p2_king_hp, "princess_left_hp": p2_pl_hp,
                    "princess_right_hp": p2_pr_hp,
                    "king_alive": p2_king_hp > 0, "pl_alive": p2_pl_hp > 0, "pr_alive": p2_pr_hp > 0,
                },
                "entities": entities_snap,
            })

        # Clean dead entities
        active_entities = [e for e in active_entities if e["alive"]]

    winner = "player1" if p2_pl_hp < p1_pl_hp else "player2"
    p1_crowns = (0 if p2_pl_hp > 0 else 1) + (0 if p2_pr_hp > 0 else 1) + (0 if p2_king_hp > 0 else 3)
    p2_crowns = (0 if p1_pl_hp > 0 else 1) + (0 if p1_pr_hp > 0 else 1) + (0 if p1_king_hp > 0 else 3)

    return {
        "version": 1,
        "recorded_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "deck1": ["knight", "archers", "musketeer", "giant", "valkyrie", "hog-rider", "minions", "baby-dragon"],
        "deck2": ["knight", "archers", "musketeer", "giant", "valkyrie", "hog-rider", "minions", "baby-dragon"],
        "sample_rate": 2,
        "total_ticks": total_ticks,
        "result": {
            "winner": winner,
            "p1_crowns": p1_crowns,
            "p2_crowns": p2_crowns,
            "ticks": total_ticks,
            "seconds": total_ticks / 20.0,
        },
        "events": events,
        "frames": frames,
    }


if __name__ == "__main__":
    # Generate demo replay for viewer testing
    print("Generating demo replay...")
    replay = generate_demo_replay()
    save_replay(replay, "demo_replay.json", compress=False)
    print(f"Demo: {len(replay['frames'])} frames, {len(replay['events'])} events")
    print(f"Result: {replay['result']}")
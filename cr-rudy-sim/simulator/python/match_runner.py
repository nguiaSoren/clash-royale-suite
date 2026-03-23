"""
Orchestrate matches and collect results.

Usage:
    results = run_batch(game_data, matchups, n_per_matchup=100)
"""
"""
Orchestrate matches and collect results.

Usage:
    from data_loader import load_game_data, random_deck
    from ai_agent import RandomAgent, RuleBasedAgent
    from match_runner import run_match, run_batch, run_round_robin

    data = load_game_data("data/")
    deck1 = random_deck(data)
    deck2 = random_deck(data)

    # Single match with agents
    result = run_match(data, deck1, deck2, RandomAgent(), RuleBasedAgent())

    # Batch of matches
    results = run_batch(data, deck1, deck2, RandomAgent(), RandomAgent(), n=100)

    # Round robin with multiple decks
    summary = run_round_robin(data, [deck1, deck2, deck3], RandomAgent(), n_per_matchup=50)
"""

import time
from typing import Optional

try:
    import cr_engine
except ImportError:
    raise ImportError(
        "cr_engine not found. Build with: cd simulator && maturin develop --release"
    )

from .ai_agent import BaseAgent, Action, DoNothingAgent


# =========================================================================
# Single match runner
# =========================================================================

def run_match(
    data: "cr_engine.GameData",
    deck1: list[str],
    deck2: list[str],
    agent1: BaseAgent,
    agent2: BaseAgent,
    max_ticks: Optional[int] = None,
    verbose: bool = False,
) -> dict:
    """Run a single match tick-by-tick with two agents making decisions.

    Args:
        data: Game data from load_game_data().
        deck1: Player 1 deck (8 card keys).
        deck2: Player 2 deck (8 card keys).
        agent1: Agent controlling player 1.
        agent2: Agent controlling player 2.
        max_ticks: Override max ticks (None = use engine default).
        verbose: Print tick-by-tick info.

    Returns:
        Result dict with winner, crowns, ticks, tower HP, etc.
    """
    match = cr_engine.new_match(data, deck1, deck2)

    agent1.on_match_start(match, 1)
    agent2.on_match_start(match, 2)

    tick = 0
    while match.is_running:
        if max_ticks and tick >= max_ticks:
            break

        # Agents decide
        action1 = agent1.decide(match, 1, data)
        action2 = agent2.decide(match, 2, data)

        # Execute actions
        _execute_action(match, 1, action1, verbose)
        _execute_action(match, 2, action2, verbose)

        # Advance simulation by 1 tick
        match.step()
        tick += 1

        if verbose and tick % 200 == 0:
            print(
                f"  tick={tick:5d}  p1_elixir={match.p1_elixir}  p2_elixir={match.p2_elixir}  "
                f"entities={match.num_entities}  phase={match.phase}"
            )

    result = match.get_result()
    result["deck1"] = deck1
    result["deck2"] = deck2

    agent1.on_match_end(match, 1, result)
    agent2.on_match_end(match, 2, result)

    return result


def _execute_action(
    match: "cr_engine.Match",
    player: int,
    action: Action,
    verbose: bool = False,
):
    """Execute an agent's action on the match."""
    if action.activate_hero is not None:
        try:
            match.activate_hero(action.activate_hero)
            if verbose:
                print(f"  P{player}: activated hero {action.activate_hero}")
        except ValueError as e:
            if verbose:
                print(f"  P{player}: hero activation failed: {e}")
        return

    if not action.play:
        return

    try:
        entity_id = match.play_card(player, action.hand_index, action.x, action.y)
        if verbose:
            print(
                f"  P{player}: played hand[{action.hand_index}] at "
                f"({action.x}, {action.y}) → entity {entity_id}"
            )
    except (ValueError, KeyError) as e:
        if verbose:
            print(f"  P{player}: play_card failed: {e}")


# =========================================================================
# Batch runner
# =========================================================================

def run_batch(
    data: "cr_engine.GameData",
    deck1: list[str],
    deck2: list[str],
    agent1: BaseAgent,
    agent2: BaseAgent,
    n: int = 100,
    verbose: bool = False,
) -> list[dict]:
    """Run N matches with the same decks and agents.

    Args:
        data: Game data.
        deck1, deck2: Decks for each player.
        agent1, agent2: Agents for each player.
        n: Number of matches to run.
        verbose: Print progress.

    Returns:
        List of result dicts.
    """
    results = []
    t0 = time.time()

    for i in range(n):
        result = run_match(data, deck1, deck2, agent1, agent2)
        results.append(result)

        if verbose and (i + 1) % 10 == 0:
            elapsed = time.time() - t0
            rate = (i + 1) / elapsed
            print(f"  [{i+1}/{n}] {rate:.1f} matches/sec")

    elapsed = time.time() - t0
    if verbose:
        print(f"Completed {n} matches in {elapsed:.2f}s ({n/elapsed:.1f} matches/sec)")

    return results


# =========================================================================
# No-agent batch (Rust-side only, much faster)
# =========================================================================

def run_batch_fast(
    data: "cr_engine.GameData",
    deck1: list[str],
    deck2: list[str],
    n: int = 1000,
) -> list[dict]:
    """Run N matches entirely in Rust (no agents, no card plays).

    This tests the engine at full speed — troops never deploy, so it's
    primarily a benchmark and timeout test. For actual gameplay, use run_batch().

    Returns:
        List of result dicts.
    """
    return cr_engine.run_batch(data, deck1, deck2, n)


# =========================================================================
# Round robin
# =========================================================================

def run_round_robin(
    data: "cr_engine.GameData",
    decks: list[list[str]],
    agent1: Optional[BaseAgent] = None,
    agent2: Optional[BaseAgent] = None,
    n_per_matchup: int = 50,
    verbose: bool = False,
) -> dict:
    """Run round-robin matches between all deck pairs.

    Args:
        data: Game data.
        decks: List of decks to test against each other.
        agent1: Agent for player 1 (default: RandomAgent).
        agent2: Agent for player 2 (default: same as agent1).
        n_per_matchup: Matches per deck pair.
        verbose: Print progress.

    Returns:
        Dict with:
            "matchups": list of {deck1_idx, deck2_idx, p1_wins, p2_wins, draws}
            "deck_win_rates": list of floats (overall win rate per deck)
            "total_matches": int
    """
    from .ai_agent import RandomAgent

    if agent1 is None:
        agent1 = RandomAgent()
    if agent2 is None:
        agent2 = agent1  # Mirror match: same agent

    n_decks = len(decks)
    matchups = []
    deck_wins = [0] * n_decks
    deck_games = [0] * n_decks

    total = 0
    for i in range(n_decks):
        for j in range(i + 1, n_decks):
            if verbose:
                print(f"Matchup: deck[{i}] vs deck[{j}]")

            results = run_batch(data, decks[i], decks[j], agent1, agent2, n_per_matchup)

            p1_wins = sum(1 for r in results if r["winner"] == "player1")
            p2_wins = sum(1 for r in results if r["winner"] == "player2")
            draws = sum(1 for r in results if r["winner"] == "draw")

            matchups.append({
                "deck1_idx": i,
                "deck2_idx": j,
                "p1_wins": p1_wins,
                "p2_wins": p2_wins,
                "draws": draws,
                "p1_win_rate": p1_wins / n_per_matchup,
            })

            deck_wins[i] += p1_wins
            deck_wins[j] += p2_wins
            deck_games[i] += n_per_matchup
            deck_games[j] += n_per_matchup
            total += n_per_matchup

    deck_win_rates = [
        deck_wins[i] / deck_games[i] if deck_games[i] > 0 else 0.0
        for i in range(n_decks)
    ]

    return {
        "matchups": matchups,
        "deck_win_rates": deck_win_rates,
        "total_matches": total,
    }

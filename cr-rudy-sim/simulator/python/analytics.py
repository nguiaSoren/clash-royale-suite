"""
Win rate analysis, balance testing, visualization.

Usage:
    from analytics import summarize_results, print_matchup_table, card_win_rates

    results = run_batch(data, deck1, deck2, agent1, agent2, n=100)
    summary = summarize_results(results)
    print_matchup_table(round_robin_results)
"""

from typing import Optional
from collections import Counter, defaultdict


# =========================================================================
# Result summarization
# =========================================================================

def summarize_results(results: list[dict]) -> dict:
    """Summarize a batch of match results.

    Args:
        results: List of result dicts from match_runner.

    Returns:
        Dict with aggregate statistics.
    """
    n = len(results)
    if n == 0:
        return {"n": 0}

    p1_wins = sum(1 for r in results if r["winner"] == "player1")
    p2_wins = sum(1 for r in results if r["winner"] == "player2")
    draws = sum(1 for r in results if r["winner"] == "draw")
    in_progress = sum(1 for r in results if r["winner"] == "in_progress")

    ticks = [r["ticks"] for r in results]
    p1_crowns = [r["p1_crowns"] for r in results]
    p2_crowns = [r["p2_crowns"] for r in results]

    return {
        "n": n,
        "p1_wins": p1_wins,
        "p2_wins": p2_wins,
        "draws": draws,
        "in_progress": in_progress,
        "p1_win_rate": p1_wins / n,
        "p2_win_rate": p2_wins / n,
        "draw_rate": draws / n,
        "avg_ticks": sum(ticks) / n,
        "avg_duration_sec": sum(ticks) / n / 20.0,
        "min_ticks": min(ticks),
        "max_ticks": max(ticks),
        "avg_p1_crowns": sum(p1_crowns) / n,
        "avg_p2_crowns": sum(p2_crowns) / n,
        "three_crown_rate": sum(
            1 for r in results if r["p1_crowns"] >= 3 or r["p2_crowns"] >= 3
        ) / n,
    }


def print_summary(results: list[dict], label: str = ""):
    """Print a formatted summary of match results."""
    s = summarize_results(results)
    if s["n"] == 0:
        print("No results to summarize.")
        return

    header = f"Match Summary ({s['n']} games)"
    if label:
        header += f" — {label}"
    print(f"\n{'=' * 50}")
    print(header)
    print(f"{'=' * 50}")
    print(f"  P1 wins:      {s['p1_wins']:>5d}  ({s['p1_win_rate']:.1%})")
    print(f"  P2 wins:      {s['p2_wins']:>5d}  ({s['p2_win_rate']:.1%})")
    print(f"  Draws:        {s['draws']:>5d}  ({s['draw_rate']:.1%})")
    if s["in_progress"] > 0:
        print(f"  In progress:  {s['in_progress']:>5d}")
    print(f"  Avg duration: {s['avg_duration_sec']:.1f}s ({s['avg_ticks']:.0f} ticks)")
    print(f"  Avg crowns:   P1={s['avg_p1_crowns']:.2f}  P2={s['avg_p2_crowns']:.2f}")
    print(f"  3-crown rate: {s['three_crown_rate']:.1%}")
    print()


# =========================================================================
# Matchup table
# =========================================================================

def print_matchup_table(round_robin: dict, deck_names: Optional[list[str]] = None):
    """Print a formatted matchup table from round_robin results.

    Args:
        round_robin: Output from match_runner.run_round_robin().
        deck_names: Optional labels for each deck (default: Deck 0, Deck 1, ...).
    """
    n_decks = len(round_robin["deck_win_rates"])
    if deck_names is None:
        deck_names = [f"Deck {i}" for i in range(n_decks)]

    # Build win rate matrix
    matrix = [[None] * n_decks for _ in range(n_decks)]
    for m in round_robin["matchups"]:
        i, j = m["deck1_idx"], m["deck2_idx"]
        matrix[i][j] = m["p1_win_rate"]
        matrix[j][i] = 1.0 - m["p1_win_rate"]

    # Print header
    max_name = max(len(n) for n in deck_names)
    header = " " * (max_name + 2)
    for name in deck_names:
        header += f"{name:>10s}"
    header += f"{'Overall':>10s}"
    print(f"\n{header}")
    print("-" * len(header))

    # Print rows
    for i, name in enumerate(deck_names):
        row = f"{name:<{max_name + 2}}"
        for j in range(n_decks):
            if i == j:
                row += f"{'—':>10s}"
            elif matrix[i][j] is not None:
                row += f"{matrix[i][j]:>9.1%} "
            else:
                row += f"{'':>10s}"
        row += f"{round_robin['deck_win_rates'][i]:>9.1%} "
        print(row)

    print(f"\nTotal matches: {round_robin['total_matches']}")


# =========================================================================
# Per-card analysis
# =========================================================================

def card_win_rates(results: list[dict]) -> dict:
    """Calculate win rate for each card across all matches.

    A card's win rate = (matches where the deck containing it won) / (matches it appeared in).

    Args:
        results: List of result dicts (must include "deck1" and "deck2" fields).

    Returns:
        Dict mapping card_key → {"win_rate": float, "appearances": int, "wins": int}
    """
    card_stats = defaultdict(lambda: {"wins": 0, "appearances": 0})

    for r in results:
        deck1 = r.get("deck1", [])
        deck2 = r.get("deck2", [])
        winner = r["winner"]

        for card in deck1:
            card_stats[card]["appearances"] += 1
            if winner == "player1":
                card_stats[card]["wins"] += 1

        for card in deck2:
            card_stats[card]["appearances"] += 1
            if winner == "player2":
                card_stats[card]["wins"] += 1

    result = {}
    for card, stats in card_stats.items():
        result[card] = {
            "wins": stats["wins"],
            "appearances": stats["appearances"],
            "win_rate": stats["wins"] / stats["appearances"] if stats["appearances"] > 0 else 0.0,
        }

    return result


def print_card_win_rates(results: list[dict], top_n: int = 20):
    """Print card win rates sorted by win rate."""
    rates = card_win_rates(results)
    if not rates:
        print("No card data to analyze.")
        return

    sorted_cards = sorted(
        rates.items(),
        key=lambda x: x[1]["win_rate"],
        reverse=True,
    )

    print(f"\n{'Card':25s} {'Win Rate':>10s} {'Wins':>6s} {'Games':>6s}")
    print("-" * 50)
    for card, stats in sorted_cards[:top_n]:
        print(
            f"{card:25s} {stats['win_rate']:>9.1%} "
            f"{stats['wins']:>6d} {stats['appearances']:>6d}"
        )


# =========================================================================
# Duration analysis
# =========================================================================

def duration_histogram(results: list[dict], bins: int = 10) -> list[tuple]:
    """Compute a histogram of match durations.

    Returns:
        List of (bin_start_sec, bin_end_sec, count) tuples.
    """
    durations = [r["ticks"] / 20.0 for r in results]
    if not durations:
        return []

    min_d, max_d = min(durations), max(durations)
    bin_width = (max_d - min_d) / bins if max_d > min_d else 1.0

    histogram = []
    for i in range(bins):
        lo = min_d + i * bin_width
        hi = lo + bin_width
        count = sum(1 for d in durations if lo <= d < hi or (i == bins - 1 and d == hi))
        histogram.append((round(lo, 1), round(hi, 1), count))

    return histogram


def print_duration_histogram(results: list[dict], bins: int = 10):
    """Print a text-based histogram of match durations."""
    hist = duration_histogram(results, bins)
    if not hist:
        print("No results to plot.")
        return

    max_count = max(h[2] for h in hist)
    bar_width = 40

    print(f"\nMatch Duration Distribution ({len(results)} matches)")
    print("-" * 60)
    for lo, hi, count in hist:
        bar_len = int(count / max_count * bar_width) if max_count > 0 else 0
        bar = "█" * bar_len
        print(f"  {lo:>5.0f}-{hi:>5.0f}s │{bar:<{bar_width}} {count}")


# =========================================================================
# Crown distribution
# =========================================================================

def crown_distribution(results: list[dict]) -> dict:
    """Count how often each crown count occurs."""
    p1_crowns = Counter(r["p1_crowns"] for r in results)
    p2_crowns = Counter(r["p2_crowns"] for r in results)
    return {"p1": dict(sorted(p1_crowns.items())), "p2": dict(sorted(p2_crowns.items()))}

"""
Load game data and pass to the Rust engine.

Usage:
    from data_loader import load_game_data, list_playable_cards, random_deck
    data = load_game_data("data/")
    cards = list_playable_cards(data)
    deck = random_deck(data)
"""

import random
from typing import Optional

try:
    import cr_engine
except ImportError:
    raise ImportError(
        "cr_engine not found. Build with: cd simulator && maturin develop --release"
    )


def load_game_data(data_dir: str = "data/") -> "cr_engine.GameData":
    """Load all game data from JSON files into the Rust engine.

    Args:
        data_dir: Path to the data/ directory containing royaleapi/ and wiki/ subdirs.

    Returns:
        cr_engine.GameData object (immutable, shareable across matches).
    """
    data = cr_engine.load_data(data_dir)
    print(f"Loaded: {data}")
    return data


def list_playable_cards(data: "cr_engine.GameData") -> list[dict]:
    """List all playable cards with their elixir cost and type.

    Returns:
        List of dicts: {"key": str, "elixir": int, "type": str, "has_evo": bool, "has_hero": bool}
    """
    return data.list_cards()


def card_keys(data: "cr_engine.GameData") -> list[str]:
    """Get all playable card keys (characters + buildings)."""
    return [c["key"] for c in data.list_cards()]


def character_keys(data: "cr_engine.GameData") -> list[str]:
    """Get all character card keys."""
    return data.character_keys()


def building_keys(data: "cr_engine.GameData") -> list[str]:
    """Get all building card keys."""
    return data.building_keys()


def random_deck(data: "cr_engine.GameData", size: int = 8) -> list[str]:
    """Generate a random deck of `size` unique cards.

    Draws from all playable characters and buildings.
    """
    all_keys = card_keys(data)
    if len(all_keys) < size:
        # Pad with duplicates if not enough unique cards
        return random.choices(all_keys, k=size)
    return random.sample(all_keys, size)


def validate_deck(data: "cr_engine.GameData", deck: list[str]) -> Optional[str]:
    """Validate a deck. Returns None if valid, else an error string."""
    return data.validate_deck(deck)


def get_elixir_cost(data: "cr_engine.GameData", card_key: str) -> int:
    """Get elixir cost for a card. Returns -1 if not found."""
    return data.get_elixir_cost(card_key)


def deck_avg_elixir(data: "cr_engine.GameData", deck: list[str]) -> float:
    """Calculate average elixir cost of a deck."""
    costs = [data.get_elixir_cost(k) for k in deck]
    valid = [c for c in costs if c > 0]
    return sum(valid) / len(valid) if valid else 0.0


def get_card_info(data: "cr_engine.GameData", card_key: str) -> Optional[dict]:
    """Get detailed stats for a character card."""
    try:
        return data.get_character_stats(card_key)
    except KeyError:
        return None


def print_card_table(data: "cr_engine.GameData"):
    """Print a formatted table of all playable cards."""
    cards = sorted(list_playable_cards(data), key=lambda c: (c["type"], c["elixir"], c["key"]))
    print(f"{'Key':25s} {'Type':12s} {'Elixir':>6s} {'Evo':>4s} {'Hero':>5s}")
    print("-" * 55)
    for c in cards:
        evo = "Yes" if c["has_evo"] else ""
        hero = "Yes" if c["has_hero"] else ""
        print(f"{c['key']:25s} {c['type']:12s} {c['elixir']:>6d} {evo:>4s} {hero:>5s}")
    print(f"\nTotal: {len(cards)} playable cards")

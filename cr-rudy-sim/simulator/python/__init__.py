"""
Clash Royale Simulator — Python layer.

Quick start:
    from python.data_loader import load_game_data, random_deck
    from python.ai_agent import RandomAgent, RuleBasedAgent
    from python.match_runner import run_match, run_batch
    from python.analytics import print_summary

    data = load_game_data("data/")
    deck1 = random_deck(data)
    deck2 = random_deck(data)

    results = run_batch(data, deck1, deck2, RandomAgent(), RandomAgent(), n=100)
    print_summary(results)
"""

from .data_loader import load_game_data, random_deck, card_keys, list_playable_cards
from .ai_agent import BaseAgent, Action, DoNothingAgent, RandomAgent, RuleBasedAgent
from .match_runner import run_match, run_batch, run_round_robin
from .analytics import summarize_results, print_summary, print_card_win_rates
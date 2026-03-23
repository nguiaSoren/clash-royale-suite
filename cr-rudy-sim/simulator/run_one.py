from python.data_loader import load_game_data, random_deck
from python.ai_agent import RandomAgent, RuleBasedAgent
from python.match_runner import run_match

data = load_game_data("data/")
deck1 = random_deck(data)
deck2 = random_deck(data)

print(f"Deck 1: {deck1}")
print(f"Deck 2: {deck2}")
print()

result = run_match(data, deck1, deck2, RandomAgent(), RuleBasedAgent(), verbose=True)

print(f"\n{'='*40}")
print(f"Winner: {result['winner']}")
print(f"Crowns: P1={result['p1_crowns']} P2={result['p2_crowns']}")
print(f"Duration: {result['ticks'] / 20:.1f} seconds ({result['ticks']} ticks)")
print(f"P1 King HP: {result['p1_king_hp']}")
print(f"P2 King HP: {result['p2_king_hp']}")
print(f"P1 Towers Alive: {result['p1_towers_alive']}")
print(f"P2 Towers Alive: {result['p2_towers_alive']}")
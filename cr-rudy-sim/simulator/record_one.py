import sys
sys.path.insert(0, ".")

from python.data_loader import load_game_data, random_deck
from python.ai_agent import RuleBasedAgent, RandomAgent
from python.replay_recorder import record_match, save_replay

# "data" = your JSON card stats (cards.json, characters, spells, etc.)
# load_game_data reads all those JSONs into the Rust engine
data = load_game_data("data/")

# random_deck picks 8 random card keys from your card pool
deck1 = random_deck(data)
deck2 = random_deck(data)

print(f"Deck 1: {deck1}")
print(f"Deck 2: {deck2}")

# This runs a full match with two AI agents playing against each other
# AND records every tick's game state into a replay dict

replay = record_match(data, deck1, deck2,
    RandomAgent(play_probability=0.02, min_play_gap=30),
    RandomAgent(play_probability=0.02, min_play_gap=30))

# Save the replay to a JSON file
save_replay(replay, "my_replay.json", compress=False)

print(f"Winner: {replay['result']['winner']}")
print(f"Crowns: P1={replay['result']['p1_crowns']} P2={replay['result']['p2_crowns']}")
print(f"Duration: {replay['result']['seconds']:.1f}s")
print(f"\nNow open cr_replay_viewer.jsx in Claude and drag my_replay.json into it")
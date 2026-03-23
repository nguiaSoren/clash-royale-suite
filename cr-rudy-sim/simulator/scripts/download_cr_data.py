import urllib.request
import os

URLS = [
    "https://royaleapi.github.io/cr-api-data/json/cards_stats_characters.json",
    "https://royaleapi.github.io/cr-api-data/json/cards_stats_projectile.json",
    "https://royaleapi.github.io/cr-api-data/json/cards_stats_spell.json",
    "https://royaleapi.github.io/cr-api-data/json/cards_stats_character_buff.json",
    "https://royaleapi.github.io/cr-api-data/json/cards_stats_building.json"
]

OUTPUT_DIR = "cr_data"
os.makedirs(OUTPUT_DIR, exist_ok=True)

for url in URLS:
    filename = url.split("/")[-1]
    filepath = os.path.join(OUTPUT_DIR, filename)
    print(f"Downloading {filename}...")
    urllib.request.urlretrieve(url, filepath)
    print(f"  Saved to {filepath}")

print("\nDone! All files saved to the 'cr_data' folder.")
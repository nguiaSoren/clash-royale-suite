"""
Clash Royale Wiki Scraper Pipeline
===================================
Step 1: Scrape raw text from Fandom wiki pages (evo + hero)
Step 2: Use Anthropic API to structure raw text into simulator schema

Requirements:
    pip install cloudscraper beautifulsoup4 anthropic

Usage:
    export ANTHROPIC_API_KEY="sk-ant-..."
    
    python cr_wiki_scraper.py                # Full pipeline
    python cr_wiki_scraper.py --step1-only   # Scrape only (no LLM)
    python cr_wiki_scraper.py --step2-only   # Structure only (needs raw files)

Output:
    raw_wiki_data/          → Raw scraped text per card (step 1)
    evo_hero_abilities.json → Final structured data (step 2)
"""

import json
import os
import sys
import time
import re
import argparse
from pathlib import Path

from bs4 import BeautifulSoup

# ---------------------------------------------------------------------------
# CONFIG
# ---------------------------------------------------------------------------

WIKI_BASE = "https://clashroyale.fandom.com/wiki"
RAW_DIR = Path("raw_wiki_data")
OUTPUT_FILE = Path("evo_hero_abilities.json")
REQUEST_DELAY = 3  # seconds between requests

# Known card lists — used as fallback if wiki index pages are blocked,
# and also used to validate/supplement discovery results.
KNOWN_EVOLUTIONS = [
    "Barbarians", "Royal_Giant", "Firecracker", "Skeletons",
    "Mortar", "Knight", "Royal_Recruits", "Bats",
    "Archers", "Ice_Spirit", "Valkyrie", "Wall_Breakers",
    "Bomber", "Musketeer", "Skeleton_Barrel", "Witch",
    "Wizard", "Tesla", "Zap", "Battle_Ram",
    "Goblin_Barrel", "Goblin_Giant", "Goblin_Drill", "Goblin_Cage",
    "P.E.K.K.A.", "Mega_Knight", "Electro_Dragon", "Royal_Ghost",
    "Furnace", "Cannon", "Executioner", "Skeleton_Army",
    "Lumberjack", "Baby_Dragon", "Dart_Goblin",
    "Giant_Snowball", "Royal_Hogs",
]

KNOWN_HEROES = [
    "Knight", "Giant", "Mini_P.E.K.K.A.", "Musketeer",
    "Ice_Golem", "Wizard", "Goblins", "Mega_Minion",
    "Barbarian_Barrel", "Magic_Archer",
]

# ---------------------------------------------------------------------------
# HTTP CLIENT — handles Cloudflare protection
# ---------------------------------------------------------------------------

_scraper = None

def get_scraper():
    """Lazy-init a cloudscraper session that bypasses Cloudflare."""
    global _scraper
    if _scraper is None:
        try:
            import cloudscraper
            _scraper = cloudscraper.create_scraper(
                browser={"browser": "chrome", "platform": "windows", "mobile": False}
            )
            print("[HTTP] Using cloudscraper (Cloudflare bypass)")
        except ImportError:
            print("[HTTP] cloudscraper not installed, falling back to requests")
            print("[HTTP] Install it for better results: pip install cloudscraper")
            import requests
            _scraper = requests.Session()
            _scraper.headers.update({
                "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                              "AppleWebKit/537.36 (KHTML, like Gecko) "
                              "Chrome/124.0.0.0 Safari/537.36",
                "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                "Accept-Language": "en-US,en;q=0.5",
            })
    return _scraper


def fetch_page(url: str, retries: int = 3) -> str | None:
    """Fetch a URL with retry logic. Returns HTML string or None."""
    scraper = get_scraper()
    for attempt in range(1, retries + 1):
        try:
            resp = scraper.get(url, timeout=30)
            if resp.status_code == 200:
                return resp.text
            elif resp.status_code == 403:
                print(f"  → 403 Forbidden (attempt {attempt}/{retries})")
                if attempt < retries:
                    wait = 5 * attempt
                    print(f"  → Waiting {wait}s before retry...")
                    time.sleep(wait)
            elif resp.status_code == 404:
                print(f"  → 404 Not Found — page doesn't exist")
                return None
            else:
                print(f"  → HTTP {resp.status_code} (attempt {attempt}/{retries})")
                if attempt < retries:
                    time.sleep(3)
        except Exception as e:
            print(f"  → Error: {e} (attempt {attempt}/{retries})")
            if attempt < retries:
                time.sleep(3)

    return None


# ---------------------------------------------------------------------------
# STEP 1: DISCOVER + SCRAPE
# ---------------------------------------------------------------------------

def discover_evolution_cards() -> list[dict]:
    """Try to scrape the Card Evolution index page for auto-discovery."""
    url = f"{WIKI_BASE}/Card_Evolution"
    print(f"[DISCOVER] Fetching evolution index: {url}")
    html = fetch_page(url)

    if not html:
        print("[DISCOVER] Could not fetch index page — using known list")
        return []

    soup = BeautifulSoup(html, "html.parser")
    evolutions = []

    for link in soup.find_all("a", href=True):
        href = link["href"]
        match = re.match(r"^/wiki/([^/]+)/Evolution$", href)
        if match:
            card_name = match.group(1)
            if not any(e["card_name"] == card_name for e in evolutions):
                evolutions.append({
                    "card_name": card_name,
                    "url": f"{WIKI_BASE}/{card_name}/Evolution",
                    "type": "evolution"
                })

    print(f"[DISCOVER] Found {len(evolutions)} evolution pages from index")
    return evolutions


def discover_hero_cards() -> list[dict]:
    """Try to scrape the Heroes index page for auto-discovery."""
    url = f"{WIKI_BASE}/Heroes"
    print(f"[DISCOVER] Fetching hero index: {url}")
    html = fetch_page(url)

    if not html:
        print("[DISCOVER] Could not fetch index page — using known list")
        return []

    soup = BeautifulSoup(html, "html.parser")
    heroes = []

    for link in soup.find_all("a", href=True):
        href = link["href"]
        match = re.match(r"^/wiki/([^/]+)/Hero$", href)
        if match:
            card_name = match.group(1)
            if not any(h["card_name"] == card_name for h in heroes):
                heroes.append({
                    "card_name": card_name,
                    "url": f"{WIKI_BASE}/{card_name}/Hero",
                    "type": "hero"
                })

    print(f"[DISCOVER] Found {len(heroes)} hero pages from index")
    return heroes


def build_card_list() -> list[dict]:
    """Build the full card list: try auto-discovery, fall back to known list."""
    evos = discover_evolution_cards()
    heroes = discover_hero_cards()

    # Merge with known lists (known list acts as fallback + supplement)
    evo_names = {e["card_name"] for e in evos}
    for name in KNOWN_EVOLUTIONS:
        if name not in evo_names:
            evos.append({
                "card_name": name,
                "url": f"{WIKI_BASE}/{name}/Evolution",
                "type": "evolution"
            })

    hero_names = {h["card_name"] for h in heroes}
    for name in KNOWN_HEROES:
        if name not in hero_names:
            heroes.append({
                "card_name": name,
                "url": f"{WIKI_BASE}/{name}/Hero",
                "type": "hero"
            })

    all_cards = evos + heroes
    print(f"\n[CARD LIST] Total: {len(all_cards)} ({len(evos)} evos + {len(heroes)} heroes)")
    return all_cards


def extract_article_text(html: str) -> str:
    """Parse HTML and extract the main article text."""
    soup = BeautifulSoup(html, "html.parser")

    content_div = soup.find("div", class_="mw-parser-output")
    if not content_div:
        return ""

    parts = []

    for element in content_div.find_all(["p", "table", "ul", "ol", "h2", "h3"]):
        if element.find_parent(class_=["navbox", "toc", "references"]):
            continue

        if element.name in ("h2", "h3"):
            heading_text = element.get_text(strip=True)
            # Strip "[edit]" suffix that Fandom adds
            heading_text = re.sub(r"\[edit[^\]]*\]", "", heading_text).strip()
            parts.append(f"\n## {heading_text}\n")

        elif element.name == "table":
            rows = element.find_all("tr")
            for row in rows:
                cells = row.find_all(["th", "td"])
                cell_texts = [c.get_text(strip=True) for c in cells]
                if any(cell_texts):
                    parts.append(" | ".join(cell_texts))

        elif element.name in ("ul", "ol"):
            for li in element.find_all("li", recursive=False):
                parts.append(f"  - {li.get_text(strip=True)}")

        else:
            text = element.get_text(strip=True)
            if text:
                parts.append(text)

    return "\n".join(parts)


def scrape_all_pages(cards: list[dict]) -> dict:
    """Scrape all pages. Returns summary stats."""
    RAW_DIR.mkdir(exist_ok=True)
    stats = {"success": 0, "skipped": 0, "failed": 0, "already_exists": 0}

    for i, card in enumerate(cards):
        card_name = card["card_name"]
        card_type = card["type"]
        filename = RAW_DIR / f"{card_type}_{card_name}.txt"

        if filename.exists() and filename.stat().st_size > 200:
            print(f"[SKIP] {filename.name} already exists ({filename.stat().st_size} bytes)")
            stats["already_exists"] += 1
            continue

        print(f"[SCRAPE {i+1}/{len(cards)}] {card['url']}")
        html = fetch_page(card["url"])

        if html:
            text = extract_article_text(html)
            if len(text) > 100:
                filename.write_text(text, encoding="utf-8")
                print(f"  → OK: {len(text)} chars saved")
                stats["success"] += 1
            else:
                print(f"  → WARNING: Only {len(text)} chars extracted (page may be empty)")
                filename.write_text(text, encoding="utf-8")
                stats["failed"] += 1
        else:
            print(f"  → FAILED after retries")
            filename.write_text("ERROR: Could not fetch page", encoding="utf-8")
            stats["failed"] += 1

        if i < len(cards) - 1:
            time.sleep(REQUEST_DELAY)

    return stats


# ---------------------------------------------------------------------------
# STEP 2: STRUCTURE WITH LLM
# ---------------------------------------------------------------------------

SCHEMA_TEMPLATE_EVO = """{
  "id": "<card_key>_evo",
  "base_card_key": "<lowercase with hyphens, e.g. 'knight', 'mega-knight'>",
  "base_card_sc_key": "<PascalCase, e.g. 'Knight', 'MegaKnight'>",
  "name": "<display name, e.g. 'Evolved Knight'>",
  "cycle_cost": <1 or 2>,
  "elixir": <integer>,
  "rarity": "<Common|Rare|Epic|Legendary>",
  "stat_modifiers": {
    "hitpoints_multiplier": <number or null, e.g. 1.10 for +10%>,
    "damage_multiplier": <number or null>,
    "hit_speed_multiplier": <number or null>,
    "speed_override": <integer or null>,
    "spawn_count_override": <integer or null>,
    "range_override": <integer or null>,
    "shield_hitpoints": <integer or null>
  },
  "ability": {
    "name": "<ability name>",
    "description": "<concise mechanical description>",
    "type": "<passive|on_attack|on_hit|on_kill|on_death|on_deploy|on_damage_taken|continuous|conditional>",
    "trigger": {
      "condition": "<while_not_attacking|on_each_attack|on_kill|on_deploy|when_below_half_hp|always_active|after_first_hit>",
      "cooldown_ms": <integer or null>,
      "max_stacks": <integer or null>
    },
    "effects": [
      {
        "effect_type": "<damage_reduction|damage_buff|speed_buff|hitspeed_buff|heal|spawn_unit|area_pull|area_damage|projectile_chain|projectile_bounce|shield_grant|freeze|stun|slow|rage|clone_on_death|respawn|knockback|custom>",
        "target": "<self|enemies_in_radius|all_enemies|allies_in_radius|nearest_enemy|attacking_unit|ground_and_air>",
        "radius": <integer or null>,
        "value": <number or null>,
        "duration_ms": <integer or null>,
        "damage": <integer or null>,
        "spawn_character": "<sc_key or null>",
        "spawn_count": <integer or null>,
        "spawn_interval_ms": <integer or null>,
        "buff_reference": "<character_buff name or null>",
        "projectile_behavior": <object or null>,
        "pull_strength": <integer or null>,
        "affects_air": <boolean or null>,
        "affects_ground": <boolean or null>
      }
    ]
  },
  "release_season": <integer or null>,
  "last_balance_date": "<YYYY-MM-DD or null>"
}"""

SCHEMA_TEMPLATE_HERO = """{
  "id": "<card_key>_hero",
  "base_card_key": "<lowercase with hyphens>",
  "base_card_sc_key": "<PascalCase>",
  "name": "<e.g. 'Hero Knight'>",
  "elixir": <integer>,
  "rarity": "<rarity>",
  "stat_modifiers": {
    "hitpoints_multiplier": <number or null>,
    "damage_multiplier": <number or null>,
    "hit_speed_multiplier": <number or null>,
    "speed_override": <integer or null>,
    "range_override": <integer or null>,
    "shield_hitpoints": <integer or null>
  },
  "ability": {
    "name": "<ability name>",
    "description": "<concise mechanical description>",
    "elixir_cost": <integer>,
    "activation": "manual",
    "effects": [
      {
        "effect_type": "<same enums as evo + taunt|decoy_spawn|dash_backward|multi_shot>",
        "target": "<same enums as evo>",
        "radius": <integer or null>,
        "value": <number or null>,
        "duration_ms": <integer or null>,
        "damage": <integer or null>,
        "spawn_character": "<sc_key or null>",
        "spawn_count": <integer or null>,
        "buff_reference": "<buff name or null>",
        "affects_air": <boolean or null>,
        "affects_ground": <boolean or null>,
        "taunt_radius": <integer or null>,
        "taunt_duration_ms": <integer or null>,
        "dash_distance": <integer or null>,
        "heal_amount": <integer or null>,
        "decoy_hitpoints": <integer or null>,
        "decoy_duration_ms": <integer or null>,
        "projectile_count": <integer or null>,
        "shield_hitpoints": <integer or null>
      }
    ]
  },
  "slot_type": "hero",
  "unlock_arena": <integer>,
  "fragments_to_unlock": 200,
  "release_season": <integer or null>,
  "last_balance_date": "<YYYY-MM-DD or null>"
}"""


def structure_with_llm(raw_text: str, card_name: str, card_type: str) -> dict | None:
    """Use Anthropic API to convert raw wiki text into structured JSON."""
    try:
        import anthropic
    except ImportError:
        print("ERROR: pip install anthropic")
        return None

    client = anthropic.Anthropic()

    if card_type == "evolution":
        schema = SCHEMA_TEMPLATE_EVO
        task = "evolution card"
    else:
        schema = SCHEMA_TEMPLATE_HERO
        task = "hero card"

    prompt = f"""You are a Clash Royale data parser. Below is raw text scraped from the Fandom wiki page for the {card_name.replace('_', ' ')} {task}.

Extract ALL simulation-relevant data and return ONLY valid JSON matching this schema (no markdown, no explanation, no backticks):

{schema}

RULES:
- Return ONLY the JSON object. No text before or after.
- Use null for any field you cannot determine from the text.
- For stat_modifiers, only set values explicitly mentioned (e.g. "hitpoints 10% higher" → hitpoints_multiplier: 1.10).
- For effects, Break complex abilities into multiple effect objects if they do multiple things.
- Durations should be in milliseconds (e.g. "3 seconds" → 3000).
- Radii use the same unit scale as the game (tiles × 1000, e.g. "6.5 tiles" → 6500).If only "tiles" mentioned, multiply by 1000.
- cycle_cost: how many times base card plays before evo activates (1 = every 2nd play, 2 = every 3rd play).
- For effect_type, prefer specific types over "custom". Only use "custom" if nothing else fits.
- Extract balance change dates from the History section if present
- base_card_key should be lowercase with hyphens (e.g. "mega-knight", "royal-giant").
- base_card_sc_key should be PascalCase no spaces (e.g. "MegaKnight", "RoyalGiant").

VALUE FORMAT (CRITICAL — follow exactly):
- For 'value' field in effects, use INTEGER PERCENTAGES. NEVER use decimal multipliers.
    "60% less damage"    → value: 60
    "+30% speed"         → value: 30
    "50% more damage"    → value: 50
    "doubles damage"     → value: 100
    "triples damage"     → value: 200
    "20% slower"         → value: 20
- For distance/knockback values: use game units (tiles × 1000).
    "4 tiles knockback"  → value: 4000
    "2 tile radius"      → radius: 2000
- For count-based effects (bounces, chains, spawns): use raw count.
    "bounces 3 times"    → in projectile_behavior.bounce_count: 3
    "chains to 5 targets" → in projectile_behavior.chain_count: 5
- For flat HP heal amounts: use raw HP at tournament level 11.
    "heals 45 HP per skeleton" → value: 45
- For pull_strength: use game units (tiles × 1000).

RAW WIKI TEXT:
{raw_text[:8000]}

Return ONLY the JSON object, nothing else.

"""

    try:
        response = client.messages.create(
            model="claude-sonnet-4-20250514",
            max_tokens=2000,
            messages=[{"role": "user", "content": prompt}]
        )
        text = response.content[0].text.strip()
        text = re.sub(r"^```json\s*", "", text)
        text = re.sub(r"\s*```$", "", text)
        return json.loads(text)
    except json.JSONDecodeError as e:
        print(f"  → JSON parse error: {e}")
        print(f"  → Response preview: {text[:300]}")
        return None
    except Exception as e:
        print(f"  → API error: {e}")
        return None


def structure_all(card_type: str) -> list[dict]:
    """Process all raw files of a given type through the LLM."""
    results = []
    raw_files = sorted(RAW_DIR.glob(f"{card_type}_*.txt"))

    if not raw_files:
        print(f"[WARN] No raw files for {card_type}. Run step 1 first.")
        return results

    for i, filepath in enumerate(raw_files):
        card_name = filepath.stem.replace(f"{card_type}_", "")
        raw_text = filepath.read_text(encoding="utf-8")

        if raw_text.startswith("ERROR:"):
            print(f"[SKIP] {filepath.name} — scrape failed")
            continue

        if len(raw_text) < 100:
            print(f"[SKIP] {filepath.name} — too short ({len(raw_text)} chars)")
            continue

        print(f"[LLM {i+1}/{len(raw_files)}] {card_name} ({card_type})...")
        result = structure_with_llm(raw_text, card_name, card_type)

        if result:
            results.append(result)
            ability_name = result.get("ability", {}).get("name", "?")
            print(f"  → OK: {result.get('name', '?')} — {ability_name}")
        else:
            print(f"  → FAILED")

        if i < len(raw_files) - 1:
            time.sleep(2)

    return results


# ---------------------------------------------------------------------------
# MAIN
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Clash Royale Wiki Scraper Pipeline")
    parser.add_argument("--step1-only", action="store_true", help="Only scrape raw text")
    parser.add_argument("--step2-only", action="store_true", help="Only structure with LLM")
    args = parser.parse_args()

    run_step1 = not args.step2_only
    run_step2 = not args.step1_only

    if run_step1:
        print("\n" + "=" * 60)
        print("STEP 1: Discover and scrape wiki pages")
        print("=" * 60)

        all_cards = build_card_list()
        stats = scrape_all_pages(all_cards)

        print(f"\n[STEP 1 DONE]")
        print(f"  New pages scraped: {stats['success']}")
        print(f"  Already existed:   {stats['already_exists']}")
        print(f"  Failed:            {stats['failed']}")
        print(f"  Total raw files:   {len(list(RAW_DIR.glob('*.txt')))}")

    if run_step2:
        print("\n" + "=" * 60)
        print("STEP 2: Structure raw text with LLM")
        print("=" * 60)

        if not os.environ.get("ANTHROPIC_API_KEY"):
            print("ERROR: Set ANTHROPIC_API_KEY environment variable")
            print("  export ANTHROPIC_API_KEY='sk-ant-...'")
            sys.exit(1)

        evolutions = structure_all("evolution")
        heroes = structure_all("hero")

        output = {
            "evolutions": evolutions,
            "heroes": heroes,
            "_metadata": {
                "schema_version": "2.0",
                "generated_by": "cr_wiki_scraper.py",
                "evolution_count": len(evolutions),
                "hero_count": len(heroes),
                "source": "clashroyale.fandom.com",
            }
        }

        OUTPUT_FILE.write_text(
            json.dumps(output, indent=2, ensure_ascii=False), encoding="utf-8"
        )
        print(f"\n[STEP 2 DONE] → {OUTPUT_FILE}")
        print(f"  Evolutions: {len(evolutions)}")
        print(f"  Heroes:     {len(heroes)}")


if __name__ == "__main__":
    main()
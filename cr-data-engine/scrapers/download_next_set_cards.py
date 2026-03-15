"""
RoyaleAPI Auto-Fetcher — uses curl_cffi to bypass Cloudflare

Install:
  pip install curl_cffi beautifulsoup4 pillow torch py-real-esrgan
"""

from curl_cffi import requests as cf_requests
from bs4 import BeautifulSoup
import os, urllib.request

# ── Config ───────────────────────────────────────────────────────────────────
URL         = "https://royaleapi.com/cards/popular?sort=rating"
#OUTPUT_ROOT = "/Users/soren/Desktop/Clash Royale/Hand Card"
OUTPUT_ROOT = "/Volumes/Extreme SSD/Hand Card/"
CDN_WIDTH   = 600
CDN_HEIGHT  = 720
# ─────────────────────────────────────────────────────────────────────────────


def sanitize(name: str) -> str:
    return name.replace("/", "-").replace("\\", "-").replace(":", "-")


def fetch_html(url):
    print(f"Fetching {url} ...")
    resp = cf_requests.get(url, impersonate="chrome120")
    resp.raise_for_status()
    print(f"  → Got {len(resp.text)} chars of HTML\n")
    return resp.text


def parse_cards(html):
    soup = BeautifulSoup(html, "html.parser")
    cards, version = [], None

    for div in soup.select("div.grid_item[data-card]"):
        slug    = div.get("data-card")
        img     = div.select_one("img.deck_card")
        if not img:
            continue
        name    = img.get("alt", slug)
        is_evo  = div.get("data-evo")  == "1"
        is_hero = div.get("data-hero") == "1"
        src     = img.get("src", "")
        if "/static/img/cards/" in src and version is None:
            version = src.split("/static/img/cards/")[1].split("/")[0]
            print(f"  → CDN version: {version}")
        cards.append({"slug": slug, "name": name, "evo": is_evo, "hero": is_hero})

    print(f"  → Found {len(cards)} cards\n")
    return cards, version or "v9-f09d5c9d"


def get_base_name(name, is_evo, is_hero):
    """Strip variant suffix to get the base card name."""
    if is_evo:
        return name.replace(" Evolution", "").strip()
    if is_hero:
        if name.startswith("Hero "):
            return name[5:].strip()
        return name
    return name


def build_url(slug, version):
    return (f"https://cdns3.royaleapi.com/cdn-cgi/image/"
            f"w={CDN_WIDTH},h={CDN_HEIGHT},format=png"
            f"/static/img/cards/{version}/{slug}.png")


def load_upscaler():
    try:
        import torch
        from py_real_esrgan.model import RealESRGAN
        device = torch.device(
            "mps"  if torch.backends.mps.is_available() else
            "cuda" if torch.cuda.is_available()          else
            "cpu"
        )
        print(f"  → Using device: {device}")
        model = RealESRGAN(device, scale=4)
        model.load_weights("weights/RealESRGAN_x4.pth", download=True)
        return model
    except Exception as e:
        print(f"  → Upscaler not available: {e}\n")
        return None


def upscale(model, src, dest):
    if os.path.exists(dest):
        print(f"  [skip] already upscaled: {os.path.basename(dest)}")
        return
    try:
        from PIL import Image
        image = Image.open(src).convert("RGBA")
        rgb   = image.convert("RGB")
        alpha = image.split()[3]
        sr_rgb   = model.predict(rgb)
        sr_alpha = alpha.resize(sr_rgb.size, Image.LANCZOS)
        sr_rgb.putalpha(sr_alpha)
        sr_rgb.save(dest)
        print(f"  [4x↑]  {os.path.basename(dest)}")
    except Exception as e:
        print(f"  [err]  upscale failed: {e}")


def download(url, dest):
    if os.path.exists(dest):
        print(f"  [skip] already exists: {os.path.basename(dest)}")
        return
    try:
        req = urllib.request.Request(url, headers={
            "User-Agent": "Mozilla/5.0",
            "Referer": "https://royaleapi.com/"
        })
        with urllib.request.urlopen(req) as r:
            with open(dest, "wb") as f:
                f.write(r.read())
        print(f"  [ok]   {os.path.basename(dest)}")
    except Exception as e:
        print(f"  [err]  {e}")


def main():
    os.makedirs(OUTPUT_ROOT, exist_ok=True)

    html = fetch_html(URL)
    cards, version = parse_cards(html)

    print("Loading upscaler...")
    model = load_upscaler()
    print()

    for card in cards:
        slug    = card["slug"]
        name    = card["name"]
        is_evo  = card["evo"]
        is_hero = card["hero"]

        base = sanitize(get_base_name(name, is_evo, is_hero))  # e.g. "Royal Ghost"
        sane_name = sanitize(name)                              # e.g. "Royal Ghost Evolution"

        # Parent folder: Hand {base}
        # Subfolder:     Hand Evo {base} / Hand Hero {base} / Hand {base}
        if is_evo:
            subfolder = f"Hand Evo {base}"
        elif is_hero:
            subfolder = f"Hand Hero {base}"
        else:
            subfolder = f"Hand {base}"

        parent    = os.path.join(OUTPUT_ROOT, f"Hand {base}")
        dest_dir  = os.path.join(parent, subfolder)
        dest_file = os.path.join(dest_dir, f"Hand {sane_name}_2.png")
        dest_up   = os.path.join(dest_dir, f"Hand {sane_name} 1200px_2.png")

        # Creates folder if it doesn't exist, uses existing folder if it does
        os.makedirs(dest_dir, exist_ok=True)

        print(f"▸ {name}  →  Hand {base}/{subfolder}/")
        download(build_url(slug, version), dest_file)
        if model and os.path.exists(dest_file):
            upscale(model, dest_file, dest_up)

    print("\n✅ Done!")


if __name__ == "__main__":
    main()
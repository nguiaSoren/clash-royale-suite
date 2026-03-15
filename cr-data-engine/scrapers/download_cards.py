"""
Clash Royale - Hand Card Image Downloader + Upscaler
1. Fetches the latest card data live from the official Clash Royale API
2. Saves the raw JSON to /Users/soren/Desktop/Clash Royale/Hand Card/cards.json
3. Downloads all card images (300px) into structured folders
4. Upscales each image 4x to ~1200px using Real-ESRGAN

Folder structure:
  /Users/soren/Desktop/Clash Royale/Hand Card/
    Hand_Knight/
      Hand Knight/
        Hand Knight.png           ← original 300px
        Hand Knight 1200px.png    ← upscaled
      Hand Evo Knight/
        Hand Evo Knight.png
        Hand Evo Knight 1200px.png
      Hand Hero Knight/
        Hand Hero Knight.png
        Hand Hero Knight 1200px.png

Install dependencies before running:
  pip install torch pillow py-real-esrgan

  Note: py-real-esrgan 2.0.0 requires two manual patches to work with newer huggingface_hub:
  1. In py_real_esrgan/model.py, replace "from huggingface_hub import hf_hub_url, cached_download"
     with "from huggingface_hub import hf_hub_url, hf_hub_download"
  2. Replace the cached_download() call with:
     hf_hub_download(repo_id=config['repo_id'], filename=config['filename'], local_dir=cache_dir)
  See README or ask Claude if unsure
"""

import json
import os
import urllib.request

# ── Config ──────────────────────────────────────────────────────────────────
API_TOKEN   = ""   # ← replace with your real token
API_URL     = "https://api.clashroyale.com/v1/cards"
OUTPUT_ROOT = "/Users/soren/Desktop/Clash Royale/Hand Card"

VARIANTS = [
    ("medium",          "Hand {card_name}",      "Hand {card_name}"),
    ("evolutionMedium", "Hand Evo {card_name}",  "Hand Evo {card_name}"),
    ("heroMedium",      "Hand Hero {card_name}", "Hand Hero {card_name}"),
]
# ────────────────────────────────────────────────────────────────────────────


def load_upscaler():
    """Load Real-ESRGAN model (downloads weights automatically on first run)."""
    try:
        import torch
        from PIL import Image
        from py_real_esrgan.model import RealESRGAN

        device = torch.device("mps" if torch.backends.mps.is_available()   # Apple Silicon
                         else "cuda" if torch.cuda.is_available()           # Nvidia GPU
                         else "cpu")                                        # fallback
        print(f"  → Using device: {device}")
        model = RealESRGAN(device, scale=4)
        model.load_weights("weights/RealESRGAN_x4.pth", download=True)
        return model
    except Exception as import_err:
        print(f"  → Import error: {import_err}")
        print("\n⚠️  Real-ESRGAN not installed. Run: pip install torch pillow py-real-esrgan")
        print("   Skipping upscaling — only 300px images will be saved.\n")
        return None


def upscale(model, src: str, dest: str):
    """Upscale src image and save to dest."""
    if os.path.exists(dest):
        print(f"  [skip] already upscaled: {os.path.basename(dest)}")
        return
    try:
        from PIL import Image
        image = Image.open(src).convert("RGBA")   # ← preserve transparency
        
        # Real-ESRGAN expects RGB, so split out the alpha, upscale RGB separately
        rgb = image.convert("RGB")
        alpha = image.split()[3]                  # extract alpha channel
        
        sr_rgb = model.predict(rgb)               # upscale the RGB
        
        # Upscale alpha channel to match new size
        sr_alpha = alpha.resize(sr_rgb.size, Image.LANCZOS)
        
        sr_rgb.putalpha(sr_alpha)                 # reattach alpha
        sr_rgb.save(dest)                         # PNG saves with transparency
        print(f"  [4x↑]  {os.path.basename(dest)}")
    except Exception as e:
        print(f"  [err]  upscale failed for {os.path.basename(src)} — {e}")



def fetch_cards():
    """Fetch all cards from the Clash Royale API, save JSON, return data."""
    print("Fetching latest card data from API...")
    req = urllib.request.Request(API_URL, headers={"Authorization": f"Bearer {API_TOKEN}"})
    with urllib.request.urlopen(req) as resp:
        data = json.loads(resp.read().decode("utf-8"))

    print(f"  → Got {len(data.get('items', []))} cards.")

    os.makedirs(OUTPUT_ROOT, exist_ok=True)
    json_path = os.path.join(OUTPUT_ROOT, "cards.json")
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    print(f"  → JSON saved to: {json_path}\n")

    return data


def sanitize(name: str) -> str:
    return name.replace("/", "-").replace("\\", "-").replace(":", "-")


def download(url: str, dest: str):
    if os.path.exists(dest):
        print(f"  [skip] already exists: {os.path.basename(dest)}")
        return
    try:
        urllib.request.urlretrieve(url, dest)
        print(f"  [ok]   {os.path.basename(dest)}")
    except Exception as e:
        print(f"  [err]  {os.path.basename(dest)} — {e}")


def main():
    # ── Step 1: Fetch card data ──
    data = fetch_cards()
    cards = data.get("items", [])

    # ── Step 2: Load upscaler ──
    print("Loading Real-ESRGAN upscaler...")
    model = load_upscaler()
    print()

    # ── Step 3: Download + upscale ──
    for card in cards:
        card_name = card["name"]
        hand_name = f"Hand {card_name}"
        icon_urls = card.get("iconUrls", {})

        folder_name = f"Hand {sanitize(card_name)}"
        card_root   = os.path.join(OUTPUT_ROOT, folder_name)

        print(f"▸ {hand_name}")

        for url_key, subfolder_tpl, filename_tpl in VARIANTS:
            url = icon_urls.get(url_key)
            if not url:
                continue

            subfolder_name = subfolder_tpl.format(card_name=card_name)
            filename       = filename_tpl.format(card_name=card_name) + ".png"
            filename_up    = filename_tpl.format(card_name=card_name) + " 1200px.png"

            dest_dir    = os.path.join(card_root, sanitize(subfolder_name))
            dest_file   = os.path.join(dest_dir, sanitize(filename))
            dest_up     = os.path.join(dest_dir, sanitize(filename_up))

            os.makedirs(dest_dir, exist_ok=True)

            # Download original 300px
            download(url, dest_file)

            # Upscale to ~1200px
            if model and os.path.exists(dest_file):
                upscale(model, dest_file, dest_up)

    print("\n✅ Done!")


if __name__ == "__main__":
    main()

"""
Clash Royale - Background Augmentation Script
Composites each base card onto a programmatically generated
Clash Royale hand slot background.

Run with:
  /Users/soren/environments/openai/bin/python3 augment_background.py
"""

import os
import numpy as np
from PIL import Image, ImageDraw

#OUTPUT_ROOT = "/Users/soren/Desktop/Clash Royale/Hand Card"
OUTPUT_ROOT = "/Volumes/Extreme SSD/Hand Card"

BG_COLORS = [
    ((26, 79, 140), (20, 60, 110)),    # default in-game blue
    ((15, 50, 100), (10, 35, 75)),     # very dark/night mode
    ((50, 120, 180), (35, 95, 150)),   # washed out/bright screen
    ((26, 79, 100), (20, 60, 80)),     # greenish tint (older devices)
    ((40, 70, 160), (28, 52, 130)),    # purple/cool tint
    ((80, 80, 80), (50, 50, 50)),      # grayscale (low quality screenshot)
]

SLOT_COLORS = [
    (15, 50, 95),      # default
    (8, 30, 60),       # very dark
    (30, 80, 130),     # washed out
    (15, 50, 75),      # greenish
    (25, 45, 120),     # purple
    (45, 45, 45),      # grayscale
]

def make_background(card_w: int, card_h: int, bg_top, bg_bot, slot_color) -> Image.Image:
    """Generate a single card slot background."""
    # Add padding around the card
    pad_x = int(card_w * 0.15)
    pad_y = int(card_h * 0.15)
    total_w = card_w + pad_x * 2
    total_h = card_h + pad_y * 2

    # Gradient background
    bg = Image.new("RGBA", (total_w, total_h), (0, 0, 0, 255))
    arr = np.array(bg, dtype=np.float32)
    for y in range(total_h):
        t = y / total_h
        r = bg_top[0] + (bg_bot[0] - bg_top[0]) * t
        g = bg_top[1] + (bg_bot[1] - bg_top[1]) * t
        b = bg_top[2] + (bg_bot[2] - bg_top[2]) * t
        arr[y, :, 0] = r
        arr[y, :, 1] = g
        arr[y, :, 2] = b
    bg = Image.fromarray(arr.astype(np.uint8))

    # Draw darker rounded rectangle slot
    draw = ImageDraw.Draw(bg)
    slot_rect = [
        pad_x - int(pad_x * 0.3),
        pad_y - int(pad_y * 0.3),
        pad_x + card_w + int(pad_x * 0.3),
        pad_y + card_h + int(pad_y * 0.3),
    ]
    radius = int(card_w * 0.08)
    draw.rounded_rectangle(slot_rect, radius=radius, fill=(*slot_color, 255))

    return bg, pad_x, pad_y


def composite(card: Image.Image, bg: Image.Image, pad_x: int, pad_y: int) -> Image.Image:
    """Paste card onto background using alpha channel."""
    result = bg.copy()
    result.paste(card, (pad_x, pad_y), card)
    return result



def save(img: Image.Image, path: str):
    # Save augmented images as JPEG to save disk space
    jpeg_path = os.path.splitext(path)[0] + ".jpg"
    if os.path.exists(jpeg_path):
        print(f"    [skip] {os.path.basename(jpeg_path)}")
        return
    # Convert RGBA to RGB before saving as JPEG (JPEG doesn't support transparency)
    if img.mode == "RGBA":
        # Paste onto white background for JPEG
        bg = Image.new("RGB", img.size, (255, 255, 255))
        bg.paste(img, mask=img.split()[3])
        img = bg
    img.save(jpeg_path, "JPEG", quality=90)
    print(f"    [ok]   {os.path.basename(jpeg_path)}")


def main():
    card_folders = sorted([
        f for f in os.listdir(OUTPUT_ROOT)
        if os.path.isdir(os.path.join(OUTPUT_ROOT, f)) and f.startswith("Hand ")
    ])

    print(f"Found {len(card_folders)} card folders.\n")

    for card_folder in card_folders:
        card_root = os.path.join(OUTPUT_ROOT, card_folder)
        subfolders = sorted([
            f for f in os.listdir(card_root)
            if os.path.isdir(os.path.join(card_root, f))
        ])

        print(f"▸ {card_folder}")

        for subfolder in subfolders:
            subfolder_path = os.path.join(card_root, subfolder)

            pngs = sorted([
                f for f in os.listdir(subfolder_path)
                if (f.endswith(".png") or f.endswith(".jpg"))
                and not f.startswith("._")
                and not any(f.startswith(p) for p in [
                    "Background", "Brightness", "Blur",
                    "Noise", "Loading", "Crop"
                ])
            ])

            print(f"  ↳ {subfolder} ({len(pngs)} source images)")

            for png_file in pngs:
                src_path = os.path.join(subfolder_path, png_file)
                try:
                    card = Image.open(src_path).convert("RGBA")
                except Exception as e:
                    print(f"  [err]  skipping {png_file} — {e}")
                    continue

                w, h = card.size

                for i, (bg_colors, slot_color) in enumerate(zip(BG_COLORS, SLOT_COLORS)):
                    bg_top, bg_bot = bg_colors
                    bg, pad_x, pad_y = make_background(w, h, bg_top, bg_bot, slot_color)
                    result = composite(card, bg, pad_x, pad_y)

                    out_name = f"Background v{i+1:02d} {png_file}"
                    out_path = os.path.join(subfolder_path, out_name)
                    save(result, out_path)

    print("\n✅ Done!")


if __name__ == "__main__":
    main()

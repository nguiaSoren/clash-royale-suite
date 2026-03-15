"""
Clash Royale - Linear Crop Augmentation Script
Generates 152 crop states per card (19 steps × 8 directions).
Directions: right, left, top, bottom, top-right, top-left, bottom-right, bottom-left
Steps: 5% to 95% in 5% increments — pixels become transparent.

Note on high crop percentages (80-95%):
At very high crop percentages, some cards may only show their shared border frame
rather than unique card art. This is card-dependent and direction-dependent —
there is no single cutoff that works perfectly for all cards and all directions.
This is intentional and acceptable for ML training because:
  - The useful crops (5%-70%) vastly outnumber the ambiguous ones
  - The model learns that border-only crops are ambiguous, which is correct behavior
  - The dataset is large enough that these edge cases get drowned out
Manually tuning crop limits per card per direction would be too much work
for marginal ML benefit.
"""

import os
import numpy as np
from PIL import Image

#OUTPUT_ROOT = "/Users/soren/Desktop/Clash Royale/Hand Card"
OUTPUT_ROOT = "/Volumes/Extreme SSD/Hand Card"
STEPS = [round(i * 0.05, 2) for i in range(1, 20)]  # 0.05 to 0.95

DIRECTIONS = [
    "right",
    "left",
    "top",
    "bottom",
    "top-right",
    "top-left",
    "bottom-right",
    "bottom-left",
]


def apply_crop(img: Image.Image, direction: str, pct: float) -> Image.Image:
    """Make a portion of the image transparent based on direction and percentage."""
    img = img.convert("RGBA")
    arr = np.array(img, dtype=np.uint8)
    h, w = arr.shape[:2]

    cut_w = int(w * pct)  # how many pixels to cut horizontally
    cut_h = int(h * pct)  # how many pixels to cut vertically

    if direction == "right":
        arr[:, w - cut_w:, 3] = 0

    elif direction == "left":
        arr[:, :cut_w, 3] = 0

    elif direction == "top":
        arr[:cut_h, :, 3] = 0

    elif direction == "bottom":
        arr[h - cut_h:, :, 3] = 0

    elif direction == "top-right":
        # Triangle from top-right corner
        for y in range(h):
            ratio = 1 - (y / h)
            cut = int(w * pct * ratio * 2)
            cut = min(cut, w)
            if cut > 0:
                arr[y, w - cut:, 3] = 0

    elif direction == "top-left":
        # Triangle from top-left corner
        for y in range(h):
            ratio = 1 - (y / h)
            cut = int(w * pct * ratio * 2)
            cut = min(cut, w)
            if cut > 0:
                arr[y, :cut, 3] = 0

    elif direction == "bottom-right":
        # Triangle from bottom-right corner
        for y in range(h):
            ratio = y / h
            cut = int(w * pct * ratio * 2)
            cut = min(cut, w)
            if cut > 0:
                arr[y, w - cut:, 3] = 0

    elif direction == "bottom-left":
        # Triangle from bottom-left corner
        for y in range(h):
            ratio = y / h
            cut = int(w * pct * ratio * 2)
            cut = min(cut, w)
            if cut > 0:
                arr[y, :cut, 3] = 0

    return Image.fromarray(arr)


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
                    "Crop", "Brightness", "Blur", "Noise", "Background", "Loading"
                ])
            ])

            print(f"  ↳ {subfolder} ({len(pngs)} source images)")

            for png_file in pngs:
                src_path = os.path.join(subfolder_path, png_file)
                try:
                    img = Image.open(src_path).convert("RGBA")
                except Exception as e:
                    print(f"  [err]  skipping {png_file} — {e}")
                    continue
                
                for direction in DIRECTIONS:
                    for pct in STEPS:
                        pct_label = int(pct * 100)
                        dir_label = direction.replace("-", "_")
                        out_name  = f"Crop {dir_label} {pct_label:02d}pct {png_file}"
                        out_path  = os.path.join(subfolder_path, out_name)
                        augmented = apply_crop(img, direction, pct)
                        save(augmented, out_path)

    print("\n✅ Done!")


if __name__ == "__main__":
    main()

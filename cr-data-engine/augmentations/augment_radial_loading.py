"""
Clash Royale - Loading State Augmentation Script
Generates 36 radial loading states (0° to 350° in steps of 10°).

Run with:
  /Users/soren/environments/openai/bin/python3 augment_loading.py

"""

import os
import math
import numpy as np
from PIL import Image, ImageDraw, ImageOps, ImageEnhance

#OUTPUT_ROOT = "/Users/soren/Desktop/Clash Royale/Hand Card"
OUTPUT_ROOT = "/Volumes/Extreme SSD/Hand Card"
STEPS = range(0, 360, 10)


def apply_radial_loading(img: Image.Image, angle_deg: float) -> Image.Image:
    img = img.convert("RGBA")
    w, h = img.size
    cx, cy = w // 2, h // 2

    base = img.copy()

    # --- Grayscale, preserving alpha ---
    r, g, b, alpha = img.split()          # ← renamed to alpha
    gray_rgb = ImageOps.grayscale(img).convert("RGB")
    gray = Image.merge("RGBA", (*gray_rgb.split()[:3], alpha))
    gray = ImageEnhance.Contrast(gray).enhance(1.2)

    overlay = Image.new("RGBA", (w, h), (255, 255, 255, 0))
    overlay_draw = ImageDraw.Draw(overlay)
    overlay_draw.rectangle([0, 0, w, h], fill=(255, 255, 255, 140))

    if angle_deg >= 350:
        overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    elif angle_deg > 0:
        start_rad = math.radians(-90)
        radius = max(w, h) * 3

        points = [(cx, cy)]
        n_steps = max(36, int(angle_deg))
        for i in range(n_steps + 1):
            a = start_rad + math.radians(angle_deg * i / n_steps)  # ← local a, fine now
            px = cx + radius * math.cos(a)
            py = cy + radius * math.sin(a)
            points.append((px, py))

        wedge_mask = Image.new("L", (w, h), 0)
        wedge_draw = ImageDraw.Draw(wedge_mask)
        wedge_draw.polygon(points, fill=255)

        overlay_arr = np.array(overlay)
        wedge_arr = np.array(wedge_mask)
        overlay_arr[:, :, 3] = np.where(wedge_arr > 128, 0, overlay_arr[:, :, 3])
        overlay = Image.fromarray(overlay_arr)

    processed = Image.alpha_composite(gray, overlay)

    result = base.copy()
    result.paste(processed, (0, 0), alpha)   # ← uses alpha, not a
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
                    "Loading", "Brightness", "Blur", "Noise", "Crop"
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

                for deg in STEPS:
                    prefix = f"Loading {deg:03d}"
                    out_name = f"{prefix} {png_file}"
                    out_path = os.path.join(subfolder_path, out_name)
                    augmented = apply_radial_loading(img, deg)
                    save(augmented, out_path)

    print("\n✅ Done!")


if __name__ == "__main__":
    main()
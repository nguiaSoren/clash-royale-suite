"""
Clash Royale - Brightness, Blur, and Noise Augmentation Script
Generates augmented versions of each card image.

Brightness: 7 levels (50% to 150% in 16.67% steps)
Blur:       6 levels (radius 1 to 6)
Noise:      6 levels (sigma 5 to 30 in steps of 5)

Run with:
  /Users/soren/environments/openai/bin/python3 augment_bbn.py
"""

import os
import numpy as np
from PIL import Image, ImageEnhance, ImageFilter

#OUTPUT_ROOT = "/Users/soren/Desktop/Clash Royale/Hand Card"
OUTPUT_ROOT = "/Volumes/Extreme SSD/Hand Card"

BRIGHTNESS_LEVELS = [round(0.5 + i * (1.0 / 6), 2) for i in range(7)]  # 0.5 to 1.5
BLUR_LEVELS       = list(range(1, 7))                                    # 1 to 6
NOISE_LEVELS      = list(range(5, 35, 5))                                # 5 to 30


def apply_brightness(img: Image.Image, factor: float) -> Image.Image:
    """Brighten or darken the RGB channels, preserve alpha."""
    img = img.convert("RGBA")
    r, g, b, alpha = img.split()
    rgb = Image.merge("RGB", (r, g, b))
    rgb = ImageEnhance.Brightness(rgb).enhance(factor)
    r2, g2, b2 = rgb.split()
    return Image.merge("RGBA", (r2, g2, b2, alpha))


def apply_blur(img: Image.Image, radius: int) -> Image.Image:
    """Gaussian blur on RGB channels, preserve alpha."""
    img = img.convert("RGBA")
    r, g, b, alpha = img.split()
    rgb = Image.merge("RGB", (r, g, b))
    rgb = rgb.filter(ImageFilter.GaussianBlur(radius=radius))
    r2, g2, b2 = rgb.split()
    return Image.merge("RGBA", (r2, g2, b2, alpha))


def apply_noise(img: Image.Image, sigma: int) -> Image.Image:
    """Add gaussian noise to RGB channels, preserve alpha."""
    img = img.convert("RGBA")
    arr = np.array(img, dtype=np.float32)

    noise = np.random.normal(0, sigma, arr[:, :, :3].shape)
    arr[:, :, :3] = np.clip(arr[:, :, :3] + noise, 0, 255)

    return Image.fromarray(arr.astype(np.uint8))


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
                    "Brightness", "Blur", "Noise", "Background", "Loading", "Crop"
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

                for factor in BRIGHTNESS_LEVELS:
                    label    = int(round(factor * 100))
                    out_name = f"Brightness {label:03d}pct {png_file}"
                    out_path = os.path.join(subfolder_path, out_name)
                    save(apply_brightness(img, factor), out_path)

                for radius in BLUR_LEVELS:
                    out_name = f"Blur r{radius:02d} {png_file}"
                    out_path = os.path.join(subfolder_path, out_name)
                    save(apply_blur(img, radius), out_path)

                for sigma in NOISE_LEVELS:
                    out_name = f"Noise s{sigma:02d} {png_file}"
                    out_path = os.path.join(subfolder_path, out_name)
                    save(apply_noise(img, sigma), out_path)

    print("\n✅ Done!")


if __name__ == "__main__":
    main()


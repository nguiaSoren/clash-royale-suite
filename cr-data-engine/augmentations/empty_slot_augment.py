"""
Extract empty slot from a single frame and augment it.
Crops slot 4 region from a 1080x2316 image.
"""

import os
import numpy as np
from PIL import Image, ImageEnhance, ImageFilter


# ── Config ────────────────────────────────────────────────────────────────────
# I just took an empty slot from an image and thats it
SOURCE_IMAGE = "/Users/soren/Desktop/frame_0436676.jpg"
OUTPUT_DIR   = "/Volumes/Extreme SSD/Hand Card/Hand Empty/Hand Empty"

# Slot 4 coordinates from 1080x2316 image
SLOT_COORDS  = (841, 1935, 1060, 2207)  # (left, top, right, bottom)

BRIGHTNESS_LEVELS = [round(0.5 + i * (1.0 / 6), 2) for i in range(7)]
BLUR_LEVELS       = list(range(1, 7))
NOISE_LEVELS      = list(range(5, 35, 5))
ROTATION_ANGLES = [-10, -5, 0, 5, 10]  # 5 angles

# Color tints to simulate different arenas/devices
# (R multiplier, G multiplier, B multiplier)
'''COLOR_TINTS = [
    (1.0,  1.0,  1.0),   # original
    (0.9,  0.9,  1.1),   # slightly blue
    (1.0,  0.95, 1.05),  # slight purple
    (0.95, 1.0,  0.95),  # slight green
    (1.1,  1.0,  0.9),   # slight warm
    (0.85, 0.85, 0.85),  # desaturated
    (1.2,  0.85, 0.6),   # wooden (warm brown)
    (0.7,  0.9,  1.3),   # ice (cold blue-white)
    (1.4,  0.6,  0.4),   # fire (deep orange-red)
]'''
COLOR_TINTS = [
    (0,    0,    0),    # original
    (0,    0,    30),   # more blue
    (10,   0,    20),   # slight purple
    (0,    20,   0),    # slight green
    (30,   10,   0),    # warm
    (-10, -10,  -10),   # darker/desaturated
    (40,   20,  -20),   # wooden (warm brown)
    (-20,  10,   50),   # ice (cold blue-white)
    (80,  -20,  -30),   # fire (orange-red)
]
# ─────────────────────────────────────────────────────────────────────────────


'''def apply_tint(img: Image.Image, r_mul, g_mul, b_mul) -> Image.Image:
    img = img.convert("RGB")
    arr = np.array(img, dtype=np.float32)
    arr[:, :, 0] = np.clip(arr[:, :, 0] * r_mul, 0, 255)
    arr[:, :, 1] = np.clip(arr[:, :, 1] * g_mul, 0, 255)
    arr[:, :, 2] = np.clip(arr[:, :, 2] * b_mul, 0, 255)
    return Image.fromarray(arr.astype(np.uint8))'''

def apply_tint(img: Image.Image, r_add, g_add, b_add) -> Image.Image:
    img = img.convert("RGB")
    arr = np.array(img, dtype=np.float32)
    arr[:, :, 0] = np.clip(arr[:, :, 0] + r_add, 0, 255)
    arr[:, :, 1] = np.clip(arr[:, :, 1] + g_add, 0, 255)
    arr[:, :, 2] = np.clip(arr[:, :, 2] + b_add, 0, 255)
    return Image.fromarray(arr.astype(np.uint8))


def apply_brightness(img, factor):
    return ImageEnhance.Brightness(img).enhance(factor)


def apply_blur(img, radius):
    return img.filter(ImageFilter.GaussianBlur(radius=radius))


def apply_noise(img, sigma):
    arr = np.array(img, dtype=np.float32)
    noise = np.random.normal(0, sigma, arr.shape)
    arr = np.clip(arr + noise, 0, 255)
    return Image.fromarray(arr.astype(np.uint8))


def save(img: Image.Image, path: str):
    if os.path.exists(path):
        print(f"  [skip] {os.path.basename(path)}")
        return
    img.convert("RGB").save(path, "JPEG", quality=90)
    print(f"  [ok]   {os.path.basename(path)}")


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    # ── Crop slot 4 ──────────────────────────────────────────────────────────
    print("Cropping slot 4...")
    frame = Image.open(SOURCE_IMAGE).convert("RGB")
    slot  = frame.crop(SLOT_COORDS)
    slot.save(os.path.join(OUTPUT_DIR, "Empty.jpg"), "JPEG", quality=95)
    print(f"  → Cropped: {slot.size}")

    # ── Augment ──────────────────────────────────────────────────────────────
    print("\nAugmenting...")

    for i, (r, g, b) in enumerate(COLOR_TINTS):
        tinted = apply_tint(slot, r, g, b)
        for angle in ROTATION_ANGLES:
            rotated = tinted.rotate(angle, expand=False)
            base_name = f"Empty tint{i+1:02d} rot{angle:+03d}"

            # Brightness
            for factor in BRIGHTNESS_LEVELS:
                label = int(round(factor * 100))
                save(apply_brightness(rotated, factor),
                        os.path.join(OUTPUT_DIR, f"Brightness {label:03d}pct {base_name}.jpg"))

            # Blur
            for radius in BLUR_LEVELS:
                save(apply_blur(rotated, radius),
                        os.path.join(OUTPUT_DIR, f"Blur r{radius:02d} {base_name}.jpg"))

            # Noise
            for sigma in NOISE_LEVELS:
                save(apply_noise(rotated, sigma),
                        os.path.join(OUTPUT_DIR, f"Noise s{sigma:02d} {base_name}.jpg"))

    print(f"\n✅ Done! Generated {len(os.listdir(OUTPUT_DIR))} images.")


if __name__ == "__main__":
    main()
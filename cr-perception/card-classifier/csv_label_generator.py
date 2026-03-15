"""
Clash Royale - CSV Label Generator
Generates a CSV file with relative image paths and class labels
for training MobileNetV3-Small / EfficientNet-B0.

Output format:
  image_path, label
  Hand Wizard/Hand Wizard/Hand Wizard.jpg, Hand Wizard
  Hand Evo Wizard/Hand Evo Wizard/Hand Evo Wizard.jpg, Evo Wizard

Run with:
  /Users/soren/environments/openai/bin/python3 generate_labels.py
"""

import os
import csv

ROOT        = "/Volumes/Extreme SSD/Hand Card"
OUTPUT_CSV  = "/Volumes/Extreme SSD/Hand Card/labels.csv"

# Extensions to include
VALID_EXT = {".jpg", ".jpeg", ".png"}

# Skip these prefixes — augmentation scripts, weights, etc.
SKIP_FOLDERS = {"weights"}


'''def get_label(subfolder_name: str) -> str:
    """Strip 'Hand ' prefix to get clean class label."""
    if subfolder_name.startswith("Hand "):
        return subfolder_name[5:]  # e.g. "Hand Evo Wizard" → "Evo Wizard"
    return subfolder_name          # e.g. "Empty" → "Empty"'''
def get_label(subfolder_name: str) -> str:
    return subfolder_name  # keep as-is: "Hand Wizard", "Hand Evo Wizard", "Empty"


def main():
    rows = []

    card_folders = sorted([
        f for f in os.listdir(ROOT)
        if os.path.isdir(os.path.join(ROOT, f))
        and f.startswith("Hand ")
        and f not in SKIP_FOLDERS
    ])

    print(f"Found {len(card_folders)} card folders.\n")

    for card_folder in card_folders:
        card_root = os.path.join(ROOT, card_folder)

        subfolders = sorted([
            f for f in os.listdir(card_root)
            if os.path.isdir(os.path.join(card_root, f))
        ])

        for subfolder in subfolders:
            subfolder_path = os.path.join(card_root, subfolder)
            label = get_label(subfolder)

            images = sorted([
                f for f in os.listdir(subfolder_path)
                if os.path.splitext(f)[1].lower() in VALID_EXT
                and not f.startswith("._")
            ])

            for img_file in images:
                # Relative path — works on any machine/cloud
                rel_path = os.path.join(card_folder, subfolder, img_file)
                rows.append((rel_path, label))

    # Write CSV
    with open(OUTPUT_CSV, "w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(["image_path", "label"])
        writer.writerows(rows)

    print(f"✅ Done! {len(rows)} images written to:")
    print(f"   {OUTPUT_CSV}")

    # Print class summary
    from collections import Counter
    labels = [r[1] for r in rows]
    counts = Counter(labels)
    print(f"\n{len(counts)} unique classes:")
    for label, count in sorted(counts.items()):
        print(f"  {label}: {count} images")


if __name__ == "__main__":
    main()

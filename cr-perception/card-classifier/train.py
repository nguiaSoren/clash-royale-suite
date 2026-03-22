"""
Clash Royale - Hand Card Classifier Training Script
Model: MobileNetV3-Small (pretrained on ImageNet)
Dataset: 294,976 images, 175 classes

Works on:
  - Apple Silicon M1/M2/M3 (MPS)
  - NVIDIA GPU (CUDA)
  - CPU (fallback)

Run with:
  python train.py

Install:
  pip install torch torchvision pandas scikit-learn tqdm
"""

import os
import time
import pandas as pd
import torch
import torch.nn as nn
from torch.utils.data import Dataset, DataLoader
from torchvision import transforms, models
from PIL import Image
from tqdm import tqdm
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import LabelEncoder
import json

# ── Config ───────────────────────────────────────────────────────────────────
'''ROOT_DIR    = "/Volumes/Extreme SSD/Hand Card"   # change to RunPod path when needed
CSV_PATH    = os.path.join(ROOT_DIR, "labels.csv")
SAVE_DIR    = os.path.join(ROOT_DIR, "checkpoints")

BATCH_SIZE  = 64       # reduce to 32 if OOM
LR          = 3e-4    # lower learning rate
NUM_EPOCHS  = 2       ## set to 30 for full RunPod training
NUM_WORKERS = 4        # reduce to 0 if DataLoader errors
IMG_SIZE    = 224      # MobileNetV3 input size
VAL_SPLIT   = 0.15     # 15% validation
SEED        = 42'''
# ─────────────────────────────────────────────────────────────────────────────

# ── RTX 5090 & 32 vCPU OPTIMIZED CONFIG ──────────────────────────────────────
ROOT_DIR    = "/workspace/data"   
CSV_PATH    = os.path.join(ROOT_DIR, "labels.csv")
SAVE_DIR    = "/workspace/checkpoints" 

BATCH_SIZE  = 1024      # Slightly smaller to save RAM
NUM_WORKERS = 8         # Lowering this prevents the "Killed" / OOM error
LR          = 5e-4      # Cut in half to stop accuracy from "bouncing"
NUM_EPOCHS  = 15        # 15 epochs will take ~65 mins and is likely enough
VAL_SPLIT   = 0.10
SEED        = 42
IMG_SIZE    = 224       # Required for MobileNetV3
# ─────────────────────────────────────────────────────────────────────────────

def get_device():
    if torch.cuda.is_available():
        device = torch.device("cuda")
        print(f"  → Using CUDA: {torch.cuda.get_device_name(0)}")
    elif torch.backends.mps.is_available():
        device = torch.device("mps")
        print(f"  → Using MPS (Apple Silicon)")
    else:
        device = torch.device("cpu")
        print(f"  → Using CPU")
    return device


class CardDataset(Dataset):
    def __init__(self, df, root_dir, label_encoder, transform=None):
        self.df            = df.reset_index(drop=True)
        self.root_dir      = root_dir
        self.label_encoder = label_encoder
        self.transform     = transform

    def __len__(self):
        return len(self.df)

    def __getitem__(self, idx):
        row       = self.df.iloc[idx]
        img_path  = os.path.join(self.root_dir, row["image_path"])
        label     = self.label_encoder.transform([row["label"]])[0]

        try:
            img = Image.open(img_path).convert("RGB")
        except Exception:
            # Return black image if file is corrupted
            img = Image.new("RGB", (IMG_SIZE, IMG_SIZE), (0, 0, 0))

        if self.transform:
            img = self.transform(img)

        return img, label


def get_transforms():
    train_tf = transforms.Compose([
        transforms.Resize((IMG_SIZE, IMG_SIZE)),
        transforms.RandomHorizontalFlip(),
        transforms.RandomRotation(5),
        transforms.ColorJitter(brightness=0.2, contrast=0.2),
        transforms.ToTensor(),
        transforms.Normalize([0.485, 0.456, 0.406],
                             [0.229, 0.224, 0.225]),
    ])
    val_tf = transforms.Compose([
        transforms.Resize((IMG_SIZE, IMG_SIZE)),
        transforms.ToTensor(),
        transforms.Normalize([0.485, 0.456, 0.406],
                             [0.229, 0.224, 0.225]),
    ])
    return train_tf, val_tf


def build_model(num_classes: int) -> nn.Module:
    model = models.mobilenet_v3_small(weights="IMAGENET1K_V1")
    # Replace classifier head for our number of classes
    in_features = model.classifier[3].in_features
    model.classifier[3] = nn.Linear(in_features, num_classes)
    return model


def train_epoch(model, loader, criterion, optimizer, device, epoch):
    model.train()
    total_loss, correct, total = 0, 0, 0

    pbar = tqdm(loader, desc=f"Epoch {epoch} [Train]", leave=False)
    for imgs, labels in pbar:
        imgs, labels = imgs.to(device), labels.to(device)

        optimizer.zero_grad()
        
        outputs = model(imgs)
        loss    = criterion(outputs, labels)
        loss.backward()
        optimizer.step()

        total_loss += loss.item() * imgs.size(0)
        correct    += (outputs.argmax(1) == labels).sum().item()
        total      += imgs.size(0)

        pbar.set_postfix(loss=f"{loss.item():.4f}",
                         acc=f"{correct/total:.4f}")

    return total_loss / total, correct / total


def val_epoch(model, loader, criterion, device, epoch):
    model.eval()
    total_loss, correct, total = 0, 0, 0

    with torch.no_grad():
        pbar = tqdm(loader, desc=f"Epoch {epoch} [Val]  ", leave=False)
        for imgs, labels in pbar:
            imgs, labels = imgs.to(device), labels.to(device)
            outputs = model(imgs)
            loss    = criterion(outputs, labels)

            total_loss += loss.item() * imgs.size(0)
            correct    += (outputs.argmax(1) == labels).sum().item()
            total      += imgs.size(0)

    return total_loss / total, correct / total


def main():
    os.makedirs(SAVE_DIR, exist_ok=True)
    device = get_device()

    # ── Load CSV ─────────────────────────────────────────────────────────────
    print("\nLoading CSV...")
    df = pd.read_csv(CSV_PATH)
    print(f"  → {len(df)} images, {df['label'].nunique()} classes")

    # ── Encode labels ─────────────────────────────────────────────────────────
    le = LabelEncoder()
    le.fit(df["label"])
    num_classes = len(le.classes_)
    print(f"  → {num_classes} unique classes")

    # Save label encoder classes for inference later
    classes_path = os.path.join(SAVE_DIR, "classes.json")
    with open(classes_path, "w") as f:
        json.dump(le.classes_.tolist(), f, indent=2)
    print(f"  → Classes saved to {classes_path}")

    # ── Train/Val split ───────────────────────────────────────────────────────
    train_df, val_df = train_test_split(
        df, test_size=VAL_SPLIT, random_state=SEED, stratify=df["label"]
    )
    print(f"  → Train: {len(train_df)} | Val: {len(val_df)}")

    # ── Datasets & Loaders ────────────────────────────────────────────────────
    train_tf, val_tf = get_transforms()

    train_ds = CardDataset(train_df, ROOT_DIR, le, train_tf)
    val_ds   = CardDataset(val_df,   ROOT_DIR, le, val_tf)

    train_loader = DataLoader(train_ds, batch_size=BATCH_SIZE,
                              shuffle=True,  num_workers=NUM_WORKERS,
                              pin_memory=True)
    val_loader   = DataLoader(val_ds,   batch_size=BATCH_SIZE,
                              shuffle=False, num_workers=NUM_WORKERS,
                              pin_memory=True)

    # ── Model ─────────────────────────────────────────────────────────────────
    print("\nBuilding model...")
    model     = build_model(num_classes).to(device)
    criterion = nn.CrossEntropyLoss()
    optimizer = torch.optim.Adam(model.parameters(), lr=LR, weight_decay=1e-4)
    scheduler = torch.optim.lr_scheduler.StepLR(optimizer, step_size=10, gamma=0.1)

    print(f"  → MobileNetV3-Small | {num_classes} classes | LR={LR}")

    # ── Training loop ─────────────────────────────────────────────────────────
    print(f"\nTraining for {NUM_EPOCHS} epochs...\n")
    best_val_acc = 0.0
    history      = []

    for epoch in range(1, NUM_EPOCHS + 1):
        t0 = time.time()

        train_loss, train_acc = train_epoch(model, train_loader, criterion,
                                            optimizer, device, epoch)
        val_loss,   val_acc   = val_epoch(model, val_loader, criterion,
                                          device, epoch)
        scheduler.step()

        elapsed = time.time() - t0
        history.append({
            "epoch":      epoch,
            "train_loss": round(train_loss, 4),
            "train_acc":  round(train_acc,  4),
            "val_loss":   round(val_loss,   4),
            "val_acc":    round(val_acc,    4),
        })

        print(f"Epoch {epoch:02d}/{NUM_EPOCHS} | "
              f"Train loss: {train_loss:.4f} acc: {train_acc:.4f} | "
              f"Val loss: {val_loss:.4f} acc: {val_acc:.4f} | "
              f"Time: {elapsed:.1f}s")

        # Save best model
        if val_acc > best_val_acc:
            best_val_acc = val_acc
            torch.save({
                "epoch":      epoch,
                "model":      model.state_dict(),
                "optimizer":  optimizer.state_dict(),
                "val_acc":    val_acc,
                "num_classes": num_classes,
            }, os.path.join(SAVE_DIR, "best_model.pth"))
            print(f"  ✅ Saved best model (val_acc={val_acc:.4f})")

        # Save latest checkpoint every epoch
        torch.save({
            "epoch":      epoch,
            "model":      model.state_dict(),
            "optimizer":  optimizer.state_dict(),
            "val_acc":    val_acc,
            "num_classes": num_classes,
        }, os.path.join(SAVE_DIR, "latest_checkpoint.pth"))

    # ── Save history ──────────────────────────────────────────────────────────
    history_path = os.path.join(SAVE_DIR, "history.json")
    with open(history_path, "w") as f:
        json.dump(history, f, indent=2)

    print(f"\n✅ Training complete!")
    print(f"   Best val accuracy: {best_val_acc:.4f}")
    print(f"   Checkpoints saved to: {SAVE_DIR}")
    print(f"   History saved to: {history_path}")


if __name__ == "__main__":
    main()
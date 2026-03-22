"""
Export your PyTorch MobileNetV3 checkpoint to ONNX so Rust can load it.

Usage:
    python export_to_onnx.py

This reads from checkpoints/best_model.pth and writes checkpoints/best_model.onnx
as a single self-contained file (no external .data file).
"""

import torch
import torch.nn as nn
from torchvision import models
import json
import os

MODEL_PATH = "checkpoints/best_model.pth"
CLASS_PATH = "checkpoints/classes.json"
ONNX_PATH = "checkpoints/best_model.onnx"
ONNX_DATA_PATH = "checkpoints/best_model.onnx.data"

# Load class count
with open(CLASS_PATH, "r") as f:
    class_names = json.load(f)

# Rebuild architecture
model = models.mobilenet_v3_small(weights=None)
model.classifier[3] = nn.Linear(model.classifier[3].in_features, len(class_names))

# Load weights
checkpoint = torch.load(MODEL_PATH, map_location="cpu")
state_dict = checkpoint["model"] if "model" in checkpoint else checkpoint
model.load_state_dict(state_dict)
model.eval()

# Dummy input — batch=1, 3 channels, 224×224
dummy = torch.randn(1, 3, 224, 224)

# Remove old external data file if it exists
if os.path.exists(ONNX_DATA_PATH):
    os.remove(ONNX_DATA_PATH)
    print(f"🗑️  Removed old {ONNX_DATA_PATH}")

# Export — MobileNetV3-Small is tiny (~6MB), no need for external data
torch.onnx.export(
    model,
    dummy,
    ONNX_PATH,
    input_names=["input"],
    output_names=["output"],
    dynamic_axes={
        "input": {0: "batch"},
        "output": {0: "batch"},
    },
    opset_version=17,
)

# Verify it's a single file (no external data was created)
if os.path.exists(ONNX_DATA_PATH):
    # If torch still created external data, merge it back in
    import onnx
    from onnx.external_data_helper import convert_model_to_external_data
    
    model_proto = onnx.load(ONNX_PATH, load_external_data=True)
    # Save with all data embedded
    onnx.save_model(model_proto, ONNX_PATH, 
                    save_as_external_data=False)
    os.remove(ONNX_DATA_PATH)
    print("📦 Merged external data back into single .onnx file")

size_mb = os.path.getsize(ONNX_PATH) / (1024 * 1024)
print(f"✅ Exported to {ONNX_PATH} ({size_mb:.1f} MB, single file)")
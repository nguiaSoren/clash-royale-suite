import torch
import torch.nn as nn
from torchvision import transforms, models
from PIL import Image
import json
import os

# --- SETTINGS ---
MODEL_FILE = "checkpoints/best_model.pth"
CLASS_FILE = "checkpoints/classes.json"
IMAGE_PATH = "frame_0398266.jpg"  # <-- Put your screenshot path here

# --- COORDINATES (From your Rust Logic) ---

GENERIC_LAYOUT = [
    {"x": 40/960,  "y": 1271/1404, "w": 83/960,  "h": 108/1404}, # Next
    {"x": 207/960, "y": 1078/1404, "w": 181/960, "h": 241/1404}, # Slot 1
    {"x": 393/960, "y": 1080/1404, "w": 183/960, "h": 239/1404}, # Slot 2
    {"x": 577/960, "y": 1081/1404, "w": 180/960, "h": 237/1404}, # Slot 3
    {"x": 764/960, "y": 1080/1404, "w": 186/960, "h": 239/1404}, # Slot 4
]

IPHONE_LAYOUT = [ # 0.46 Ratio
    {"x": 0.05, "y": 0.92, "w": 0.08, "h": 0.06},
    {"x": 0.22, "y": 0.82, "w": 0.18, "h": 0.13},
    {"x": 0.40, "y": 0.82, "w": 0.18, "h": 0.13},
    {"x": 0.58, "y": 0.82, "w": 0.18, "h": 0.13},
    {"x": 0.78, "y": 0.82, "w": 0.18, "h": 0.13},
]

IPAD_LAYOUT = [ # 0.75 Ratio
    {"x": 0.07, "y": 0.92, "w": 0.09, "h": 0.06},
    {"x": 0.28, "y": 0.82, "w": 0.13, "h": 0.13},
    {"x": 0.42, "y": 0.82, "w": 0.13, "h": 0.13},
    {"x": 0.56, "y": 0.82, "w": 0.13, "h": 0.13},
    {"x": 0.70, "y": 0.82, "w": 0.13, "h": 0.13},
]

# Mapping ratios to Names and Layouts
DEVICE_CONFIGS = {
    0.46: {"name": "iPhone / Tall Phone", "layout": IPHONE_LAYOUT},
    0.75: {"name": "iPad / Tablet",        "layout": IPAD_LAYOUT},
    0.68: {"name": "Generic Android / HD", "layout": GENERIC_LAYOUT}
}

def get_prediction(image_path):
    # Load model and classes
    with open(CLASS_FILE, "r") as f:
        class_names = json.load(f)
    
    model = models.mobilenet_v3_small(weights=None)
    model.classifier[3] = nn.Linear(model.classifier[3].in_features, len(class_names))
    model.load_state_dict(torch.load(MODEL_FILE, map_location='cpu')['model'])
    model.eval()

    # Determine layout
    img = Image.open(image_path).convert("RGB")
    w, h = img.size
    ratio = w / h
    
    # Find the key (ratio) that is closest to our image ratio
    best_match_ratio = min(DEVICE_CONFIGS.keys(), key=lambda x: abs(x - ratio))
    selected_config = DEVICE_CONFIGS[best_match_ratio]
    
    print(f"\n✅ DETECTED DEVICE: **{selected_config['name']}**")
    print(f"   (Matched {best_match_ratio} against actual {ratio:.2f})\n")

    preprocess = transforms.Compose([
        transforms.Resize((224, 224)),
        transforms.ToTensor(),
        transforms.Normalize([0.485, 0.456, 0.406], [0.229, 0.224, 0.225])
    ])

    print("--- CARD PREDICTIONS ---")
    for i, box in enumerate(selected_config['layout']):
        # Crop
        crop = img.crop((box['x']*w, box['y']*h, (box['x']+box['w'])*w, (box['y']+box['h'])*h))
        input_tensor = preprocess(crop).unsqueeze(0)
        
        with torch.no_grad():
            output = model(input_tensor)
            probs = torch.nn.functional.softmax(output[0], dim=0)
            conf, idx = torch.max(probs, 0)
            
        label = "NEXT CARD" if i == 0 else f"SLOT {i}"
        print(f"{label:10} | {class_names[idx]:20} | Confidence: {conf:.1%}")

if __name__ == "__main__":
    if os.path.exists(IMAGE_PATH):
        get_prediction(IMAGE_PATH)
    else:
        print(f"Error: Could not find {IMAGE_PATH}")
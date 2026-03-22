# Clash Royale Real-Time Card Classification Pipeline

A high-performance video inference pipeline for classifying Clash Royale hand cards in real-time gameplay footage. The system was originally prototyped in Python (PyTorch + OpenCV) and rewritten from scratch in **Rust** with ONNX Runtime to maximize throughput through systems-level optimizations, then extended with a **stateful gatekeeper** that eliminates redundant inference on unchanged card slots.

## Overview

The pipeline processes recorded iPhone gameplay, detects 5 card slots in the player's hand using fixed-region ROIs, classifies each card using a fine-tuned MobileNetV3-Small model, and renders annotated overlays onto the output video. A per-slot state machine ("gatekeeper") monitors pixel-level changes and skips inference on slots whose cards haven't visually changed.

**Model Performance:**
- Architecture: MobileNetV3-Small (fine-tuned classifier head)
- Best Validation Accuracy: **97.89%** (epoch 11)
- Training Accuracy: 98.0% at convergence
- Trained for 15 epochs with learning rate scheduling (LR step-down at epoch 11)

**Inference Pipeline Performance (Apple M-series, CoreML/ANE):**

| Metric | Without Gatekeeper | With Gatekeeper |
|--------|-------------------|-----------------|
| Frames | 8,702 | 8,702 |
| Wall time | 6m31s | 12m27s |
| ms/frame | 44.9ms | 85.8ms |
| FPS | 22.3 | 11.6 |
| Inferences run | 43,510 | 22,206 |
| Inferences skipped | 0 | 21,304 (49%) |

> **Why is the gatekeeper slower for offline video?** CoreML's execution provider optimizes for fixed tensor shapes. Without the gatekeeper, every frame sends a consistent `[5, 3, 224, 224]` batch that CoreML compiles once and runs on the ANE. With the gatekeeper, batch sizes vary dynamically (`[1,...]`, `[2,...]`, `[3,...]`, etc.), which forces CoreML to recompile execution plans or fall back to CPU for unfamiliar shapes. The overhead of dynamic dispatch more than cancels out the 49% reduction in inference calls.
>
> **Where the gatekeeper wins:** In a live real-time stream — where you're GPU-constrained, paying per API call, or sharing compute with other workloads — skipping 49% of inference calls is a meaningful reduction. The design targets real-time deployment, not batch video processing.

## Architecture

```
┌─────────────┐     ┌────────────────────────────────────────────────────────┐
│  OpenCV      │     │                  GATEKEEPER                            │
│  VideoCapture│     │                                                        │
│  (decode)    ├────►│  For each card slot:                                   │
│              │     │                                                        │
│              │     │  ┌─────────┐  pixels changed?  ┌──────────┐           │
│              │     │  │ LOCKED  │ ─── yes ──────────►│ UNLOCKED │           │
│              │     │  │ (skip)  │                    │ (infer)  │           │
│              │     │  └────▲────┘                    └────┬─────┘           │
│              │     │       │                              │                 │
│              │     │       └──── conf ≥ 90% ◄────────────┘                 │
│              │     │              + snapshot fingerprint                    │
│              │     └──────────────────────────┬─────────────────────────────┘
│              │                                │
│              │     ┌──────────────────────────▼─────────────────────────────┐
│              │     │  Rayon Thread Pool (parallel, unlocked slots only)      │
│              │     │  crop₁ ─► resize ─► normalize ─┐                       │
│              │     │  crop₃ ─► resize ─► normalize ─┼──► ONNX Runtime       │
│              │     │  crop₅ ─► resize ─► normalize ─┘    batched forward    │
│              │     └────────────────────────────────────────────────────────┘
│              │                                │
│              │     ┌──────────────────────────▼──────┐
│              │     │  OpenCV VideoWriter              │
│              │     │  (draw overlays + encode)        │
└──────────────┘     └─────────────────────────────────┘
```

## Systems Optimizations (Python → Rust)

### 1. Batched Inference
The Python version ran **5 separate forward passes** per frame (one per card slot). The Rust version stacks preprocessed crops into a single NCHW tensor and executes **one batched forward pass**, eliminating per-call dispatch overhead, redundant kernel launches, and repeated memory allocation.

### 2. Parallel Preprocessing with Rayon
The crop→resize→normalize operations are embarrassingly parallel — each reads from the shared source frame (read-only) and writes to its own contiguous buffer. Rayon's work-stealing thread pool distributes these across available cores with zero contention. The results are flattened into the batch tensor with a single allocation.

### 3. Hardware-Accelerated Execution (CoreML / Apple Neural Engine)
The ONNX Runtime session is configured with the CoreML Execution Provider, which maps supported operations to Apple's Neural Engine (ANE) on M-series chips. The ANE is a dedicated inference accelerator that runs independently of the CPU and GPU, enabling ~9ms per-card classification without competing for compute resources.

### 4. Gatekeeper State Machine (Inference Pre-Filter)
A per-slot state machine that sits between the video source and the inference engine, eliminating redundant compute:

**Fingerprinting:** Each card slot is reduced to a compact pixel "fingerprint" — an 8×8 grid of sampled RGB values (192 floats). Comparing two fingerprints is O(192) and takes sub-microsecond, vs the ~9ms cost of a full inference call.

**State transitions:**
- **UNLOCKED → LOCKED:** When inference returns ≥90% softmax confidence, the slot is locked and its fingerprint is saved. Inference stops for that slot.
- **LOCKED → UNLOCKED:** On every frame, the locked slot's current fingerprint is compared to the saved one. If the mean absolute difference exceeds 6% (e.g., the card slot goes grey during a loading animation), the slot unlocks and inference resumes.

**Result:** 49% of inference calls eliminated (21,304 out of 43,510 slot evaluations) over an 8,702-frame match. Only unlocked slots are batched and sent to the model.

### 5. Zero-Overhead Abstractions
- **Precomputed pixel rectangles**: The floating-point ROI ratios are converted to integer pixel coordinates once at startup via `std::array::from_fn`, avoiding repeated floating-point arithmetic on every frame.
- **Deterministic memory layout**: The batch tensor is assembled from contiguous `Vec<f32>` buffers in CHW layout, matching ONNX Runtime's expected memory format with no transposition or copying.
- **Direct BGR→RGB conversion**: OpenCV's `cvt_color` operates in-place on the Mat before a single copy to the `image` crate's `RgbImage`, minimizing intermediate allocations.

### 6. Model Format Conversion
PyTorch models cannot be loaded in Rust. The model is exported to ONNX format (Open Neural Network Exchange), an open standard that enables runtime-agnostic inference. The export preserves dynamic batch axes, allowing the same model file to handle both single and batched inputs.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `opencv` | Video decode/encode, frame drawing |
| `ort` | ONNX Runtime bindings (with CoreML EP) |
| `ndarray` | Tensor construction |
| `rayon` | Data-parallel preprocessing |
| `image` | Crop and resize operations |
| `indicatif` | Terminal progress bar |
| `serde_json` | Class label loading |
| `anyhow` | Error handling |

## Build & Run

### Prerequisites
- Rust 1.75+ (`rustup.rs`)
- OpenCV 4.x (`brew install opencv` on macOS)
- Python + PyTorch (for one-time ONNX export)

### Setup

```bash
# Set pkg-config path for OpenCV (macOS)
export PKG_CONFIG_PATH="$(brew --prefix opencv)/lib/pkgconfig:$PKG_CONFIG_PATH"

# Export model from PyTorch to ONNX (one-time)
python export_to_onnx.py

# Build both binaries
cargo build --release

# Run batch mode (max throughput, fixed [5,3,224,224], no gatekeeper)
cargo run --release --bin batch

# Run gatekeeper mode (stateful inference skipping, 49% fewer inference calls)
cargo run --release --bin gatekeeper
```

### Configuration

Edit the constants at the top of each binary (`src/batch.rs` or `src/gatekeeper.rs`):

| Constant | Description |
|----------|-------------|
| `VIDEO_PATH` | Input gameplay video |
| `OUTPUT_PATH` | Annotated output video |
| `MODEL_PATH` | Path to `.onnx` model |
| `CLASS_PATH` | Path to `classes.json` |
| `IPHONE_LAYOUT` | Normalized ROI coordinates per card slot |
| `LOCK_CONFIDENCE` | Softmax threshold to lock a slot (default: 0.90) |
| `PIXEL_CHANGE_THRESHOLD` | Fingerprint Δ to unlock a slot (default: 0.06) |

### Visual Feedback

In the output video, card overlays indicate gatekeeper state:
- **White border** = UNLOCKED (inference running)
- **Green border** = LOCKED (inference skipped, reusing cached label)

## Training History

| Epoch | Train Loss | Train Acc | Val Loss | Val Acc |
|-------|-----------|-----------|----------|---------|
| 1 | 0.509 | 90.85% | 0.177 | 96.32% |
| 5 | 0.100 | 97.69% | 0.484 | 94.22% |
| 11 | 0.089 | 97.94% | 0.092 | **97.89%** |
| 15 | 0.086 | 98.00% | 0.091 | 97.89% |

The model uses a learning rate step-down at epoch 11, which stabilizes validation loss and closes the train-val gap.

## Project Structure

```
cr-perception/
└── rust-inference-pipeline/
    ├── Cargo.toml              # Two binary targets: batch + gatekeeper
    ├── export_to_onnx.py       # PyTorch → ONNX export
    ├── README.md
    ├── src/
    │   ├── batch.rs            # bin: max throughput (fixed [5,3,224,224], 22 FPS)
    │   └── gatekeeper.rs       # bin: stateful inference skipping (49% skip rate)
    └── checkpoints/
        ├── best_model.onnx     # Exported model
        └── classes.json        # Class labels
```

## License

This project is for academic and personal use.
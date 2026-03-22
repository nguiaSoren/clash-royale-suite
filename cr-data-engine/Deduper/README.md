# 🦀 Rust CV Pipeline Optimizer (Gatekeeper)
A high-performance, system-level data processing engine and real-time state machine designed to eliminate temporal redundancy in Computer Vision (CV) pipelines. Built entirely in Rust, this project drastically reduces compute overhead and memory footprints by acting as a "Gatekeeper" before expensive downstream AI/ML models are invoked.


## 📖 Architecture Overview
Modern CV models (PyTorch, TensorFlow) are incredibly compute-heavy. When running on live video streams or massive uncurated datasets, processing identical, redundant frames (e.g., static screens, idle states) wastes GPU cycles, spikes API costs, and increases system latency.

This repository solves this at the systems level using two distinct paradigms:

1. **`batch_processor/`**: A multi-threaded, high-throughput engine for sanitizing massive historical datasets.


## 🚀 1. The Batch Engine (Dataset Sanitization)
The batch processor is designed to maximize SSD read/write bandwidth and CPU utilization to crunch massive datasets in record time.

### Performance Metrics (Real-World Benchmark)
Executed on a highly nested dataset of raw gameplay footage spanning 268 directories:
* **Total Frames Analyzed:** 440,056
* **Processing Time:** 33 minutes (1,990 seconds)
* **Throughput:** ~221 FPS (including heavy disk I/O)
* **Dataset Reduction:** **61.1%** (Dropped 269,214 temporal duplicates)
* **Verified Unique Frames Saved:** 170,842

### Core Systems Engineering Principles
* **Zero-Copy Memory Management:** Standard Python pipelines allocate new RAM for every image crop. This engine utilizes `image::crop_imm`, creating lightweight, immutable mathematical windows into the original memory buffer, preventing RAM thrashing and garbage collection spikes.
* **Work-Stealing Multi-Threading:** Bypasses interpreted language bottlenecks (like the Python GIL) using Rust's `rayon`. It dynamically partitions the 440,000-file workload across all physical/logical OS threads, stealing work from idle queues to maintain 100% CPU saturation.
* **OS-Level I/O Optimization:** Instead of loading and re-encoding unique JPEGs, the engine issues direct `std::fs::copy` system calls, duplicating the raw binary blobs at the maximum bandwidth of the storage hardware.



## 🛠️ Installation & Usage
### Prerequisites
* Rust & Cargo (1.70+)

### Running the Batch Processor
```bash
git clone https://github.com/yourusername/rust-cv-pipeline-optimizer.git
cd rust-cv-pipeline-optimizer/batch_processor

# IMPORTANT: Always compile in release mode for maximum systems optimization
cargo run --release
```
*(Ensure you map your input and output directories in `src/main.rs` before execution).*

## 🔮 Future Roadmap (Systems & Linux Integration)
To further push the boundaries of real-time systems programming, future iterations will focus on:
* **Linux `v4l2` Integration:** Directly hooking into the Video4Linux API to intercept webcam/capture card buffers before they hit user space.
* **SIMD Instructions:** Utilizing explicit AVX2/NEON SIMD instructions for the pixel-averaging math to push the theoretical limit of the CPU.
* **Zero-Copy IPC:** Passing the unique frames from the Rust Gatekeeper to a Python PyTorch process using shared memory (`mmap`) or Unix Domain Sockets rather than disk I/O.

---
*Developed with a focus on strong computer systems fundamentals, robust I/O handling, and real-time execution.*




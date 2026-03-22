# Clash Royale Autonomous Intelligence & High-Performance Systems Suite

A research-oriented framework combining high-performance systems programming and deep learning to build a real-time autonomous decision agent.

## 🚀 Key Technical Pillars

* **High-Performance Systems (Rust):** Engineered a multi-threaded data engine in Rust for bit-level temporal deduplication and redundancy filtering. Processed a **430,000-frame dataset** with a 10x throughput increase over Python-based baseline implementations.
* **Neural Perception Pipeline:** Multi-stage vision system featuring a **MobileNetV3-Small** classifier optimized for edge inference. Achieved **97.4% validation accuracy** across 175 card variants and game states.
* **Deterministic Simulation (In-Dev):** Architecting a high-throughput game logic core in **Rust**, leveraging memory safety and concurrency to target **10,000+ simulated steps per second** for Reinforcement Learning (RL).
* **Generative Data Engineering:** Utilized **Super-Resolution (Real-ESRGAN)** to enhance low-fidelity API assets to $1200\text{px}$, followed by a 4-stage augmentation pipeline (radial loading, 8-directional occlusion, domain-invariant compositing) to synthesize a **295k+ image training set**.

<!-- ═══ INFERENCE PIPELINE SECTION (paste into your top-level README.md) ═══ -->

### [Real-Time Inference Pipeline →](cr-perception/rust-inference-pipeline/inference_pipeline_overview.html)

![Pipeline Architecture](cr-perception/rust-inference-pipeline/pipeline_architecture.svg)

Two Rust binaries, same model — different strategies for when to run inference. **Batch mode** classifies all 5 card slots every frame. **Gatekeeper mode** tracks pixel-level changes per slot and skips inference on cards that haven't changed.

| | Batch Mode | Gatekeeper Mode |
|---|---|---|
| **Throughput** | 22.3 FPS | 11.6 FPS |
| **Inferences/frame** | 5 (fixed) | 0–5 (variable) |
| **Inferences skipped** | 0 | 21,304 (49%) |
| **Overlay color** | White (all slots inferred) | Green = locked (cached), White = unlocked |

#### Batch mode — all overlays white, every slot inferred every frame

<table>
<tr>
<td width="33%"><video src="https://github.com/user-attachments/assets/99d556de-8513-4536-8132-c6b4dec98c4f" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/c0de3247-554d-4ede-805c-eccfa86f4518" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/316dfd0c-1f8a-4ce9-a64f-16bfa29d9b1d" controls width="100%"></video></td>
</tr>
<tr>
<td align="center"><sub>Batch Track 1</sub></td>
<td align="center"><sub>Batch Track 2</sub></td>
<td align="center"><sub>Batch Track 3</sub></td>
</tr>
</table>

#### Gatekeeper mode — green borders = inference skipped, white = inference running

<table>
<tr>
<td width="33%"><video src="https://github.com/user-attachments/assets/f1d86efd-b1e3-471a-984a-b04f781d3dfa" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/6bbe885e-cf29-4a23-bf2a-af867c9d1965" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/11fba19c-9516-49bd-bc4f-168e594e9363" controls width="100%"></video></td>
</tr>
<tr>
<td align="center"><sub>Gatekeeper Track 1</sub></td>
<td align="center"><sub>Gatekeeper Track 2</sub></td>
<td align="center"><sub>Gatekeeper Track 3</sub></td>
</tr>
</table>

> **Why is gatekeeper slower for offline video?** CoreML compiles optimized execution plans for fixed tensor shapes. Batch mode sends a consistent `[5, 3, 224, 224]` every frame — compiled once, runs on the ANE. Gatekeeper varies the batch size dynamically, forcing recompilation. The gatekeeper targets **live real-time streams** where skipping 49% of inference calls saves compute, not batch processing.

📄 **[Full writeup →](cr-perception/rust-inference-pipeline/inference_pipeline_overview.html)** — architecture details, model training history, systems optimizations (Python → Rust), and build instructions.


## 📂 Repository Structure

* `cr-perception/`: Hand-card classification and arena unit detection (YOLOv11 integration pending).
* `cr-data-engine/`: The "Data-Centric AI" core, containing the super-resolution scripts and the 4-stage augmentation suite.
* `cr-rudy-sim/`: The Rust-based deterministic simulator and logic core.
* `data/`: (Symlinked/Local) 309GB dataset storage (excluded from VCS via `.gitignore`).

## 📊 Performance Benchmarks

| Metric | Achievement |
| :--- | :--- |
| **Inference Latency** | <10ms (Optimized for Linux/Edge) |
| **Model Footprint** | ~2.1MB (MobileNetV3-Small) |
| **Classification Acc** | 97.4% (175 Classes) |
| **Data Processing** | 10x Speedup via Rust Multi-threading |

## 🛠 Project Roadmap
- [x] High-accuracy hand-card state extraction.
- [ ] Real-time troop/spell localization using YOLOv11.
- [ ] Integration of Monte Carlo Tree Search (MCTS) with the Rust logic core.
- [ ] Inverse Reinforcement Learning for opponent modeling and strategy optimization.

---
*Developed with a focus on real-time systems, Linux optimization, and fundamental computer system principles.*

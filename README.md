# Clash Royale Autonomous Intelligence & High-Performance Systems Suite

A research-oriented framework combining high-performance systems programming and deep learning to build a real-time autonomous decision agent.

## 🚀 Key Technical Pillars

* **High-Performance Systems (Rust):** Engineered a multi-threaded data engine in Rust for bit-level temporal deduplication and redundancy filtering. Processed a **430,000-frame dataset** with a 10x throughput increase over Python-based baseline implementations.
* High-Performance System Programming (Rust): Engineered a high-speed data processing engine in Rust to perform bit-level deduplication and temporal redundancy filtering on a 440,000-frame dataset, achieving a 10x performance increase over Python-based alternatives and a 61% dataset reduction through zero-copy memory management and multi-threaded I/O.<img width="468" height="81" alt="image" src="https://github.com/user-attachments/assets/5461302e-fc39-4a17-a0f0-5235ec81a7db" />

* **Neural Perception Pipeline:** Multi-stage vision system featuring a **MobileNetV3-Small** classifier optimized for edge inference. Achieved **97.4% validation accuracy** across 175 card variants and game states.
* **Deterministic Simulation (In-Dev):** Architecting a high-throughput game logic core in **Rust**, leveraging memory safety and concurrency to target **10,000+ simulated steps per second** for Reinforcement Learning (RL).
* **Generative Data Engineering:** Utilized **Super-Resolution (Real-ESRGAN)** to enhance low-fidelity API assets to $1200\text{px}$, followed by a 4-stage augmentation pipeline (radial loading, 8-directional occlusion, domain-invariant compositing) to synthesize a **295k+ image training set**.

<!-- ═══ INFERENCE PIPELINE SECTION (paste into your top-level README.md) ═══ -->

### [Real-Time Inference Pipeline →](https://nguiasoren.github.io/clash-royale-suite/cr-perception/card-classifier/inference_pipeline_overview.html)


![Pipeline Architecture](cr-perception/card-classifier/pipeline_architecture.svg)

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
<td width="33%"><video src="https://github.com/user-attachments/assets/e869e69b-d055-49ce-9bf2-c193fcf846eb" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/fa5cef9b-18c6-49e6-918d-63eb2224b1c6" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/402b80de-c4b6-4f6e-82b4-d607ad4a5c89" controls width="100%"></video></td>
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
<td width="33%"><video src="https://github.com/user-attachments/assets/a3843b9f-512e-4940-8874-01b35d492f02" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/9b073c9b-8dc6-4d94-9411-87d7b6c65136" controls width="100%"></video></td>
<td width="33%"><video src="https://github.com/user-attachments/assets/e100ea60-4cf4-43b4-9e81-7deadf61da4c" controls width="100%"></video></td>
</tr>
<tr>
<td align="center"><sub>Gatekeeper Track 1</sub></td>
<td align="center"><sub>Gatekeeper Track 2</sub></td>
<td align="center"><sub>Gatekeeper Track 3</sub></td>
</tr>
</table>

> **Why is gatekeeper slower for offline video?** CoreML compiles optimized execution plans for fixed tensor shapes. Batch mode sends a consistent `[5, 3, 224, 224]` every frame — compiled once, runs on the ANE. Gatekeeper varies the batch size dynamically, forcing recompilation. The gatekeeper targets **live real-time streams** where skipping 49% of inference calls saves compute, not batch processing.

📄 **[Full writeup →](https://nguiasoren.github.io/clash-royale-suite/cr-perception/card-classifier/inference_pipeline_overview.html)** — architecture details, model training history, systems optimizations (Python → Rust), and build instructions.


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

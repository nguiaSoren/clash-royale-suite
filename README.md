# Clash Royale Autonomous Intelligence & High-Performance Systems Suite

A research-oriented framework combining high-performance systems programming and deep learning to build a real-time autonomous decision agent.

## 🚀 Key Technical Pillars

* **High-Performance Systems (Rust):** Engineered a multi-threaded data engine in Rust for bit-level temporal deduplication and redundancy filtering. Processed a **430,000-frame dataset** with a 10x throughput increase over Python-based baseline implementations.
* **Neural Perception Pipeline:** Multi-stage vision system featuring a **MobileNetV3-Small** classifier optimized for edge inference. Achieved **97.4% validation accuracy** across 175 card variants and game states.
* **Deterministic Simulation (In-Dev):** Architecting a high-throughput game logic core in **Rust**, leveraging memory safety and concurrency to target **10,000+ simulated steps per second** for Reinforcement Learning (RL).
* **Generative Data Engineering:** Utilized **Super-Resolution (Real-ESRGAN)** to enhance low-fidelity API assets to $1200\text{px}$, followed by a 4-stage augmentation pipeline (radial loading, 8-directional occlusion, domain-invariant compositing) to synthesize a **295k+ image training set**.

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
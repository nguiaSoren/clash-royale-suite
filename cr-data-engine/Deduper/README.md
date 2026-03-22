# Batch Processor — Temporal Deduplication Engine

📄 **[View the full writeup →](https://nguiasoren.github.io/clash-royale-suite/cr-data-engine/data_engine_writeup.html)**

High-throughput Rust engine for eliminating temporal redundancy from large-scale CV datasets.

| Metric | Value |
|--------|-------|
| Frames processed | 440,056 |
| Duplicates dropped | 269,214 (61.1%) |
| Throughput | ~221 FPS |
| Wall time | 33 minutes |

Built with Rayon (parallel processing), zero-copy cropping (`crop_imm`), and OS-level file copy (`std::fs::copy`).

```bash
cargo build --release
cargo run --release
```

Configure input/output paths in `src/main.rs`.

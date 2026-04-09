// telemetry.rs
//
// Drop-in telemetry logger for cr-extract.
// Both batch.rs and gatekeeper.rs import this to emit per-frame JSON
// that the web dashboard consumes.
//
// ── LOCATION ──
//   clash_tracker_rust/src/telemetry.rs
//
// ── WIRING ──
//   In lib.rs:       pub mod telemetry;
//   In batch.rs:     use clash_tracker_rust::telemetry::*;
//   In gatekeeper.rs: use clash_tracker_rust::telemetry::*;
//
// ── OUTPUT ──
//   Running either binary now also writes a .telemetry.json alongside the .mp4.
//   Example: cargo run --release --bin batch
//            → tracked_clash_gameplay.mp4
//            → tracked_clash_gameplay.telemetry.json

use serde::Serialize;
use std::fs;
use std::time::Instant;

/// Per-slot telemetry for a single frame.
#[derive(Debug, Clone, Serialize)]
pub struct SlotTelemetry {
    /// Slot index (0 = next card, 1–4 = hand slots).
    pub slot: usize,
    /// Was ONNX inference actually run on this slot?
    pub inferred: bool,
    /// Detected card name (from classes.json, "Hand " prefix stripped).
    pub card: String,
    /// Softmax confidence (0.0–1.0).
    pub confidence: f32,
    /// Time spent on this slot's work in milliseconds.
    /// - Batch mode: inference_ms / NUM_CARDS (amortized)
    /// - Gatekeeper locked: sub-microsecond (fingerprint check only)
    /// - Gatekeeper unlocked: per-slot preprocess + inference share
    pub latency_ms: f64,
    /// (Gatekeeper only) Was this slot locked (green border)?
    pub locked: bool,
    /// (Gatekeeper only) Fingerprint distance from last lock snapshot.
    /// 0.0 if batch mode or if slot was unlocked and had no prior fingerprint.
    pub fingerprint_delta: f32,
}

/// Per-frame telemetry record.
#[derive(Debug, Clone, Serialize)]
pub struct FrameTelemetry {
    /// Frame number (0-indexed).
    pub frame: u64,
    /// Timestamp in milliseconds from video start.
    pub timestamp_ms: f64,
    /// Which mode produced this record: "batch" or "gatekeeper".
    pub mode: String,
    /// Total wall-clock time for this frame (decode + preprocess + inference + draw).
    pub total_latency_ms: f64,
    /// Time spent decoding the frame from the video container.
    pub decode_ms: f64,
    /// Time spent on preprocessing (crop, resize, normalize — all slots combined).
    pub preprocess_ms: f64,
    /// Time spent on ONNX inference (all inferred slots combined).
    pub inference_ms: f64,
    /// (Gatekeeper only) Time spent on fingerprint checks for locked slots.
    pub fingerprint_ms: f64,
    /// Time spent drawing overlays onto the frame.
    pub draw_ms: f64,
    /// Number of slots where inference was run.
    pub inferences_run: usize,
    /// Number of slots where inference was skipped (gatekeeper locked).
    pub inferences_skipped: usize,
    /// Per-slot details (always NUM_CARDS entries).
    pub slots: Vec<SlotTelemetry>,
}

/// Collects telemetry across all frames and writes to JSON at the end.
pub struct TelemetryLogger {
    pub frames: Vec<FrameTelemetry>,
    pub video_name: String,
    pub mode: String,
    pub fps: f64,
    pub total_frames: u64,
    pub width: i32,
    pub height: i32,
}

impl TelemetryLogger {
    pub fn new(
        video_name: &str,
        mode: &str,
        fps: f64,
        total_frames: u64,
        width: i32,
        height: i32,
    ) -> Self {
        Self {
            frames: Vec::with_capacity(total_frames as usize),
            video_name: video_name.to_string(),
            mode: mode.to_string(),
            fps,
            total_frames,
            width,
            height,
        }
    }

    pub fn push(&mut self, frame: FrameTelemetry) {
        self.frames.push(frame);
    }

    /// Write the complete telemetry to a JSON file.
    ///
    /// Call with the output path, e.g. "tracked_clash_gameplay.telemetry.json"
    pub fn save(&self, output_path: &str) -> anyhow::Result<()> {
        #[derive(Serialize)]
        struct TelemetryFile<'a> {
            video_name: &'a str,
            mode: &'a str,
            fps: f64,
            total_frames: u64,
            width: i32,
            height: i32,
            frames: &'a [FrameTelemetry],
        }

        let file = TelemetryFile {
            video_name: &self.video_name,
            mode: &self.mode,
            fps: self.fps,
            total_frames: self.total_frames,
            width: self.width,
            height: self.height,
            frames: &self.frames,
        };

        let json = serde_json::to_string(&file)?;
        fs::write(output_path, &json)?;

        // ── Summary ──
        let total_inferences: usize = self.frames.iter().map(|f| f.inferences_run).sum();
        let total_skipped: usize = self.frames.iter().map(|f| f.inferences_skipped).sum();
        let avg_latency: f64 = if self.frames.is_empty() {
            0.0
        } else {
            self.frames.iter().map(|f| f.total_latency_ms).sum::<f64>()
                / self.frames.len() as f64
        };

        let size_kb = json.len() as f64 / 1024.0;
        println!("\n📊 Telemetry saved to: {} ({:.0} KB)", output_path, size_kb);
        println!(
            "   {} frames | avg {:.1}ms/frame | {} inferences | {} skipped",
            self.frames.len(),
            avg_latency,
            total_inferences,
            total_skipped
        );

        Ok(())
    }
}

/// Convenience: start a scoped timer. Returns an Instant.
/// Use with `timer.elapsed().as_secs_f64() * 1000.0` to get ms.
pub fn start_timer() -> Instant {
    Instant::now()
}

/// Convert an Instant elapsed to milliseconds.
pub fn elapsed_ms(t: &Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
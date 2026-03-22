use anyhow::{Context, Result};
use image::{imageops::FilterType, RgbImage};
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array4;
use opencv::{
    core::{self, Mat, Scalar, Size},
    imgproc,
    prelude::*,
    videoio::{self, VideoCapture, VideoWriter},
};
use ort::session::Session;
use ort::value::Tensor;
use rayon::prelude::*;
use std::fs;

// ─── CONFIG ────────────────────────────────────────────────────────────────────

const VIDEO_PATH: &str = "/Users/soren/Desktop/Clash Royale/Hand Card/Clash royale game.MOV";
const OUTPUT_PATH: &str = "tracked_clash_gameplay.mp4";
const MODEL_PATH: &str = "checkpoints/best_model.onnx";
const CLASS_PATH: &str = "checkpoints/classes.json";

const NUM_CARDS: usize = 5;

// ImageNet normalization constants
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

// ─── GATEKEEPER CONFIG ─────────────────────────────────────────────────────────

/// Minimum softmax confidence to lock a slot (stop running inference).
const LOCK_CONFIDENCE: f32 = 0.90;

/// Maximum allowed difference in the pixel fingerprint before a locked slot
/// is unlocked. Range 0.0–1.0 where 0.0 = identical, 1.0 = completely different.
/// Tuned for card-slot–sized crops: the grey loading animation easily exceeds this.
const PIXEL_CHANGE_THRESHOLD: f32 = 0.06;

/// Number of sample points per channel for the fingerprint.
/// 8×8 grid × 3 channels = 192 values. Sub-microsecond to compute.
const FINGERPRINT_GRID: usize = 8;

// ─── TYPES ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct CardBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Debug, Clone, Copy)]
struct CardRect {
    left: u32,
    top: u32,
    w: u32,
    h: u32,
}

const IPHONE_LAYOUT: [CardBox; NUM_CARDS] = [
    CardBox { x: 0.044, y: 0.911, w: 0.112, h: 0.063 },
    CardBox { x: 0.215, y: 0.820, w: 0.190, h: 0.124 },
    CardBox { x: 0.406, y: 0.821, w: 0.191, h: 0.124 },
    CardBox { x: 0.600, y: 0.820, w: 0.179, h: 0.124 },
    CardBox { x: 0.780, y: 0.820, w: 0.194, h: 0.125 },
];

// ─── GATEKEEPER STATE MACHINE ──────────────────────────────────────────────────

/// A compact pixel "fingerprint" of a card slot — an 8×8 grid of average RGB
/// values sampled from the crop. Comparing two fingerprints is O(192) and
/// takes sub-microsecond, vs the ~9ms cost of a full inference call.
type Fingerprint = Vec<f32>; // length = FINGERPRINT_GRID * FINGERPRINT_GRID * 3

/// Per-slot state for the gatekeeper.
#[derive(Clone)]
struct SlotState {
    /// Is this slot locked (inference skipped)?
    locked: bool,
    /// Cached classification result (class index + confidence).
    class_idx: usize,
    confidence: f32,
    /// Pixel fingerprint taken at the moment the slot was locked.
    fingerprint: Fingerprint,
}

impl SlotState {
    fn new() -> Self {
        Self {
            locked: false,
            class_idx: 0,
            confidence: 0.0,
            fingerprint: Vec::new(),
        }
    }
}

/// Compute a lightweight pixel fingerprint for a card crop.
/// Samples an 8×8 grid of pixels from the RgbImage region,
/// storing normalized (0.0–1.0) R, G, B values.
fn compute_fingerprint(img: &RgbImage, rect: &CardRect) -> Fingerprint {
    let mut fp = Vec::with_capacity(FINGERPRINT_GRID * FINGERPRINT_GRID * 3);

    for gy in 0..FINGERPRINT_GRID {
        for gx in 0..FINGERPRINT_GRID {
            // Map grid position to pixel coordinate within the crop
            let px = rect.left + (rect.w * gx as u32) / (FINGERPRINT_GRID as u32);
            let py = rect.top + (rect.h * gy as u32) / (FINGERPRINT_GRID as u32);

            // Clamp to image bounds
            let px = px.min(img.width().saturating_sub(1));
            let py = py.min(img.height().saturating_sub(1));

            let pixel = img.get_pixel(px, py);
            fp.push(pixel[0] as f32 / 255.0);
            fp.push(pixel[1] as f32 / 255.0);
            fp.push(pixel[2] as f32 / 255.0);
        }
    }
    fp
}

/// Compute the mean absolute difference between two fingerprints.
/// Returns 0.0 for identical, up to ~1.0 for maximally different.
fn fingerprint_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0; // Force unlock if fingerprints are incompatible
    }
    let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum();
    sum / a.len() as f32
}

// ─── HELPERS ───────────────────────────────────────────────────────────────────

fn mat_to_rgb_image(mat: &Mat) -> Result<RgbImage> {
    let rows = mat.rows() as u32;
    let cols = mat.cols() as u32;

    let mut rgb_mat = Mat::default();
    imgproc::cvt_color(mat, &mut rgb_mat, imgproc::COLOR_BGR2RGB, 0, core::AlgorithmHint::ALGO_HINT_DEFAULT)?;

    let data = rgb_mat.data_bytes()?.to_vec();
    RgbImage::from_raw(cols, rows, data).context("Failed to create RgbImage from Mat data")
}

fn preprocess_single_crop(img: &RgbImage, rect: &CardRect) -> Vec<f32> {
    let crop = image::imageops::crop_imm(img, rect.left, rect.top, rect.w, rect.h).to_image();
    let resized = image::imageops::resize(&crop, 224, 224, FilterType::Triangle);

    let mut buf = vec![0.0f32; 3 * 224 * 224];
    for y in 0..224usize {
        for x in 0..224usize {
            let pixel = resized.get_pixel(x as u32, y as u32);
            for c in 0..3usize {
                let val = pixel[c] as f32 / 255.0;
                buf[c * (224 * 224) + y * 224 + x] = (val - MEAN[c]) / STD[c];
            }
        }
    }
    buf
}

/// Preprocess only the *unlocked* slots in parallel, returning a batch tensor
/// and the mapping of which batch index corresponds to which slot.
fn preprocess_unlocked_parallel(
    img: &RgbImage,
    rects: &[CardRect; NUM_CARDS],
    slots: &[SlotState; NUM_CARDS],
) -> (Array4<f32>, Vec<usize>) {
    // Collect indices of unlocked slots
    let unlocked_indices: Vec<usize> = (0..NUM_CARDS)
        .filter(|&i| !slots[i].locked)
        .collect();

    let batch_size = unlocked_indices.len();
    if batch_size == 0 {
        // All locked — return a dummy 1-element tensor (won't be used)
        return (
            Array4::<f32>::zeros((1, 3, 224, 224)),
            unlocked_indices,
        );
    }

    // Parallel preprocess only unlocked slots
    let crops: Vec<Vec<f32>> = unlocked_indices
        .par_iter()
        .map(|&i| preprocess_single_crop(img, &rects[i]))
        .collect();

    let flat: Vec<f32> = crops.into_iter().flatten().collect();
    let tensor = Array4::from_shape_vec((batch_size, 3, 224, 224), flat)
        .expect("Shape mismatch when building batched tensor");

    (tensor, unlocked_indices)
}

fn softmax_argmax(logits: &[f32]) -> (usize, f32) {
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&v| (v - max_logit).exp()).collect();
    let sum: f32 = exps.iter().sum();
    let probs: Vec<f32> = exps.iter().map(|&e| e / sum).collect();

    let (idx, &conf) = probs
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap();
    (idx, conf)
}

fn draw_card_overlay(
    frame: &mut Mat,
    rect: &CardRect,
    name: &str,
    conf: f32,
    locked: bool,
) -> Result<()> {
    // Locked slots get a green border, unlocked get white
    let border_color = if locked {
        Scalar::new(0.0, 220.0, 0.0, 0.0) // Green = gated (inference skipped)
    } else {
        Scalar::new(255.0, 255.0, 255.0, 0.0) // White = active inference
    };
    let black = Scalar::new(0.0, 0.0, 0.0, 0.0);
    let bg_color = if locked {
        Scalar::new(200.0, 255.0, 200.0, 0.0) // Light green background
    } else {
        Scalar::new(255.0, 255.0, 255.0, 0.0)
    };

    let left = rect.left as i32;
    let top = rect.top as i32;
    let card_w = rect.w as i32;
    let card_h = rect.h as i32;

    imgproc::rectangle(
        frame,
        core::Rect::new(left, top, card_w, card_h),
        border_color,
        2,
        imgproc::LINE_8,
        0,
    )?;

    let font = imgproc::FONT_HERSHEY_SIMPLEX;
    let thickness = 2;
    let padding = 4;

    let full_text = format!("{} ({:.0}%)", name, conf * 100.0);
    let mut baseline = 0;

    let mut scale = 0.6;
    let mut text_size =
        imgproc::get_text_size(&full_text, font, scale, thickness, &mut baseline)?;

    while text_size.width > card_w - padding * 2 && scale > 0.25 {
        scale -= 0.05;
        text_size =
            imgproc::get_text_size(&full_text, font, scale, thickness, &mut baseline)?;
    }

    let line_h = text_size.height + baseline + padding;

    if text_size.width <= card_w - padding * 2 {
        let bg_rect = core::Rect::new(left, top, card_w, line_h + padding);
        imgproc::rectangle(frame, bg_rect, bg_color, -1, imgproc::LINE_8, 0)?;
        imgproc::put_text(
            frame,
            &full_text,
            core::Point::new(left + padding, top + text_size.height + padding),
            font,
            scale,
            black,
            thickness,
            imgproc::LINE_8,
            false,
        )?;
    } else {
        let line1 = name.to_string();
        let line2 = format!("({:.0}%)", conf * 100.0);

        let mut s = scale;
        let mut sz1 =
            imgproc::get_text_size(&line1, font, s, thickness, &mut baseline)?;
        while sz1.width > card_w - padding * 2 && s > 0.2 {
            s -= 0.05;
            sz1 = imgproc::get_text_size(&line1, font, s, thickness, &mut baseline)?;
        }
        let _sz2 = imgproc::get_text_size(&line2, font, s, thickness, &mut baseline)?;
        let lh = sz1.height + baseline + padding;

        let bg_h = lh * 2 + padding;
        let bg_rect = core::Rect::new(left, top, card_w, bg_h.min(card_h));
        imgproc::rectangle(frame, bg_rect, bg_color, -1, imgproc::LINE_8, 0)?;

        imgproc::put_text(
            frame,
            &line1,
            core::Point::new(left + padding, top + sz1.height + padding),
            font,
            s,
            black,
            thickness,
            imgproc::LINE_8,
            false,
        )?;

        imgproc::put_text(
            frame,
            &line2,
            core::Point::new(left + padding, top + sz1.height + padding + lh),
            font,
            s,
            black,
            thickness,
            imgproc::LINE_8,
            false,
        )?;
    }

    Ok(())
}

// ─── MAIN ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    // --- Load class names ---
    let raw = fs::read_to_string(CLASS_PATH).context("Failed to read classes.json")?;
    let class_names_raw: Vec<String> = serde_json::from_str(&raw)?;
    let class_names: Vec<String> = class_names_raw
        .iter()
        .map(|n| n.replace("Hand ", ""))
        .collect();
    let num_classes = class_names.len();

    // --- Load ONNX model with CoreML (Apple Neural Engine) ---
    let model_bytes = std::fs::read(MODEL_PATH)
        .context("Cannot read model file — make sure checkpoints/best_model.onnx exists")?;

    let mut session = Session::builder()
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .with_intra_threads(4)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .with_execution_providers([
            ort::execution_providers::CoreMLExecutionProvider::default()
                .with_subgraphs(true)
                .build(),
        ])
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .commit_from_memory(&model_bytes)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("Failed to load ONNX model")?;

    println!("🧠 Model loaded (CoreML/ANE will be used if available, CPU fallback otherwise)");

    // --- Open video ---
    let mut cap = VideoCapture::from_file(VIDEO_PATH, videoio::CAP_ANY)?;
    if !cap.is_opened()? {
        anyhow::bail!("Cannot open video: {}", VIDEO_PATH);
    }

    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    let width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
    let height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
    let total_frames = cap.get(videoio::CAP_PROP_FRAME_COUNT)? as u64;

    let card_rects: [CardRect; NUM_CARDS] = std::array::from_fn(|i| {
        let b = &IPHONE_LAYOUT[i];
        CardRect {
            left: (b.x * width as f64) as u32,
            top: (b.y * height as f64) as u32,
            w: (b.w * width as f64) as u32,
            h: (b.h * height as f64) as u32,
        }
    });

    let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v')?;
    let mut out = VideoWriter::new(OUTPUT_PATH, fourcc, fps, Size::new(width, height), true)?;
    if !out.is_opened()? {
        anyhow::bail!("Cannot open output video writer");
    }

    println!(
        "🚀 Processing {} frames with Gatekeeper (lock threshold: {:.0}%, pixel Δ: {:.1}%)...",
        total_frames,
        LOCK_CONFIDENCE * 100.0,
        PIXEL_CHANGE_THRESHOLD * 100.0
    );

    let pb = ProgressBar::new(total_frames);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )?
            .progress_chars("█▓░"),
    );

    // ─── GATEKEEPER STATE ──────────────────────────────────────────────────────
    let mut slot_states: [SlotState; NUM_CARDS] = std::array::from_fn(|_| SlotState::new());

    // Stats tracking
    let mut total_slot_evaluations: u64 = 0; // Total slot×frame opportunities
    let mut inferences_skipped: u64 = 0;     // Slots where inference was skipped (locked)
    let mut inferences_run: u64 = 0;         // Slots where inference was actually run

    // --- Processing loop ---
    let mut frame = Mat::default();
    let mut frames_processed: u64 = 0;

    loop {
        let ok = cap.read(&mut frame)?;
        if !ok || frame.empty() {
            if frames_processed >= total_frames.saturating_sub(1) {
                break;
            }
            frames_processed += 1;
            pb.inc(1);
            continue;
        }

        let rgb_img = mat_to_rgb_image(&frame)?;

        // ── GATEKEEPER PHASE 1: Check locked slots for pixel changes ──
        for i in 0..NUM_CARDS {
            if slot_states[i].locked {
                let current_fp = compute_fingerprint(&rgb_img, &card_rects[i]);
                let dist = fingerprint_distance(&slot_states[i].fingerprint, &current_fp);

                if dist > PIXEL_CHANGE_THRESHOLD {
                    // Pixels changed significantly → unlock, re-run inference
                    slot_states[i].locked = false;
                }
            }
        }

        // Count how many slots are locked vs unlocked this frame
        let num_locked = slot_states.iter().filter(|s| s.locked).count();
        let num_unlocked = NUM_CARDS - num_locked;
        total_slot_evaluations += NUM_CARDS as u64;
        inferences_skipped += num_locked as u64;

        // ── GATEKEEPER PHASE 2: Run inference only on unlocked slots ──
        if num_unlocked > 0 {
            let (batch_tensor, unlocked_indices) =
                preprocess_unlocked_parallel(&rgb_img, &card_rects, &slot_states);

            let shape = batch_tensor.shape().to_vec();
            let (flat_data, _offset) = batch_tensor.into_raw_vec_and_offset();
            let input_value = Tensor::from_array((shape, flat_data))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let outputs = session
                .run(ort::inputs!["input" => input_value])
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            let output_tensor = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let (_shape, all_logits) = output_tensor;

            // Map results back to the correct slot indices
            for (batch_idx, &slot_idx) in unlocked_indices.iter().enumerate() {
                let logits = &all_logits[batch_idx * num_classes..(batch_idx + 1) * num_classes];
                let (idx, conf) = softmax_argmax(logits);

                slot_states[slot_idx].class_idx = idx;
                slot_states[slot_idx].confidence = conf;
                inferences_run += 1;

                // GATEKEEPER PHASE 3: Lock if confidence is high enough
                if conf >= LOCK_CONFIDENCE {
                    slot_states[slot_idx].locked = true;
                    slot_states[slot_idx].fingerprint =
                        compute_fingerprint(&rgb_img, &card_rects[slot_idx]);
                }
            }
        }

        // ── Draw all slots (locked or not) using their cached state ──
        for (i, rect) in card_rects.iter().enumerate() {
            let state = &slot_states[i];
            let name = &class_names[state.class_idx];
            draw_card_overlay(&mut frame, rect, name, state.confidence, state.locked)?;
        }

        out.write(&frame)?;
        frames_processed += 1;
        pb.inc(1);
    }

    pb.finish_with_message("Done!");

    // ─── GATEKEEPER STATS ──────────────────────────────────────────────────────
    let skip_rate = if total_slot_evaluations > 0 {
        (inferences_skipped as f64 / total_slot_evaluations as f64) * 100.0
    } else {
        0.0
    };

    println!("\n┌─────────────────────────────────────────┐");
    println!("│         🚪 GATEKEEPER STATS              │");
    println!("├─────────────────────────────────────────┤");
    println!("│  Total slot evaluations: {:>14}  │", total_slot_evaluations);
    println!("│  Inferences run:        {:>14}  │", inferences_run);
    println!("│  Inferences skipped:    {:>14}  │", inferences_skipped);
    println!("│  Skip rate:             {:>13.1}%  │", skip_rate);
    println!("└─────────────────────────────────────────┘");
    println!("\n✅ Export Complete: {}", OUTPUT_PATH);
    Ok(())
}
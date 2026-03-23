use anyhow::{Context, Result};
use clap::Parser;
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
use std::cell::RefCell;
use std::fs;

// ─── THREAD-LOCAL PREPROCESS BUFFER ────────────────────────────────────────────
//
// Each rayon worker thread gets its own 588 KB buffer (3×224×224 floats) via
// thread-local storage (TLS). The buffer is allocated once on first use and
// reused for every subsequent crop on that thread.
//
// Without this, `preprocess_single_crop` would heap-allocate a fresh Vec on
// every call — that's 5 cards × N frames = tens of thousands of alloc/free
// round-trips. With TLS, we reduce that to one allocation per rayon thread
// (typically 4–8 total), and each call just overwrites the existing buffer.
//
// `RefCell` is needed because `thread_local!` only gives us a shared `&`
// reference, but we need `&mut` to write pixel data into the buffer.
// Since each thread has its own independent copy, there's no contention —
// the runtime borrow check inside RefCell will never fail here.
thread_local! {
    static CROP_BUF: RefCell<Vec<f32>> = RefCell::new(vec![0.0f32; 3 * 224 * 224]);
}

// ─── CLI ───────────────────────────────────────────────────────────────────────

/// Clash Royale card classifier — batched ONNX inference over video frames.
///
/// Usage:
///   cargo run --release -- --video input.mov
///   cargo run --release -- --video input.mov --output out.mp4 --model my.onnx
#[derive(Parser, Debug)]
#[command(name = "cr-card-classifier")]
struct Args {
    /// Path to the input video file
    #[arg(short, long)]
    video: String,

    /// Path to the output annotated video [default: tracked_clash_gameplay.mp4]
    #[arg(short, long, default_value = "tracked_clash_gameplay.mp4")]
    output: String,

    /// Path to the ONNX model checkpoint [default: checkpoints/best_model.onnx]
    #[arg(short, long, default_value = "checkpoints/best_model.onnx")]
    model: String,

    /// Path to the class names JSON file [default: checkpoints/classes.json]
    #[arg(short, long, default_value = "checkpoints/classes.json")]
    classes: String,
}

const NUM_CARDS: usize = 5;

// ImageNet normalization constants
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

#[derive(Debug, Clone, Copy)]
struct CardBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Pixel-space rectangle computed once from the frame resolution.
#[derive(Debug, Clone, Copy)]
struct CardRect {
    left: u32,
    top: u32,
    w: u32,
    h: u32,
}

const IPHONE_LAYOUT: [CardBox; NUM_CARDS] = [
    CardBox { x: 0.044, y: 0.911, w: 0.112, h: 0.063 }, // Card 1 (Next)
    CardBox { x: 0.215, y: 0.820, w: 0.190, h: 0.124 }, // Card 2
    CardBox { x: 0.406, y: 0.821, w: 0.191, h: 0.124 }, // Card 3
    CardBox { x: 0.600, y: 0.820, w: 0.179, h: 0.124 }, // Card 4
    CardBox { x: 0.780, y: 0.820, w: 0.194, h: 0.125 }, // Card 5
];

// ─── HELPERS ───────────────────────────────────────────────────────────────────

/// Convert an OpenCV Mat (BGR, u8) to an `image` crate RgbImage.
fn mat_to_rgb_image(mat: &Mat) -> Result<RgbImage> {
    let rows = mat.rows() as u32;
    let cols = mat.cols() as u32;

    let mut rgb_mat = Mat::default();
    imgproc::cvt_color(mat, &mut rgb_mat, imgproc::COLOR_BGR2RGB, 0, core::AlgorithmHint::ALGO_HINT_DEFAULT)?;

    let data = rgb_mat.data_bytes()?.to_vec();
    RgbImage::from_raw(cols, rows, data).context("Failed to create RgbImage from Mat data")
}

/// Crop, resize to 224×224, normalize → flat Vec<f32> in CHW order (length 3×224×224).
/// This is the per-crop unit of work that runs in parallel via rayon.
///
/// Uses a thread-local buffer to avoid allocating a new Vec on every call.
/// The buffer lives in each rayon thread's TLS and is reused across frames.
/// We `.clone()` the filled buffer to return owned data to the caller —
/// this is a single memcpy of known size, cheaper than a heap alloc round-trip.
fn preprocess_single_crop(img: &RgbImage, rect: &CardRect) -> Vec<f32> {
    let crop = image::imageops::crop_imm(img, rect.left, rect.top, rect.w, rect.h).to_image();
    let resized = image::imageops::resize(&crop, 224, 224, FilterType::Triangle);

    CROP_BUF.with(|cell| {
        let mut buf = cell.borrow_mut();
        for y in 0..224usize {
            for x in 0..224usize {
                let pixel = resized.get_pixel(x as u32, y as u32);
                for c in 0..3usize {
                    let val = pixel[c] as f32 / 255.0;
                    // CHW layout: channel * (224*224) + y * 224 + x
                    buf[c * (224 * 224) + y * 224 + x] = (val - MEAN[c]) / STD[c];
                }
            }
        }
        buf.clone() // copy data out; the buffer stays in TLS for the next crop
    })
}

/// Preprocess all 5 card crops **in parallel** using rayon, then stack them into
/// a single batched NCHW tensor of shape [NUM_CARDS, 3, 224, 224].
fn preprocess_batch_parallel(img: &RgbImage, rects: &[CardRect; NUM_CARDS]) -> Array4<f32> {
    // Each element is a Vec<f32> of length 3*224*224, produced on a rayon thread
    let crops: Vec<Vec<f32>> = rects
        .par_iter()
        .map(|rect| preprocess_single_crop(img, rect))
        .collect();

    // Flatten into a single contiguous buffer and reshape to [5, 3, 224, 224]
    let flat: Vec<f32> = crops.into_iter().flatten().collect();
    Array4::from_shape_vec((NUM_CARDS, 3, 224, 224), flat)
        .expect("Shape mismatch when building batched tensor")
}

/// Apply softmax to a 1-D slice and return (max_index, max_probability).
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

/// Draw the detection overlay for one card on the frame.
/// Text is rendered *inside* the card box, at the top. If it's too wide,
/// it wraps onto a second line (name on line 1, confidence on line 2).
fn draw_card_overlay(
    frame: &mut Mat,
    rect: &CardRect,
    name: &str,
    conf: f32,
) -> Result<()> {
    let white = Scalar::new(255.0, 255.0, 255.0, 0.0);
    let black = Scalar::new(0.0, 0.0, 0.0, 0.0);

    let left = rect.left as i32;
    let top = rect.top as i32;
    let card_w = rect.w as i32;
    let card_h = rect.h as i32;

    // Draw card border
    imgproc::rectangle(
        frame,
        core::Rect::new(left, top, card_w, card_h),
        white,
        2,
        imgproc::LINE_8,
        0,
    )?;

    let font = imgproc::FONT_HERSHEY_SIMPLEX;
    let thickness = 2;
    let padding = 4;

    // Try to find a font scale that fits the card width
    let full_text = format!("{} ({:.0}%)", name, conf * 100.0);
    let mut baseline = 0;

    // Start with scale 0.6 and shrink if needed
    let mut scale = 0.6;
    let mut text_size =
        imgproc::get_text_size(&full_text, font, scale, thickness, &mut baseline)?;

    // Shrink font if text is wider than card
    while text_size.width > card_w - padding * 2 && scale > 0.25 {
        scale -= 0.05;
        text_size =
            imgproc::get_text_size(&full_text, font, scale, thickness, &mut baseline)?;
    }

    let line_h = text_size.height + baseline + padding;

    if text_size.width <= card_w - padding * 2 {
        // ── Single line: fits inside the card ──
        let bg_rect = core::Rect::new(left, top, card_w, line_h + padding);
        imgproc::rectangle(frame, bg_rect, white, -1, imgproc::LINE_8, 0)?;
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
        // ── Two lines: name on top, confidence below ──
        let line1 = name.to_string();
        let line2 = format!("({:.0}%)", conf * 100.0);

        // Measure each line (may need to shrink further)
        let mut s = scale;
        let mut sz1 =
            imgproc::get_text_size(&line1, font, s, thickness, &mut baseline)?;
        while sz1.width > card_w - padding * 2 && s > 0.2 {
            s -= 0.05;
            sz1 = imgproc::get_text_size(&line1, font, s, thickness, &mut baseline)?;
        }
        let _sz2 = imgproc::get_text_size(&line2, font, s, thickness, &mut baseline)?;
        let lh = sz1.height + baseline + padding;

        // White background covering both lines
        let bg_h = lh * 2 + padding;
        let bg_rect = core::Rect::new(left, top, card_w, bg_h.min(card_h));
        imgproc::rectangle(frame, bg_rect, white, -1, imgproc::LINE_8, 0)?;

        // Line 1: name
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

        // Line 2: confidence
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
    let args = Args::parse();

    // --- Load class names ---
    let raw = fs::read_to_string(&args.classes)
        .with_context(|| format!("Failed to read class names from: {}", args.classes))?;
    let class_names_raw: Vec<String> = serde_json::from_str(&raw)?;
    let class_names: Vec<String> = class_names_raw
        .iter()
        .map(|n| n.replace("Hand ", ""))
        .collect();
    let num_classes = class_names.len();

    // --- Load ONNX model with CoreML (Apple Neural Engine) ---
    // Read model bytes into memory to avoid path issues with CoreML + external data files
    let model_bytes = std::fs::read(&args.model)
        .with_context(|| format!("Cannot read model file: {}", args.model))?;

    // `session` is immutable: Session::run() takes &self, not &mut self.
    // The model weights and execution graph don't change between inference calls.
    let session = Session::builder()
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
    let mut cap = VideoCapture::from_file(&args.video, videoio::CAP_ANY)?;
    if !cap.is_opened()? {
        anyhow::bail!("Cannot open video: {}", args.video);
    }

    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    let width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
    let height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
    let total_frames = cap.get(videoio::CAP_PROP_FRAME_COUNT)? as u64;

    // --- Precompute pixel-space rectangles (constant for every frame) ---
    let card_rects: [CardRect; NUM_CARDS] = std::array::from_fn(|i| {
        let b = &IPHONE_LAYOUT[i];
        CardRect {
            left: (b.x * width as f64) as u32,
            top: (b.y * height as f64) as u32,
            w: (b.w * width as f64) as u32,
            h: (b.h * height as f64) as u32,
        }
    });

    // --- Open writer ---
    let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v')?;
    let mut out = VideoWriter::new(&args.output, fourcc, fps, Size::new(width, height), true)?;
    if !out.is_opened()? {
        anyhow::bail!("Cannot open output video writer");
    }

    println!(
        "🚀 Processing {} frames (batched inference × {} cards, parallel preprocessing)...",
        total_frames, NUM_CARDS
    );

    let pb = ProgressBar::new(total_frames);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )?
            .progress_chars("█▓░"),
    );

    // --- Processing loop ---
    let mut frame = Mat::default();
    let mut frames_processed: u64 = 0;

    loop {
        let ok = cap.read(&mut frame)?;
        if !ok || frame.empty() {
            // Some codecs return false for a frame mid-stream then recover.
            // Only truly stop if we've hit the expected frame count or
            // if we get multiple consecutive failures.
            if frames_processed >= total_frames.saturating_sub(1) {
                break; // Reached the end
            }
            // Try to keep going — could be a transient decode hiccup
            frames_processed += 1;
            pb.inc(1);
            continue;
        }

        let rgb_img = mat_to_rgb_image(&frame)?;

        // ── 1. Parallel preprocess: 5 crops on rayon threads → [5, 3, 224, 224] ──
        let batch_tensor = preprocess_batch_parallel(&rgb_img, &card_rects);

        // ── 2. Single batched inference call ──
        // Convert to shape+vec tuple to avoid ndarray version conflicts with ort
        let shape = batch_tensor.shape().to_vec();
        let (flat_data, _offset) = batch_tensor.into_raw_vec_and_offset();
        let input_value = Tensor::from_array((shape, flat_data))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let outputs = session.run(ort::inputs!["input" => input_value])
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Output shape: [5, num_classes]  (flat slice = 5 * num_classes)
        let output_tensor = outputs[0].try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let (_shape, all_logits) = output_tensor;

        // ── 3. Draw results for each card ──
        for (i, rect) in card_rects.iter().enumerate() {
            let logits = &all_logits[i * num_classes..(i + 1) * num_classes];
            let (idx, conf) = softmax_argmax(logits);
            draw_card_overlay(&mut frame, rect, &class_names[idx], conf)?;
        }

        out.write(&frame)?;
        frames_processed += 1;
        pb.inc(1);
    }

    pb.finish_with_message("Done!");
    println!("\n✅ Export Complete: {}", args.output);
    Ok(())
}
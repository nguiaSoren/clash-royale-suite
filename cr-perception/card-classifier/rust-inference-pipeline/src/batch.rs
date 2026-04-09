use anyhow::{Context, Result};
use clash_tracker::telemetry::{
    self, elapsed_ms, FrameTelemetry, SlotTelemetry, TelemetryLogger,
};
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

const INPUT_DIR: &str = "/Users/soren/Pictures";
const OUTPUT_DIR: &str = "tracked_output";
const MODEL_PATH: &str = "checkpoints/best_model.onnx";
const CLASS_PATH: &str = "checkpoints/classes.json";

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

struct DeviceLayout {
    name: &'static str,
    src_width: u32,
    src_height: u32,
    cards: [CardBox; NUM_CARDS],
}

const LAYOUTS: &[DeviceLayout] = &[
    DeviceLayout {
        name: "classic",
        src_width: 1080, src_height: 1920,
        cards: [
            CardBox { x: 0.2074, y: 0.9536, w: 0.0565, h: 0.0401 }, // Next
            CardBox { x: 0.3139, y: 0.8724, w: 0.1259, h: 0.0984 }, // Card 2
            CardBox { x: 0.4398, y: 0.8719, w: 0.1185, h: 0.1021 }, // Card 3
            CardBox { x: 0.5583, y: 0.8724, w: 0.1269, h: 0.1016 }, // Card 4
            CardBox { x: 0.6861, y: 0.8724, w: 0.1213, h: 0.1021 }, // Card 5
        ],
    },
    DeviceLayout {
        name: "double",
        src_width: 864, src_height: 1920,
        cards: [
            CardBox { x: 0.0509, y: 0.9437, w: 0.0856, h: 0.0484 }, // Next
            CardBox { x: 0.2164, y: 0.8458, w: 0.1829, h: 0.1234 }, // Card 2
            CardBox { x: 0.3993, y: 0.8453, w: 0.1968, h: 0.1245 }, // Card 3
            CardBox { x: 0.5949, y: 0.8469, w: 0.1817, h: 0.1219 }, // Card 4
            CardBox { x: 0.7778, y: 0.8458, w: 0.1898, h: 0.1229 }, // Card 5
        ],
    },
    DeviceLayout {
        name: "golem",
        src_width: 888, src_height: 1920,
        cards: [
            CardBox { x: 0.0236, y: 0.8750, w: 0.0755, h: 0.0401 }, // Next
            CardBox { x: 0.1273, y: 0.8604, w: 0.1363, h: 0.0870 }, // Card 2
            CardBox { x: 0.2635, y: 0.8604, w: 0.1318, h: 0.0885 }, // Card 3
            CardBox { x: 0.3941, y: 0.8609, w: 0.1306, h: 0.0880 }, // Card 4
            CardBox { x: 0.5225, y: 0.8609, w: 0.1385, h: 0.0870 }, // Card 5
        ],
    },
    DeviceLayout {
        name: "grand",
        src_width: 1080, src_height: 2304,
        cards: [
            CardBox { x: 0.0444, y: 0.9258, w: 0.0991, h: 0.0521 }, // Next
            CardBox { x: 0.2102, y: 0.8242, w: 0.1889, h: 0.1285 }, // Card 2
            CardBox { x: 0.4000, y: 0.8247, w: 0.1907, h: 0.1285 }, // Card 3
            CardBox { x: 0.5917, y: 0.8251, w: 0.1880, h: 0.1285 }, // Card 4
            CardBox { x: 0.7796, y: 0.8242, w: 0.1870, h: 0.1298 }, // Card 5
        ],
    },
    DeviceLayout {
        name: "hog26",
        src_width: 1080, src_height: 2304,
        cards: [
            CardBox { x: 0.0519, y: 0.9366, w: 0.0815, h: 0.0473 }, // Next
            CardBox { x: 0.2185, y: 0.8325, w: 0.1833, h: 0.1254 }, // Card 2
            CardBox { x: 0.4056, y: 0.8316, w: 0.1889, h: 0.1280 }, // Card 3
            CardBox { x: 0.5944, y: 0.8303, w: 0.1852, h: 0.1298 }, // Card 4
            CardBox { x: 0.7806, y: 0.8312, w: 0.1917, h: 0.1298 }, // Card 5
        ],
    },
    DeviceLayout {
        name: "logbait",
        src_width: 1080, src_height: 2304,
        cards: [
            CardBox { x: 0.0519, y: 0.9366, w: 0.0815, h: 0.0473 }, // Next
            CardBox { x: 0.2185, y: 0.8325, w: 0.1833, h: 0.1254 }, // Card 2
            CardBox { x: 0.4056, y: 0.8316, w: 0.1889, h: 0.1280 }, // Card 3
            CardBox { x: 0.5944, y: 0.8303, w: 0.1852, h: 0.1298 }, // Card 4
            CardBox { x: 0.7806, y: 0.8312, w: 0.1917, h: 0.1298 }, // Card 5
        ],
    },
    DeviceLayout {
        name: "topladder",
        src_width: 1080, src_height: 1920,
        cards: [
            CardBox { x: 0.1324, y: 0.9271, w: 0.0870, h: 0.0620 }, // Next
            CardBox { x: 0.2769, y: 0.8339, w: 0.1537, h: 0.1208 }, // Card 2
            CardBox { x: 0.4324, y: 0.8339, w: 0.1491, h: 0.1250 }, // Card 3
            CardBox { x: 0.5806, y: 0.8333, w: 0.1546, h: 0.1260 }, // Card 4
            CardBox { x: 0.7352, y: 0.8339, w: 0.1583, h: 0.1250 }, // Card 5
        ],
    },
];

/// Pick the layout whose source dimensions are closest (Euclidean) to the video.
fn select_layout(video_w: u32, video_h: u32) -> &'static DeviceLayout {
    LAYOUTS
        .iter()
        .min_by_key(|l| {
            let dw = l.src_width as i64 - video_w as i64;
            let dh = l.src_height as i64 - video_h as i64;
            (dw * dw + dh * dh) as u64
        })
        .expect("LAYOUTS must not be empty")
}

// ─── HELPERS ───────────────────────────────────────────────────────────────────

fn mat_to_rgb_image(mat: &Mat) -> Result<RgbImage> {
    let rows = mat.rows() as u32;
    let cols = mat.cols() as u32;

    let mut rgb_mat = Mat::default();
    imgproc::cvt_color(
        mat,
        &mut rgb_mat,
        imgproc::COLOR_BGR2RGB,
        0,
        core::AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;

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

fn preprocess_batch_parallel(img: &RgbImage, rects: &[CardRect; NUM_CARDS]) -> Array4<f32> {
    let crops: Vec<Vec<f32>> = rects
        .par_iter()
        .map(|rect| preprocess_single_crop(img, rect))
        .collect();

    let flat: Vec<f32> = crops.into_iter().flatten().collect();
    Array4::from_shape_vec((NUM_CARDS, 3, 224, 224), flat)
        .expect("Shape mismatch when building batched tensor")
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

fn draw_card_overlay(frame: &mut Mat, rect: &CardRect, name: &str, conf: f32) -> Result<()> {
    let white = Scalar::new(255.0, 255.0, 255.0, 0.0);
    let black = Scalar::new(0.0, 0.0, 0.0, 0.0);

    let left = rect.left as i32;
    let top = rect.top as i32;
    let card_w = rect.w as i32;
    let card_h = rect.h as i32;

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

    let full_text = format!("{} ({:.0}%)", name, conf * 100.0);
    let mut baseline = 0;

    let mut scale = 0.6;
    let mut text_size =
        imgproc::get_text_size(&full_text, font, scale, thickness, &mut baseline)?;

    while text_size.width > card_w - padding * 2 && scale > 0.25 {
        scale -= 0.05;
        text_size = imgproc::get_text_size(&full_text, font, scale, thickness, &mut baseline)?;
    }

    let line_h = text_size.height + baseline + padding;

    if text_size.width <= card_w - padding * 2 {
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
        let line1 = name.to_string();
        let line2 = format!("({:.0}%)", conf * 100.0);

        let mut s = scale;
        let mut sz1 = imgproc::get_text_size(&line1, font, s, thickness, &mut baseline)?;
        while sz1.width > card_w - padding * 2 && s > 0.2 {
            s -= 0.05;
            sz1 = imgproc::get_text_size(&line1, font, s, thickness, &mut baseline)?;
        }
        let _sz2 = imgproc::get_text_size(&line2, font, s, thickness, &mut baseline)?;
        let lh = sz1.height + baseline + padding;

        let bg_h = lh * 2 + padding;
        let bg_rect = core::Rect::new(left, top, card_w, bg_h.min(card_h));
        imgproc::rectangle(frame, bg_rect, white, -1, imgproc::LINE_8, 0)?;

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

const VIDEO_EXTENSIONS: &[&str] = &["mov", "mp4", "avi", "mkv", "m4v", "webm"];

fn collect_videos(dir: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut videos = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("Cannot read directory: {}", dir))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                    videos.push(path);
                }
            }
        }
    }
    videos.sort();
    Ok(videos)
}

fn process_video(
    video_path: &std::path::Path,
    output_path: &str,
    session: &mut Session,
    class_names: &[String],
    num_classes: usize,
) -> Result<()> {
    let video_str = video_path.to_string_lossy();

    let mut cap = VideoCapture::from_file(&video_str, videoio::CAP_ANY)?;
    if !cap.is_opened()? {
        anyhow::bail!("Cannot open video: {}", video_str);
    }

    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    let width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
    let height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
    let total_frames = cap.get(videoio::CAP_PROP_FRAME_COUNT)? as u64;

    // ── SELECT LAYOUT ──
    let layout = select_layout(width as u32, height as u32);
    println!("📐 Video {}x{} → layout \"{}\" (src {}x{})",
        width, height, layout.name, layout.src_width, layout.src_height);

    let card_rects: [CardRect; NUM_CARDS] = std::array::from_fn(|i| {
        let b = &layout.cards[i];
        CardRect {
            left: (b.x * width as f64) as u32,
            top: (b.y * height as f64) as u32,
            w: (b.w * width as f64) as u32,
            h: (b.h * height as f64) as u32,
        }
    });

    let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v')?;
    let mut out = VideoWriter::new(output_path, fourcc, fps, Size::new(width, height), true)?;
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

    // ─── TELEMETRY SETUP ──────────────────────────────────────────────────────
    let telemetry_path = output_path.replace(".mp4", ".telemetry.json");
    let mut tlog = TelemetryLogger::new(&video_str, "batch", fps, total_frames, width, height);

    // --- Processing loop ---
    let mut frame = Mat::default();
    let mut frames_processed: u64 = 0;

    loop {
        // ── DECODE ──
        let t_frame = telemetry::start_timer();
        let t_decode = telemetry::start_timer();
        let ok = cap.read(&mut frame)?;
        if !ok || frame.empty() {
            if frames_processed >= total_frames.saturating_sub(1) {
                break;
            }
            frames_processed += 1;
            pb.inc(1);
            continue;
        }
        let decode_ms = elapsed_ms(&t_decode);

        let rgb_img = mat_to_rgb_image(&frame)?;

        // ── PREPROCESS ──
        let t_pre = telemetry::start_timer();
        let batch_tensor = preprocess_batch_parallel(&rgb_img, &card_rects);
        let preprocess_ms = elapsed_ms(&t_pre);

        // ── INFERENCE ──
        let t_inf = telemetry::start_timer();
        let shape = batch_tensor.shape().to_vec();
        let (flat_data, _offset) = batch_tensor.into_raw_vec_and_offset();
        let input_value =
            Tensor::from_array((shape, flat_data)).map_err(|e| anyhow::anyhow!("{e}"))?;
        let outputs = session
            .run(ort::inputs!["input" => input_value])
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let output_tensor = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let (_shape, all_logits) = output_tensor;
        let inference_ms = elapsed_ms(&t_inf);

        // ── DRAW + COLLECT SLOT DATA ──
        let t_draw = telemetry::start_timer();
        let mut slot_data = Vec::with_capacity(NUM_CARDS);
        for (i, rect) in card_rects.iter().enumerate() {
            let logits = &all_logits[i * num_classes..(i + 1) * num_classes];
            let (idx, conf) = softmax_argmax(logits);
            draw_card_overlay(&mut frame, rect, &class_names[idx], conf)?;

            slot_data.push(SlotTelemetry {
                slot: i,
                inferred: true,
                card: class_names[idx].clone(),
                confidence: conf,
                latency_ms: inference_ms / NUM_CARDS as f64,
                locked: false,
                fingerprint_delta: 0.0,
            });
        }
        let draw_ms = elapsed_ms(&t_draw);

        let total_ms = elapsed_ms(&t_frame);

        // ── LOG FRAME ──
        tlog.push(FrameTelemetry {
            frame: frames_processed,
            timestamp_ms: frames_processed as f64 / fps * 1000.0,
            mode: "batch".into(),
            total_latency_ms: total_ms,
            decode_ms,
            preprocess_ms,
            inference_ms,
            fingerprint_ms: 0.0,
            draw_ms,
            inferences_run: NUM_CARDS,
            inferences_skipped: 0,
            slots: slot_data,
        });

        out.write(&frame)?;
        frames_processed += 1;
        pb.inc(1);
    }

    pb.finish_with_message("Done!");
    println!("\n✅ Export Complete: {}", output_path);

    // ── SAVE TELEMETRY ──
    tlog.save(&telemetry_path)?;

    Ok(())
}

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

    // --- Collect and process all videos ---
    fs::create_dir_all(OUTPUT_DIR).context("Cannot create output directory")?;

    let videos = collect_videos(INPUT_DIR)?;
    if videos.is_empty() {
        println!("⚠️  No video files found in {}", INPUT_DIR);
        return Ok(());
    }

    println!("📁 Found {} video(s) in {}\n", videos.len(), INPUT_DIR);

    for (idx, video_path) in videos.iter().enumerate() {
        let stem = video_path.file_stem().unwrap().to_string_lossy();
        let output_path = format!("{}/{}_tracked.mp4", OUTPUT_DIR, stem);

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📹 [{}/{}] {}", idx + 1, videos.len(), video_path.display());
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        if let Err(e) = process_video(video_path, &output_path, &mut session, &class_names, num_classes) {
            eprintln!("❌ Error processing {}: {:#}", video_path.display(), e);
        }
        println!();
    }

    println!("🏁 All done! Output files in {}/", OUTPUT_DIR);
    Ok(())
}
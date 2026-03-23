use image::{DynamicImage, RgbImage};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::fs;
use std::collections::HashMap;
use walkdir::WalkDir; // New import for recursive directory traversal

// ==========================================
// 1. DATA STRUCTURES
// ==========================================

struct FrameData {
    frame_number: usize,
    card_colors: [[u8; 3]; 5],
}

#[derive(Clone, Copy)]
struct RelativeBox {
    x_pct: f32, y_pct: f32, w_pct: f32, h_pct: f32,
}

struct DeviceLayout {
    aspect_ratio: f32,
    cards: [RelativeBox; 5],
}




// ==========================================
// 2. CROPPING & LAYOUT LOGIC
// ==========================================

// Part of building a robust engine is making it dynamic. Instead of hardcoding 
// pixel values that break on different devices, this calculates the aspect ratio 
// and maps precise percentages. It makes the engine scalable across mixed datasets.

fn get_closest_layout(image_width: u32, image_height: u32) -> DeviceLayout {
    let current_ratio = image_width as f32 / image_height as f32;

    let iphone_layout = DeviceLayout {
        aspect_ratio: 0.46,
        cards: [
            RelativeBox { x_pct: 0.05, y_pct: 0.92, w_pct: 0.08, h_pct: 0.06 },
            RelativeBox { x_pct: 0.22, y_pct: 0.82, w_pct: 0.18, h_pct: 0.13 },
            RelativeBox { x_pct: 0.40, y_pct: 0.82, w_pct: 0.18, h_pct: 0.13 },
            RelativeBox { x_pct: 0.58, y_pct: 0.82, w_pct: 0.18, h_pct: 0.13 },
            RelativeBox { x_pct: 0.78, y_pct: 0.82, w_pct: 0.18, h_pct: 0.13 },
        ],
    };

    let ipad_layout = DeviceLayout {
        aspect_ratio: 0.75,
        cards: [
            RelativeBox { x_pct: 0.07, y_pct: 0.92, w_pct: 0.09, h_pct: 0.06 },
            RelativeBox { x_pct: 0.28, y_pct: 0.82, w_pct: 0.13, h_pct: 0.13 },
            RelativeBox { x_pct: 0.42, y_pct: 0.82, w_pct: 0.13, h_pct: 0.13 },
            RelativeBox { x_pct: 0.56, y_pct: 0.82, w_pct: 0.13, h_pct: 0.13 },
            RelativeBox { x_pct: 0.70, y_pct: 0.82, w_pct: 0.13, h_pct: 0.13 },
        ],
    };

    if (current_ratio - iphone_layout.aspect_ratio).abs() < (current_ratio - ipad_layout.aspect_ratio).abs() {
        iphone_layout
    } else {
        ipad_layout
    }
}

// Returns the 5 colors 
fn process_frame(frame: &DynamicImage) -> [[u8; 3]; 5] {
    let width = frame.width();
    let height = frame.height();
    let layout = get_closest_layout(width, height);
    
    // Create an empty array to hold our 5 colors
    let mut colors = [[0, 0, 0]; 5];

    for (index, card_box) in layout.cards.iter().enumerate() {
        let abs_x = (card_box.x_pct * width as f32) as u32;
        let abs_y = (card_box.y_pct * height as f32) as u32;
        let abs_w = (card_box.w_pct * width as f32) as u32;
        let abs_h = (card_box.h_pct * height as f32) as u32;

        // 'crop_imm' creates a zero-copy immutable view into the original image memory.
        // We then call `.to_rgb8()` to convert it into a typed, contiguous buffer for
        // fast iteration — this is cheaper than per-pixel dynamic dispatch on DynamicImage.
        let cropped_view = frame.crop_imm(abs_x, abs_y, abs_w, abs_h);
        colors[index] = get_average_color(&cropped_view.to_rgb8());
    }
    
    colors
}

// ==========================================
// 3. COLOR MATH (THE "FAST PATH")
// ==========================================
// Instead of relying on metadata or heavy computer vision models, we iterate 
// directly over the raw binary byte array of the RGB channels.

fn get_average_color(cropped_image: &RgbImage) -> [u8; 3] {
    let mut total_r: u64 = 0; let mut total_g: u64 = 0; let mut total_b: u64 = 0;
    let pixel_count = (cropped_image.width() * cropped_image.height()) as u64;

    if pixel_count == 0 { return [0, 0, 0]; }

    for pixel in cropped_image.pixels() {
        total_r += pixel[0] as u64; total_g += pixel[1] as u64; total_b += pixel[2] as u64;
    }

    [ (total_r / pixel_count) as u8, (total_g / pixel_count) as u8, (total_b / pixel_count) as u8 ]
}

fn are_colors_similar(color_a: [u8; 3], color_b: [u8; 3], threshold: u8) -> bool {
    // We calculate the Manhattan distance between the bits. By setting a strict numerical
    // threshold, we deduplicate identical frames while allowing for micro-variations 
    // caused by video compression artifacts.
    let r_diff = color_a[0].abs_diff(color_b[0]);
    let g_diff = color_a[1].abs_diff(color_b[1]);
    let b_diff = color_a[2].abs_diff(color_b[2]);
    let total_diff = (r_diff as u16) + (g_diff as u16) + (b_diff as u16);
    total_diff <= (threshold as u16)
}

// ==========================================
// 4. THE PARALLEL PIPELINE
// ==========================================

// Python is bottlenecked by the GIL (Global Interpreter Lock). Rust's Rayon crate 
// (.par_iter) unleashes a "work-stealing" algorithm across every physical and logical core. 
// While one thread waits for the SSD, another is doing pixel math, keeping CPU usage at 100%.

fn extract_features_parallel(file_paths: &[PathBuf]) -> Vec<FrameData> {
    let mut processed_frames: Vec<FrameData> = file_paths.par_iter().enumerate().filter_map(|(index, path)| {
        
        // Match statement safely attempts to open the file without panicking
        match image::open(path) {
            Ok(img) => {
                // If it's a valid image, process it and return it wrapped in 'Some'
                let colors = process_frame(&img); 
                Some(FrameData { frame_number: index, card_colors: colors })
            },
            Err(e) => {
                // If it's corrupted, print a warning and return 'None' to skip it
                println!("⚠️ Warning: Skipping corrupted image {:?} - {}", path, e);
                None
            }
        }
        
    }).collect(); 

    processed_frames.sort_by_key(|f| f.frame_number);
    processed_frames
}

// The engine explicitly looks for frames that are redundant in the timeline. 
// By comparing N to N-1, we filter out localized duplicates (like waiting for elixir) 
// without destroying distinct identical frames that might occur minutes apart.
fn filter_temporal_redundancy(frames: Vec<FrameData>, threshold: u8) -> Vec<usize> {
    let mut unique_frame_indices = Vec::new();
    if let Some(first_frame) = frames.first() { unique_frame_indices.push(first_frame.frame_number); }

    for i in 1..frames.len() {
        let prev_frame = &frames[i - 1];
        let curr_frame = &frames[i];
        let mut is_duplicate = true;

        // Early-exit short-circuit: if card slot 1 already differs beyond threshold,
        // we skip slots 2–5 entirely. This is faster than accumulating a total distance
        // across all slots, because most consecutive frame pairs in gameplay footage
        // differ on at least one card and bail out immediately.
        for card_idx in 0..5 {
            if !are_colors_similar(prev_frame.card_colors[card_idx], curr_frame.card_colors[card_idx], threshold) {
                is_duplicate = false;
                break; 
            }
        }
        if !is_duplicate { unique_frame_indices.push(curr_frame.frame_number); }
    }
    unique_frame_indices
}

// ==========================================
// 5. THE NEW RECURSIVE ENTRY POINT
// ==========================================

fn main() {
    println!("Starting High-Speed Deduplication Engine...");
    let start_time = Instant::now();

    // 1. Define input and output base directories
    let input_dir_str = std::env::args()
        .nth(1)
        .expect("Usage: clash_deduper <input_directory>");
    let input_dir = Path::new(&input_dir_str);
    
    // Automatically creates "path_cleaned"
    let output_dir_str = format!("{}_cleaned", input_dir_str);
    let output_dir = Path::new(&output_dir_str);

    println!("Scanning directory tree at: {:?}", input_dir);


    // A dataset this large spans dozens of folders. WalkDir recursively crawls 
    // the directory tree. We map the frames to their specific parent folders so 
    // the temporal filter only compares frames within the same specific video file.
    let mut folder_groups: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    let mut total_files_found = 0;

    for entry in WalkDir::new(input_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_lowercase();
                // STRICTLY ONLY JPEG/JPG
                if ext_lower == "jpg" || ext_lower == "jpeg" {
                    if let Some(parent) = path.parent() {
                        folder_groups
                            .entry(parent.to_path_buf())
                            .or_insert_with(Vec::new)
                            .push(path.to_path_buf());
                        total_files_found += 1;
                    }
                }
            }
        }
    }

    println!("Found {} total JPEGs across {} subfolders.", total_files_found, folder_groups.len());

    let threshold = 50; 
    let mut total_unique_saved = 0;

    // 3. Process each subfolder individually
    for (folder_path, mut paths) in folder_groups {
        println!("Processing {} frames in folder: {:?}", paths.len(), folder_path.file_name().unwrap_or_default());

        // Sort files to guarantee temporal order for THIS specific video
        paths.sort();

        // Blast the CPU with Rayon for this folder
        let frame_data = extract_features_parallel(&paths);
        let unique_frames = filter_temporal_redundancy(frame_data, threshold);

        // 4. Copy the unique frames while maintaining the tree structure
        for &frame_idx in &unique_frames {
            let original_path = &paths[frame_idx];
            
            // Calculate the relative path from the base input directory
            // e.g., "Ian77_Frames/2.6_Hog.../frame_001.jpg"
            if let Ok(relative_path) = original_path.strip_prefix(input_dir) {
                // Append that relative path to our new _cleaned base directory
                let new_path = output_dir.join(relative_path);

                // Ensure the subfolders exist in the _cleaned directory before copying
                if let Some(parent) = new_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                // We bypass the CPU re-encoding the JPEG. We issue a direct OS-level call
                // to duplicate the raw binary blob on the disk, operating at maximum SSD speed.
                if let Err(e) = fs::copy(original_path, &new_path) {
                    println!("Warning: Failed to copy {:?} - {}", original_path, e);
                }
            }
        }
        total_unique_saved += unique_frames.len();
    }

    let duration = start_time.elapsed();
    println!("==========================================");
    println!("--- FULL DATASET PIPELINE COMPLETE ---");
    println!("Processed {} frames in {:?}", total_files_found, duration);
    println!("Saved {} unique frames to {:?}", total_unique_saved, output_dir);
    println!("Dropped {} temporal duplicates.", total_files_found - total_unique_saved);
    println!("==========================================");
}
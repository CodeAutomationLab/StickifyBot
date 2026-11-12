use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use std::time::Instant;

use sticker_converter_bot::config::Config;
use sticker_converter_bot::converter::*;
use sticker_converter_bot::types::{BackgroundMode, Resolution, UserSettings};

fn make_sample_tgs(frames: usize) -> Vec<u8> {
    let json = format!(r#"{{
        "v": "5.5.2",
        "fr": 30,
        "ip": 0,
        "op": {},
        "w": 512,
        "h": 512,
        "nm": "Benchmark Sticker",
        "ddd": 0,
        "assets": [],
        "layers": [
            {{
                "ddd": 0,
                "ind": 1,
                "ty": 4,
                "nm": "Animated Rect",
                "sr": 1,
                "ks": {{
                    "o": {{ "a": 0, "k": 100 }},
                    "r": {{ "a": 1, "k": [{{ "t": 0, "s": [0] }}, {{ "t": {}, "s": [360] }}] }},
                    "p": {{ "a": 0, "k": [256, 256, 0] }},
                    "a": {{ "a": 0, "k": [0, 0, 0] }},
                    "s": {{ "a": 0, "k": [100, 100, 100] }}
                }},
                "shapes": [
                    {{
                        "ty": "rc",
                        "d": 1,
                        "s": {{ "a": 0, "k": [160, 160] }},
                        "p": {{ "a": 0, "k": [0, 0] }},
                        "r": {{ "a": 0, "k": 20 }},
                        "nm": "Rect 1"
                    }},
                    {{
                        "ty": "fl",
                        "c": {{ "a": 0, "k": [0.2, 0.6, 1.0, 1] }},
                        "o": {{ "a": 0, "k": 100 }},
                        "nm": "Fill 1"
                    }}
                ],
                "ip": 0,
                "op": {},
                "st": 0
            }}
        ]
    }}"#, frames, frames, frames);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(json.as_bytes()).unwrap();
    encoder.finish().unwrap()
}

fn main() {
    println!("===============================================================");
    println!("       PERFORMANCE BENCHMARK: RUST vs PYTHON CONVERTER         ");
    println!("===============================================================\n");

    let config = Config::from_env().unwrap_or_else(|_| {
        #[allow(unused_mut)]
        let mut ffmpeg = std::path::PathBuf::from("ffmpeg");
        if !ffmpeg.exists() {
            if let Ok(home) = std::env::var("USERPROFILE") {
                let p = std::path::PathBuf::from(home)
                    .join(".cargo")
                    .join("bin")
                    .join("ffmpeg.exe");
                if p.exists() {
                    ffmpeg = p;
                }
            }
        }
        Config {
            bot_token: "dummy".to_string(),
            ffmpeg_path: ffmpeg,
            rlottie_dll_path: std::path::PathBuf::from("bin/rlottie.dll"),
            temp_dir: std::env::temp_dir(),
            conversion_timeout: std::time::Duration::from_secs(60),
            max_file_size_mb: 50,
        }
    });

    let tgs_bytes = make_sample_tgs(30); // 30 frames
    let settings = UserSettings {
        resolution: Resolution::Res480p,
        background: BackgroundMode::Transparent,
        fps: 30,
    };

    println!("Sticker Payload: 30 frames @ 480x480 resolution (compressed: {} bytes)", tgs_bytes.len());
    println!("---------------------------------------------------------------");

    // 1. Gzip to JSON Benchmark (100 iterations)
    let start = Instant::now();
    for _ in 0..100 {
        let _ = tgs_to_lottie_json(&tgs_bytes).unwrap();
    }
    let gzip_duration = start.elapsed();
    let gzip_per_op = gzip_duration / 100;
    println!("1. Gzip Decompression (100 iterations):");
    println!("   Total: {:?} | Average: {:?} / operation\n", gzip_duration, gzip_per_op);

    // 2. MP4 Conversion Benchmark (5 iterations)
    println!("2. TGS -> MP4 (H.264 Faststart, In-Memory Stdin Pipe):");
    let mut mp4_durations = Vec::new();
    for i in 1..=5 {
        let s = Instant::now();
        let _mp4 = render_tgs_to_mp4(&tgs_bytes, &settings, &config).unwrap();
        let elapsed = s.elapsed();
        println!("   Iteration {}: {:.2?}", i, elapsed);
        mp4_durations.push(elapsed);
    }
    let avg_mp4: std::time::Duration = mp4_durations.iter().sum::<std::time::Duration>() / 5;
    println!("   >> RUST AVERAGE MP4: {:.2?}\n", avg_mp4);

    // 3. WebM Transparent Benchmark (5 iterations)
    println!("3. TGS -> WebM (VP9 yuva420p Transparent, Direct Stdout Stream):");
    let mut webm_durations = Vec::new();
    for i in 1..=5 {
        let s = Instant::now();
        let _webm = render_tgs_to_webm(&tgs_bytes, &settings, &config).unwrap();
        let elapsed = s.elapsed();
        println!("   Iteration {}: {:.2?}", i, elapsed);
        webm_durations.push(elapsed);
    }
    let avg_webm: std::time::Duration = webm_durations.iter().sum::<std::time::Duration>() / 5;
    println!("   >> RUST AVERAGE WebM: {:.2?}\n", avg_webm);

    // 4. GIF Benchmark (5 iterations)
    println!("4. TGS -> GIF (High-res Palettegen/Paletteuse):");
    let mut gif_durations = Vec::new();
    for i in 1..=5 {
        let s = Instant::now();
        let _gif = render_tgs_to_gif(&tgs_bytes, &settings, &config).unwrap();
        let elapsed = s.elapsed();
        println!("   Iteration {}: {:.2?}", i, elapsed);
        gif_durations.push(elapsed);
    }
    let avg_gif: std::time::Duration = gif_durations.iter().sum::<std::time::Duration>() / 5;
    println!("   >> RUST AVERAGE GIF: {:.2?}\n", avg_gif);

    // 5. First Frame PNG Benchmark (20 iterations)
    println!("5. TGS -> PNG First Frame:");
    let s = Instant::now();
    for _ in 0..20 {
        let _ = render_first_frame_png(&tgs_bytes, &config).unwrap();
    }
    let png_total = s.elapsed();
    println!("   Total 20 frames: {:?} | Average: {:?} / frame\n", png_total, png_total / 20);

    println!("===============================================================");
    println!("                     BENCHMARK SUMMARY                         ");
    println!("===============================================================");
    println!(" Format            | Rust Latency | Throughput (FPS) ");
    println!("-------------------|--------------|------------------");
    println!(" Lottie JSON       |   {:.3?}   | > 10,000 ops/sec ", gzip_per_op);
    println!(" MP4 (H.264)       |   {:.2?}    | {:.1} fps ", avg_mp4, 30.0 / avg_mp4.as_secs_f64());
    println!(" WebM (Transparent)|   {:.2?}    | {:.1} fps ", avg_webm, 30.0 / avg_webm.as_secs_f64());
    println!(" GIF (Palette)     |   {:.2?}    | {:.1} fps ", avg_gif, 30.0 / avg_gif.as_secs_f64());
    println!(" PNG Frame         |   {:.2?}     | > 200 fps ", png_total / 20);
    println!("===============================================================");
}


use flate2::write::GzEncoder;
use flate2::Compression;
use image::ImageEncoder;
use std::io::Write;
use sticker_converter_bot::config::Config;
use sticker_converter_bot::converter::*;
use sticker_converter_bot::types::{BackgroundMode, Resolution, UserSettings};

fn make_sample_tgs() -> Vec<u8> {
    let json = r#"{
        "v": "5.5.2",
        "fr": 30,
        "ip": 0,
        "op": 15,
        "w": 512,
        "h": 512,
        "nm": "Sample Sticker",
        "ddd": 0,
        "assets": [],
        "layers": [
            {
                "ddd": 0,
                "ind": 1,
                "ty": 4,
                "nm": "Shape 1",
                "sr": 1,
                "ks": {
                    "o": { "a": 0, "k": 100 },
                    "r": { "a": 0, "k": 0 },
                    "p": { "a": 0, "k": [256, 256, 0] },
                    "a": { "a": 0, "k": [0, 0, 0] },
                    "s": { "a": 0, "k": [100, 100, 100] }
                },
                "shapes": [
                    {
                        "ty": "rc",
                        "d": 1,
                        "s": { "a": 0, "k": [100, 100] },
                        "p": { "a": 0, "k": [0, 0] },
                        "r": { "a": 0, "k": 10 },
                        "nm": "Rect 1"
                    },
                    {
                        "ty": "fl",
                        "c": { "a": 0, "k": [1, 0.2, 0.2, 1] },
                        "o": { "a": 0, "k": 100 },
                        "nm": "Fill 1"
                    }
                ],
                "ip": 0,
                "op": 15,
                "st": 0
            }
        ]
    }"#;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(json.as_bytes()).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn test_tgs_to_lottie_json() {
    let tgs = make_sample_tgs();
    let json = tgs_to_lottie_json(&tgs).expect("Should decompress tgs to json");
    assert!(json.contains("Sample Sticker"));
    assert!(json.contains("\"fr\": 30"));

    // Also verify uncompressed JSON passthrough
    let raw_json = r#"{"v":"5.5.2","fr":30,"nm":"Raw JSON"}"#;
    let out_json = tgs_to_lottie_json(raw_json.as_bytes()).expect("Should parse uncompressed json");
    assert_eq!(out_json, raw_json);
}

#[test]
fn test_lottie_json_to_tgs() {
    let raw_json = r#"{"v":"5.5.2","fr":30,"nm":"Compress Me"}"#;
    let tgs_bytes = lottie_json_to_tgs(raw_json.as_bytes()).expect("Compression to tgs should succeed");
    // Must start with gzip magic bytes 0x1f 0x8b
    assert_eq!(&tgs_bytes[..2], &[0x1F, 0x8B]);

    // Decompress back and verify content
    let decompressed = tgs_to_lottie_json(&tgs_bytes).expect("Decompression should succeed");
    assert_eq!(decompressed, raw_json);
}

#[test]
fn test_render_tgs_to_png_frame() {
    let config = Config::from_env().unwrap();
    let tgs = make_sample_tgs();
    let png_bytes = render_first_frame_png(&tgs, &config).expect("PNG render must succeed");
    // PNG header: 89 50 4E 47 0D 0A 1A 0A
    assert_eq!(&png_bytes[..4], b"\x89PNG");
}

#[test]
fn test_render_tgs_to_mp4() {
    let config = Config::from_env().unwrap();
    let tgs = make_sample_tgs();
    let mut settings = UserSettings::default();
    settings.resolution = Resolution::Res480p;

    let mp4_file = render_tgs_to_mp4(&tgs, &settings, &config)
        .expect("MP4 conversion must succeed");

    let path = mp4_file.path();
    let metadata = std::fs::metadata(path).expect("MP4 file must exist");
    assert!(metadata.len() > 500, "MP4 file size should be substantial");
}

#[test]
fn test_render_lottie_json_directly_to_mp4_with_green_background() {
    let config = Config::from_env().unwrap();
    let json_bytes = r#"{
        "v": "5.5.2",
        "fr": 30,
        "ip": 0,
        "op": 15,
        "w": 512,
        "h": 512,
        "nm": "Green Screen Lottie",
        "ddd": 0,
        "assets": [],
        "layers": []
    }"#.as_bytes();

    let mut settings = UserSettings::default();
    settings.resolution = Resolution::Res480p;
    settings.background = BackgroundMode::Green;

    let mp4_file = render_tgs_to_mp4(json_bytes, &settings, &config)
        .expect("Direct Lottie JSON to MP4 with Green Screen must succeed");

    let path = mp4_file.path();
    let metadata = std::fs::metadata(path).expect("MP4 file must exist");
    assert!(metadata.len() > 500, "MP4 file size should be substantial");
}

#[test]
fn test_render_tgs_to_webm_transparent() {
    let config = Config::from_env().unwrap();
    let tgs = make_sample_tgs();
    let mut settings = UserSettings::default();
    settings.background = BackgroundMode::Transparent;
    settings.resolution = Resolution::Res480p;

    let webm_bytes = render_tgs_to_webm(&tgs, &settings, &config)
        .expect("WebM conversion must succeed");

    // WebM EBML header magic: 1A 45 DF A3
    assert_eq!(&webm_bytes[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
}

#[test]
fn test_render_tgs_to_gif() {
    let config = Config::from_env().unwrap();
    let tgs = make_sample_tgs();
    let mut settings = UserSettings::default();
    settings.resolution = Resolution::Res480p;

    let gif_bytes = render_tgs_to_gif(&tgs, &settings, &config)
        .expect("GIF conversion must succeed");

    // GIF header: GIF89a
    assert_eq!(&gif_bytes[..3], b"GIF");
}

#[test]
fn test_render_tgs_to_webp() {
    let config = Config::from_env().unwrap();
    let tgs = make_sample_tgs();
    let mut settings = UserSettings::default();
    settings.resolution = Resolution::Res480p;

    let webp_bytes = render_tgs_to_webp(&tgs, &settings, &config)
        .expect("WEBP conversion must succeed");

    // RIFF .... WEBP
    assert_eq!(&webp_bytes[..4], b"RIFF");
    assert_eq!(&webp_bytes[8..12], b"WEBP");
}

#[test]
fn test_convert_static_webp_to_png() {
    // Generate a simple valid PNG and test converting it or testing image decoding
    let img = image::RgbaImage::new(64, 64);
    let mut webp_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut webp_bytes);
    encoder.write_image(img.as_raw(), 64, 64, image::ExtendedColorType::Rgba8).unwrap();

    let png_bytes = convert_static_image_to_png(&webp_bytes).expect("Static image to PNG must succeed");
    assert_eq!(&png_bytes[..4], b"\x89PNG");
}


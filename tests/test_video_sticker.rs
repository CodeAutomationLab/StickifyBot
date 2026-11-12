use std::io::Write as _;
use std::process::Command;
use sticker_converter_bot::config::Config;
use sticker_converter_bot::converter::*;
use sticker_converter_bot::types::{BackgroundMode, Resolution, StickerType, UserSettings};

/// Create a small VP9 webm sample with varying alpha (transparent circle on a
/// semi-transparent background), mirroring the format Telegram uses for video stickers.
fn make_sample_webm(config: &Config) -> Vec<u8> {
    let out_path = std::env::temp_dir().join("sticker_bot_test_input.webm");
    let status = Command::new(&config.ffmpeg_path)
        .args([
            "-y",
            "-loglevel", "error",
            "-f", "lavfi",
            "-i", "testsrc2=size=128x128:rate=30:duration=1,format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(lt(hypot(X-64,Y-64),40),255,64)'",
            "-c:v", "libvpx-vp9",
            "-pix_fmt", "yuva420p",
            "-deadline", "realtime",
            "-cpu-used", "8",
            &out_path.to_string_lossy(),
        ])
        .status()
        .expect("ffmpeg must be available to generate the sample webm");
    assert!(status.success(), "failed to generate the sample webm");
    std::fs::read(&out_path).expect("sample webm must exist")
}

fn settings_for(background: BackgroundMode) -> UserSettings {
    UserSettings {
        resolution: Resolution::Res480p,
        background,
        fps: 30,
    }
}

#[test]
fn test_video_sticker_to_mp4() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);
    let settings = settings_for(BackgroundMode::Dark);

    let mp4_file = convert_video_sticker_to_mp4(&webm, &settings, &config)
        .expect("video sticker -> MP4 must succeed");

    let bytes = std::fs::read(mp4_file.path()).expect("mp4 must exist");
    assert!(bytes.len() > 500, "mp4 output should be substantial, got {} bytes", bytes.len());
    // MP4 starts with a box header then "ftyp"
    assert_eq!(&bytes[4..8], b"ftyp", "output must be an MP4 container");
}

#[test]
fn test_video_sticker_to_webm_transparent() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);
    let settings = settings_for(BackgroundMode::Transparent);

    let out = convert_video_sticker_to_webm(&webm, &settings, &config)
        .expect("video sticker -> WebM (transparent) must succeed");

    assert_eq!(&out[..4], &[0x1A, 0x45, 0xDF, 0xA3], "output must be a WebM container");
    assert!(out.len() > 200, "webm output should be substantial");
}

#[test]
fn test_video_sticker_to_webm_dark() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);
    let settings = settings_for(BackgroundMode::Dark);

    let out = convert_video_sticker_to_webm(&webm, &settings, &config)
        .expect("video sticker -> WebM (dark) must succeed");

    assert_eq!(&out[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
}

#[test]
fn test_video_sticker_to_gif() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);
    let settings = settings_for(BackgroundMode::Dark);

    let gif = convert_video_sticker_to_gif(&webm, &settings, &config)
        .expect("video sticker -> GIF must succeed (two-pass palette must be valid)");

    assert_eq!(&gif[..3], b"GIF", "output must be a GIF");
}

#[test]
fn test_video_sticker_to_gif_transparent() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);
    let settings = settings_for(BackgroundMode::Transparent);

    let gif = convert_video_sticker_to_gif(&webm, &settings, &config)
        .expect("video sticker -> GIF (transparent) must succeed");

    assert_eq!(&gif[..3], b"GIF");
}

#[test]
fn test_video_sticker_to_webp() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);
    let settings = settings_for(BackgroundMode::Dark);

    let webp = convert_video_sticker_to_webp(&webm, &settings, &config)
        .expect("video sticker -> WEBP must succeed");

    assert_eq!(&webp[..4], b"RIFF");
    assert_eq!(&webp[8..12], b"WEBP");
}

#[test]
fn test_video_sticker_to_webp_transparent_keeps_alpha() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);
    let settings = settings_for(BackgroundMode::Transparent);

    let webp = convert_video_sticker_to_webp(&webm, &settings, &config)
        .expect("video sticker -> WEBP (transparent) must succeed");

    assert_eq!(&webp[..4], b"RIFF");
    assert_eq!(&webp[8..12], b"WEBP");
    // Animated WebP with alpha uses the ALPH chunk or a VP8L frame; ensure the
    // output is not just an opaque lossy VP8-only file.
    let has_alpha = {
        let head = &webp[..webp.len().min(4096)];
        head.windows(4).any(|w| w == b"ALPH") || head.windows(4).any(|w| w == b"VP8L")
    };
    assert!(has_alpha, "transparent WEBP must carry an alpha channel (ALPH/VP8L chunk)");
}

#[test]
fn test_video_sticker_first_frame_png() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);

    let png = convert_video_sticker_first_frame_png(&webm, &config)
        .expect("video sticker -> first frame PNG must succeed");

    assert_eq!(&png[..4], b"\x89PNG");
}

#[test]
fn test_render_sticker_webm_within_telegram_limits() {
    let config = Config::from_env().unwrap();
    let webm = make_sample_webm(&config);

    let sticker = render_sticker_webm(&webm, StickerType::Video, &config)
        .expect("sticker-compliant WebM must succeed");

    assert_eq!(&sticker[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
    assert!(
        sticker.len() <= 256 * 1024,
        "sticker output must respect Telegram's 256 KB limit, got {} bytes",
        sticker.len()
    );
}

#[test]
fn test_render_tgs_sticker_webm_within_telegram_limits() {
    // Reuse the tiny lottie sample from the converter tests.
    let config = Config::from_env().unwrap();
    let json = r#"{
        "v": "5.5.2", "fr": 30, "ip": 0, "op": 15, "w": 512, "h": 512,
        "nm": "Sample", "ddd": 0, "assets": [],
        "layers": [{
            "ddd": 0, "ind": 1, "ty": 4, "nm": "Shape 1", "sr": 1,
            "ks": {
                "o": { "a": 0, "k": 100 },
                "r": { "a": 0, "k": 0 },
                "p": { "a": 0, "k": [256, 256, 0] },
                "a": { "a": 0, "k": [0, 0, 0] },
                "s": { "a": 0, "k": [100, 100, 100] }
            },
            "shapes": [{
                "ty": "rc", "d": 1,
                "s": { "a": 0, "k": [300, 300] },
                "p": { "a": 0, "k": [0, 0] },
                "r": { "a": 0, "k": 20 },
                "nm": "Rect 1"
            }, {
                "ty": "fl",
                "c": { "a": 0, "k": [1, 0.5, 0, 1] },
                "o": { "a": 0, "k": 100 },
                "nm": "Fill 1"
            }],
            "ip": 0, "op": 15, "st": 0
        }]
    }"#;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(json.as_bytes()).unwrap();
    let tgs = encoder.finish().unwrap();

    let sticker = render_sticker_webm(&tgs, StickerType::Animated, &config)
        .expect("sticker-compliant WebM from TGS must succeed");

    assert_eq!(&sticker[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
    assert!(sticker.len() <= 256 * 1024);
}

#[test]
fn test_unpremultiply_bgra_roundtrip() {
    // Valid premultiplied BGRA (values <= alpha): B=32, G=64, R=100, A=128
    let prem: [u32; 1] = [(32u32) | (64u32 << 8) | (100u32 << 16) | (128u32 << 24)];
    let mut out = vec![0u8; 4];
    unpremultiply_bgra(&prem, &mut out);

    // straight = prem * 255 / alpha, rounded
    let expected_b = ((32u32 * 255 + 64) / 128) as u8;
    let expected_g = ((64u32 * 255 + 64) / 128) as u8;
    let expected_r = ((100u32 * 255 + 64) / 128) as u8;
    assert_eq!(out[0], expected_b);
    assert_eq!(out[1], expected_g);
    assert_eq!(out[2], expected_r);
    assert_eq!(out[3], 128);
}

#[test]
fn test_blend_premultiplied_bgra_to_bgra_is_opaque() {
    // Fully transparent pixel must become exactly the background color
    let prem: [u32; 1] = [0u32];
    let mut out = vec![0u8; 4];
    blend_premultiplied_bgra_to_bgra(&prem, &mut out, 10, 20, 30);
    assert_eq!(out[0], 30); // B
    assert_eq!(out[1], 20); // G
    assert_eq!(out[2], 10); // R
    assert_eq!(out[3], 255); // fully opaque
}

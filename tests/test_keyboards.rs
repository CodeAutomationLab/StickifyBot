use sticker_converter_bot::keyboards::*;
use sticker_converter_bot::types::{BackgroundMode, Resolution, StickerType, UserSettings};

#[test]
fn test_main_keyboard_layout() {
    let token = "a1b2c3d4";
    let bg = BackgroundMode::Transparent;
    let kb = build_main_keyboard(token, StickerType::Animated, &bg);

    // 4 rows
    assert_eq!(kb.inline_keyboard.len(), 4);
    // Row 1: 2 buttons (MP4, WebM)
    assert_eq!(kb.inline_keyboard[0].len(), 2);
    // Row 2: 2 buttons (WEBP, GIF)
    assert_eq!(kb.inline_keyboard[1].len(), 2);
    // Row 3: 2 buttons (Settings, More Formats)
    assert_eq!(kb.inline_keyboard[2].len(), 2);
    // Row 4: 1 button (Original File)
    assert_eq!(kb.inline_keyboard[3].len(), 1);

    // Check callback data lengths (Telegram limit is 64 bytes)
    for row in &kb.inline_keyboard {
        for btn in row {
            if let teloxide::types::InlineKeyboardButtonKind::CallbackData(data) = &btn.kind {
                assert!(data.len() <= 64, "Callback data exceeds 64 bytes: {}", data);
            }
        }
    }
}

#[test]
fn test_lottie_json_main_keyboard_layout() {
    let token = "a1b2c3d4";
    let bg = BackgroundMode::Dark;
    let kb = build_main_keyboard(token, StickerType::LottieJson, &bg);

    // 4 rows
    assert_eq!(kb.inline_keyboard.len(), 4);
    // Row 1: MP4, GIF
    assert_eq!(kb.inline_keyboard[0][0].text, "🎬 MP4 Video");
    assert_eq!(kb.inline_keyboard[0][1].text, "🎞 GIF");
    // Row 2: Telegram Sticker, WebM
    assert_eq!(kb.inline_keyboard[1][0].text, "🌟 Telegram Sticker");
    // Row 3: .TGS Sticker File, WEBP
    assert_eq!(kb.inline_keyboard[2][0].text, "📦 .TGS Sticker File");
    assert_eq!(kb.inline_keyboard[2][1].text, "🖼 WEBP");
    // Row 4: Settings, Original JSON
    assert_eq!(kb.inline_keyboard[3][0].text, "⚙️ Settings");
    assert_eq!(kb.inline_keyboard[3][1].text, "📁 Original JSON");
}

#[test]
fn test_background_keyboard_includes_green() {
    let token = "a1b2c3d4";
    let kb = build_background_keyboard(token, &BackgroundMode::Green, StickerType::LottieJson);
    // 5 rows: Transparent, Dark, Green, White, Back
    assert_eq!(kb.inline_keyboard.len(), 5);
    // Check that Green has checkmark
    assert_eq!(kb.inline_keyboard[2][0].text, "✅ 🟩 Green Screen");
}

#[test]
fn test_settings_keyboard_layout() {
    let token = "a1b2c3d4";
    let settings = UserSettings::default();
    let kb = build_settings_keyboard(token, &settings, StickerType::Animated);

    // 4 rows: Resolution, Background, FPS, Back
    assert_eq!(kb.inline_keyboard.len(), 4);
}

#[test]
fn test_more_formats_keyboard() {
    let token = "a1b2c3d4";
    let kb_anim = build_more_formats_keyboard(token, StickerType::Animated);
    // Row 1: Lottie JSON, PNG Frame; Row 2: Back
    assert_eq!(kb_anim.inline_keyboard.len(), 2);

    let kb_vid = build_more_formats_keyboard(token, StickerType::Video);
    // Video doesn't have Lottie JSON: Row 1: PNG Frame; Row 2: Back
    assert_eq!(kb_vid.inline_keyboard.len(), 2);
}

#[test]
fn test_fps_keyboard() {
    let token = "a1b2c3d4";
    let kb = build_fps_keyboard(token, 0);
    // Row 1: Auto, 30, 60; Row 2: Back
    assert_eq!(kb.inline_keyboard.len(), 2);
    assert_eq!(kb.inline_keyboard[0].len(), 3);
}

#[test]
fn test_status_caption_formatting() {
    let settings = UserSettings {
        resolution: Resolution::Native,
        background: BackgroundMode::Dark,
        fps: 30,
    };
    let caption = format_status_caption(&settings, "MP4", Some(1024 * 329), Some(2.4));
    assert!(caption.contains("✅ <b>Conversion completed</b> (⏱️ 2.4s)"));
    assert!(caption.contains("Format: <b>MP4</b>"));
    assert!(caption.contains("Resolution: <b>512px</b>"));
    assert!(caption.contains("Background: <b>Dark / Black</b>"));
    assert!(caption.contains("Size: <b>329 KB</b>"));
    assert!(caption.contains("Processed in: <b>2.4s</b>"));
}

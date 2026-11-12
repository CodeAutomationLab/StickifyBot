use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use crate::types::{BackgroundMode, Resolution, StickerType, UserSettings};

/// Build the compact 4-row main conversion keyboard
pub fn build_main_keyboard(
    token: &str,
    sticker_type: StickerType,
    bg: &BackgroundMode,
) -> InlineKeyboardMarkup {
    let webm_label = if matches!(bg, BackgroundMode::Transparent) {
        "🌐 WebM (Transparent)"
    } else {
        "🌐 WebM"
    };

    let row1 = vec![
        InlineKeyboardButton::callback("🎬 MP4 Video", format!("fmt:mp4:{}", token)),
        InlineKeyboardButton::callback(webm_label, format!("fmt:webm:{}", token)),
    ];

    let row2 = vec![
        InlineKeyboardButton::callback("🖼 WEBP", format!("fmt:webp:{}", token)),
        InlineKeyboardButton::callback("🎞 GIF", format!("fmt:gif:{}", token)),
    ];

    let row3 = vec![
        InlineKeyboardButton::callback("⚙️ Settings", format!("menu:settings:{}", token)),
        InlineKeyboardButton::callback("📦 More Formats", format!("menu:more:{}", token)),
    ];

    let row4 = vec![
        InlineKeyboardButton::callback("📁 Original File", format!("fmt:orig:{}", token)),
    ];

    let mut rows = vec![row1, row2, row3, row4];

    // If static sticker, remove video-only options
    if matches!(sticker_type, StickerType::Static) {
        rows = vec![
            vec![
                InlineKeyboardButton::callback("🖼 WEBP", format!("fmt:webp:{}", token)),
                InlineKeyboardButton::callback("📸 PNG Frame", format!("fmt:png:{}", token)),
            ],
            vec![
                InlineKeyboardButton::callback("⚙️ Settings", format!("menu:settings:{}", token)),
                InlineKeyboardButton::callback("📁 Original File", format!("fmt:orig:{}", token)),
            ],
        ];
    } else if matches!(sticker_type, StickerType::LottieJson) {
        rows = vec![
            vec![
                InlineKeyboardButton::callback("🎬 MP4 Video", format!("fmt:mp4:{}", token)),
                InlineKeyboardButton::callback("🎞 GIF", format!("fmt:gif:{}", token)),
            ],
            vec![
                InlineKeyboardButton::callback("🌟 Telegram Sticker", format!("fmt:sticker:{}", token)),
                InlineKeyboardButton::callback(webm_label, format!("fmt:webm:{}", token)),
            ],
            vec![
                InlineKeyboardButton::callback("📦 .TGS Sticker File", format!("fmt:tgs:{}", token)),
                InlineKeyboardButton::callback("🖼 WEBP", format!("fmt:webp:{}", token)),
            ],
            vec![
                InlineKeyboardButton::callback("⚙️ Settings", format!("menu:settings:{}", token)),
                InlineKeyboardButton::callback("📁 Original JSON", format!("fmt:orig:{}", token)),
            ],
        ];
    }

    InlineKeyboardMarkup::new(rows)
}

/// Build the dedicated Settings menu
pub fn build_settings_keyboard(
    token: &str,
    settings: &UserSettings,
    _sticker_type: StickerType,
) -> InlineKeyboardMarkup {
    let row_res = vec![InlineKeyboardButton::callback(
        format!("📐 Resolution ({})", settings.resolution.label()),
        format!("menu:res:{}", token),
    )];

    let row_bg = vec![InlineKeyboardButton::callback(
        format!("🎨 Background ({})", settings.background.label()),
        format!("menu:bg:{}", token),
    )];

    let fps_label = if settings.fps == 0 {
        "Auto (Native)".to_string()
    } else {
        format!("{} fps", settings.fps)
    };
    let row_fps = vec![InlineKeyboardButton::callback(
        format!("⚡ FPS ({})", fps_label),
        format!("menu:fps:{}", token),
    )];

    let row_back = vec![InlineKeyboardButton::callback(
        "◀️ Back to Main Menu",
        format!("menu:main:{}", token),
    )];

    InlineKeyboardMarkup::new(vec![row_res, row_bg, row_fps, row_back])
}

/// Build the Resolution selection submenu
pub fn build_resolution_keyboard(token: &str, current: Resolution) -> InlineKeyboardMarkup {
    let resolutions = [
        (Resolution::Native, "512", "512px (Native)"),
        (Resolution::Res480p, "480", "480p"),
        (Resolution::Res720p, "720", "720p HD"),
        (Resolution::Res1080p, "1080", "1080p Full HD"),
        (Resolution::Res1440p, "1440", "1440p 2K"),
        (Resolution::Res4K, "2160", "2160p 4K UHD"),
    ];

    let mut rows = Vec::new();
    for chunk in resolutions.chunks(2) {
        let mut row = Vec::new();
        for (res, code, label) in chunk {
            let text = if *res == current {
                format!("✅ {}", label)
            } else {
                label.to_string()
            };
            row.push(InlineKeyboardButton::callback(
                text,
                format!("set:res:{}:{}", code, token),
            ));
        }
        rows.push(row);
    }

    rows.push(vec![InlineKeyboardButton::callback(
        "◀️ Back to Settings",
        format!("menu:settings:{}", token),
    )]);

    InlineKeyboardMarkup::new(rows)
}

/// Build the Background selection submenu
pub fn build_background_keyboard(
    token: &str,
    current: &BackgroundMode,
    _sticker_type: StickerType,
) -> InlineKeyboardMarkup {
    let backgrounds = [
        (BackgroundMode::Transparent, "transparent", "✨ Transparent"),
        (BackgroundMode::Dark, "dark", "⬛ Dark / Black"),
        (BackgroundMode::Green, "green", "🟩 Green Screen"),
        (BackgroundMode::White, "white", "⬜ Light / White"),
    ];

    let mut rows = Vec::new();
    for (bg, code, label) in &backgrounds {
        let text = if bg == current {
            format!("✅ {}", label)
        } else {
            label.to_string()
        };
        rows.push(vec![InlineKeyboardButton::callback(
            text,
            format!("set:bg:{}:{}", code, token),
        )]);
    }

    rows.push(vec![InlineKeyboardButton::callback(
        "◀️ Back to Settings",
        format!("menu:settings:{}", token),
    )]);

    InlineKeyboardMarkup::new(rows)
}

/// Build the FPS selection submenu
pub fn build_fps_keyboard(token: &str, current_fps: u32) -> InlineKeyboardMarkup {
    let row_fps = vec![
        InlineKeyboardButton::callback(
            if current_fps == 0 { "✅ Auto (Native)" } else { "Auto (Native)" },
            format!("set:fps:0:{}", token),
        ),
        InlineKeyboardButton::callback(
            if current_fps == 30 { "✅ 30 FPS" } else { "30 FPS" },
            format!("set:fps:30:{}", token),
        ),
        InlineKeyboardButton::callback(
            if current_fps == 60 { "✅ 60 FPS" } else { "60 FPS" },
            format!("set:fps:60:{}", token),
        ),
    ];

    let row_back = vec![InlineKeyboardButton::callback(
        "◀️ Back to Settings",
        format!("menu:settings:{}", token),
    )];

    InlineKeyboardMarkup::new(vec![row_fps, row_back])
}

/// Build More Formats submenu
pub fn build_more_formats_keyboard(token: &str, sticker_type: StickerType) -> InlineKeyboardMarkup {
    let mut row_formats = Vec::new();

    if matches!(sticker_type, StickerType::Animated) {
        row_formats.push(InlineKeyboardButton::callback(
            "📦 Lottie JSON",
            format!("fmt:json:{}", token),
        ));
    }

    row_formats.push(InlineKeyboardButton::callback(
        "📸 PNG Frame",
        format!("fmt:png:{}", token),
    ));

    let row_back = vec![InlineKeyboardButton::callback(
        "◀️ Back to Main Menu",
        format!("menu:main:{}", token),
    )];

    InlineKeyboardMarkup::new(vec![row_formats, row_back])
}

/// Format the modern Telegram status message caption
pub fn format_status_caption(
    settings: &UserSettings,
    format_name: &str,
    file_size_bytes: Option<usize>,
    elapsed_secs: Option<f64>,
) -> String {
    let top_time_str = if let Some(secs) = elapsed_secs {
        format!(" (⏱️ {:.1}s)", secs)
    } else {
        String::new()
    };

    let size_str = if let Some(bytes) = file_size_bytes {
        let kb = bytes / 1024;
        format!("\n• Size: <b>{} KB</b>", kb)
    } else {
        String::new()
    };

    let processed_str = if let Some(secs) = elapsed_secs {
        format!("\n• Processed in: <b>{:.1}s</b>", secs)
    } else {
        String::new()
    };

    format!(
        "✅ <b>Conversion completed</b>{}\n\n\
        Your animation is ready!\n\n\
        ⚙️ <b>Current settings</b>\n\
        • Format: <b>{}</b>\n\
        • Resolution: <b>{}</b>\n\
        • Background: <b>{}</b>\
        {}{}",
        top_time_str,
        format_name,
        settings.resolution.label(),
        settings.background.label(),
        size_str,
        processed_str
    )
}


use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StickerType {
    Animated,   // .tgs (gzip Lottie)
    Video,      // .webm (VP9)
    Static,     // .webp
    LottieJson, // .json (raw Lottie JSON)
}

impl StickerType {
    pub fn extension(&self) -> &'static str {
        match self {
            StickerType::Animated => "tgs",
            StickerType::Video => "webm",
            StickerType::Static => "webp",
            StickerType::LottieJson => "json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    Native,  // 512px
    Res480p, // 480px
    Res720p, // 720px
    Res1080p, // 1080px
    Res1440p, // 1440px
    Res4K,   // 2160px
}

impl Resolution {
    pub fn pixels(&self) -> u32 {
        match self {
            Resolution::Native => 512,
            Resolution::Res480p => 480,
            Resolution::Res720p => 720,
            Resolution::Res1080p => 1080,
            Resolution::Res1440p => 1440,
            Resolution::Res4K => 2160,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Resolution::Native => "512px",
            Resolution::Res480p => "480p",
            Resolution::Res720p => "720p HD",
            Resolution::Res1080p => "1080p FHD",
            Resolution::Res1440p => "1440p 2K",
            Resolution::Res4K => "2160p 4K UHD",
        }
    }

    pub fn from_str_code(s: &str) -> Option<Self> {
        match s {
            "512" | "native" => Some(Resolution::Native),
            "480" => Some(Resolution::Res480p),
            "720" => Some(Resolution::Res720p),
            "1080" => Some(Resolution::Res1080p),
            "1440" => Some(Resolution::Res1440p),
            "2160" | "4k" => Some(Resolution::Res4K),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackgroundMode {
    Transparent,
    Dark,
    White,
    Green,
    Custom(String),
}

impl BackgroundMode {
    pub fn label(&self) -> &'static str {
        match self {
            BackgroundMode::Transparent => "Transparent",
            BackgroundMode::Dark => "Dark / Black",
            BackgroundMode::White => "Light / White",
            BackgroundMode::Green => "Green Screen",
            BackgroundMode::Custom(_) => "Custom",
        }
    }

    pub fn hex_color(&self) -> Option<&str> {
        match self {
            BackgroundMode::Transparent => None,
            BackgroundMode::Dark => Some("000000"),
            BackgroundMode::White => Some("ffffff"),
            BackgroundMode::Green => Some("00ff00"),
            BackgroundMode::Custom(ref hex) => Some(hex.as_str()),
        }
    }

    pub fn from_str_code(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "trans" | "transparent" => Some(BackgroundMode::Transparent),
            "dark" | "black" => Some(BackgroundMode::Dark),
            "white" | "light" => Some(BackgroundMode::White),
            "green" => Some(BackgroundMode::Green),
            custom => Some(BackgroundMode::Custom(custom.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSettings {
    pub resolution: Resolution,
    pub background: BackgroundMode,
    pub fps: u32,
}

impl Default for UserSettings {
    fn default() -> Self {
        UserSettings {
            resolution: Resolution::Native,
            background: BackgroundMode::Dark,
            fps: 30, // 30 FPS default for lightning-fast, smooth conversions
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Mp4,
    Webm,
    Webp,
    Gif,
    LottieJson,
    PngFrame,
    Tgs,
    Original,
}


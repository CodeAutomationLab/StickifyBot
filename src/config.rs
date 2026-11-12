use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub bot_token: String,
    pub ffmpeg_path: PathBuf,
    pub rlottie_dll_path: PathBuf,
    /// Directory for temporary conversion output (TEMP_DIR, defaults to the OS temp dir)
    pub temp_dir: PathBuf,
    /// Hard limit for a single conversion; stuck FFmpeg processes are killed after this (CONVERSION_TIMEOUT_SEC)
    pub conversion_timeout: Duration,
    /// Maximum accepted input file size in Megabytes (MAX_FILE_SIZE_MB)
    pub max_file_size_mb: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let bot_token = std::env::var("BOT_TOKEN")
            .map_err(|_| "BOT_TOKEN environment variable not set".to_string())?;

        let ffmpeg_path = if let Ok(custom) = std::env::var("FFMPEG_PATH") {
            PathBuf::from(custom)
        } else {
            find_ffmpeg()
        };

        let rlottie_dll_path = if let Ok(custom) = std::env::var("RLOTTIE_DLL_PATH") {
            PathBuf::from(custom)
        } else {
            find_rlottie_dll()
        };

        let temp_dir = match std::env::var("TEMP_DIR") {
            Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
            _ => std::env::temp_dir(),
        };
        // Ensure it exists up front — conversions (and tests) rely on it.
        if let Err(e) = std::fs::create_dir_all(&temp_dir) {
            return Err(format!("Cannot create TEMP_DIR {:?}: {}", temp_dir, e));
        }

        let timeout_secs = std::env::var("CONVERSION_TIMEOUT_SEC")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(60)
            .clamp(5, 3600);
        let conversion_timeout = Duration::from_secs(timeout_secs);

        let max_file_size_mb = std::env::var("MAX_FILE_SIZE_MB")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(50)
            .clamp(1, 2000);

        Ok(Config {
            bot_token,
            ffmpeg_path,
            rlottie_dll_path,
            temp_dir,
            conversion_timeout,
            max_file_size_mb,
        })
    }
}

fn find_ffmpeg() -> PathBuf {
    // Check ~/.cargo/bin/ffmpeg.exe
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(&home).join(".cargo").join("bin").join("ffmpeg.exe");
        if p.exists() {
            return p;
        }
    }
    // Check current directory or PATH
    PathBuf::from("ffmpeg")
}

fn find_rlottie_dll() -> PathBuf {
    // Check local or system paths for Windows dll or Linux shared library
    let candidates = [
        PathBuf::from("bin/rlottie.dll"),
        PathBuf::from("rust_bot/bin/rlottie.dll"),
        PathBuf::from("rlottie.dll"),
        PathBuf::from("rust_bot/rlottie.dll"),
        PathBuf::from("bin/librlottie.so"),
        PathBuf::from("rust_bot/bin/librlottie.so"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu/librlottie.so"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu/librlottie.so.0"),
        // Debian's rlottie package ships the file with a trailing "-1"
        // (SONAME quirk): librlottie.so.0-1 / librlottie.so.0.1.0-1
        PathBuf::from("/usr/lib/x86_64-linux-gnu/librlottie.so.0-1"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu/librlottie.so.0.1.0-1"),
        PathBuf::from("/usr/lib/librlottie.so"),
        PathBuf::from("/usr/local/lib/librlottie.so"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    #[cfg(target_os = "windows")]
    {
        PathBuf::from("rlottie.dll")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("librlottie.so")
    }
}

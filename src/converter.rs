use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use std::io::{Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

use crate::config::Config;
use crate::rlottie_bridge::RLottieAnimation;
use crate::types::{BackgroundMode, Resolution, StickerType, UserSettings};

#[derive(Debug)]
pub enum ConversionError {
    GzipError(String),
    RLottieError(String),
    FFmpegError(String),
    ImageError(String),
    IoError(String),
    Timeout(String),
    StickerTooLarge(usize),
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversionError::GzipError(e) => write!(f, "Gzip decompression failed: {}", e),
            ConversionError::RLottieError(e) => write!(f, "RLottie rendering failed: {}", e),
            ConversionError::FFmpegError(e) => write!(f, "FFmpeg encoding failed: {}", e),
            ConversionError::ImageError(e) => write!(f, "Image encoding failed: {}", e),
            ConversionError::IoError(e) => write!(f, "I/O error: {}", e),
            ConversionError::Timeout(e) => write!(f, "Conversion timed out: {}", e),
            ConversionError::StickerTooLarge(size) => write!(
                f,
                "Resulting sticker file is {} KB — Telegram's video sticker limit is 256 KB even at maximum compression. Try a lower resolution.",
                size / 1024
            ),
        }
    }
}

impl std::error::Error for ConversionError {}

/// Convert .tgs (gzip Lottie) or raw Lottie JSON bytes into a Lottie JSON string
pub fn tgs_to_lottie_json(tgs_bytes: &[u8]) -> Result<String, ConversionError> {
    if tgs_bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(tgs_bytes);
        let mut json_str = String::new();
        decoder
            .read_to_string(&mut json_str)
            .map_err(|e| ConversionError::GzipError(e.to_string()))?;
        Ok(json_str)
    } else {
        // Already uncompressed JSON
        String::from_utf8(tgs_bytes.to_vec())
            .map_err(|e| ConversionError::RLottieError(format!("Invalid UTF-8 in Lottie JSON: {}", e)))
    }
}

/// Compress raw Lottie JSON bytes into .tgs (gzip Lottie format)
pub fn lottie_json_to_tgs(json_bytes: &[u8]) -> Result<Vec<u8>, ConversionError> {
    // If it's already gzipped .tgs, return as-is
    if json_bytes.starts_with(&[0x1f, 0x8b]) {
        return Ok(json_bytes.to_vec());
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(json_bytes)
        .map_err(|e| ConversionError::GzipError(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| ConversionError::GzipError(e.to_string()))
}

// ---------------------------------------------------------------------------
// Frame pipeline: render -> premultiplied-BGRA transform -> FFmpeg stdin
// ---------------------------------------------------------------------------

/// How premultiplied BGRA frames are converted before being fed to FFmpeg.
enum FrameTransform {
    /// Un-premultiply into straight BGRA (alpha preserved) — for transparent outputs.
    Unpremultiply,
    /// Blend over a solid color into opaque BGRA (alpha = 255) — for solid-background outputs.
    BlendOpaque { r: u8, g: u8, b: u8 },
    /// Blend over a solid color into BGR0 (alpha byte = 0) — for MP4 (yuv420p) outputs.
    BlendBgr0 { r: u8, g: u8, b: u8 },
}

/// Un-premultiply a premultiplied BGRA frame into straight-alpha BGRA bytes.
///
/// RLottie renders premultiplied BGRA; FFmpeg rawvideo `bgra` expects straight
/// alpha. Feeding premultiplied data un-un-premultiplied makes semi-transparent
/// pixels appear dark and dirty.
#[inline]
pub fn unpremultiply_bgra(frame_u32_buf: &[u32], out_bytes: &mut [u8]) {
    let byte_source = unsafe {
        std::slice::from_raw_parts(frame_u32_buf.as_ptr() as *const u8, frame_u32_buf.len() * 4)
    };

    for (src_px, dst_px) in byte_source.chunks_exact(4).zip(out_bytes.chunks_exact_mut(4)) {
        let a = src_px[3];
        if a == 255 || a == 0 {
            dst_px[0] = src_px[0];
            dst_px[1] = src_px[1];
            dst_px[2] = src_px[2];
            dst_px[3] = a;
        } else {
            let av = a as u32;
            // straight = premultiplied * 255 / alpha (rounded). Since prem <= alpha,
            // the result can never exceed 255 — but clamp defensively so malformed
            // input can't wrap around.
            dst_px[0] = ((src_px[0] as u32 * 255 + av / 2) / av).min(255) as u8;
            dst_px[1] = ((src_px[1] as u32 * 255 + av / 2) / av).min(255) as u8;
            dst_px[2] = ((src_px[2] as u32 * 255 + av / 2) / av).min(255) as u8;
            dst_px[3] = a;
        }
    }
}

/// Blend a premultiplied BGRA frame onto an opaque solid background color in-place,
/// writing the resulting opaque 32-bit BGRA pixels (alpha = 255) into an output buffer.
#[inline]
pub fn blend_premultiplied_bgra_to_bgra(
    frame_u32_buf: &[u32],
    out_bytes: &mut [u8],
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
) {
    let byte_source = unsafe {
        std::slice::from_raw_parts(frame_u32_buf.as_ptr() as *const u8, frame_u32_buf.len() * 4)
    };

    for (src_px, dst_px) in byte_source.chunks_exact(4).zip(out_bytes.chunks_exact_mut(4)) {
        let a = src_px[3];
        if a == 255 {
            dst_px[0] = src_px[0];
            dst_px[1] = src_px[1];
            dst_px[2] = src_px[2];
        } else if a == 0 {
            dst_px[0] = bg_b;
            dst_px[1] = bg_g;
            dst_px[2] = bg_r;
        } else {
            let inv_a = 255 - a as u16;
            dst_px[0] = (src_px[0] as u16 + (bg_b as u16 * inv_a) / 255) as u8;
            dst_px[1] = (src_px[1] as u16 + (bg_g as u16 * inv_a) / 255) as u8;
            dst_px[2] = (src_px[2] as u16 + (bg_r as u16 * inv_a) / 255) as u8;
        }
        dst_px[3] = 255;
    }
}

/// Blend a premultiplied BGRA frame onto an opaque solid background color in-place,
/// writing the resulting opaque 32-bit BGR0 pixels (alpha byte = 0) into an output buffer.
#[inline]
pub fn blend_premultiplied_bgra_to_bgr0(
    frame_u32_buf: &[u32],
    out_bytes: &mut [u8],
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
) {
    let is_black = bg_r == 0 && bg_g == 0 && bg_b == 0;
    let byte_source = unsafe {
        std::slice::from_raw_parts(frame_u32_buf.as_ptr() as *const u8, frame_u32_buf.len() * 4)
    };

    if is_black {
        // RLottie premultiplied BGRA over black is ALREADY the final BGR values!
        out_bytes.copy_from_slice(byte_source);
        for px in out_bytes.chunks_exact_mut(4) {
            px[3] = 0;
        }
    } else {
        for (src_px, dst_px) in byte_source.chunks_exact(4).zip(out_bytes.chunks_exact_mut(4)) {
            let a = src_px[3];
            if a == 255 {
                dst_px[0] = src_px[0];
                dst_px[1] = src_px[1];
                dst_px[2] = src_px[2];
            } else if a == 0 {
                dst_px[0] = bg_b;
                dst_px[1] = bg_g;
                dst_px[2] = bg_r;
            } else {
                let inv_a = 255 - a as u16;
                dst_px[0] = (src_px[0] as u16 + (bg_b as u16 * inv_a) / 255) as u8;
                dst_px[1] = (src_px[1] as u16 + (bg_g as u16 * inv_a) / 255) as u8;
                dst_px[2] = (src_px[2] as u16 + (bg_r as u16 * inv_a) / 255) as u8;
            }
            dst_px[3] = 0;
        }
    }
}

fn transform_premultiplied(frame: &[u32], out: &mut [u8], t: &FrameTransform) {
    match t {
        FrameTransform::Unpremultiply => unpremultiply_bgra(frame, out),
        FrameTransform::BlendOpaque { r, g, b } => blend_premultiplied_bgra_to_bgra(frame, out, *r, *g, *b),
        FrameTransform::BlendBgr0 { r, g, b } => blend_premultiplied_bgra_to_bgr0(frame, out, *r, *g, *b),
    }
}

fn bg_rgb(settings: &UserSettings) -> (u8, u8, u8) {
    match settings.background {
        BackgroundMode::White => (255u8, 255u8, 255u8),
        BackgroundMode::Green => (0u8, 255u8, 0u8),
        BackgroundMode::Custom(ref c) if c.len() == 6 => {
            let r = u8::from_str_radix(&c[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&c[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&c[4..6], 16).unwrap_or(0);
            (r, g, b)
        }
        _ => (0, 0, 0), // Dark / black
    }
}

// ---------------------------------------------------------------------------
// FFmpeg process runner with timeout watchdog
// ---------------------------------------------------------------------------

struct FfmpegOutput {
    stdout: Vec<u8>,
    stderr: String,
    success: bool,
    status_code: Option<i32>,
    timed_out: bool,
}

/// Run an FFmpeg command whose video input is piped through stdin.
///
/// - `writer`: owns stdin; writes the raw input payload (or dumps pre-rendered bytes).
/// - `collect_stdout`: when true, FFmpeg's stdout (pipe:1 output) is captured;
///   when false the output is written to a file by FFmpeg itself and stdout is discarded.
/// - A watchdog thread kills the process if it exceeds `timeout`, so a stuck
///   FFmpeg can never hang the bot forever.
fn run_ffmpeg(
    mut cmd: Command,
    writer: Box<dyn FnOnce(ChildStdin) + Send>,
    timeout: Duration,
    collect_stdout: bool,
) -> Result<FfmpegOutput, ConversionError> {
    cmd.stdin(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdout(if collect_stdout {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    let mut child = cmd
        .spawn()
        .map_err(|e| ConversionError::FFmpegError(format!("Failed to spawn FFmpeg: {}", e)))?;

    let stdin = child.stdin.take().expect("stdin must be piped");
    let writer_handle = std::thread::spawn(move || writer(stdin));

    let stderr_pipe = child.stderr.take().expect("stderr must be piped");
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut pipe = stderr_pipe;
        let _ = pipe.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    });

    let stdout_handle = if collect_stdout {
        let pipe = child.stdout.take().expect("stdout must be piped");
        Some(std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut pipe = pipe;
            let _ = pipe.read_to_end(&mut buf);
            buf
        }))
    } else {
        None
    };

    // Shared handle so a watchdog thread can kill a stuck FFmpeg.
    let child_shared = Arc::new(Mutex::new(child));
    let timed_out = Arc::new(AtomicBool::new(false));
    {
        let wd_child = Arc::clone(&child_shared);
        let wd_flag = Arc::clone(&timed_out);
        std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(200));
                // Exit as soon as the process is gone.
                if let Ok(mut c) = wd_child.try_lock() {
                    if let Ok(Some(_)) = c.try_wait() {
                        return;
                    }
                }
                if start.elapsed() >= timeout {
                    if let Ok(mut c) = wd_child.try_lock() {
                        let _ = c.kill();
                    }
                    wd_flag.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });
    }

    // Poll instead of blocking wait() so the watchdog can grab the lock and kill
    // the process on timeout.
    let status = loop {
        std::thread::sleep(Duration::from_millis(100));
        let polled = {
            let mut guard = child_shared
                .lock()
                .map_err(|e| ConversionError::IoError(e.to_string()))?;
            guard.try_wait().map_err(|e| ConversionError::IoError(e.to_string()))?
        };
        if let Some(status) = polled {
            break status;
        }
    };

    let _ = writer_handle.join();
    let stderr = stderr_handle.join().unwrap_or_default();
    let stdout = stdout_handle.and_then(|h| h.join().ok()).unwrap_or_default();

    Ok(FfmpegOutput {
        stdout,
        stderr,
        success: status.success(),
        status_code: status.code(),
        timed_out: timed_out.load(Ordering::Relaxed),
    })
}

fn ffmpeg_failure(context: &str, output: &FfmpegOutput, timeout: Duration) -> ConversionError {
    if output.timed_out {
        return ConversionError::Timeout(format!(
            "{} exceeded the {:.0}s limit",
            context,
            timeout.as_secs_f64()
        ));
    }
    // Keep only the tail of FFmpeg's stderr — that's where the real error is.
    let tail = output
        .stderr
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    let detail = if tail.is_empty() {
        match output.status_code {
            Some(code) => format!("exit status {}", code),
            None => "unknown error".to_string(),
        }
    } else {
        tail
    };
    ConversionError::FFmpegError(format!("{} failed: {}", context, detail))
}

// ---------------------------------------------------------------------------
// TGS render plan (shared by MP4 / WebM / GIF / WEBP pipelines)
// ---------------------------------------------------------------------------

/// Everything needed to stream-render a Lottie animation frame by frame.
struct TgsRenderPlan {
    anim: RLottieAnimation,
    width: usize,
    height: usize,
    fps: f64,
    native_fps: f64,
    total_frames: usize,
    render_count: usize,
}

impl TgsRenderPlan {
    fn build(tgs_bytes: &[u8], settings: &UserSettings, config: &Config) -> Result<Self, ConversionError> {
        let json = tgs_to_lottie_json(tgs_bytes)?;
        let anim = RLottieAnimation::from_json(&json, &config.rlottie_dll_path)
            .map_err(|e| ConversionError::RLottieError(e.to_string()))?;

        let target_dim = settings.resolution.pixels() as usize;
        let width = target_dim;
        let height = target_dim;
        let total_frames = anim.total_frames().max(1);
        let native_fps = anim.frame_rate();
        let fps = if settings.fps > 0 {
            settings.fps as f64
        } else if native_fps > 0.0 {
            native_fps
        } else {
            60.0 // Standard Telegram animated sticker framerate
        };

        let native_duration = total_frames as f64 / if native_fps > 0.0 { native_fps } else { 60.0 };
        let render_count = (native_duration * fps).round().max(1.0) as usize;

        Ok(TgsRenderPlan {
            anim,
            width,
            height,
            fps,
            native_fps,
            total_frames,
            render_count,
        })
    }

    #[inline]
    fn frame_index(&self, k: usize) -> usize {
        if self.native_fps > 0.0 && self.fps > 0.0 {
            (((k as f64 / self.fps) * self.native_fps).round() as usize).min(self.total_frames - 1)
        } else {
            k.min(self.total_frames - 1)
        }
    }
}

/// Build the stdin writer that renders every frame, transforms it and streams
/// it into FFmpeg — never buffering more than one frame in memory.
fn tgs_frame_writer(plan: TgsRenderPlan, transform: FrameTransform) -> Box<dyn FnOnce(ChildStdin) + Send> {
    Box::new(move |mut stdin: ChildStdin| {
        let frame_pixel_count = plan.width * plan.height;
        let mut frame_u32_buf = vec![0u32; frame_pixel_count];
        let mut out_frame_bytes = vec![0u8; frame_pixel_count * 4];
        let stride = plan.width * 4;

        for k in 0..plan.render_count {
            let frame_idx = plan.frame_index(k);
            plan.anim.render_frame(frame_idx, &mut frame_u32_buf, plan.width, plan.height, stride);
            transform_premultiplied(&frame_u32_buf, &mut out_frame_bytes, &transform);
            if stdin.write_all(&out_frame_bytes).is_err() {
                break; // FFmpeg died or closed the pipe; report its stderr instead
            }
        }
        let _ = stdin.flush();
    })
}

/// Build the stdin writer that dumps a pre-existing byte payload (video stickers).
fn byte_writer(bytes: Vec<u8>) -> Box<dyn FnOnce(ChildStdin) + Send> {
    Box::new(move |mut stdin: ChildStdin| {
        let _ = stdin.write_all(&bytes);
        let _ = stdin.flush();
    })
}

// ---------------------------------------------------------------------------
// PNG frame extraction
// ---------------------------------------------------------------------------

/// Render first frame of animated sticker as a crisp PNG (straight alpha)
pub fn render_first_frame_png(
    tgs_bytes: &[u8],
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    let json = tgs_to_lottie_json(tgs_bytes)?;
    let anim = RLottieAnimation::from_json(&json, &config.rlottie_dll_path)
        .map_err(|e| ConversionError::RLottieError(e.to_string()))?;

    let (src_w, src_h) = anim.dimensions();
    let width = if src_w > 0 { src_w } else { 512 };
    let height = if src_h > 0 { src_h } else { 512 };

    let mut frame_u32_buf = vec![0u32; width * height];
    let stride = width * 4;
    anim.render_frame(0, &mut frame_u32_buf, width, height, stride);

    // Convert premultiplied BGRA to straight RGBA for the PNG encoder.
    // Feeding premultiplied values as straight alpha makes edges darker.
    let byte_source = unsafe {
        std::slice::from_raw_parts(frame_u32_buf.as_ptr() as *const u8, frame_u32_buf.len() * 4)
    };
    let mut rgba_bytes = vec![0u8; width * height * 4];
    for (src_px, dst_px) in byte_source.chunks_exact(4).zip(rgba_bytes.chunks_exact_mut(4)) {
        let b = src_px[0];
        let g = src_px[1];
        let r = src_px[2];
        let a = src_px[3];
        if a > 0 && a < 255 {
            let av = a as u32;
            dst_px[0] = ((r as u32 * 255 + av / 2) / av) as u8;
            dst_px[1] = ((g as u32 * 255 + av / 2) / av) as u8;
            dst_px[2] = ((b as u32 * 255 + av / 2) / av) as u8;
        } else {
            dst_px[0] = r;
            dst_px[1] = g;
            dst_px[2] = b;
        }
        dst_px[3] = a;
    }

    let mut png_data = Vec::new();
    let encoder = PngEncoder::new(&mut png_data);
    encoder
        .write_image(&rgba_bytes, width as u32, height as u32, ExtendedColorType::Rgba8)
        .map_err(|e| ConversionError::ImageError(e.to_string()))?;

    Ok(png_data)
}

// ---------------------------------------------------------------------------
// TGS -> MP4
// ---------------------------------------------------------------------------

/// Render TGS frames into MP4 with faststart, streaming frames directly to FFmpeg
pub fn render_tgs_to_mp4(
    tgs_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<NamedTempFile, ConversionError> {
    let plan = TgsRenderPlan::build(tgs_bytes, settings, config)?;

    let (bg_r, bg_g, bg_b) = bg_rgb(settings);
    let transform = FrameTransform::BlendBgr0 { r: bg_r, g: bg_g, b: bg_b };

    let temp_output = NamedTempFile::new_in(&config.temp_dir)
        .map_err(|e| ConversionError::IoError(e.to_string()))?;
    let temp_path = temp_output.path().to_string_lossy().to_string();

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args([
        "-y",
        "-loglevel", "error",
        "-f", "rawvideo",
        "-pix_fmt", "bgr0",
        "-s", &format!("{}x{}", plan.width, plan.height),
        "-framerate", &format!("{}", plan.fps),
        "-i", "-",
        "-c:v", "libx264",
        "-pix_fmt", "yuv420p",
        "-preset", "ultrafast",
        "-crf", "23",
        "-movflags", "+faststart",
        "-f", "mp4",
        &temp_path,
    ]);

    let writer = tgs_frame_writer(plan, transform);
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, false)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg MP4", &output, config.conversion_timeout));
    }

    Ok(temp_output)
}

// ---------------------------------------------------------------------------
// TGS -> WebM / GIF / WEBP (streamed, alpha-correct)
// ---------------------------------------------------------------------------

/// Render TGS frames into WebM (VP9 yuva420p for transparent, yuv420p for solid)
pub fn render_tgs_to_webm(
    tgs_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    render_tgs_webm_core(tgs_bytes, settings, config, 30, false)
}

pub(crate) fn render_tgs_webm_core(
    tgs_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
    crf: u32,
    sticker_mode: bool,
) -> Result<Vec<u8>, ConversionError> {
    let plan = TgsRenderPlan::build(tgs_bytes, settings, config)?;
    let is_transparent = matches!(settings.background, BackgroundMode::Transparent);

    let transform = if is_transparent {
        FrameTransform::Unpremultiply
    } else {
        let (r, g, b) = bg_rgb(settings);
        FrameTransform::BlendOpaque { r, g, b }
    };

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-loglevel".into(), "error".into(),
        "-f".into(), "rawvideo".into(),
        "-pix_fmt".into(), "bgra".into(),
        "-s".into(), format!("{}x{}", plan.width, plan.height),
        "-framerate".into(), format!("{}", plan.fps),
        "-i".into(), "-".into(),
        "-c:v".into(), "libvpx-vp9".into(),
        "-pix_fmt".into(), if is_transparent { "yuva420p".into() } else { "yuv420p".into() },
        "-deadline".into(), "realtime".into(),
        "-cpu-used".into(), "8".into(),
        "-b:v".into(), "0".into(),
        "-crf".into(), crf.to_string(),
    ];
    if sticker_mode {
        // Telegram video stickers may not exceed 3 seconds.
        args.push("-t".into());
        args.push("3".into());
    }
    args.extend([
        "-f".into(), "webm".into(),
        "pipe:1".into(),
    ]);

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args(&args);

    let writer = tgs_frame_writer(plan, transform);
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, true)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg WebM", &output, config.conversion_timeout));
    }

    Ok(output.stdout)
}

/// Render TGS frames into animated GIF using palettegen and paletteuse.
///
/// The palette two-pass must run through `-filter_complex` (explicit labels);
/// multi-output filtergraphs are rejected by `-vf` on FFmpeg <= 6.
pub fn render_tgs_to_gif(
    tgs_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    let plan = TgsRenderPlan::build(tgs_bytes, settings, config)?;
    let is_transparent = matches!(settings.background, BackgroundMode::Transparent);

    let transform = if is_transparent {
        FrameTransform::Unpremultiply
    } else {
        let (r, g, b) = bg_rgb(settings);
        FrameTransform::BlendOpaque { r, g, b }
    };

    // Frames are blended/un-premultiplied in Rust already, so the graph is a
    // simple palette two-pass — no color/overlay filters needed.
    let filter = if is_transparent {
        "[0:v]split[s0][s1];[s0]palettegen=reserve_transparent=1[p];[s1][p]paletteuse=alpha_threshold=128[v]"
    } else {
        "[0:v]split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse[v]"
    };

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args([
        "-y",
        "-loglevel", "error",
        "-f", "rawvideo",
        "-pix_fmt", "bgra",
        "-s", &format!("{}x{}", plan.width, plan.height),
        "-framerate", &format!("{}", plan.fps),
        "-i", "-",
        "-filter_complex", filter,
        "-map", "[v]",
        "-r", &format!("{}", plan.fps),
        "-f", "gif",
        "pipe:1",
    ]);

    let writer = tgs_frame_writer(plan, transform);
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, true)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg GIF", &output, config.conversion_timeout));
    }

    Ok(output.stdout)
}

/// Render TGS frames into animated WEBP (alpha preserved via bgra)
pub fn render_tgs_to_webp(
    tgs_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    let plan = TgsRenderPlan::build(tgs_bytes, settings, config)?;
    let is_transparent = matches!(settings.background, BackgroundMode::Transparent);

    let transform = if is_transparent {
        FrameTransform::Unpremultiply
    } else {
        let (r, g, b) = bg_rgb(settings);
        FrameTransform::BlendOpaque { r, g, b }
    };

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args([
        "-y",
        "-loglevel", "error",
        "-f", "rawvideo",
        "-pix_fmt", "bgra",
        "-s", &format!("{}x{}", plan.width, plan.height),
        "-framerate", &format!("{}", plan.fps),
        "-i", "-",
        "-r", &format!("{}", plan.fps),
        "-c:v", "libwebp",
        "-pix_fmt", "bgra",
        "-lossless", "0",
        "-compression_level", "4",
        "-q:v", "80",
        "-loop", "0",
        "-f", "webp",
        "pipe:1",
    ]);

    let writer = tgs_frame_writer(plan, transform);
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, true)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg WEBP", &output, config.conversion_timeout));
    }

    Ok(output.stdout)
}

// ---------------------------------------------------------------------------
// Telegram-sticker compliant WebM (<= 256 KB, <= 3 s)
// ---------------------------------------------------------------------------

const TELEGRAM_STICKER_MAX_BYTES: usize = 250 * 1024; // 256 KB limit minus safety margin

/// Build a WebM that Telegram accepts as a video sticker: 512px, transparent,
/// <= 3 s and <= 256 KB. Retries with progressively stronger compression.
pub fn render_sticker_webm(
    bytes: &[u8],
    sticker_type: StickerType,
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    let mut settings = UserSettings::default();
    settings.resolution = Resolution::Native;
    settings.background = BackgroundMode::Transparent;
    settings.fps = 30;

    let crf_passes = [30u32, 40, 50];
    let mut last_size = 0usize;

    for (idx, crf) in crf_passes.iter().enumerate() {
        let out = match sticker_type {
            StickerType::Video => {
                convert_video_sticker_to_webm_core(bytes, &settings, config, *crf, true)?
            }
            _ => render_tgs_webm_core(bytes, &settings, config, *crf, true)?,
        };
        last_size = out.len();
        if out.len() <= TELEGRAM_STICKER_MAX_BYTES {
            return Ok(out);
        }
        if idx == crf_passes.len() - 1 {
            return Err(ConversionError::StickerTooLarge(last_size));
        }
    }

    Err(ConversionError::StickerTooLarge(last_size))
}

// ---------------------------------------------------------------------------
// Video sticker (.webm) conversions
// ---------------------------------------------------------------------------

/// Convert video sticker (.webm) into MP4
pub fn convert_video_sticker_to_mp4(
    webm_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<NamedTempFile, ConversionError> {
    let temp_output = NamedTempFile::new_in(&config.temp_dir)
        .map_err(|e| ConversionError::IoError(e.to_string()))?;
    let temp_path = temp_output.path().to_string_lossy().to_string();

    let bg_color = solid_bg_hex(settings);

    let target_dim = settings.resolution.pixels();
    let filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color={}",
        target_dim, target_dim, target_dim, target_dim, bg_color
    );

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args([
        "-y",
        "-loglevel", "error",
        "-i", "pipe:0",
        "-vf", &filter,
        "-an",
        "-c:v", "libx264",
        "-pix_fmt", "yuv420p",
        "-preset", "ultrafast",
        "-crf", "23",
        "-movflags", "+faststart",
        "-f", "mp4",
        &temp_path,
    ]);

    let writer = byte_writer(webm_bytes.to_vec());
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, false)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg video sticker conversion", &output, config.conversion_timeout));
    }

    Ok(temp_output)
}

/// Solid background color as a hex string for FFmpeg filters (`color=`, `pad=`).
/// Custom colors are only used when they are valid 6-digit hex.
fn solid_bg_hex(settings: &UserSettings) -> String {
    match settings.background {
        BackgroundMode::White => "ffffff".to_string(),
        BackgroundMode::Green => "00ff00".to_string(),
        BackgroundMode::Custom(ref c) if c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()) => {
            c.to_lowercase()
        }
        _ => "000000".to_string(),
    }
}

/// Convert video sticker (.webm) into GIF
pub fn convert_video_sticker_to_gif(
    webm_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    let target_dim = settings.resolution.pixels();
    let is_transparent = matches!(settings.background, BackgroundMode::Transparent);

    // Two-pass palette must go through -filter_complex with explicit labels;
    // multi-output graphs are rejected by -vf on FFmpeg <= 6 (Debian bookworm).
    let scale = format!("scale={}:{}:force_original_aspect_ratio=decrease", target_dim, target_dim);
    let head = if is_transparent {
        format!("[0:v]{}[base];[base]split[s0][s1];[s0]palettegen=reserve_transparent=1[p];[s1][p]paletteuse=alpha_threshold=128[v]", scale)
    } else {
        let bg = solid_bg_hex(settings);
        format!(
            "[0:v]{},pad={}:{}:(ow-iw)/2:(oh-ih)/2:color={}[base];[base]split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse[v]",
            scale, target_dim, target_dim, bg
        )
    };

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args(["-y", "-loglevel", "error"]);
    if is_transparent {
        // libvpx VP9 decoder keeps the alpha channel (native decoder drops it)
        cmd.args(["-c:v", "libvpx-vp9"]);
    }
    cmd.args([
        "-i", "pipe:0",
        "-filter_complex", &head,
        "-map", "[v]",
        "-f", "gif",
        "pipe:1",
    ]);

    let writer = byte_writer(webm_bytes.to_vec());
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, true)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg video to GIF", &output, config.conversion_timeout));
    }

    Ok(output.stdout)
}

/// Convert video sticker (.webm) into WEBP (alpha preserved for Transparent)
pub fn convert_video_sticker_to_webp(
    webm_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    let target_dim = settings.resolution.pixels();
    let is_transparent = matches!(settings.background, BackgroundMode::Transparent);

    let scale = format!("scale={}:{}:force_original_aspect_ratio=decrease", target_dim, target_dim);
    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args(["-y", "-loglevel", "error"]);
    if is_transparent {
        // libvpx VP9 decoder keeps the alpha channel (native decoder drops it)
        cmd.args(["-c:v", "libvpx-vp9"]);
    }
    cmd.arg("-i").arg("pipe:0");
    if is_transparent {
        cmd.args([
            "-vf", &scale,
            "-c:v", "libwebp",
            "-pix_fmt", "bgra",
            "-lossless", "0",
            "-q:v", "80",
            "-loop", "0",
            "-f", "webp",
            "pipe:1",
        ]);
    } else {
        let bg = solid_bg_hex(settings);
        let filter = format!(
            "{},pad={}:{}:(ow-iw)/2:(oh-ih)/2:color={}",
            scale, target_dim, target_dim, bg
        );
        cmd.args([
            "-vf", &filter,
            "-c:v", "libwebp",
            "-pix_fmt", "yuv420p",
            "-lossless", "0",
            "-q:v", "80",
            "-loop", "0",
            "-f", "webp",
            "pipe:1",
        ]);
    }

    let writer = byte_writer(webm_bytes.to_vec());
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, true)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg video to WEBP", &output, config.conversion_timeout));
    }

    Ok(output.stdout)
}

/// Convert video sticker (.webm) into WebM (e.g. resized or background changed)
pub fn convert_video_sticker_to_webm(
    webm_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    convert_video_sticker_to_webm_core(webm_bytes, settings, config, 30, false)
}

pub(crate) fn convert_video_sticker_to_webm_core(
    webm_bytes: &[u8],
    settings: &UserSettings,
    config: &Config,
    crf: u32,
    sticker_mode: bool,
) -> Result<Vec<u8>, ConversionError> {
    let target_dim = settings.resolution.pixels();
    let is_transparent = matches!(settings.background, BackgroundMode::Transparent);

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args(["-y", "-loglevel", "error"]);
    if is_transparent {
        // FFmpeg's native VP9 decoder drops the alpha channel; the libvpx
        // decoder preserves it as yuva420p, which transparent outputs need.
        cmd.args(["-c:v", "libvpx-vp9"]);
    }
    cmd.arg("-i").arg("pipe:0");

    let scale = format!("scale={}:{}:force_original_aspect_ratio=decrease", target_dim, target_dim);
    if is_transparent {
        cmd.args([
            "-vf", &scale,
            "-c:v", "libvpx-vp9",
            "-pix_fmt", "yuva420p",
            "-b:v", "0",
            "-crf", &crf.to_string(),
        ]);
    } else {
        let bg = solid_bg_hex(settings);
        let filter = format!(
            "{},pad={}:{}:(ow-iw)/2:(oh-ih)/2:color={}",
            scale, target_dim, target_dim, bg
        );
        cmd.args([
            "-vf", &filter,
            "-c:v", "libvpx-vp9",
            "-pix_fmt", "yuv420p",
            "-b:v", "0",
            "-crf", &crf.to_string(),
        ]);
    }

    if sticker_mode {
        cmd.args(["-t", "3"]);
    }

    cmd.args(["-f", "webm", "pipe:1"]);

    let writer = byte_writer(webm_bytes.to_vec());
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, true)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg video to WebM", &output, config.conversion_timeout));
    }

    Ok(output.stdout)
}

/// Extract first frame of video sticker as PNG (alpha preserved)
pub fn convert_video_sticker_first_frame_png(
    webm_bytes: &[u8],
    config: &Config,
) -> Result<Vec<u8>, ConversionError> {
    let mut cmd = Command::new(&config.ffmpeg_path);
    // libvpx VP9 decoder keeps the alpha channel (native decoder drops it)
    cmd.args([
        "-y",
        "-loglevel", "error",
        "-c:v", "libvpx-vp9",
        "-i", "pipe:0",
        "-vframes", "1",
        "-f", "image2pipe",
        "-c:v", "png",
        "pipe:1",
    ]);

    let writer = byte_writer(webm_bytes.to_vec());
    let output = run_ffmpeg(cmd, writer, config.conversion_timeout, true)?;

    if output.timed_out || !output.success {
        return Err(ffmpeg_failure("FFmpeg extract video frame", &output, config.conversion_timeout));
    }

    Ok(output.stdout)
}

// ---------------------------------------------------------------------------
// Static images
// ---------------------------------------------------------------------------

/// Convert static image (WEBP / PNG) to high-res PNG
pub fn convert_static_image_to_png(image_bytes: &[u8]) -> Result<Vec<u8>, ConversionError> {
    let dyn_img = image::load_from_memory(image_bytes)
        .map_err(|e| ConversionError::ImageError(e.to_string()))?;

    let rgba = dyn_img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let mut png_data = Vec::new();
    let encoder = PngEncoder::new(&mut png_data);
    encoder
        .write_image(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)
        .map_err(|e| ConversionError::ImageError(e.to_string()))?;

    Ok(png_data)
}

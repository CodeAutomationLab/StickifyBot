use std::io::Cursor;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, ChatId, InputFile, MessageEntityKind, ParseMode, Sticker};
use teloxide::utils::html;

use crate::cache::TokenCache;
use crate::config::Config;
use crate::converter::*;
use crate::emoji_service::EmojiService;
use crate::keyboards::*;
use crate::types::{BackgroundMode, Resolution, StickerType, UserSettings};

pub struct AppState {
    pub config: Config,
    pub cache: TokenCache,
    pub emoji_service: EmojiService,
}

/// Shorten long error strings so they fit into callback alerts / messages.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

/// Stop the client-side spinner with an optional alert popup.
async fn answer_callback(bot: &Bot, id: &str, alert: Option<&str>) {
    match alert {
        Some(text) => {
            let _ = bot
                .answer_callback_query(id)
                .text(text.to_string())
                .show_alert(true)
                .await;
        }
        None => {
            let _ = bot.answer_callback_query(id).await;
        }
    }
}

/// Download raw bytes for any Telegram file id
async fn download_file_bytes(bot: &Bot, file_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let file = bot.get_file(file_id).await?;
    let mut bytes = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    teloxide::net::Download::download_file(bot, &file.path, &mut cursor).await?;
    Ok(bytes)
}

/// Download sticker raw bytes from Telegram servers
async fn download_sticker_bytes(bot: &Bot, sticker: &Sticker) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    download_file_bytes(bot, &sticker.file.id).await
}

/// Process and present the instant format selection menu for a sticker or Lottie JSON
async fn process_and_send_sticker(
    bot: &Bot,
    chat_id: ChatId,
    file_bytes: Vec<u8>,
    sticker_type: StickerType,
    file_name: String,
    state: &Arc<AppState>,
) -> ResponseResult<()> {
    log::info!("📥 Received input: {:?} ({:?}, {} bytes)", file_name, sticker_type, file_bytes.len());

    let default_settings = UserSettings::default();
    let token = state.cache.insert(
        file_bytes,
        sticker_type,
        file_name,
        default_settings.clone(),
    );

    let keyboard = build_main_keyboard(&token, sticker_type, &default_settings.background);

    let prompt = match sticker_type {
        StickerType::LottieJson => {
            "📦 <b>Lottie JSON animation received!</b>\n\n\
            Choose the format you want to convert it to below:"
        }
        _ => {
            "🎨 <b>Sticker received!</b>\n\n\
            Choose the format you want to download below:"
        }
    };

    bot.send_message(chat_id, prompt)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Handle incoming user messages
pub async fn handle_message(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let max_bytes = state.config.max_file_size_mb.saturating_mul(1024 * 1024);

    // Check for commands
    if let Some(text) = msg.text() {
        if text.starts_with("/start") || text.starts_with("/help") {
            let welcome = "👋 <b>Welcome to Animated Sticker Converter Bot!</b>\n\n\
                Send me any Telegram animated sticker (<b>.tgs</b>), video sticker (<b>.webm</b>), static sticker (<b>.webp</b>), standard/custom emoji, or <b>Lottie JSON file (.json)</b>.\n\n\
                I will instantly convert it to:\n\
                • 🎬 <b>MP4 Video</b>\n\
                • 🌟 <b>Telegram Animated Sticker (.tgs)</b>\n\
                • 🌐 <b>WebM (Transparent)</b>\n\
                • 🖼 <b>WEBP</b>\n\
                • 🎞 <b>GIF</b>\n\
                • 📦 <b>Lottie JSON</b>\n\
                • 📸 <b>PNG Frame</b>";

            bot.send_message(chat_id, welcome)
                .parse_mode(ParseMode::Html)
                .await?;
            return Ok(());
        }

        // Check for raw Lottie JSON pasted in chat text
        let trimmed_text = text.trim();
        if trimmed_text.starts_with('{') && trimmed_text.ends_with('}') {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed_text) {
                if val.get("layers").is_some() || (val.get("v").is_some() && val.get("fr").is_some()) {
                    return process_and_send_sticker(
                        &bot,
                        chat_id,
                        trimmed_text.as_bytes().to_vec(),
                        StickerType::LottieJson,
                        "animation.json".to_string(),
                        &state,
                    )
                    .await;
                }
            }
        }

        // Check for Custom Emoji entity
        if let Some(entities) = msg.entities() {
            for entity in entities {
                if let MessageEntityKind::CustomEmoji { ref custom_emoji_id } = entity.kind {
                    if let Some(sticker) = state.emoji_service.resolve_custom_emoji(&bot, custom_emoji_id).await {
                        if let Ok(bytes) = download_sticker_bytes(&bot, &sticker).await {
                            let st_type = if sticker.is_animated() {
                                StickerType::Animated
                            } else if sticker.is_video() {
                                StickerType::Video
                            } else {
                                StickerType::Static
                            };
                            return process_and_send_sticker(
                                &bot,
                                chat_id,
                                bytes,
                                st_type,
                                "custom_emoji".to_string(),
                                &state,
                            )
                            .await;
                        }
                    }
                }
            }
        }

        // Check for Unicode Emoji
        if let Some(emoji_char) = state.emoji_service.extract_first_emoji(text) {
            if let Some(sticker) = state.emoji_service.resolve_emoji(&bot, &emoji_char).await {
                if let Ok(bytes) = download_sticker_bytes(&bot, &sticker).await {
                    let st_type = if sticker.is_animated() {
                        StickerType::Animated
                    } else if sticker.is_video() {
                        StickerType::Video
                    } else {
                        StickerType::Static
                    };
                    return process_and_send_sticker(
                        &bot,
                        chat_id,
                        bytes,
                        st_type,
                        format!("emoji_{}", emoji_char),
                        &state,
                    )
                    .await;
                }
            }
        }
    }

    // Check for sticker
    if let Some(sticker) = msg.sticker() {
        let st_type = if sticker.is_animated() {
            StickerType::Animated
        } else if sticker.is_video() {
            StickerType::Video
        } else {
            StickerType::Static
        };

        // u32::MAX is teloxide's "size unknown" sentinel — skip the check then.
        let reported_size = sticker.file.size;
        if reported_size != u32::MAX && (reported_size as u64) > max_bytes {
            bot.send_message(
                chat_id,
                format!(
                    "⚠️ This sticker is {} MB, which exceeds the {} MB limit.",
                    reported_size / (1024 * 1024),
                    state.config.max_file_size_mb
                ),
            )
            .await?;
            return Ok(());
        }

        match download_sticker_bytes(&bot, sticker).await {
            Ok(bytes) => {
                return process_and_send_sticker(
                    &bot,
                    chat_id,
                    bytes,
                    st_type,
                    format!("sticker.{}", st_type.extension()),
                    &state,
                )
                .await;
            }
            Err(e) => {
                log::error!("Failed to download sticker: {}", e);
                bot.send_message(chat_id, "❌ Failed to download sticker from Telegram.")
                    .await?;
                return Ok(());
            }
        }
    }

    // Check for document (Lottie JSON or .tgs file sent as document)
    if let Some(doc) = msg.document() {
        let file_name = doc.file_name.as_deref().unwrap_or("file");
        let is_json = file_name.to_lowercase().ends_with(".json")
            || doc.mime_type.as_ref().map(|m| m.essence_str() == "application/json").unwrap_or(false);
        let is_tgs = file_name.to_lowercase().ends_with(".tgs");

        if is_json || is_tgs {
            let reported_size = doc.file.size;
            if reported_size != u32::MAX && (reported_size as u64) > max_bytes {
                bot.send_message(
                    chat_id,
                    format!(
                        "⚠️ This file is {} MB, which exceeds the {} MB limit.",
                        reported_size / (1024 * 1024),
                        state.config.max_file_size_mb
                    ),
                )
                .await?;
                return Ok(());
            }

            match download_file_bytes(&bot, &doc.file.id).await {
                Ok(bytes) => {
                    let (st_type, fname) = if is_json {
                        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if val.is_object() {
                                (StickerType::LottieJson, doc.file_name.clone().unwrap_or_else(|| "animation.json".to_string()))
                            } else {
                                bot.send_message(chat_id, "⚠️ The uploaded JSON is not a valid Lottie animation object.")
                                    .await?;
                                return Ok(());
                            }
                        } else {
                            bot.send_message(chat_id, "⚠️ Invalid JSON file uploaded.")
                                .await?;
                            return Ok(());
                        }
                    } else {
                        (StickerType::Animated, doc.file_name.clone().unwrap_or_else(|| "sticker.tgs".to_string()))
                    };

                    return process_and_send_sticker(
                        &bot,
                        chat_id,
                        bytes,
                        st_type,
                        fname,
                        &state,
                    )
                    .await;
                }
                Err(e) => {
                    log::error!("Failed to download document: {}", e);
                    bot.send_message(chat_id, "❌ Failed to download file from Telegram.")
                        .await?;
                    return Ok(());
                }
            }
        }
    }

    // Unknown input in a private chat — give the user a hint instead of silence.
    if msg.chat.is_private() {
        bot.send_message(
            chat_id,
            "🤔 Send me a sticker, an emoji, a .tgs/.json file, or type /start to see what I can do.",
        )
        .await?;
    }

    Ok(())
}

/// Run one conversion and deliver the result to the chat.
///
/// Every failure (conversion, timeout, Telegram rejection) is returned as a
/// readable string so the caller can show it to the user instead of failing
/// silently.
#[allow(clippy::too_many_arguments)]
async fn convert_and_send(
    bot: &Bot,
    chat_id: ChatId,
    format_code: &str,
    format_label: &'static str,
    bytes: Vec<u8>,
    settings: &UserSettings,
    st_type: StickerType,
    token: &str,
    config: Config,
    start_time: std::time::Instant,
) -> Result<(), String> {
    match format_code {
        "mp4" => {
            let s_clone = settings.clone();
            let mp4_file = tokio::task::spawn_blocking(move || {
                if matches!(st_type, StickerType::Video) {
                    convert_video_sticker_to_mp4(&bytes, &s_clone, &config)
                } else {
                    render_tgs_to_mp4(&bytes, &s_clone, &config)
                }
            })
            .await
            .map_err(|e| format!("Internal task error: {}", e))?
            .map_err(|e| e.to_string())?;

            let elapsed = start_time.elapsed().as_secs_f64();
            let file_size = std::fs::metadata(mp4_file.path()).map(|m| m.len() as usize).ok();
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_video(chat_id, InputFile::file(mp4_file.path()))
                .caption(caption)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await
                .map_err(|e| format!("Failed to upload the MP4 file: {}", e))?;
            Ok(())
        }
        "webm" => {
            let s_clone = settings.clone();
            let webm_bytes = tokio::task::spawn_blocking(move || {
                if matches!(st_type, StickerType::Video) {
                    convert_video_sticker_to_webm(&bytes, &s_clone, &config)
                } else {
                    render_tgs_to_webm(&bytes, &s_clone, &config)
                }
            })
            .await
            .map_err(|e| format!("Internal task error: {}", e))?
            .map_err(|e| e.to_string())?;

            let elapsed = start_time.elapsed().as_secs_f64();
            let file_size = Some(webm_bytes.len());
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_document(
                chat_id,
                InputFile::memory(webm_bytes).file_name("animation.webm"),
            )
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .map_err(|e| format!("Failed to upload the WebM file: {}", e))?;
            Ok(())
        }
        "webp" => {
            let s_clone = settings.clone();
            let webp_bytes = tokio::task::spawn_blocking(move || {
                if matches!(st_type, StickerType::Video) {
                    convert_video_sticker_to_webp(&bytes, &s_clone, &config)
                } else if matches!(st_type, StickerType::Static) {
                    Ok(bytes) // Already webp
                } else {
                    render_tgs_to_webp(&bytes, &s_clone, &config)
                }
            })
            .await
            .map_err(|e| format!("Internal task error: {}", e))?
            .map_err(|e| e.to_string())?;

            let elapsed = start_time.elapsed().as_secs_f64();
            let file_size = Some(webp_bytes.len());
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_document(
                chat_id,
                InputFile::memory(webp_bytes).file_name("animation.webp"),
            )
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .map_err(|e| format!("Failed to upload the WEBP file: {}", e))?;
            Ok(())
        }
        "gif" => {
            let s_clone = settings.clone();
            let gif_bytes = tokio::task::spawn_blocking(move || {
                if matches!(st_type, StickerType::Video) {
                    convert_video_sticker_to_gif(&bytes, &s_clone, &config)
                } else {
                    render_tgs_to_gif(&bytes, &s_clone, &config)
                }
            })
            .await
            .map_err(|e| format!("Internal task error: {}", e))?
            .map_err(|e| e.to_string())?;

            let elapsed = start_time.elapsed().as_secs_f64();
            let file_size = Some(gif_bytes.len());
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_animation(
                chat_id,
                InputFile::memory(gif_bytes).file_name("animation.gif"),
            )
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .map_err(|e| format!("Failed to upload the GIF file: {}", e))?;
            Ok(())
        }
        "json" => {
            let json_str = tgs_to_lottie_json(&bytes).map_err(|e| e.to_string())?;

            let elapsed = start_time.elapsed().as_secs_f64();
            let json_bytes = json_str.into_bytes();
            let file_size = Some(json_bytes.len());
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_document(
                chat_id,
                InputFile::memory(json_bytes).file_name("animation.json"),
            )
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .map_err(|e| format!("Failed to upload the JSON file: {}", e))?;
            Ok(())
        }
        "sticker" => {
            let sticker_bytes =
                tokio::task::spawn_blocking(move || render_sticker_webm(&bytes, st_type, &config))
                    .await
                    .map_err(|e| format!("Internal task error: {}", e))?
                    .map_err(|e| e.to_string())?;

            let keyboard = build_main_keyboard(token, st_type, &settings.background);
            let sticker_file = InputFile::memory(sticker_bytes).file_name("sticker.webm");
            bot.send_sticker(chat_id, sticker_file)
                .reply_markup(keyboard)
                .await
                .map_err(|e| {
                    format!(
                        "Telegram rejected the sticker: {}. Video stickers must be ≤256 KB, ≤3 s, VP9 512px.",
                        e
                    )
                })?;
            Ok(())
        }
        "tgs" => {
            let tgs_bytes = tokio::task::spawn_blocking(move || {
                if matches!(st_type, StickerType::LottieJson) {
                    lottie_json_to_tgs(&bytes)
                } else {
                    Ok(bytes) // Already .tgs
                }
            })
            .await
            .map_err(|e| format!("Internal task error: {}", e))?
            .map_err(|e| e.to_string())?;

            let elapsed = start_time.elapsed().as_secs_f64();
            let file_size = Some(tgs_bytes.len());
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_document(
                chat_id,
                InputFile::memory(tgs_bytes).file_name("sticker.tgs"),
            )
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .map_err(|e| format!("Failed to upload the .tgs file: {}", e))?;
            Ok(())
        }
        "png" => {
            let png_bytes = tokio::task::spawn_blocking(move || {
                if matches!(st_type, StickerType::Video) {
                    convert_video_sticker_first_frame_png(&bytes, &config)
                } else if matches!(st_type, StickerType::Static) {
                    convert_static_image_to_png(&bytes)
                } else {
                    render_first_frame_png(&bytes, &config)
                }
            })
            .await
            .map_err(|e| format!("Internal task error: {}", e))?
            .map_err(|e| e.to_string())?;

            let elapsed = start_time.elapsed().as_secs_f64();
            let file_size = Some(png_bytes.len());
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_document(
                chat_id,
                InputFile::memory(png_bytes).file_name("frame.png"),
            )
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .map_err(|e| format!("Failed to upload the PNG file: {}", e))?;
            Ok(())
        }
        "orig" => {
            let ext = st_type.extension();
            let elapsed = start_time.elapsed().as_secs_f64();
            let file_size = Some(bytes.len());
            let caption = format_status_caption(settings, format_label, file_size, Some(elapsed));
            let keyboard = build_main_keyboard(token, st_type, &settings.background);

            bot.send_document(
                chat_id,
                InputFile::memory(bytes).file_name(format!("original.{}", ext)),
            )
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .map_err(|e| format!("Failed to upload the original file: {}", e))?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Handle inline keyboard callback queries
pub async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
) -> ResponseResult<()> {
    let data = match q.data {
        Some(d) => d,
        None => {
            answer_callback(&bot, &q.id, None).await;
            return Ok(());
        }
    };

    let msg = match q.message {
        Some(m) => m,
        None => {
            answer_callback(
                &bot,
                &q.id,
                Some("⚠️ This menu is no longer available. Send the sticker again."),
            )
            .await;
            return Ok(());
        }
    };

    let chat_id = msg.chat().id;
    let msg_id = msg.id();

    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() < 3 {
        answer_callback(&bot, &q.id, None).await;
        return Ok(());
    }

    let action = parts[0];

    match action {
        "fmt" => {
            let format_code = parts[1];
            let token = parts[2];

            let entry = match state.cache.get(token) {
                Some(e) => e,
                None => {
                    answer_callback(
                        &bot,
                        &q.id,
                        Some("⚠️ Session expired. Please send the sticker again."),
                    )
                    .await;
                    return Ok(());
                }
            };

            // Stop the client-side spinner right away
            answer_callback(&bot, &q.id, None).await;

            let format_label = match format_code {
                "mp4" => "MP4",
                "webm" => "WebM",
                "webp" => "WEBP",
                "gif" => "GIF",
                "json" => "Lottie JSON",
                "sticker" => "Telegram Sticker",
                "tgs" => "Telegram Sticker (.tgs)",
                "png" => "PNG Frame",
                "orig" => "Original File",
                _ => {
                    answer_callback(&bot, &q.id, Some("⚠️ Unknown format.")).await;
                    return Ok(());
                }
            };

            let est_time_str = match format_code {
                "mp4" | "webm" | "gif" | "sticker" => "~3–5 seconds",
                _ => "~1 second",
            };

            // Instant progress message
            let status_msg = bot
                .send_message(
                    chat_id,
                    format!(
                        "⏳ <b>Converting your animation...</b>\n⏱ <i>Estimated time: {}</i>",
                        est_time_str
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await
                .ok();

            // Background chat action ticker
            let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
            let ticker_bot = bot.clone();
            let chat_action = if format_code == "mp4" {
                ChatAction::UploadVideo
            } else {
                ChatAction::UploadDocument
            };
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
                loop {
                    tokio::select! {
                        _ = &mut rx => break,
                        _ = interval.tick() => {
                            let _ = ticker_bot.send_chat_action(chat_id, chat_action).await;
                        }
                    }
                }
            });

            let start_time = std::time::Instant::now();

            // Conversion + delivery; every failure becomes a readable string.
            let result = convert_and_send(
                &bot,
                chat_id,
                format_code,
                format_label,
                entry.file_bytes.clone(),
                &entry.settings,
                entry.sticker_type,
                token,
                state.config.clone(),
                start_time,
            )
            .await;

            let _ = tx.send(());

            match result {
                Ok(()) => {
                    if let Some(sm) = status_msg {
                        let _ = bot.delete_message(chat_id, sm.id).await;
                    }
                    state.cache.set_last_format(token, format_label.to_string());
                    log::info!("🚀 {} delivered to user in {:.2?}", format_label, start_time.elapsed());
                }
                Err(e) => {
                    log::error!("❌ {} conversion failed: {}", format_label, e);

                    // Surface the failure to the user instead of failing silently.
                    let _ = bot
                        .answer_callback_query(&q.id)
                        .text(format!("❌ {}", truncate_str(&e, 190)))
                        .show_alert(true)
                        .await;

                    let err_text = format!(
                        "❌ <b>Conversion failed</b>\n<code>{}</code>\n\nPick another format from the menu above.",
                        html::escape(&truncate_str(&e, 300))
                    );
                    let kb = build_main_keyboard(token, entry.sticker_type, &entry.settings.background);
                    if let Some(sm) = status_msg {
                        let _ = bot
                            .edit_message_text(chat_id, sm.id, err_text)
                            .parse_mode(ParseMode::Html)
                            .reply_markup(kb)
                            .await;
                    } else {
                        let _ = bot
                            .send_message(chat_id, err_text)
                            .parse_mode(ParseMode::Html)
                            .reply_markup(kb)
                            .await;
                    }
                }
            }
        }
        "menu" => {
            let submenu = parts[1];
            let token = parts[2];

            let entry = match state.cache.get(token) {
                Some(e) => {
                    answer_callback(&bot, &q.id, None).await;
                    e
                }
                None => {
                    answer_callback(
                        &bot,
                        &q.id,
                        Some("⚠️ Session expired. Please send the sticker again."),
                    )
                    .await;
                    return Ok(());
                }
            };

            let kb = match submenu {
                "settings" => build_settings_keyboard(token, &entry.settings, entry.sticker_type),
                "res" => build_resolution_keyboard(token, entry.settings.resolution),
                "bg" => build_background_keyboard(token, &entry.settings.background, entry.sticker_type),
                "fps" => build_fps_keyboard(token, entry.settings.fps),
                "more" => build_more_formats_keyboard(token, entry.sticker_type),
                "main" => build_main_keyboard(token, entry.sticker_type, &entry.settings.background),
                _ => return Ok(()),
            };

            if let Err(e) = bot
                .edit_message_reply_markup(chat_id, msg_id)
                .reply_markup(kb)
                .await
            {
                log::warn!("Failed to edit menu markup: {}", e);
            }
        }
        "set" => {
            if parts.len() < 4 {
                answer_callback(&bot, &q.id, None).await;
                return Ok(());
            }
            let setting_type = parts[1];
            let value = parts[2];
            let token = parts[3];

            let mut entry = match state.cache.get(token) {
                Some(e) => {
                    answer_callback(&bot, &q.id, None).await;
                    e
                }
                None => {
                    answer_callback(
                        &bot,
                        &q.id,
                        Some("⚠️ Session expired. Please send the sticker again."),
                    )
                    .await;
                    return Ok(());
                }
            };

            match setting_type {
                "res" => {
                    if let Some(res) = Resolution::from_str_code(value) {
                        entry.settings.resolution = res;
                    }
                }
                "bg" => {
                    if let Some(bg) = BackgroundMode::from_str_code(value) {
                        entry.settings.background = bg;
                    }
                }
                "fps" => {
                    if let Ok(fps) = value.parse::<u32>() {
                        entry.settings.fps = fps;
                    }
                }
                _ => {}
            }

            state.cache.update_settings(token, entry.settings.clone());

            let kb = build_settings_keyboard(token, &entry.settings, entry.sticker_type);

            let has_caption = match &msg {
                teloxide::types::MaybeInaccessibleMessage::Regular(r) => r.caption().is_some(),
                _ => false,
            };

            if has_caption {
                let last_format = entry
                    .last_format
                    .clone()
                    .unwrap_or_else(|| "File".to_string());
                let new_caption = format_status_caption(&entry.settings, &last_format, None, None);
                if let Err(e) = bot
                    .edit_message_caption(chat_id, msg_id)
                    .caption(new_caption)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(kb)
                    .await
                {
                    log::warn!("Failed to edit caption: {}", e);
                }
            } else {
                let fps_label = if entry.settings.fps == 0 {
                    "Auto (Native)".to_string()
                } else {
                    format!("{} fps", entry.settings.fps)
                };
                let text = format!(
                    "⚙️ <b>Settings Updated</b>\n\n\
                    • Resolution: <b>{}</b>\n\
                    • Background: <b>{}</b>\n\
                    • FPS: <b>{}</b>",
                    entry.settings.resolution.label(),
                    entry.settings.background.label(),
                    fps_label
                );
                if let Err(e) = bot
                    .edit_message_text(chat_id, msg_id, text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(kb)
                    .await
                {
                    log::warn!("Failed to edit settings text: {}", e);
                }
            }
        }
        _ => {
            answer_callback(&bot, &q.id, None).await;
        }
    }

    Ok(())
}

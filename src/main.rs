use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;

use sticker_converter_bot::cache::TokenCache;
use sticker_converter_bot::config::Config;
use sticker_converter_bot::emoji_service::EmojiService;
use sticker_converter_bot::handlers::{handle_callback, handle_message, AppState};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("🚀 Starting High-Performance Rust Telegram Sticker Converter Bot...");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    log::info!("✅ Config loaded successfully (Bot token: {}...)", &config.bot_token[..10]);
    log::info!("✅ Using FFmpeg at: {:?}", config.ffmpeg_path);
    log::info!("✅ Using RLottie DLL at: {:?}", config.rlottie_dll_path);
    log::info!("✅ Temp directory: {:?} | Conversion timeout: {}s | Max file size: {} MB",
        config.temp_dir, config.conversion_timeout.as_secs(), config.max_file_size_mb);

    // Make sure the temp directory exists before any conversion writes there
    if let Err(e) = std::fs::create_dir_all(&config.temp_dir) {
        log::error!("Failed to create temp directory {:?}: {}", config.temp_dir, e);
        std::process::exit(1);
    }

    let cache = TokenCache::new(Duration::from_secs(1800)); // 30 minutes TTL
    let emoji_service = EmojiService::new();

    let state = Arc::new(AppState {
        config: config.clone(),
        cache,
        emoji_service: emoji_service.clone(),
    });

    let bot = Bot::new(&config.bot_token);

    // Pre-warm AnimatedEmojies set in background so resolving emojis is instantaneous
    let prewarm_bot = bot.clone();
    let prewarm_service = emoji_service.clone();
    tokio::spawn(async move {
        log::info!("🔄 Pre-warming AnimatedEmojies cache...");
        prewarm_service.ensure_cache(&prewarm_bot).await;
        log::info!("✅ AnimatedEmojies pre-warm finished.");
    });

    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(handle_message))
        .branch(Update::filter_callback_query().endpoint(handle_callback));

    log::info!("🤖 Bot is online and listening for sticker updates...");

    let mut dp = Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .build();

    tokio::select! {
        _ = dp.dispatch() => log::info!("Dispatcher finished"),
        _ = tokio::signal::ctrl_c() => log::info!("Shutdown signal received, exiting gracefully..."),
    }

    log::info!("Bot shutdown complete.");
}

use std::time::Duration;
use sticker_converter_bot::cache::TokenCache;
use sticker_converter_bot::types::{BackgroundMode, Resolution, StickerType, UserSettings};

#[test]
fn test_cache_insert_and_retrieve() {
    let cache = TokenCache::new(Duration::from_secs(60));
    let raw_bytes = b"sample_sticker_data".to_vec();
    let settings = UserSettings::default();

    let token = cache.insert(
        raw_bytes.clone(),
        StickerType::Animated,
        "test_sticker".to_string(),
        settings.clone(),
    );

    assert_eq!(token.len(), 8, "Token should be an 8-character string");

    let entry = cache.get(&token);
    assert!(entry.is_some(), "Entry must be found in cache");
    let entry = entry.unwrap();

    assert_eq!(entry.file_bytes, raw_bytes);
    assert_eq!(entry.sticker_type, StickerType::Animated);
    assert_eq!(entry.file_name, "test_sticker");
    assert_eq!(entry.settings.resolution, Resolution::Native);
    assert_eq!(entry.settings.background, BackgroundMode::Dark);
    assert_eq!(entry.settings.fps, 30);
}

#[test]
fn test_cache_update_settings() {
    let cache = TokenCache::new(Duration::from_secs(60));
    let token = cache.insert(
        b"data".to_vec(),
        StickerType::Animated,
        "test".to_string(),
        UserSettings::default(),
    );

    let mut new_settings = UserSettings::default();
    new_settings.background = BackgroundMode::Transparent;
    new_settings.resolution = Resolution::Res1080p;
    new_settings.fps = 60;

    let updated = cache.update_settings(&token, new_settings.clone());
    assert!(updated, "Settings update should return true for existing token");

    let retrieved = cache.get(&token).unwrap();
    assert_eq!(retrieved.settings.background, BackgroundMode::Transparent);
    assert_eq!(retrieved.settings.resolution, Resolution::Res1080p);
    assert_eq!(retrieved.settings.fps, 60);
}

#[test]
fn test_cache_non_existent_token() {
    let cache = TokenCache::new(Duration::from_secs(60));
    assert!(cache.get("invalid0").is_none());
    assert!(!cache.update_settings("invalid0", UserSettings::default()));
}

#[test]
fn test_cache_last_format_tracking() {
    let cache = TokenCache::new(Duration::from_secs(60));
    let token = cache.insert(
        b"data".to_vec(),
        StickerType::Animated,
        "test".to_string(),
        UserSettings::default(),
    );

    // Starts as None (no conversion delivered yet)
    let entry = cache.get(&token).unwrap();
    assert!(entry.last_format.is_none());

    // Simulate delivering a GIF conversion
    assert!(cache.set_last_format(&token, "GIF".to_string()));
    let entry = cache.get(&token).unwrap();
    assert_eq!(entry.last_format.as_deref(), Some("GIF"));

    assert!(!cache.set_last_format("invalid0", "MP4".to_string()));
}


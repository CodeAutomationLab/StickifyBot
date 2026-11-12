use sticker_converter_bot::emoji_service::EmojiService;

#[test]
fn test_clean_emoji_variation_selector() {
    let service = EmojiService::new();
    // Variation selector-16 is \u{fe0f}
    let with_vs = "❤️\u{fe0f}";
    let cleaned = service.clean_emoji(with_vs);
    assert_eq!(cleaned, "❤");

    let normal = "🔥";
    assert_eq!(service.clean_emoji(normal), "🔥");
}

#[test]
fn test_emoji_extraction() {
    let service = EmojiService::new();
    let text = "Hello 🔥 world";
    let first = service.extract_first_emoji(text);
    assert_eq!(first, Some("🔥".to_string()));

    let no_emoji = "Hello world";
    assert_eq!(service.extract_first_emoji(no_emoji), None);
}

#[test]
fn test_emoji_extraction_with_variation_selector() {
    let service = EmojiService::new();
    // ❤️ = U+2764 U+FE0F — must be captured as one sequence
    let text = "I ❤️ you";
    let first = service.extract_first_emoji(text);
    assert_eq!(first, Some("❤️".to_string()));
}

#[test]
fn test_emoji_extraction_flags() {
    let service = EmojiService::new();
    // Regional indicator pair must be consumed as one flag emoji
    let text = "From 🇺🇸 here";
    let first = service.extract_first_emoji(text);
    assert_eq!(first, Some("🇺🇸".to_string()));

    let de = "🇩🇪";
    assert_eq!(service.extract_first_emoji(de), Some(de.to_string()));
}

#[test]
fn test_emoji_extraction_skin_tone() {
    let service = EmojiService::new();
    // 👍🏽 = U+1F44D U+1F3FD (skin tone modifier must be consumed)
    let text = "Nice 👍🏽 job";
    let first = service.extract_first_emoji(text);
    assert_eq!(first, Some("👍🏽".to_string()));
}

#[test]
fn test_emoji_extraction_zwj_sequence() {
    let service = EmojiService::new();
    // Family emoji = several people joined by U+200D (ZWJ)
    let text = "👨‍👩‍👧‍👦 ok";
    let first = service.extract_first_emoji(text);
    assert_eq!(first, Some("👨‍👩‍👧‍👦".to_string()));
}

#[test]
fn test_emoji_extraction_keycap() {
    let service = EmojiService::new();
    // 1️⃣ = '1' + FE0F + 20E3
    let text = "press 1️⃣";
    let first = service.extract_first_emoji(text);
    assert_eq!(first, Some("1️⃣".to_string()));
}

#[test]
fn test_emoji_extraction_symbols() {
    let service = EmojiService::new();
    for (input, expected) in [
        ("star ⭐ shine", "⭐"),
        ("up ⬆️ arrow", "⬆️"),
        ("play ▶️ now", "▶️"),
    ] {
        let first = service.extract_first_emoji(input);
        assert_eq!(first, Some(expected.to_string()), "failed for {:?}", input);
    }
}

#[test]
fn test_emoji_extraction_first_only() {
    let service = EmojiService::new();
    let text = "😀 then 😎";
    let first = service.extract_first_emoji(text);
    assert_eq!(first, Some("😀".to_string()));
}

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use teloxide::prelude::*;
use teloxide::types::Sticker;

#[derive(Debug, Clone)]
pub struct EmojiService {
    cache: Arc<RwLock<Option<HashMap<String, Sticker>>>>,
}

impl Default for EmojiService {
    fn default() -> Self {
        Self::new()
    }
}

/// Emoji codepoints that can start an emoji sequence (the char itself is pictographic).
fn is_emoji_start(u: u32) -> bool {
    u == 0x1F004 // mahjong tile
        || u == 0x1F0CF // playing-card joker
        || (0x1F1E6..=0x1F1FF).contains(&u) // regional indicators (flags)
        || (0x1F300..=0x1F5FF).contains(&u) // Misc Symbols and Pictographs
        || (0x1F600..=0x1F64F).contains(&u) // Emoticons
        || (0x1F680..=0x1F6FF).contains(&u) // Transport and Map
        || (0x1F700..=0x1F77F).contains(&u) // Alchemical Symbols
        || (0x1F780..=0x1F7FF).contains(&u) // Geometric Shapes Extended
        || (0x1F800..=0x1F8FF).contains(&u) // Supplemental Arrows-C
        || (0x1F900..=0x1F9FF).contains(&u) // Supplemental Symbols and Pictographs
        || (0x1FA00..=0x1FA6F).contains(&u) // Chess Symbols
        || (0x1FA70..=0x1FAFF).contains(&u) // Symbols and Pictographs Extended-A
        || (0x2600..=0x26FF).contains(&u)   // Misc symbols (heart, sun, etc.)
        || (0x2700..=0x27BF).contains(&u)   // Dingbats
        || (0x2300..=0x23FF).contains(&u)   // Misc Technical (⌚, ⏰, ⏳...)
        || (0x2B00..=0x2BFF).contains(&u)   // Misc Symbols and Arrows (⭐, ⬛, ⬆...)
        || (0x25A0..=0x25FF).contains(&u)   // Geometric Shapes (▶, ◼, ◻...)
        || (0x2934..=0x2935).contains(&u)   // Curved arrows (⤴, ⤵)
        || u == 0x3030 || u == 0x303D || u == 0x3297 || u == 0x3299
}

/// Emoji that default to text presentation (©, ®, ™, arrows, ‼, ⁉) — only count
/// as emoji when followed by the variation selector U+FE0F.
fn is_text_presentation_emoji(u: u32) -> bool {
    u == 0x00A9 || u == 0x00AE || u == 0x2122
        || (0x2194..=0x2199).contains(&u)
        || u == 0x203C || u == 0x2049
        || u == 0x24C2 // Ⓜ
}

impl EmojiService {
    pub fn new() -> Self {
        EmojiService {
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Strip variation selector \u{fe0f}
    pub fn clean_emoji(&self, s: &str) -> String {
        s.replace('\u{fe0f}', "")
    }

    /// Extract the first full emoji sequence (including flags, skin-tone and
    /// ZWJ sequences) from a text string.
    pub fn extract_first_emoji(&self, text: &str) -> Option<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            let u = c as u32;

            // Keycap sequences: digit/#/* (+ optional FE0F) + U+20E3, e.g. "1️⃣"
            if c.is_ascii_digit() || c == '#' || c == '*' {
                let mut j = i + 1;
                if j < chars.len() && chars[j] as u32 == 0xFE0F {
                    j += 1;
                }
                if j < chars.len() && chars[j] as u32 == 0x20E3 {
                    return Some(chars[i..=j].iter().collect());
                }
            }

            if is_emoji_start(u) {
                let mut end = i + 1;

                // Flag: a regional indicator pair, e.g. "🇺🇸"
                if (0x1F1E6..=0x1F1FF).contains(&u)
                    && end < chars.len()
                    && (0x1F1E6..=0x1F1FF).contains(&(chars[end] as u32))
                {
                    end += 1;
                }

                // Consume variation selectors, skin tones, and ZWJ sequences
                // (e.g. "👍🏽", "👨‍👩‍👧‍👦", "❤️").
                while end < chars.len() {
                    let n = chars[end] as u32;
                    if n == 0xFE0F || n == 0xFE0E || n == 0x20E3 || (0x1F3FB..=0x1F3FF).contains(&n) {
                        end += 1;
                        continue;
                    }
                    if n == 0x200D
                        && end + 1 < chars.len()
                        && (is_emoji_start(chars[end + 1] as u32)
                            || (0x1F1E6..=0x1F1FF).contains(&(chars[end + 1] as u32)))
                    {
                        end += 2;
                        continue;
                    }
                    break;
                }

                return Some(chars[i..end].iter().collect());
            }

            // Text-presentation emoji require the explicit FE0F selector
            if is_text_presentation_emoji(u)
                && i + 1 < chars.len()
                && chars[i + 1] as u32 == 0xFE0F
            {
                return Some(chars[i..=i + 1].iter().collect());
            }

            i += 1;
        }

        None
    }

    /// Ensure the official Telegram AnimatedEmojies sticker set is cached.
    pub async fn ensure_cache(&self, bot: &Bot) {
        {
            let guard = self.cache.read();
            if guard.is_some() {
                return;
            }
        }

        // Fetch from Telegram
        match bot.get_sticker_set("AnimatedEmojies").await {
            Ok(sticker_set) => {
                let mut map = HashMap::new();
                for sticker in sticker_set.stickers {
                    if let Some(ref em) = sticker.emoji {
                        let cleaned = self.clean_emoji(em);
                        map.insert(cleaned, sticker.clone());
                        map.insert(em.clone(), sticker);
                    }
                }
                let mut guard = self.cache.write();
                *guard = Some(map);
            }
            Err(e) => {
                log::warn!("Failed to fetch AnimatedEmojies sticker set: {}", e);
            }
        }
    }

    /// Resolve an emoji character to its animated sticker.
    ///
    /// Only the single matching sticker is cloned — never the whole map.
    pub async fn resolve_emoji(&self, bot: &Bot, emoji_char: &str) -> Option<Sticker> {
        let cleaned = self.clean_emoji(emoji_char);

        {
            let guard = self.cache.read();
            if let Some(map) = guard.as_ref() {
                return map
                    .get(&cleaned)
                    .or_else(|| map.get(emoji_char))
                    .cloned();
            }
        }

        self.ensure_cache(bot).await;

        let guard = self.cache.read();
        guard
            .as_ref()
            .and_then(|m| m.get(&cleaned).or_else(|| m.get(emoji_char)))
            .cloned()
    }

    /// Resolve a Telegram Premium custom emoji ID to its sticker
    pub async fn resolve_custom_emoji(&self, bot: &Bot, custom_emoji_id: &str) -> Option<Sticker> {
        match bot.get_custom_emoji_stickers(vec![custom_emoji_id.to_string()]).await {
            Ok(stickers) => stickers.into_iter().next(),
            Err(e) => {
                log::warn!("Failed to fetch custom emoji sticker for {}: {}", custom_emoji_id, e);
                None
            }
        }
    }
}

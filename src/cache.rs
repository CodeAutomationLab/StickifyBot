use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use crate::types::{StickerType, UserSettings};

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub file_bytes: Vec<u8>,
    pub sticker_type: StickerType,
    pub file_name: String,
    pub settings: UserSettings,
    pub created_at: Instant,
    /// Label of the last successfully delivered format ("MP4", "GIF", ...) —
    /// used so settings edits can rewrite captions with the correct format name.
    pub last_format: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TokenCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

impl TokenCache {
    pub fn new(ttl: Duration) -> Self {
        TokenCache {
            entries: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    pub fn insert(
        &self,
        file_bytes: Vec<u8>,
        sticker_type: StickerType,
        file_name: String,
        settings: UserSettings,
    ) -> String {
        self.cleanup_expired();

        let token = uuid::Uuid::new_v4()
            .simple()
            .to_string()[..8]
            .to_string();

        let entry = CacheEntry {
            file_bytes,
            sticker_type,
            file_name,
            settings,
            created_at: Instant::now(),
            last_format: None,
        };

        self.entries.write().insert(token.clone(), entry);
        token
    }

    pub fn get(&self, token: &str) -> Option<CacheEntry> {
        self.cleanup_expired();
        let lock = self.entries.read();
        lock.get(token).cloned()
    }

    pub fn update_settings(&self, token: &str, settings: UserSettings) -> bool {
        let mut lock = self.entries.write();
        if let Some(entry) = lock.get_mut(token) {
            entry.settings = settings;
            true
        } else {
            false
        }
    }

    pub fn set_last_format(&self, token: &str, label: String) -> bool {
        let mut lock = self.entries.write();
        if let Some(entry) = lock.get_mut(token) {
            entry.last_format = Some(label);
            true
        } else {
            false
        }
    }

    fn cleanup_expired(&self) {
        let ttl = self.ttl;
        let now = Instant::now();
        self.entries
            .write()
            .retain(|_, entry| now.duration_since(entry.created_at) < ttl);
    }
}


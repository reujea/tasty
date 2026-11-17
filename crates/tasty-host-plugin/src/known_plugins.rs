//! 사용자가 승인한 외부 plugin의 공개키를 known-plugins.toml에 저장한다.
//! 옛 파일의 항목에 남은 `permissions` 줄은 읽지 않는다.
//!
//! ## 파일 포맷 (TOML)
//!
//! ```toml
//! [plugins."com.example.myplugin"]
//! pubkey = "BASE64_OF_32_BYTE_ED25519_PUBKEY"
//! trusted_at = "2026-06-09T09:50:00Z"
//! publisher_fingerprint = "ab:cd:ef:..."
//!
//! [unverified."com.example.unsigned"]
//! manifest_sha256 = "9f2c..."
//! cause = "unsigned"
//! accepted_at = "2026-10-19T09:50:00Z"
//! ```
//!
//! `unverified` 는 키를 신뢰할 수 없는 플러그인을 사용자가 경고를 보고 쓰기로 고른 기록이다.
//! 매니페스트 내용이 바뀌면 기록과 맞지 않아 다시 묻는다.
//!
//! 참여자 잠금 안에서 최신 문서를 병합하고 공용 안전 교체로 기록한다.

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

/// 사용자 데이터 루트의 외부 plugin 신뢰 목록.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct KnownPlugins {
    #[serde(skip)]
    pub(crate) origin: Option<crate::file_persistence::Origin>,
    /// plugin_id → entry. `BTreeMap` 으로 toml dump 시 순서 안정.
    #[serde(default)]
    plugins: BTreeMap<String, KnownPluginEntry>,
    /// plugin_id → 미인증 플러그인을 쓰기로 고른 기록.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    unverified: BTreeMap<String, UnverifiedChoice>,
}

/// 키로 신뢰할 수 없는 플러그인을 사용자가 경고를 보고 쓰기로 고른 기록.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct UnverifiedChoice {
    /// 고를 때 본 `tasty-plugin.toml` 의 SHA-256 (hex). 내용이 다르면 기록을 쓰지 않는다.
    pub manifest_sha256: String,
    /// 고를 때의 미인증 원인 (`unknown-key` · `missing-pubkey` · `unsigned` · `signature-error`).
    pub cause: String,
    /// RFC3339 timestamp.
    pub accepted_at: String,
}

/// 단일 plugin 의 trust 항목.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct KnownPluginEntry {
    /// base64-encoded 32-byte ed25519 pubkey.
    pub pubkey: String,
    /// RFC3339 timestamp (예: `2026-06-09T09:50:00Z`).
    pub trusted_at: String,
    /// 승인 화면에 표시한 지문. UnknownKey에서는 서명 R 값으로 계산한다.
    #[serde(default)]
    pub publisher_fingerprint: String,
}

impl KnownPluginEntry {
    /// base64 pubkey 를 raw 32 byte 로 디코드. 형식 오류면 None.
    pub fn pubkey_bytes(&self) -> Option<[u8; 32]> {
        let raw = BASE64.decode(self.pubkey.as_bytes()).ok()?;
        if raw.len() != 32 {
            return None;
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&raw);
        Some(out)
    }

    /// raw 32 byte pubkey 를 base64 로 인코드.
    pub fn encode_pubkey(pk: &[u8; 32]) -> String {
        BASE64.encode(pk)
    }
}

impl KnownPlugins {
    /// 데이터 루트의 known-plugins.toml 경로. 루트를 찾지 못하면 None.
    pub fn path() -> Option<PathBuf> {
        tasty_utils::path::tasty_home().map(|h| h.join("known-plugins.toml"))
    }

    /// 파일을 읽어 파싱. 파일이 없으면 빈 DB 반환 (Ok(default)). 다른 io 에러는
    /// 그대로 전파.
    pub fn load() -> std::io::Result<Self> {
        let Some(path) = Self::path() else {
            return Ok(Self::default());
        };
        Self::load_from(path)
    }

    pub fn load_from(path: PathBuf) -> std::io::Result<Self> {
        let table = crate::file_persistence::read(&path)?;
        let mut known: Self = crate::file_persistence::decode(&table)?;
        known.origin = Some(crate::file_persistence::Origin::new(
            path,
            Ok(crate::file_persistence::encode(&known)?),
        ));
        Ok(known)
    }

    /// Latest document edit under one participating lock. Runtime callers execute on the file worker.
    pub fn update<T>(edit: impl FnOnce(&mut Self) -> std::io::Result<T>) -> std::io::Result<T> {
        let path = Self::path()
            .ok_or_else(|| std::io::Error::other("known-plugins.toml path unavailable"))?;
        crate::file_persistence::update_known(&path, edit)
    }

    /// Synchronizing callers wait for secure replacement; runtime submits the owned update instead.
    pub fn save(&self) -> std::io::Result<()> {
        crate::file_persistence::save_known(self)
    }

    /// plugin_id 로 entry 조회. 없으면 None.
    pub fn lookup(&self, plugin_id: &str) -> Option<&KnownPluginEntry> {
        self.plugins.get(plugin_id)
    }

    /// 신규 trust 항목 추가 / 기존 항목 덮어쓰기. `save` 는 호출처가 별도로.
    pub fn add(&mut self, plugin_id: String, entry: KnownPluginEntry) {
        self.plugins.insert(plugin_id, entry);
    }

    /// 키 신뢰 항목을 지우고 지운 값을 돌려준다.
    pub fn remove(&mut self, plugin_id: &str) -> Option<KnownPluginEntry> {
        self.plugins.remove(plugin_id)
    }

    /// 미인증 플러그인을 쓰기로 고른 기록.
    pub fn unverified_choice(&self, plugin_id: &str) -> Option<&UnverifiedChoice> {
        self.unverified.get(plugin_id)
    }

    /// 미인증 선택을 기록한다. 같은 ID의 이전 기록은 덮어쓴다. `save` 는 호출처가 별도로.
    pub fn accept_unverified(&mut self, plugin_id: String, choice: UnverifiedChoice) {
        self.unverified.insert(plugin_id, choice);
    }

    /// 미인증 선택 기록을 지우고 지운 값을 돌려준다.
    pub fn forget_unverified(&mut self, plugin_id: &str) -> Option<UnverifiedChoice> {
        self.unverified.remove(plugin_id)
    }

    /// 테스트 / 진단용 — 전체 plugin_id 목록.
    pub fn plugin_ids(&self) -> impl Iterator<Item = &str> {
        self.plugins.keys().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_for(pk_byte: u8) -> KnownPluginEntry {
        let pk = [pk_byte; 32];
        KnownPluginEntry {
            pubkey: KnownPluginEntry::encode_pubkey(&pk),
            trusted_at: "2026-06-09T09:50:00Z".into(),
            publisher_fingerprint: String::new(),
        }
    }

    #[test]
    fn encode_decode_pubkey_round_trip() {
        let pk = [0xABu8; 32];
        let enc = KnownPluginEntry::encode_pubkey(&pk);
        let entry = KnownPluginEntry {
            pubkey: enc,
            trusted_at: "2026-06-09T09:50:00Z".into(),
            publisher_fingerprint: String::new(),
        };
        assert_eq!(entry.pubkey_bytes(), Some(pk));
    }

    #[test]
    fn pubkey_bytes_rejects_wrong_length() {
        let entry = KnownPluginEntry {
            pubkey: BASE64.encode(b"short"),
            trusted_at: "2026-06-09T09:50:00Z".into(),
            publisher_fingerprint: String::new(),
        };
        assert_eq!(entry.pubkey_bytes(), None);
    }

    #[test]
    fn lookup_returns_added_entry() {
        let mut db = KnownPlugins::default();
        db.add("com.example.foo".into(), entry_for(0x11));
        assert!(db.lookup("com.example.foo").is_some());
        assert!(db.lookup("com.example.missing").is_none());
    }

    #[test]
    fn toml_round_trip_via_string() {
        let mut db = KnownPlugins::default();
        db.add("com.example.foo".into(), entry_for(0x22));
        let dumped = toml::to_string_pretty(&db).unwrap();
        let parsed: KnownPlugins = toml::from_str(&dumped).unwrap();
        let e = parsed.lookup("com.example.foo").unwrap();
        assert_eq!(e.pubkey_bytes(), Some([0x22u8; 32]));
    }
}

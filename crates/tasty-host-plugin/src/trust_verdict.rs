//! 플러그인 폴더 하나의 신뢰 판정. Add plugin 탭, 설치 명령, 확인 필요 탭의 등록,
//! release 부팅 탐색이 같은 판정을 쓴다.
//!
//! 매니페스트 서명과 신뢰 DB는 인증 제도가 아니다. 판정은 "키로 출처를 확인했는가"만 말하며,
//! 미인증 플러그인도 사용자가 경고를 보고 고르면 쓸 수 있다. 그 선택은 신뢰 DB의 `unverified`
//! 표에 매니페스트 SHA-256과 함께 남고, 매니페스트가 바뀌면 다시 묻는다.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::bundle_sig::{
    SigVerifyError, TrustDecision, parse_manifest_id, pubkey_fingerprint, read_pubkey_sidecar,
    signature_verifies_with,
};
use crate::known_plugins::{KnownPluginEntry, KnownPlugins, UnverifiedChoice};

/// 공개 키 파일의 키로 서명이 검증되지 않을 때의 원인 문자열.
pub const PUBKEY_MISMATCH: &str = "tasty-plugin.toml.pub does not match the signature";

/// 키로 출처를 확인하지 못한 원인.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnverifiedCause {
    /// 신뢰 목록에 없는 키로 서명했고, 공개 키 파일의 키로 서명이 검증된다.
    UnknownKey {
        /// 공개 키의 SHA-256 지문.
        fingerprint: String,
        pubkey: [u8; 32],
    },
    /// 신뢰 목록에 없는 키로 서명했고, 공개 키 파일이 없거나 읽을 수 없다.
    MissingPubkey {
        /// 서명의 R 성분에서 계산한 표시용 값. 공개 키 지문과 비교할 수 없다.
        fingerprint: String,
    },
    /// 서명 파일이 없다.
    Unsigned,
    /// 이 파일들로 서명을 검증하지 못했다. 서명 길이·읽기 오류와 공개 키 불일치를 포함한다.
    SignatureError { cause: String },
}

impl UnverifiedCause {
    /// 화면·CLI·신뢰 DB가 쓰는 원인 이름.
    pub fn token(&self) -> &'static str {
        match self {
            UnverifiedCause::UnknownKey { .. } => "unknown-key",
            UnverifiedCause::MissingPubkey { .. } => "missing-pubkey",
            UnverifiedCause::Unsigned => "unsigned",
            UnverifiedCause::SignatureError { .. } => "signature-error",
        }
    }

    /// 사람이 읽는 영어 원인. 오류 응답과 로그에 쓴다.
    pub fn describe(&self) -> String {
        match self {
            UnverifiedCause::UnknownKey { fingerprint, .. } => {
                format!("signed by a key that is not in the trust store ({fingerprint})")
            }
            UnverifiedCause::MissingPubkey { .. } => {
                "signed by a key that is not in the trust store, and tasty-plugin.toml.pub is missing or unreadable".to_string()
            }
            UnverifiedCause::Unsigned => "no signature (tasty-plugin.toml.sig is missing)".to_string(),
            UnverifiedCause::SignatureError { cause } => {
                format!("the signature could not be verified: {cause}")
            }
        }
    }
}

/// 폴더 하나의 판정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustVerdict {
    /// 임베드 키 또는 신뢰 DB의 키로 서명이 검증된다.
    Trusted,
    /// 키로 확인하지 못했지만 사용자가 같은 매니페스트를 쓰기로 골라 두었다.
    AcceptedUnverified,
    /// 키로 확인하지 못했고 사용자 선택 기록도 없다.
    Unverified(UnverifiedCause),
}

/// 서명과 신뢰 DB, 미인증 선택 기록으로 판정한다.
pub fn judge(dir: &Path) -> TrustVerdict {
    judge_with(dir, &KnownPlugins::load().unwrap_or_default())
}

pub fn judge_with(dir: &Path, known: &KnownPlugins) -> TrustVerdict {
    match judge_signature_with(dir, known) {
        TrustVerdict::Unverified(_) if unverified_choice_matches(dir, known) => {
            TrustVerdict::AcceptedUnverified
        }
        verdict => verdict,
    }
}

/// 미인증 선택 기록을 보지 않고 서명과 키 신뢰만으로 판정한다.
pub fn judge_signature(dir: &Path) -> TrustVerdict {
    judge_signature_with(dir, &KnownPlugins::load().unwrap_or_default())
}

pub fn judge_signature_with(dir: &Path, known: &KnownPlugins) -> TrustVerdict {
    match crate::bundle_sig::verify_bundle_signature_with(dir, known) {
        Ok(TrustDecision::Trusted) => TrustVerdict::Trusted,
        Ok(TrustDecision::Untrusted { fingerprint, .. }) => {
            TrustVerdict::Unverified(unknown_key_cause(dir, fingerprint))
        }
        Err(SigVerifyError::SidecarMissing) => TrustVerdict::Unverified(UnverifiedCause::Unsigned),
        // 파싱할 수 있는 임베드 키가 없는 빌드에서도 모르는 키의 서명은 같은 원인으로 본다.
        Err(SigVerifyError::NoValidTrustedKeys) => {
            TrustVerdict::Unverified(unknown_key_cause(dir, signature_r_fingerprint(dir)))
        }
        Err(e) => TrustVerdict::Unverified(UnverifiedCause::SignatureError {
            cause: e.to_string(),
        }),
    }
}

/// 모르는 키의 서명을 공개 키 파일과 대조한다. 공개 키 파일의 키가 서명 키가 아니면
/// 키를 신뢰해도 검증되지 않으므로 서명 오류로 분류한다.
fn unknown_key_cause(dir: &Path, r_fingerprint: String) -> UnverifiedCause {
    let Some(pubkey) = read_pubkey_sidecar(dir) else {
        return UnverifiedCause::MissingPubkey {
            fingerprint: r_fingerprint,
        };
    };
    match signature_verifies_with(dir, &pubkey) {
        Ok(true) => UnverifiedCause::UnknownKey {
            fingerprint: pubkey_fingerprint(&pubkey),
            pubkey,
        },
        Ok(false) => UnverifiedCause::SignatureError {
            cause: PUBKEY_MISMATCH.to_string(),
        },
        Err(e) => UnverifiedCause::SignatureError {
            cause: e.to_string(),
        },
    }
}

/// 서명의 R 성분 지문. [`verify_bundle_signature`] 가 모르는 키에 붙이는 값과 같다.
fn signature_r_fingerprint(dir: &Path) -> String {
    match std::fs::read(dir.join("tasty-plugin.toml.sig")) {
        Ok(sig) if sig.len() == 64 => {
            let mut r = [0u8; 32];
            r.copy_from_slice(&sig[..32]);
            pubkey_fingerprint(&r)
        }
        _ => String::new(),
    }
}

/// `tasty-plugin.toml` 의 SHA-256 (hex).
pub fn manifest_sha256(dir: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(dir.join("tasty-plugin.toml"))?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// 이 폴더의 매니페스트 ID에 같은 내용으로 고른 미인증 선택 기록이 있는지.
fn unverified_choice_matches(dir: &Path, known: &KnownPlugins) -> bool {
    let plugin_id = parse_manifest_id(dir);
    let Ok(digest) = manifest_sha256(dir) else {
        return false;
    };
    known
        .unverified_choice(&plugin_id)
        .is_some_and(|c| c.manifest_sha256 == digest)
}

/// 신뢰 DB에서 한 플러그인 ID의 두 기록을 바꾸기 전 값. 실패한 설치·등록을 되돌린다.
#[derive(Debug, Clone)]
pub struct TrustRecordUndo {
    path: std::path::PathBuf,
    plugin_id: String,
    entry: Option<KnownPluginEntry>,
    unverified: Option<UnverifiedChoice>,
    expected: Option<(Option<KnownPluginEntry>, Option<UnverifiedChoice>)>,
}

impl TrustRecordUndo {
    fn capture(db: &KnownPlugins, plugin_id: &str, path: std::path::PathBuf) -> Self {
        Self {
            path,
            plugin_id: plugin_id.to_string(),
            entry: db.lookup(plugin_id).cloned(),
            unverified: db.unverified_choice(plugin_id).cloned(),
            expected: None,
        }
    }

    /// 이 ID의 두 기록을 바꾸기 전 값으로 되돌린다. 다른 ID의 기록은 건드리지 않는다.
    pub fn restore(self) -> std::io::Result<()> {
        crate::file_persistence::update_known(&self.path, |db| {
            let current = (
                db.lookup(&self.plugin_id).cloned(),
                db.unverified_choice(&self.plugin_id).cloned(),
            );
            let previous = (self.entry.clone(), self.unverified.clone());
            if current == previous {
                return Ok(());
            }
            if self.expected.as_ref() != Some(&current) {
                return Err(std::io::Error::other(
                    "plugin trust record changed; rollback not applied",
                ));
            }
            match self.entry {
                Some(entry) => db.add(self.plugin_id.clone(), entry),
                None => {
                    db.remove(&self.plugin_id);
                }
            }
            match self.unverified {
                Some(choice) => db.accept_unverified(self.plugin_id, choice),
                None => {
                    db.forget_unverified(&self.plugin_id);
                }
            }
            Ok(())
        })
    }

    fn expect_current(&mut self, db: &KnownPlugins) {
        self.expected = Some((
            db.lookup(&self.plugin_id).cloned(),
            db.unverified_choice(&self.plugin_id).cloned(),
        ));
    }
}

/// 사용자가 미인증 플러그인을 쓰기로 고른 것을 기록한다. 되돌릴 값을 돌려준다.
pub fn record_unverified_choice(
    dir: &Path,
    plugin_id: &str,
    cause: &UnverifiedCause,
    accepted_at: String,
) -> std::io::Result<TrustRecordUndo> {
    let path = KnownPlugins::path()
        .ok_or_else(|| std::io::Error::other("known-plugins.toml path unavailable"))?;
    record_unverified_choice_in(path, dir, plugin_id, cause, accepted_at)
}

pub fn record_unverified_choice_in(
    path: std::path::PathBuf,
    dir: &Path,
    plugin_id: &str,
    cause: &UnverifiedCause,
    accepted_at: String,
) -> std::io::Result<TrustRecordUndo> {
    let manifest_sha256 = manifest_sha256(dir)?;
    crate::file_persistence::update_known(&path, |db| {
        let mut undo = TrustRecordUndo::capture(db, plugin_id, path.clone());
        db.accept_unverified(
            plugin_id.to_string(),
            UnverifiedChoice {
                manifest_sha256,
                cause: cause.token().to_string(),
                accepted_at,
            },
        );
        undo.expect_current(db);
        Ok(undo)
    })
}

/// 키 신뢰 항목을 기록한다. 되돌릴 값을 돌려준다.
pub fn record_trusted_key(
    plugin_id: &str,
    entry: KnownPluginEntry,
) -> std::io::Result<TrustRecordUndo> {
    let path = KnownPlugins::path()
        .ok_or_else(|| std::io::Error::other("known-plugins.toml path unavailable"))?;
    record_trusted_key_in(path, plugin_id, entry)
}

pub fn record_trusted_key_in(
    path: std::path::PathBuf,
    plugin_id: &str,
    entry: KnownPluginEntry,
) -> std::io::Result<TrustRecordUndo> {
    crate::file_persistence::update_known(&path, |db| {
        let mut undo = TrustRecordUndo::capture(db, plugin_id, path.clone());
        db.add(plugin_id.to_string(), entry);
        undo.expect_current(db);
        Ok(undo)
    })
}

/// 플러그인을 제거할 때 미인증 선택 기록도 지운다. 다시 설치하면 다시 묻는다.
pub fn forget_unverified_choice(plugin_id: &str) -> std::io::Result<()> {
    let path = KnownPlugins::path()
        .ok_or_else(|| std::io::Error::other("known-plugins.toml path unavailable"))?;
    forget_unverified_choice_in(&path, plugin_id)
}

pub fn forget_unverified_choice_in(path: &Path, plugin_id: &str) -> std::io::Result<()> {
    crate::file_persistence::update_known(path, |db| {
        db.forget_unverified(plugin_id);
        Ok(())
    })
}

/// 시험용. `seed` 키로 매니페스트에 서명해 `.sig` 를 쓰고 공개 키를 돌려준다.
#[cfg(any(test, feature = "test-support"))]
pub fn sign_manifest_for_tests(dir: &Path, seed: [u8; 32]) -> std::io::Result<[u8; 32]> {
    use ed25519_dalek::{Signer, SigningKey};
    let sk = SigningKey::from_bytes(&seed);
    let digest = Sha256::digest(std::fs::read(dir.join("tasty-plugin.toml"))?);
    std::fs::write(
        dir.join("tasty-plugin.toml.sig"),
        sk.sign(digest.as_slice()).to_bytes(),
    )?;
    Ok(sk.verifying_key().to_bytes())
}

#[cfg(test)]
#[path = "trust_verdict_tests.rs"]
mod tests;

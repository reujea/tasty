//! 설치·등록 전 신뢰 판정과 사용자 선택의 기록. 실패하면 이번 시도가 바꾼 신뢰 기록을 되돌린다.
//!
//! 판정은 debug·release 모두 [`tasty_host_plugin::trust_verdict::judge`] 를 쓴다. debug 부팅
//! 탐색만 서명을 건너뛴다(작업 공간의 번들 plugin 이 서명 없이 빌드되기 때문).

use std::path::Path;

use tasty_host_plugin::known_plugins::KnownPluginEntry;
use tasty_host_plugin::trust_verdict::{self, TrustRecordUndo, TrustVerdict, UnverifiedCause};

/// 설치·등록할 때 사용자가 고른 것.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum InstallChoice {
    /// 키로 확인된 플러그인만 설치한다. 미인증이면 경고를 담은 오류로 끝난다.
    #[default]
    VerifiedOnly,
    /// 미인증 경고를 보고 그래도 쓰기로 골랐다. 매니페스트 내용과 함께 선택을 기록한다.
    AllowUnverified,
    /// 공개 키 파일의 키를 신뢰하고 설치한다.
    TrustKey {
        /// 화면이 공개 키 파일에서 읽어 보인 키(base64). 서명 키와 다르면 설치하지 않는다.
        pubkey_b64: String,
    },
}

/// 미인증이라 설치하지 않았다. 오류 응답의 `data` 와 CLI 경고가 이 값을 쓴다.
#[derive(Debug)]
pub(crate) struct UnverifiedRefused {
    pub plugin_id: String,
    pub cause: UnverifiedCause,
}

impl std::fmt::Display for UnverifiedRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "plugin '{}' is unverified: {}. Tasty cannot confirm who made it or that its files \
             are unchanged, and a plugin runs with your account's access. Nothing was installed. \
             To install it anyway, \
             pass allow_unverified (tasty plugin install --allow-unverified) only if you trust \
             where it came from",
            self.plugin_id,
            self.cause.describe()
        )
    }
}

impl std::error::Error for UnverifiedRefused {}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// `dir` 의 판정과 `choice` 로 설치·등록해도 되는지 정하고, 고른 것을 신뢰 DB에 남긴다.
/// 기록을 바꿨으면 되돌릴 값을 돌려준다. 거부하면 아무것도 기록하지 않는다.
#[cfg(test)]
pub(crate) fn authorize(
    dir: &Path,
    plugin_id: &str,
    choice: &InstallChoice,
) -> anyhow::Result<Option<TrustRecordUndo>> {
    let path = tasty_host_plugin::known_plugins::KnownPlugins::path()
        .ok_or_else(|| anyhow::anyhow!("known-plugins.toml path unavailable"))?;
    authorize_in(path, dir, plugin_id, choice)
}

pub(crate) fn authorize_in(
    path: std::path::PathBuf,
    dir: &Path,
    plugin_id: &str,
    choice: &InstallChoice,
) -> anyhow::Result<Option<TrustRecordUndo>> {
    let known = tasty_host_plugin::known_plugins::KnownPlugins::load_from(path.clone())?;
    let save_err = |e: std::io::Error| anyhow::anyhow!("save known-plugins.toml failed: {e}");
    match (trust_verdict::judge_with(dir, &known), choice) {
        (TrustVerdict::Trusted | TrustVerdict::AcceptedUnverified, _) => Ok(None),
        (
            TrustVerdict::Unverified(UnverifiedCause::UnknownKey {
                fingerprint,
                pubkey,
            }),
            InstallChoice::TrustKey { pubkey_b64 },
        ) => {
            let chosen = decode_matching_key(dir, pubkey_b64)?;
            if chosen != pubkey {
                anyhow::bail!(
                    "plugin '{plugin_id}' was not installed: {}",
                    trust_verdict::PUBKEY_MISMATCH
                );
            }
            trust_verdict::record_trusted_key_in(
                path.clone(),
                plugin_id,
                key_entry(&pubkey, fingerprint),
            )
            .map(Some)
            .map_err(save_err)
        }
        (TrustVerdict::Unverified(cause), InstallChoice::TrustKey { .. }) => anyhow::bail!(
            "plugin '{plugin_id}' was not installed: its key cannot be trusted ({})",
            cause.describe()
        ),
        (TrustVerdict::Unverified(cause), InstallChoice::AllowUnverified) => {
            trust_verdict::record_unverified_choice_in(path, dir, plugin_id, &cause, now_rfc3339())
                .map(Some)
                .map_err(save_err)
        }
        (TrustVerdict::Unverified(cause), InstallChoice::VerifiedOnly) => Err(UnverifiedRefused {
            plugin_id: plugin_id.to_string(),
            cause,
        }
        .into()),
    }
}

/// 화면이 보인 키를 디코드하고 그 키로 서명이 검증되는지 확인한다.
fn decode_matching_key(dir: &Path, pubkey_b64: &str) -> anyhow::Result<[u8; 32]> {
    let entry = KnownPluginEntry {
        pubkey: pubkey_b64.to_string(),
        trusted_at: String::new(),
        publisher_fingerprint: String::new(),
    };
    let pubkey = entry
        .pubkey_bytes()
        .ok_or_else(|| anyhow::anyhow!("the public key is not a 32-byte ed25519 key"))?;
    match tasty_host_plugin::bundle_sig::signature_verifies_with(dir, &pubkey) {
        Ok(true) => Ok(pubkey),
        Ok(false) => anyhow::bail!("{}", trust_verdict::PUBKEY_MISMATCH),
        Err(e) => anyhow::bail!("the signature could not be verified: {e}"),
    }
}

fn key_entry(pubkey: &[u8; 32], fingerprint: String) -> KnownPluginEntry {
    KnownPluginEntry {
        pubkey: KnownPluginEntry::encode_pubkey(pubkey),
        trusted_at: now_rfc3339(),
        publisher_fingerprint: fingerprint,
    }
}

/// `plugin.install` 실패 응답. 미인증이라 거부했으면 `data` 에 원인을 실어, 호출자가 메시지를
/// 파싱하지 않고 경고와 선택 방법을 보일 수 있게 한다.
#[cfg(test)]
pub(crate) fn install_error_response(
    id: serde_json::Value,
    e: &anyhow::Error,
) -> crate::ipc::protocol::JsonRpcResponse {
    match e.downcast_ref::<UnverifiedRefused>() {
        Some(refused) => crate::ipc::protocol::JsonRpcResponse::error_with_data(
            id,
            -32000,
            e.to_string(),
            serde_json::json!({
                "unverified": refused.cause.token(),
                "plugin_id": refused.plugin_id,
            }),
        ),
        None => crate::ipc::protocol::JsonRpcResponse::error(id, -32000, e.to_string()),
    }
}

/// 되돌릴 기록이 있으면 되돌린다. 실패는 원래 오류를 가리지 않도록 로그로만 남긴다.
#[cfg(test)]
pub(crate) fn undo(undo: Option<TrustRecordUndo>, plugin_id: &str) {
    if let Some(undo) = undo
        && let Err(e) = undo.restore()
    {
        tracing::error!("plugin '{plugin_id}': restoring known-plugins.toml failed: {e}");
    }
}

#[cfg(test)]
#[path = "install_trust_tests.rs"]
mod tests;

//! tasty-plugin.toml의 SHA-256 digest에 대한 Ed25519 서명을 검증한다.
//! 바이너리·번역 파일 등 디렉터리 전체를 서명하는 것은 아니다.
//!
//! 임베드 키로 검증하거나, 사용자 신뢰 목록의 키로 확인한다.
//! 실제 설치·검색 경로는 release에서 검증하고 debug에서는 우회할 수 있다.

use std::path::Path;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::known_plugins::KnownPlugins;

/// OUT_DIR에 준비된 release·dev 공개키. build.rs가 슬롯을 채운다.
/// 검증 함수는 파싱할 수 없는 키를 건너뛴다.
pub const TRUSTED_PUBKEYS: &[[u8; 32]] = &[
    *include_bytes!(concat!(env!("OUT_DIR"), "/release-pubkey.bin")),
    *include_bytes!(concat!(env!("OUT_DIR"), "/dev-pubkey.bin")),
];

/// 호환용 별칭. release 공개키 슬롯을 가리킨다.
pub const TASTY_BUNDLE_PUBKEY: [u8; 32] = TRUSTED_PUBKEYS[0];

/// `verify_bundle_signature` 의 결과 — 신뢰 단계 분기.
#[derive(Debug, Clone)]
pub enum TrustDecision {
    /// 임베드 키 또는 known_plugins.toml 의 trust 키로 검증 통과.
    Trusted,
    /// 알 수 없는 키 — 사용자 모달 결정 필요.
    ///
    /// known_plugins.toml 에도 없고 임베드 키와도 불일치한다.
    /// 호출처는 본 variant 의 `plugin_id` / `fingerprint` 를 사용자에게 노출하고, 승인 시
    /// `KnownPlugins::add` 로 trust DB 에 기록해야 한다.
    Untrusted {
        plugin_id: String,
        /// 서명의 R 값에서 계산한 표시 지문.
        fingerprint: String,
    },
}

#[derive(Debug)]
pub enum SigVerifyError {
    SidecarMissing,
    SidecarReadError(std::io::Error),
    ManifestReadError(std::io::Error),
    InvalidSignatureLength,
    /// 파싱 가능한 임베드 키가 없고 사용자 신뢰 키로도 검증하지 못했다.
    NoValidTrustedKeys,
}

impl std::fmt::Display for SigVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SidecarMissing => write!(f, "tasty-plugin.toml.sig sidecar missing"),
            Self::SidecarReadError(e) => write!(f, "sidecar read error: {e}"),
            Self::ManifestReadError(e) => write!(f, "manifest read error: {e}"),
            Self::InvalidSignatureLength => write!(f, "signature length != 64 bytes"),
            Self::NoValidTrustedKeys => {
                write!(
                    f,
                    "no valid trusted public keys embedded (all placeholders?)"
                )
            }
        }
    }
}

impl std::error::Error for SigVerifyError {}

/// 매니페스트 sha256 digest 와 sig 를 읽어 `(digest, sig)` 로 반환.
fn read_digest_and_sig(dir: &Path) -> Result<([u8; 32], [u8; 64]), SigVerifyError> {
    let manifest_path = dir.join("tasty-plugin.toml");
    let sig_path = dir.join("tasty-plugin.toml.sig");
    if !sig_path.exists() {
        return Err(SigVerifyError::SidecarMissing);
    }
    let manifest_bytes =
        std::fs::read(&manifest_path).map_err(SigVerifyError::ManifestReadError)?;
    let sig_bytes = std::fs::read(&sig_path).map_err(SigVerifyError::SidecarReadError)?;
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SigVerifyError::InvalidSignatureLength)?;
    let digest: [u8; 32] = Sha256::digest(&manifest_bytes).into();
    Ok((digest, sig_array))
}

/// 매니페스트에서 plugin id 를 *최선 노력 (best-effort)* 추출.
///
/// 매니페스트 파싱이 실패해도 trust 판단 자체는 진행 가능해야 하므로 panic 하지
/// 않고 fallback 값을 반환한다. plugin_id 가 비면 디렉토리 basename 으로.
pub(crate) fn parse_manifest_id(dir: &Path) -> String {
    let manifest_path = dir.join("tasty-plugin.toml");
    let text = std::fs::read_to_string(&manifest_path).unwrap_or_default();
    let value: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(_) => toml::Value::Table(Default::default()),
    };

    value
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<unknown>")
                .to_string()
        })
}

/// 외부 publisher 가 동봉한 `tasty-plugin.toml.pub` sidecar 를 읽어
/// raw 32 byte ed25519 pubkey 를 반환. 파일이 없거나 길이가 다르면 None.
///
/// 본 함수는 [`verify_bundle_signature`] 의 *core 검증* 에는 참여하지 않는다.
/// `TrustDecision::Untrusted { .. }` 인 plugin 을 사용자가 trust
/// 하기로 결정했을 때, [`crate::known_plugins::KnownPluginEntry::pubkey`] 에
/// 저장할 raw pubkey 를 얻기 위한 *trust 저장 시점 helper*. signature 자체는
/// pubkey 를 노출하지 않으므로 publisher 가 sidecar 로 제공해야 trust 가 가능.
pub fn read_pubkey_sidecar(dir: &Path) -> Option<[u8; 32]> {
    let path = dir.join("tasty-plugin.toml.pub");
    let bytes = std::fs::read(&path).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

/// 매니페스트 서명이 `pk` 로 검증되는지. 공개 키 파일의 키가 실제 서명 키인지 대조할 때 쓴다.
/// 서명 파일을 읽지 못하면 `Err`, 키 형식이 틀리거나 검증에 실패하면 `Ok(false)`.
pub fn signature_verifies_with(dir: &Path, pk: &[u8; 32]) -> Result<bool, SigVerifyError> {
    let (digest, sig_array) = read_digest_and_sig(dir)?;
    let Ok(vk) = VerifyingKey::from_bytes(pk) else {
        return Ok(false);
    };
    Ok(vk
        .verify(&digest, &Signature::from_bytes(&sig_array))
        .is_ok())
}

/// 32 byte ed25519 pubkey 의 SHA-256 fingerprint (hex, 콜론 구분).
pub fn pubkey_fingerprint(pk: &[u8; 32]) -> String {
    let digest = Sha256::digest(pk);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex.as_bytes()
        .chunks(2)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(":")
}

/// 서명 검증 + multi-stage trust 판단.
///
/// 검증 순서:
/// 1. sidecar (`tasty-plugin.toml.sig`) 존재 / 길이 / digest 계산 — 실패 시 Err.
/// 2. [`TRUSTED_PUBKEYS`] 순회 — 매칭 키 발견 시 `Trusted` (early return).
/// 3. 매니페스트에서 plugin_id 추출 → [`KnownPlugins`] lookup.
///    - 매칭 + 서명 검증 통과 → `Trusted`.
///    - 매칭 + 서명 검증 실패 → 무시하고 다음 단계로 (DB 의 키와 sig 가 안 맞음).
/// 4. 위 모두 실패 → `Untrusted`.
pub fn verify_bundle_signature(dir: &Path) -> Result<TrustDecision, SigVerifyError> {
    verify_bundle_signature_with(dir, &KnownPlugins::load().unwrap_or_default())
}

/// Verify against an already captured trust store; worker callers must not resolve another home.
pub fn verify_bundle_signature_with(
    dir: &Path,
    known: &KnownPlugins,
) -> Result<TrustDecision, SigVerifyError> {
    let (digest, sig_array) = read_digest_and_sig(dir)?;
    let sig = Signature::from_bytes(&sig_array);

    // 파싱할 수 있는 임베드 키로 서명을 확인한다.
    let mut had_valid_embedded = false;
    for pk in TRUSTED_PUBKEYS {
        let Ok(vk) = VerifyingKey::from_bytes(pk) else {
            continue;
        };
        had_valid_embedded = true;
        if vk.verify(&digest, &sig).is_ok() {
            return Ok(TrustDecision::Trusted);
        }
    }

    // Step 2: known_plugins.toml lookup. load 실패는 비치명 (DB 없을 수 있음).
    let plugin_id = parse_manifest_id(dir);

    if let Some(entry) = known.lookup(&plugin_id)
        && let Some(db_pk) = entry.pubkey_bytes()
        && let Ok(vk) = VerifyingKey::from_bytes(&db_pk)
        && vk.verify(&digest, &sig).is_ok()
    {
        return Ok(TrustDecision::Trusted);
    }

    // 사용자 키로도 확인하지 못했고 파싱 가능한 임베드 키가 없으면 오류다.
    if !had_valid_embedded {
        return Err(SigVerifyError::NoValidTrustedKeys);
    }

    // 이 지문은 서명의 R 값에서 계산한다. 공개키 지문이 아니므로
    // 발급자의 공개키 지문과 직접 비교할 수 없다.
    let r_component: [u8; 32] = sig_array[..32].try_into().unwrap_or([0u8; 32]);
    Ok(TrustDecision::Untrusted {
        plugin_id,
        fingerprint: pubkey_fingerprint(&r_component),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, content: &str) {
        std::fs::write(dir.join("tasty-plugin.toml"), content).unwrap();
    }

    fn write_sig(dir: &Path, bytes: &[u8]) {
        std::fs::write(dir.join("tasty-plugin.toml.sig"), bytes).unwrap();
    }

    fn sign_digest(sk: &SigningKey, manifest: &[u8]) -> Vec<u8> {
        let digest = Sha256::digest(manifest);
        sk.sign(digest.as_slice()).to_bytes().to_vec()
    }

    /// 테스트에서 만든 키로 서명 검증만 수행한다. 사용자 신뢰 목록은 사용하지 않는다.
    fn verify_with_custom_key(dir: &Path, vk: &VerifyingKey) -> bool {
        let (digest, sig_array) = match read_digest_and_sig(dir) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&sig_array);
        vk.verify(&digest, &sig).is_ok()
    }

    #[test]
    fn verify_with_valid_signature() {
        let seed = [7u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        let tmp = TempDir::new().unwrap();
        let manifest = b"id = \"com.example.foo\"\nversion = \"0.7.0\"\n";
        write_manifest(tmp.path(), std::str::from_utf8(manifest).unwrap());
        let sig = sign_digest(&sk, manifest);
        write_sig(tmp.path(), &sig);
        assert!(verify_with_custom_key(tmp.path(), &vk));
    }

    #[test]
    fn verify_rejects_tampered_manifest() {
        let seed = [7u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        let tmp = TempDir::new().unwrap();
        let original = b"id = \"com.example.foo\"\nversion = \"0.7.0\"\n";
        let sig = sign_digest(&sk, original);
        write_manifest(
            tmp.path(),
            "id = \"com.example.foo\"\nversion = \"9.9.9\"\n",
        );
        write_sig(tmp.path(), &sig);
        assert!(!verify_with_custom_key(tmp.path(), &vk));
    }

    #[test]
    fn verify_rejects_missing_sidecar() {
        let tmp = TempDir::new().unwrap();
        write_manifest(tmp.path(), "id = \"com.example.foo\"\n");
        let err = verify_bundle_signature(tmp.path()).unwrap_err();
        assert!(matches!(err, SigVerifyError::SidecarMissing));
    }

    #[test]
    fn verify_rejects_invalid_signature_length() {
        let tmp = TempDir::new().unwrap();
        write_manifest(tmp.path(), "id = \"com.example.foo\"\n");
        write_sig(tmp.path(), &[0u8; 32]);
        let err = verify_bundle_signature(tmp.path()).unwrap_err();
        assert!(matches!(err, SigVerifyError::InvalidSignatureLength));
    }

    /// 임베드 키와 일치하지 않는 서명은 신뢰하면 안 된다.
    /// 임베드 키의 파싱 가능 여부에 따라 UnknownKey 또는 NoValidTrustedKeys를 허용한다.
    #[test]
    fn placeholder_pubkeys_never_grant_trust() {
        let sk = SigningKey::from_bytes(&[42u8; 32]);
        let tmp = TempDir::new().unwrap();
        let manifest = b"id = \"com.example.bar\"\n";
        write_manifest(tmp.path(), std::str::from_utf8(manifest).unwrap());
        write_sig(tmp.path(), &sign_digest(&sk, manifest));
        let result = verify_bundle_signature(tmp.path());
        match result {
            Ok(TrustDecision::Trusted) => panic!("placeholder must not Trust"),
            Ok(TrustDecision::Untrusted { .. }) => {}
            Err(SigVerifyError::NoValidTrustedKeys) => {}
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn pubkey_sidecar_reads_32_byte_file() {
        let tmp = TempDir::new().unwrap();
        let pk = [0x55u8; 32];
        std::fs::write(tmp.path().join("tasty-plugin.toml.pub"), pk).unwrap();
        assert_eq!(read_pubkey_sidecar(tmp.path()), Some(pk));
    }

    #[test]
    fn pubkey_sidecar_rejects_wrong_length() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("tasty-plugin.toml.pub"), [0u8; 16]).unwrap();
        assert_eq!(read_pubkey_sidecar(tmp.path()), None);
    }

    #[test]
    fn pubkey_sidecar_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(read_pubkey_sidecar(tmp.path()), None);
    }

    /// 시나리오: 외부 publisher 가 자체 key 로 서명 → 임베드 키 mismatch.
    /// `verify_bundle_signature` 는 `Untrusted` 반환해야 함 (Add UI 가 빨간 경고).
    /// 또한 `.pub` sidecar 가 있으면 trust 저장에 필요한 raw pubkey 가 동봉됨.
    #[test]
    fn untrusted_external_signer_with_pubkey_sidecar() {
        let sk = SigningKey::from_bytes(&[99u8; 32]);
        let vk = sk.verifying_key();
        let tmp = TempDir::new().unwrap();
        let manifest = b"id = \"com.example.extp\"\nversion = \"0.1.0\"\n";
        write_manifest(tmp.path(), std::str::from_utf8(manifest).unwrap());
        write_sig(tmp.path(), &sign_digest(&sk, manifest));
        std::fs::write(
            tmp.path().join("tasty-plugin.toml.pub"),
            vk.to_bytes().as_slice(),
        )
        .unwrap();

        // verify_bundle_signature 는 Untrusted (UnknownKey) — 단, placeholder
        // 키 환경에서는 NoValidTrustedKeys 도 가능 (테스트 환경 의존).
        match verify_bundle_signature(tmp.path()) {
            Ok(TrustDecision::Untrusted { .. }) => {}
            Err(SigVerifyError::NoValidTrustedKeys) => {} // placeholder key 환경
            other => panic!("expected Untrusted/UnknownKey, got {other:?}"),
        }

        // `.pub` sidecar 로 publisher pubkey 를 추출 가능 → Add UI 가
        // TrustAndInstall 액션을 enqueue 할 수 있음.
        assert_eq!(read_pubkey_sidecar(tmp.path()), Some(vk.to_bytes()));
    }

    #[test]
    fn untrusted_external_signer_without_pubkey_sidecar() {
        let sk = SigningKey::from_bytes(&[88u8; 32]);
        let tmp = TempDir::new().unwrap();
        let manifest = b"id = \"com.example.nopub\"\nversion = \"0.1.0\"\n";
        write_manifest(tmp.path(), std::str::from_utf8(manifest).unwrap());
        write_sig(tmp.path(), &sign_digest(&sk, manifest));
        // sidecar 없음 → Add UI 가 UntrustedNoPubkey 분기, install 차단.
        assert!(read_pubkey_sidecar(tmp.path()).is_none());
    }

    #[test]
    fn fingerprint_format_is_colon_separated_hex() {
        let pk = [0x12u8; 32];
        let fp = pubkey_fingerprint(&pk);
        // 32 byte digest → 64 hex chars → 32 두-자리 그룹 → 31 colon
        assert_eq!(fp.matches(':').count(), 31);
        assert_eq!(fp.len(), 64 + 31);
    }
}

/// 임시 HOME으로 사용자 신뢰 목록을 격리해 검증한다.
/// Windows의 홈 조회는 HOME 환경변수를 쓰지 않으므로 Unix에서만 실행한다.
#[cfg(all(test, unix))]
mod integration_tests {
    use super::*;
    use crate::known_plugins::KnownPluginEntry;
    use ed25519_dalek::{Signer, SigningKey};
    use tempfile::TempDir;

    /// 다른 테스트와 경합하지 않도록 공용 가드로 HOME을 바꾸고 복원한다.
    use crate::test_support::HomeEnvGuard;

    /// 매니페스트 작성 + ed25519 signing — 매니페스트 digest 에 sk 서명.
    fn make_bundle(bundle_dir: &Path, sk: &SigningKey, plugin_id: &str) -> [u8; 32] {
        let manifest = format!("id = \"{plugin_id}\"\nversion = \"1.0.0\"\n");
        std::fs::write(bundle_dir.join("tasty-plugin.toml"), &manifest).unwrap();
        let digest = Sha256::digest(manifest.as_bytes());
        let sig = sk.sign(digest.as_slice()).to_bytes().to_vec();
        std::fs::write(bundle_dir.join("tasty-plugin.toml.sig"), &sig).unwrap();
        sk.verifying_key().to_bytes()
    }

    /// `<home>/.tasty-debug/known-plugins.toml` 에 trust 항목 1 개 작성. 옛 파일처럼
    /// `permissions` 줄을 함께 남겨도 읽기와 판정에 영향이 없어야 한다.
    ///
    /// cargo test 는 debug 컴파일이라 코드(`tasty_home()`)가 `.tasty-debug` 루트를
    /// 본다 — HOME override 기반 테스트도 같은 디렉터리에 써야 정합한다.
    fn write_known_db(home: &Path, plugin_id: &str, pk: &[u8; 32]) {
        let tasty_dir = home.join(".tasty-debug");
        std::fs::create_dir_all(&tasty_dir).unwrap();
        let contents = format!(
            "[plugins.\"{plugin_id}\"]\npubkey = \"{}\"\npermissions = [\"filesystem.read\"]\ntrusted_at = \"2026-06-09T00:00:00Z\"\npublisher_fingerprint = \"\"\n",
            KnownPluginEntry::encode_pubkey(pk),
        );
        std::fs::write(tasty_dir.join("known-plugins.toml"), contents).unwrap();
    }

    /// B-1: 임베드 키로는 검증 실패하지만 known_plugins.toml 의 trust 항목이
    /// 일치 → `Trusted` 반환.
    #[test]
    fn known_plugins_trust_grants_trusted() {
        let home_tmp = HomeEnvGuard::derived_from_home();

        let bundle_tmp = TempDir::new().unwrap();
        let sk = SigningKey::from_bytes(&[99u8; 32]);
        let pk = make_bundle(bundle_tmp.path(), &sk, "com.example.bnk");
        write_known_db(home_tmp.path(), "com.example.bnk", &pk);

        let result = verify_bundle_signature(bundle_tmp.path()).unwrap();
        assert!(
            matches!(result, TrustDecision::Trusted),
            "expected Trusted, got {result:?}"
        );
    }

    /// 사용자 신뢰 목록이 비어 있고 임베드 키와 맞지 않는 서명은 신뢰하지 않는다.
    /// 키의 파싱 가능 여부에 따라 UnknownKey 또는 NoValidTrustedKeys를 허용한다.
    #[test]
    fn placeholder_embed_with_empty_db_never_trusts() {
        let _home = HomeEnvGuard::derived_from_home();

        let bundle_tmp = TempDir::new().unwrap();
        let sk = SigningKey::from_bytes(&[17u8; 32]);
        // pk 반환값 무시 — placeholder 시나리오는 trust DB 가 비어 있어 어떤
        // pubkey 로도 매칭이 일어나지 않는다.
        let _pk = make_bundle(bundle_tmp.path(), &sk, "com.example.ph");

        let result = verify_bundle_signature(bundle_tmp.path());
        match result {
            Ok(TrustDecision::Trusted) => panic!("placeholder must never grant Trusted"),
            Ok(TrustDecision::Untrusted { .. }) => {}
            Err(SigVerifyError::NoValidTrustedKeys) => {}
            other => panic!("unexpected result: {other:?}"),
        }
    }
}

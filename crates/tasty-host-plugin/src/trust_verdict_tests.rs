use super::*;
use crate::test_support::HomeEnvGuard;
use ed25519_dalek::{Signer, SigningKey};

const MANIFEST: &str = "id = \"com.example.trust\"\npermissions = [\"fs.read\"]\n";

fn plugin_dir(manifest: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("tasty-plugin.toml"), manifest).expect("manifest");
    dir
}

fn sign(dir: &Path, sk: &SigningKey) {
    let manifest = std::fs::read(dir.join("tasty-plugin.toml")).expect("read");
    let digest = Sha256::digest(&manifest);
    let sig = sk.sign(digest.as_slice()).to_bytes();
    std::fs::write(dir.join("tasty-plugin.toml.sig"), sig).expect("sig");
}

fn write_pub(dir: &Path, sk: &SigningKey) {
    std::fs::write(
        dir.join("tasty-plugin.toml.pub"),
        sk.verifying_key().to_bytes(),
    )
    .expect("pub");
}

fn cause(dir: &Path) -> UnverifiedCause {
    match judge(dir) {
        TrustVerdict::Unverified(cause) => cause,
        other => panic!("expected an unverified verdict, got {other:?}"),
    }
}

#[test]
fn each_unverified_state_gets_its_own_cause() {
    let _home = HomeEnvGuard::tasty_home();
    let sk = SigningKey::from_bytes(&[9u8; 32]);
    let other = SigningKey::from_bytes(&[10u8; 32]);

    let unsigned = plugin_dir(MANIFEST);
    assert_eq!(cause(unsigned.path()), UnverifiedCause::Unsigned);

    let short_sig = plugin_dir(MANIFEST);
    std::fs::write(short_sig.path().join("tasty-plugin.toml.sig"), [0u8; 32]).expect("sig");
    assert!(matches!(
        cause(short_sig.path()),
        UnverifiedCause::SignatureError { .. }
    ));

    let no_pub = plugin_dir(MANIFEST);
    sign(no_pub.path(), &sk);
    assert!(matches!(
        cause(no_pub.path()),
        UnverifiedCause::MissingPubkey { .. }
    ));

    let with_pub = plugin_dir(MANIFEST);
    sign(with_pub.path(), &sk);
    write_pub(with_pub.path(), &sk);
    let pk = sk.verifying_key().to_bytes();
    assert_eq!(
        cause(with_pub.path()),
        UnverifiedCause::UnknownKey {
            fingerprint: pubkey_fingerprint(&pk),
            pubkey: pk,
        }
    );

    let wrong_pub = plugin_dir(MANIFEST);
    sign(wrong_pub.path(), &sk);
    write_pub(wrong_pub.path(), &other);
    assert_eq!(
        cause(wrong_pub.path()),
        UnverifiedCause::SignatureError {
            cause: PUBKEY_MISMATCH.to_string()
        }
    );
}

#[test]
fn a_recorded_choice_holds_until_the_manifest_changes() {
    let _home = HomeEnvGuard::tasty_home();
    let dir = plugin_dir(MANIFEST);
    let undo = record_unverified_choice(
        dir.path(),
        "com.example.trust",
        &UnverifiedCause::Unsigned,
        "2026-10-19T00:00:00Z".into(),
    )
    .expect("record");
    assert_eq!(judge(dir.path()), TrustVerdict::AcceptedUnverified);
    // 선택 기록은 판정에만 쓰이고 서명 판정 자체는 그대로다.
    assert_eq!(
        judge_signature(dir.path()),
        TrustVerdict::Unverified(UnverifiedCause::Unsigned)
    );

    std::fs::write(
        dir.path().join("tasty-plugin.toml"),
        "id = \"com.example.trust\"\npermissions = [\"fs.read\", \"network\"]\n",
    )
    .expect("rewrite");
    assert_eq!(cause(dir.path()), UnverifiedCause::Unsigned);

    undo.restore().expect("restore");
    assert!(
        KnownPlugins::load()
            .expect("load")
            .unverified_choice("com.example.trust")
            .is_none()
    );
}

#[test]
fn undo_restores_only_the_records_of_that_id() {
    let _home = HomeEnvGuard::tasty_home();
    let entry = |byte: u8| KnownPluginEntry {
        pubkey: KnownPluginEntry::encode_pubkey(&[byte; 32]),
        trusted_at: "2026-10-01T00:00:00Z".into(),
        publisher_fingerprint: String::new(),
    };
    let mut db = KnownPlugins::default();
    db.add("com.example.trust".into(), entry(1));
    db.add("com.example.other".into(), entry(2));
    db.save().expect("save");

    let undo = record_trusted_key("com.example.trust", entry(3)).expect("record");
    let dir = plugin_dir(MANIFEST);
    let choice_undo = record_unverified_choice(
        dir.path(),
        "com.example.trust",
        &UnverifiedCause::Unsigned,
        "2026-10-19T00:00:00Z".into(),
    )
    .expect("record choice");
    choice_undo.restore().expect("restore choice");
    undo.restore().expect("restore key");

    let db = KnownPlugins::load().expect("load");
    assert_eq!(
        db.lookup("com.example.trust")
            .and_then(|e| e.pubkey_bytes()),
        Some([1; 32])
    );
    assert_eq!(
        db.lookup("com.example.other")
            .and_then(|e| e.pubkey_bytes()),
        Some([2; 32])
    );
    assert!(db.unverified_choice("com.example.trust").is_none());
}

#[test]
fn trusting_the_matching_key_makes_the_plugin_trusted() {
    let _home = HomeEnvGuard::tasty_home();
    let sk = SigningKey::from_bytes(&[11u8; 32]);
    let dir = plugin_dir(MANIFEST);
    sign(dir.path(), &sk);
    write_pub(dir.path(), &sk);
    let UnverifiedCause::UnknownKey {
        fingerprint,
        pubkey,
    } = cause(dir.path())
    else {
        panic!("unknown key expected");
    };
    record_trusted_key(
        "com.example.trust",
        KnownPluginEntry {
            pubkey: KnownPluginEntry::encode_pubkey(&pubkey),
            trusted_at: "2026-10-19T00:00:00Z".into(),
            publisher_fingerprint: fingerprint,
        },
    )
    .expect("record");
    assert_eq!(judge(dir.path()), TrustVerdict::Trusted);
}

#[test]
fn removing_a_plugin_forgets_its_choice() {
    let _home = HomeEnvGuard::tasty_home();
    let dir = plugin_dir(MANIFEST);
    record_unverified_choice(
        dir.path(),
        "com.example.trust",
        &UnverifiedCause::Unsigned,
        "2026-10-19T00:00:00Z".into(),
    )
    .expect("record");
    forget_unverified_choice("com.example.trust").expect("forget");
    assert_eq!(cause(dir.path()), UnverifiedCause::Unsigned);
}

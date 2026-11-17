use super::*;
use tasty_host_plugin::known_plugins::KnownPlugins;
use tasty_host_plugin::trust_verdict::{judge, sign_manifest_for_tests};

const ID: &str = "com.example.install-trust";

struct TempHome {
    _dir: tempfile::TempDir,
}
impl TempHome {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        tasty_utils::path::push_home_override(dir.path().to_path_buf());
        Self { _dir: dir }
    }
}
impl Drop for TempHome {
    fn drop(&mut self) {
        tasty_utils::path::pop_home_override();
    }
}

fn plugin_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("tasty-plugin.toml"),
        format!("id = \"{ID}\"\n"),
    )
    .expect("manifest");
    dir
}

#[test]
fn an_unverified_plugin_needs_an_explicit_choice_and_records_nothing_without_one() {
    let _home = TempHome::new();
    let dir = plugin_dir();
    let err = authorize(dir.path(), ID, &InstallChoice::VerifiedOnly)
        .expect_err("unverified plugin must not pass without a choice");
    let refused = err
        .downcast_ref::<UnverifiedRefused>()
        .expect("the refusal carries the cause");
    assert_eq!(refused.cause, UnverifiedCause::Unsigned);
    assert!(err.to_string().contains("no signature"), "{err}");
    let response = install_error_response(serde_json::json!(73), &err);
    assert_eq!(response.id, serde_json::json!(73));
    assert_eq!(
        response.error.as_ref().unwrap().data.as_ref().unwrap()["plugin_id"],
        ID
    );
    assert_eq!(
        response.error.as_ref().unwrap().data.as_ref().unwrap()["unverified"],
        "unsigned"
    );
    assert!(
        KnownPlugins::load()
            .expect("load")
            .unverified_choice(ID)
            .is_none()
    );

    let undo = authorize(dir.path(), ID, &InstallChoice::AllowUnverified)
        .expect("explicit choice is accepted");
    assert!(undo.is_some());
    assert_eq!(judge(dir.path()), TrustVerdict::AcceptedUnverified);
    // 선택을 기록했으면 다음 판정은 그대로 통과하고 기록을 다시 바꾸지 않는다.
    assert!(
        authorize(dir.path(), ID, &InstallChoice::VerifiedOnly)
            .expect("recorded choice")
            .is_none()
    );

    super::undo(undo, ID);
    assert!(
        KnownPlugins::load()
            .expect("load")
            .unverified_choice(ID)
            .is_none()
    );
}

#[test]
fn trusting_a_key_that_did_not_sign_the_manifest_fails_before_anything_is_saved() {
    let _home = TempHome::new();
    let dir = plugin_dir();
    let real = sign_manifest_for_tests(dir.path(), [21; 32]).expect("sign");
    std::fs::write(dir.path().join("tasty-plugin.toml.pub"), real).expect("pub");
    let other = tempfile::tempdir().expect("tempdir");
    std::fs::write(other.path().join("tasty-plugin.toml"), "id = \"x\"\n").expect("manifest");
    let wrong = sign_manifest_for_tests(other.path(), [22; 32]).expect("sign other");

    let err = authorize(
        dir.path(),
        ID,
        &InstallChoice::TrustKey {
            pubkey_b64: KnownPluginEntry::encode_pubkey(&wrong),
        },
    )
    .expect_err("a key that did not sign the manifest must be refused");
    assert!(
        err.to_string().contains(trust_verdict::PUBKEY_MISMATCH),
        "{err}"
    );
    assert!(KnownPlugins::load().expect("load").lookup(ID).is_none());
}

#[test]
fn trusting_the_signing_key_records_its_real_fingerprint() {
    let _home = TempHome::new();
    let dir = plugin_dir();
    let pubkey = sign_manifest_for_tests(dir.path(), [23; 32]).expect("sign");
    std::fs::write(dir.path().join("tasty-plugin.toml.pub"), pubkey).expect("pub");

    let undo = authorize(
        dir.path(),
        ID,
        &InstallChoice::TrustKey {
            pubkey_b64: KnownPluginEntry::encode_pubkey(&pubkey),
        },
    )
    .expect("matching key");
    let db = KnownPlugins::load().expect("load");
    let entry = db.lookup(ID).expect("trusted entry");
    assert_eq!(entry.pubkey_bytes(), Some(pubkey));
    assert_eq!(
        entry.publisher_fingerprint,
        tasty_host_plugin::bundle_sig::pubkey_fingerprint(&pubkey)
    );
    assert_eq!(judge(dir.path()), TrustVerdict::Trusted);

    super::undo(undo, ID);
    assert!(KnownPlugins::load().expect("load").lookup(ID).is_none());
}

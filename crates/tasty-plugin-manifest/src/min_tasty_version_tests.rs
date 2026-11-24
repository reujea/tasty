//! `min_tasty_version`의 형식 검사와 본체 버전 비교.
//! 비교는 [`check_min_tasty_version_source`]를 버전을 직접 넣어 잰다(전역 본체 버전을 쓰지 않는다).

use super::*;

fn source_with(line: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1.0"
        api_version = "1"
        {line}
        [entry]
        type = "process"
        command = "x"
    "#
    )
}

fn manifest_with(line: &str) -> anyhow::Result<Manifest> {
    let m: Manifest = toml::from_str(&source_with(line))?;
    m.validate()?;
    Ok(m)
}

fn host(v: &str) -> semver::Version {
    semver::Version::parse(v).expect("host version")
}

fn judge(source: &str, host_version: &str) -> anyhow::Result<()> {
    check_min_tasty_version_source(source, &host(host_version))
}

#[test]
fn a_manifest_requiring_a_newer_tasty_is_rejected_with_both_versions() {
    let err = judge(&source_with(r#"min_tasty_version = "0.15.0""#), "0.14.9")
        .expect_err("older host")
        .downcast::<TastyVersionTooOld>()
        .expect("typed refusal");
    assert_eq!(
        err,
        TastyVersionTooOld {
            required: "0.15.0".into(),
            current: "0.14.9".into(),
        }
    );
    let text = err.to_string();
    assert!(text.contains("0.15.0") && text.contains("0.14.9"), "{text}");
}

#[test]
fn each_version_part_is_compared_as_a_number() {
    let src = source_with(r#"min_tasty_version = "1.10.2""#);
    for older in ["0.99.99", "1.9.99", "1.10.1"] {
        assert!(judge(&src, older).is_err(), "{older}");
    }
    for newer in ["1.10.2", "1.10.3", "1.11.0", "2.0.0"] {
        assert!(judge(&src, newer).is_ok(), "{newer}");
    }
}

#[test]
fn a_host_prerelease_counts_as_its_release_numbers() {
    assert!(
        judge(
            &source_with(r#"min_tasty_version = "0.15.0""#),
            "0.15.0-dev"
        )
        .is_ok()
    );
}

/// 이 판이 모르는 계약(선언 없는 popup 참조, 모르는 entry type, 다른 api_version)을 쓰는
/// 매니페스트도 선언 검사보다 먼저 "더 새 Tasty가 필요하다"로 거절된다.
#[test]
fn a_newer_requirement_is_reported_before_contracts_this_tasty_does_not_know() {
    let unknown_popup = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1.0"
        api_version = "1"
        min_tasty_version = "99.0.0"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        [contributes.commands.action]
        kind = "open_popup"
        popup_id = "com.example.x/ghost"
        [[contributes.popup]]
        id = "main"
        [contributes.popup.trigger]
        kind = "ipc"
    "#;
    let unknown_entry = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1.0"
        api_version = "1"
        min_tasty_version = "99.0.0"
        [entry]
        type = "wasm"
        module = "x.wasm"
    "#;
    let newer_api = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1.0"
        api_version = "2"
        min_tasty_version = "99.0.0"
        [entry]
        type = "process"
        command = "x"
    "#;
    for (name, src) in [
        ("unknown popup", unknown_popup),
        ("unknown entry", unknown_entry),
        ("api 2", newer_api),
    ] {
        // 이 판의 load는 각각 다른 사유로 거절한다.
        let typed = toml::from_str::<Manifest>(src)
            .map_err(anyhow::Error::from)
            .and_then(|m| m.validate());
        assert!(typed.is_err(), "{name}: this Tasty must not accept it");
        let err = judge(src, "0.14.0").expect_err(name);
        assert!(err.is::<TastyVersionTooOld>(), "{name}: {err}");
    }
}

#[test]
fn a_manifest_without_the_field_has_no_constraint() {
    let m = manifest_with("").expect("valid");
    assert_eq!(m.min_tasty_version, None);
    assert!(judge(&source_with(""), "0.0.0").is_ok());
}

#[test]
fn source_that_is_not_toml_is_left_to_load() {
    assert!(judge("not = [toml", "0.0.0").is_ok());
}

#[test]
fn locked_official_plugin_manifests_still_load_without_a_constraint() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tasty-test-support/fixtures/official-plugins");
    let mut seen = 0;
    for entry in std::fs::read_dir(&root).expect("fixtures") {
        let dir = entry.expect("entry").path();
        let path = dir.join("tasty-plugin.toml");
        if !path.exists() {
            continue;
        }
        Manifest::load(&dir).expect("fixture manifest loads");
        let source = std::fs::read_to_string(&path).expect("read");
        assert!(judge(&source, "0.0.0").is_ok(), "{dir:?}");
        seen += 1;
    }
    assert!(seen > 0, "no fixture manifests under {root:?}");
}

#[test]
fn a_malformed_min_tasty_version_is_a_manifest_error() {
    for bad in [
        "0.15",
        "0.15.0-rc.1",
        "0.15.0+build",
        "v0.15.0",
        "01.0.0",
        "",
        "0.15.0.1",
    ] {
        let line = format!(r#"min_tasty_version = "{bad}""#);
        let err = manifest_with(&line).expect_err(bad).to_string();
        assert!(err.contains("min_tasty_version"), "{bad}: {err}");
        // 비교 단계도 형식 오류로 거절한다(버전 부족이나 '제약 없음'이 아니다).
        let err = judge(&source_with(&line), "99.0.0").expect_err(bad);
        assert!(!err.is::<TastyVersionTooOld>(), "{bad}: {err}");
        assert!(
            err.to_string().contains("min_tasty_version"),
            "{bad}: {err}"
        );
    }
    // 문자열이 아닌 값은 TOML 역직렬화에서 거절되고, 비교 단계도 형식 오류로 거절한다.
    assert!(manifest_with("min_tasty_version = 15").is_err());
    let err = judge(&source_with("min_tasty_version = 15"), "99.0.0").expect_err("int");
    assert!(err.to_string().contains("min_tasty_version"), "{err}");
}

#[test]
fn a_larger_requirement_still_passes_validate_so_readers_can_load_it() {
    // 설치본 버전 읽기처럼 비교하지 않는 경로가 매니페스트를 읽을 수 있어야 한다.
    let m = manifest_with(r#"min_tasty_version = "99.0.0""#).expect("validate passes");
    assert_eq!(m.min_tasty_version.as_deref(), Some("99.0.0"));
}

/// 본체 버전의 prerelease·build 꼬리는 떼고 숫자 셋만 저장한다(develop의 `-dev` 같은 버전으로도 시작한다).
/// 이 바이너리가 쓰는 값(1.0.0·거절)과 숫자 셋이 같아 다른 시험과 충돌하지 않는다.
#[test]
fn the_host_version_setter_drops_prerelease_and_build_suffixes() {
    assert!(host_version::parse_release_version("0.14.0").is_some());
    assert!(host_version::parse_release_version("0.14.0-dev").is_none());
    assert!(set_host_tasty_version("not-a-version", VersionShortfall::Refuse).is_err());
    for version in ["1.0.0-rc.1", "1.0.0+build.5", "1.0.0"] {
        set_host_tasty_version(version, VersionShortfall::Refuse).expect(version);
    }
    assert_eq!(host_tasty_version(), Some(&semver::Version::new(1, 0, 0)));
}

/// 받아들이는 자리의 공용 단계는 타입 역직렬화·선언 검사보다 먼저 버전을 판정한다.
#[test]
fn load_for_host_reports_the_version_before_a_contract_this_tasty_does_not_know() {
    // 이 시험 바이너리에서 전역 본체 버전을 정하는 곳은 여기뿐이다.
    set_host_tasty_version("1.0.0", VersionShortfall::Refuse).expect("host version");
    let dir = tempfile::tempdir().expect("tempdir");
    let source = source_with(r#"min_tasty_version = "1.0.1""#).replace(
        "type = \"process\"\n        command = \"x\"",
        "type = \"wasm\"\n        module = \"x.wasm\"",
    );
    assert!(
        source.contains("wasm"),
        "fixture must use an unknown entry type"
    );
    std::fs::write(dir.path().join("tasty-plugin.toml"), source).expect("write");
    assert!(
        Manifest::load(dir.path()).is_err(),
        "this Tasty cannot read it"
    );
    let err = Manifest::load_for_host(dir.path()).expect_err("refused");
    assert!(err.is::<TastyVersionTooOld>(), "{err}");
    let ok = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        ok.path().join("tasty-plugin.toml"),
        source_with(r#"min_tasty_version = "1.0.0""#),
    )
    .expect("write");
    assert!(Manifest::load_for_host(ok.path()).is_ok());
}

/// 받아들이는 자리는 매니페스트를 한 번만 읽고, 판정한 원문을 그대로 역직렬화한다.
#[test]
fn load_for_host_reads_the_manifest_once() {
    set_host_tasty_version("1.0.0", VersionShortfall::Refuse).expect("host version");
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("tasty-plugin.toml"),
        source_with(r#"min_tasty_version = "1.0.0""#),
    )
    .expect("write");
    host_version::SOURCE_READS.with(|n| n.set(0));
    Manifest::load_for_host(dir.path()).expect("loads");
    assert_eq!(host_version::SOURCE_READS.with(|n| n.get()), 1);
}

/// 공식 번들은 TOML로 읽히지 않는 매니페스트를 복사 전에 거절한다.
/// 일반 받아들임 단계는 그 오류를 뒤의 load에 맡긴다.
#[test]
fn a_bundle_manifest_that_is_not_toml_is_refused_before_copy() {
    set_host_tasty_version("1.0.0", VersionShortfall::Refuse).expect("host version");
    let dir = tempfile::tempdir().expect("tempdir");
    let source = source_with("min_tasty_version = 0.15.0");
    assert!(
        source.parse::<toml::Table>().is_err(),
        "fixture must not be TOML"
    );
    std::fs::write(dir.path().join("tasty-plugin.toml"), source).expect("write");
    let err = Manifest::admit_bundle_host_tasty(dir.path()).expect_err("refused");
    assert!(err.to_string().contains("invalid manifest"), "{err}");
    assert!(Manifest::admit_host_tasty(dir.path()).is_ok());
}

//! `min_tasty_version`의 형식 검사와 본체 버전 비교.

use super::*;

fn manifest_with(line: &str) -> anyhow::Result<Manifest> {
    let src = format!(
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
    );
    let m: Manifest = toml::from_str(&src)?;
    m.validate()?;
    Ok(m)
}

fn host(v: &str) -> semver::Version {
    semver::Version::parse(v).expect("host version")
}

#[test]
fn a_manifest_requiring_a_newer_tasty_is_rejected_with_both_versions() {
    let m = manifest_with(r#"min_tasty_version = "0.15.0""#).expect("format is valid");
    let err = m
        .check_min_tasty_version(&host("0.14.9"))
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
    let m = manifest_with(r#"min_tasty_version = "1.10.2""#).expect("valid");
    for older in ["0.99.99", "1.9.99", "1.10.1"] {
        assert!(m.check_min_tasty_version(&host(older)).is_err(), "{older}");
    }
    for newer in ["1.10.2", "1.10.3", "1.11.0", "2.0.0"] {
        assert!(m.check_min_tasty_version(&host(newer)).is_ok(), "{newer}");
    }
}

#[test]
fn a_host_prerelease_counts_as_its_release_numbers() {
    let m = manifest_with(r#"min_tasty_version = "0.15.0""#).expect("valid");
    assert!(m.check_min_tasty_version(&host("0.15.0-dev")).is_ok());
}

#[test]
fn a_manifest_without_the_field_has_no_constraint() {
    let m = manifest_with("").expect("valid");
    assert_eq!(m.min_tasty_version, None);
    assert!(m.check_min_tasty_version(&host("0.0.0")).is_ok());
    assert!(m.require_host_tasty().is_ok());
}

#[test]
fn locked_official_plugin_manifests_still_load_without_a_constraint() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tasty-test-support/fixtures/official-plugins");
    let mut seen = 0;
    for entry in std::fs::read_dir(&root).expect("fixtures") {
        let dir = entry.expect("entry").path();
        if !dir.join("tasty-plugin.toml").exists() {
            continue;
        }
        let m = Manifest::load(&dir).expect("fixture manifest loads");
        assert!(m.check_min_tasty_version(&host("0.0.0")).is_ok(), "{dir:?}");
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
        let err = manifest_with(&format!(r#"min_tasty_version = "{bad}""#))
            .expect_err(bad)
            .to_string();
        assert!(err.contains("min_tasty_version"), "{bad}: {err}");
    }
    // 문자열이 아닌 값은 TOML 역직렬화에서 거절된다.
    assert!(manifest_with("min_tasty_version = 15").is_err());
}

#[test]
fn an_unvalidated_malformed_value_is_a_format_error_not_a_version_refusal() {
    let mut m = manifest_with("").expect("valid");
    m.min_tasty_version = Some("0.15".into());
    let err = m
        .check_min_tasty_version(&host("0.14.0"))
        .expect_err("format");
    assert!(err.downcast_ref::<TastyVersionTooOld>().is_none(), "{err}");
    assert!(err.to_string().contains("min_tasty_version"), "{err}");
}

#[test]
fn a_larger_requirement_still_passes_validate_so_readers_can_load_it() {
    // 번들 버전 읽기처럼 비교하지 않는 경로가 매니페스트를 읽을 수 있어야 한다.
    let m = manifest_with(r#"min_tasty_version = "99.0.0""#).expect("validate passes");
    assert_eq!(m.min_tasty_version.as_deref(), Some("99.0.0"));
}

#[test]
fn the_host_version_setter_accepts_only_release_versions() {
    assert!(host_version::parse_release_version("0.14.0").is_some());
    assert!(host_version::parse_release_version("0.14.0-dev").is_none());
    assert!(set_host_tasty_version("not-a-version").is_err());
}

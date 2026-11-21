//! 번들 플러그인 매니페스트(workspace와 공식 플러그인 픽스처)의 `ipc_namespace` 선언이 호스트의 실제 유효성 검증을
//! 통과하는지, 예약 prefix를 선언하면 그 이유로 거절되는지 확인한다.
//! 호스트 메서드와 겹치는 namespace를 플러그인의 수신 dispatch가 처리하는지는 플러그인 소스가 필요해
//! 공식 플러그인 레포가 검사한다.

const MANIFEST_NAME: &str = "tasty-plugin.toml";

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// 역직렬화와 유효성 검증은 별개다. 여기서는 매니페스트를 읽는다.
fn parse_manifest(dir: &std::path::Path) -> tasty_plugin_manifest::Manifest {
    let text = read(&dir.join(MANIFEST_NAME));
    toml::from_str(&text)
        .unwrap_or_else(|e| panic!("{}/{MANIFEST_NAME} 파싱 실패: {e}", dir.display()))
}

/// 2026-09-06 번들 매니페스트 9개를 측정한 뒤 빈 수집을 찾도록 둔 하한이다.
const MIN_BUNDLED_PLUGINS: usize = 6;

/// 번들 플러그인 매니페스트(workspace와 공식 플러그인 픽스처)에 실제 validate를 적용한다.
#[test]
fn every_bundled_manifest_passes_the_real_validation() {
    let dirs = tasty_test_support::bundled_plugin_manifest_dirs();
    assert!(
        dirs.len() >= MIN_BUNDLED_PLUGINS,
        "번들 매니페스트를 {}개만 읽었다(하한 {MIN_BUNDLED_PLUGINS}, 2026-09-06 측정 9개). 실제 플러그인 감소와 디렉터리 수집 실패를 구별한다. 하한 변경에는 측정 근거를 남긴다.",
        dirs.len()
    );
    for dir in &dirs {
        parse_manifest(dir).validate().unwrap_or_else(|e| {
            panic!(
                "{}/{MANIFEST_NAME}이 유효성 검증에 실패했다: {e}",
                dir.display()
            )
        });
    }
}

/// 검증된 실제 매니페스트의 prefix만 예약어로 바꿔 그 이유로 거절되는지 확인한다.
#[test]
fn the_validation_rejects_a_reserved_prefix() {
    let (dir, mut manifest) = tasty_test_support::bundled_plugin_manifest_dirs()
        .into_iter()
        .map(|d| {
            let m = parse_manifest(&d);
            (d, m)
        })
        .find(|(_, m)| !m.contributes.ipc_namespace.is_empty())
        .expect("예약 prefix 검사에 쓸 namespace 선언을 찾지 못했다");
    // 공식 플러그인의 계약은 잠금 파일이 가리키는 릴리스 산출물이다. 같은 id의 사본이 아니라 픽스처를 흔든다.
    assert_eq!(
        dir.parent(),
        Some(tasty_test_support::official_plugin_fixtures_root().as_path()),
        "namespace 선언을 공식 플러그인 픽스처가 아닌 곳에서 골랐다: {}",
        dir.display()
    );

    manifest
        .validate()
        .expect("prefix를 바꾸기 전 매니페스트부터 유효성 검사에 실패했다");

    let reserved = tasty_plugin_manifest::validators::RESERVED_IPC_PREFIXES
        .first()
        .expect("예약 목록이 비었다 — 흔들 값이 없다");
    manifest.contributes.ipc_namespace[0].prefix = (*reserved).to_string();
    let err = manifest
        .validate()
        .expect_err("예약된 prefix 를 선언했는데 검증이 통과했다");
    assert!(
        format!("{err}").contains(reserved),
        "거절은 했는데 이유가 그 prefix 가 아니다 — 다른 규칙에 먼저 걸렸다: {err}"
    );
}

//! macOS 번들 빌드 스크립트의 플러그인 위치와 Info.plist 설명 키·ATS 면제를 검사한다.
//! 정적 검사는 자동으로 헤드리스 조합(check-headless)에서 실행된다.
//! 실제 codesign 검사는 macOS에서만 실행할 수 있으며 이 자동 경로의 Linux 실행에는 포함되지 않는다.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// PLUGINS_DIR 할당의 $APP_DIR 뒤 경로를 읽어 빌드 스크립트와 같은 위치를 검사한다.
fn staged_plugins_rel_path() -> String {
    let script_path = repo_root().join("scripts/build-macos-dmg.sh");
    let script = std::fs::read_to_string(&script_path).expect("build-macos-dmg.sh read 실패");
    for line in script.lines() {
        let Some(rest) = line.trim_start().strip_prefix("PLUGINS_DIR=") else {
            continue;
        };
        let value = rest.trim().trim_matches('"');
        let rel = value.strip_prefix("$APP_DIR/").unwrap_or_else(|| {
            panic!(
                "PLUGINS_DIR 이 $APP_DIR 기준이 아님: {value} ({})",
                script_path.display()
            )
        });
        assert!(!rel.is_empty(), "PLUGINS_DIR 상대 경로가 빔");
        return rel.to_string();
    }
    panic!("PLUGINS_DIR 할당을 찾지 못함: {}", script_path.display());
}

/// 키 존재와 문자열만 검사하므로 셸 변수를 치환하지 않고 Info.plist heredoc을 읽는다.
fn info_plist_template() -> String {
    let script_path = repo_root().join("scripts/build-macos-dmg.sh");
    let script = std::fs::read_to_string(&script_path).expect("build-macos-dmg.sh read 실패");
    let mut body = String::new();
    let mut inside = false;
    for line in script.lines() {
        if !inside {
            if line.starts_with("cat > ")
                && line.contains("Info.plist")
                && line.ends_with("<< PLIST")
            {
                inside = true;
            }
            continue;
        }
        if line == "PLIST" {
            return body;
        }
        body.push_str(line);
        body.push('\n');
    }
    panic!(
        "Info.plist 생성 heredoc 을 찾지 못함: {}",
        script_path.display()
    );
}

/// 원격 HTTP 보기는 임의 호스트와 `*.tasty.localhost` 별칭을 로컬 endpoint를 거쳐 연다. ATS는 웹 뷰
/// 로드에만 꺼야 한다. `NSExceptionDomains`에 적은 도메인은 이 면제를 받지 못하므로 두지 않는다.
#[test]
fn info_plist_exempts_only_web_content_from_ats() {
    let plist = info_plist_template();
    let compact: String = plist.split_whitespace().collect();
    assert!(
        compact.contains(
            "<key>NSAppTransportSecurity</key><dict>\
<key>NSAllowsArbitraryLoadsInWebContent</key><true/></dict>"
        ),
        "Info.plist 에 웹 뷰 전용 ATS 면제가 없다"
    );
    assert!(
        !compact.contains("<key>NSAllowsArbitraryLoads</key>"),
        "ATS 면제는 웹 뷰로 한정한다"
    );
    assert!(
        !compact.contains("<key>NSExceptionDomains</key>"),
        "도메인 예외는 웹 뷰 면제를 덮는다"
    );
}

#[test]
fn info_plist_declares_tcc_usage_descriptions() {
    const REQUIRED_KEYS: &[&str] = &[
        "NSDownloadsFolderUsageDescription",
        "NSDocumentsFolderUsageDescription",
        "NSDesktopFolderUsageDescription",
        "NSRemovableVolumesUsageDescription",
        "NSNetworkVolumesUsageDescription",
    ];

    let plist = info_plist_template();
    let lines: Vec<&str> = plist.lines().map(str::trim).collect();
    for key in REQUIRED_KEYS {
        let marker = format!("<key>{key}</key>");
        let idx = lines
            .iter()
            .position(|l| *l == marker)
            .unwrap_or_else(|| panic!("Info.plist 에 {key} 가 없다"));
        let value = lines
            .get(idx + 1)
            .unwrap_or_else(|| panic!("{key} 뒤에 값이 없다"));
        let text = value
            .strip_prefix("<string>")
            .and_then(|v| v.strip_suffix("</string>"))
            .unwrap_or_else(|| panic!("{key} 값이 문자열이 아니다: {value}"));
        assert!(
            !text.trim().is_empty(),
            "{key} 설명 문구가 비었다 — 빈 문자열은 프롬프트에 아무것도 띄우지 않는다"
        );
    }
}

#[test]
fn staged_plugins_are_outside_contents_macos() {
    let rel = staged_plugins_rel_path();
    assert!(
        rel.starts_with("Contents/"),
        "plugin staging 경로가 번들 Contents/ 밖이다: {rel}"
    );
    assert!(
        !rel.starts_with("Contents/MacOS/"),
        "plugin 을 Contents/MacOS/ 하위에 staging 하면 codesign 이 그 디렉터리를 \
         nested bundle 로 파싱하려다 실패한다 (bundle format unrecognized). \
         현재 경로: {rel}"
    );
}

#[cfg(target_os = "macos")]
const PROBE_INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Probe</string>
    <key>CFBundleIdentifier</key>
    <string>com.zilhak.tasty.codesign-probe</string>
    <key>CFBundleExecutable</key>
    <string>probe</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
</dict>
</plist>
"#;

/// 스크립트의 플러그인 위치를 재현한 최소 번들을 만들어 ad-hoc 서명·검증한다.
#[cfg(target_os = "macos")]
#[test]
fn staged_layout_is_codesignable() {
    const DONOR_BIN: &str = "/bin/echo";

    let tmp = tempfile::tempdir().expect("tempdir 생성 실패");
    let app = tmp.path().join("Probe.app");
    let contents = app.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS")).expect("Contents/MacOS 생성 실패");
    std::fs::copy(DONOR_BIN, contents.join("MacOS/probe")).expect("메인 실행 파일 복사 실패");
    std::fs::write(contents.join("Info.plist"), PROBE_INFO_PLIST).expect("Info.plist 쓰기 실패");

    let plugin_dir = app.join(staged_plugins_rel_path()).join("com.tasty.probe");
    std::fs::create_dir_all(&plugin_dir).expect("plugin staging 디렉터리 생성 실패");
    std::fs::copy(DONOR_BIN, plugin_dir.join("tasty-plugin-probe"))
        .expect("plugin 바이너리 복사 실패");
    std::fs::write(
        plugin_dir.join("tasty-plugin.toml"),
        "id = \"com.tasty.probe\"\n",
    )
    .expect("plugin 매니페스트 쓰기 실패");

    run_codesign(&["--force", "--sign", "-"], &app, "서명");
    run_codesign(&["--verify", "--deep", "--strict"], &app, "검증");

    // 링커의 자동 서명과 구별하도록 _CodeSignature 생성도 확인한다.
    assert!(
        contents.join("_CodeSignature").is_dir(),
        "codesign 이 성공했다는데 _CodeSignature/ 가 없다"
    );
}

#[cfg(target_os = "macos")]
fn run_codesign(args: &[&str], app: &std::path::Path, what: &str) {
    let output = std::process::Command::new("codesign")
        .args(args)
        .arg(app)
        .output()
        .expect("codesign 실행 실패 (Xcode Command Line Tools 필요)");
    assert!(
        output.status.success(),
        "codesign {what} 실패 ({}):\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

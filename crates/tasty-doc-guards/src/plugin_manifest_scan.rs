//! 플러그인 매니페스트 최상위 값을 `scripts/lib/toml-scan.awk` 레코드로 읽는다.
//! sync 스크립트의 교체 전 검사와 같은 스캐너를 써서 셸과 시험이 같은 원문을 다르게 읽지 않게 한다.
//! 스캐너는 awk로 돌므로 파일·원문을 넘기는 함수는 Unix에서만 있다.

use std::collections::BTreeSet;
#[cfg(unix)]
use std::path::Path;

/// 스캐너가 읽은 매니페스트 최상위(첫 표 머리 앞) 값.
#[derive(Debug, PartialEq, Eq)]
pub struct TopLevel {
    pub id: Option<String>,
    pub version: Option<String>,
    /// 키가 없으면 호스트 기본값 true다.
    pub bundle: bool,
}

/// `toml-scan.awk` 출력(탭 구분 레코드)에서 최상위 `id`·`version`·`bundle`을 뽑는다. sync 스크립트의
/// 교체 전 검사와 같은 규칙이다: 읽지 못한 키, 불 값이 아닌 `bundle`, 두 번 나온 `id`·`version`·`bundle`은 오류다.
pub fn top_level(scan: &str) -> Result<TopLevel, String> {
    let mut top = TopLevel {
        id: None,
        version: None,
        bundle: true,
    };
    let mut seen = BTreeSet::new();
    let mut top_done = false;
    for record in scan.lines() {
        let fields: Vec<&str> = record.split('\t').collect();
        // 어느 위치든 첫 조각이 bundle 인 표([bundle], [bundle."x.y"])는 최상위 bundle 을 불 값이 아닌 표로
        // 만든다. 첫 조각을 디코딩하지 못한 표(unknown)도 bundle 일 수 있어 실패다. 빈 인용 키 [""] 는 bundle 이
        // 아니다.
        if let ["H", ..] = fields.as_slice() {
            match fields.get(3..5) {
                Some(["known", "bundle"]) => return Err("bundle 이 표다".to_string()),
                Some(["known", _]) => {}
                _ => return Err("표 머리의 첫 조각을 확신할 수 없다".to_string()),
            }
            top_done = true;
            continue;
        }
        if top_done {
            continue;
        }
        if let ["K", "dotted", _, _, _, _, "bundle", ..] = fields.as_slice() {
            return Err("bundle 이 점 구분 키로 표가 된다".to_string());
        }
        if let ["K", _, key @ ("id" | "version" | "bundle"), ..] = fields.as_slice()
            && !seen.insert(*key)
        {
            return Err(format!("최상위 {key} 가 두 번 나온다"));
        }
        match fields.as_slice() {
            ["E", why] => return Err(format!("스캐너가 읽지 못했다: {why}")),
            ["K", "unknown", ..] => return Err("최상위에 읽을 수 없는 키가 있다".to_string()),
            ["K", _, "bundle", "bool", v, ..] => top.bundle = *v == "true",
            ["K", _, "bundle", ..] => return Err("bundle 값이 불 값이 아니다".to_string()),
            ["K", _, "id", "basic" | "literal", v, ..] => top.id = Some((*v).to_string()),
            ["K", _, "version", "basic" | "literal", v, ..] => top.version = Some((*v).to_string()),
            _ => {}
        }
    }
    Ok(top)
}

#[cfg(unix)]
fn scanner() -> std::process::Command {
    let mut cmd = std::process::Command::new("awk");
    cmd.arg("-f")
        .arg(crate::repo_root().join("scripts/lib/toml-scan.awk"));
    cmd
}

/// 스캐너 출력. 실패하면 스캐너의 `E` 레코드 이유를 돌려준다.
#[cfg(unix)]
fn scanner_records(out: std::process::Output) -> Result<String, String> {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        return Ok(text);
    }
    let why = text
        .lines()
        .find_map(|l| l.strip_prefix("E\t"))
        .unwrap_or("이유 없음");
    Err(format!(
        "스캐너가 종료 {:?}로 끝났다: {why}",
        out.status.code()
    ))
}

/// 실제 매니페스트 파일을 경로 인자로 스캐너에 넘긴다. sync 스크립트와 같은 호출이다.
#[cfg(unix)]
pub fn scan_file(path: &Path) -> Result<String, String> {
    let out = scanner()
        .arg(path)
        .output()
        .map_err(|e| format!("awk를 실행하지 못했다: {e}"))?;
    scanner_records(out)
}

/// 짧은 회귀 입력을 표준 입력으로 넘긴다. 쓰기는 다른 스레드에서 해 출력 파이프가 차도 멈추지 않는다.
#[cfg(unix)]
pub fn scan_text(body: &str) -> Result<String, String> {
    use std::io::Write;
    let mut child = scanner()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("awk를 실행하지 못했다: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("awk 표준 입력을 열지 못했다")?;
    let body = body.to_owned();
    let writer = std::thread::spawn(move || stdin.write_all(body.as_bytes()));
    let out = child
        .wait_with_output()
        .map_err(|e| format!("awk를 기다리지 못했다: {e}"))?;
    writer
        .join()
        .map_err(|_| "awk에 원문을 넘기는 스레드가 panic했다".to_string())?
        .map_err(|e| format!("awk에 원문을 넘기지 못했다: {e}"))?;
    scanner_records(out)
}

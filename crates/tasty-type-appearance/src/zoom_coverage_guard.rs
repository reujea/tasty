//! 숫자를 직접 담은 LogicalPx 접근자에 self.ui_zoom이 있는지 소스에서 검사한다.
//! 본문에 이름이 있는지를 보는 텍스트 검사이며 실제 곱셈·반올림의 정확성을 증명하지는 않는다.
//! 다른 필드나 접근자로 위임하면 대상에서 제외하며 위임처의 배율 적용은 별도 검사에 맡긴다.
//!
//! CI의 lib 검사에서 실행되도록 이 위치에 둔다. theme.rs와 생성 접근자 파일을 읽으며
//! 컴파일 시점의 CARGO_MANIFEST_DIR를 사용한다. 접근자 수 하한으로 빈 수집을 거부한다.

use std::fs;
use std::path::PathBuf;

/// 접근자 수의 최소 하한. 수집이 줄면 파일·파서 변경 이유를 확인한 뒤 조정한다.
const ACCESSOR_FLOOR: usize = 70;

/// 스캔 대상 — 이 크레이트에서 `LogicalPx` 접근자를 정의하는 파일 전부.
const SCAN_FILES: &[&str] = &["theme.rs", "generated_component.rs"];

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// 접근자 하나의 판정 결과.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// 숫자와 self.ui_zoom이 모두 있다.
    ZoomAware,
    /// 숫자는 있지만 self.ui_zoom이 없다.
    LiteralWithoutZoom,
    /// 숫자 리터럴을 찾지 못했다. 위임 접근자는 이쪽으로 분류한다.
    Delegating,
}

/// 본문에 `LogicalPx(` 로 감싼 소수 리터럴이 있는지. `f32` 접미(`340.0f32`)도 같이 본다.
fn holds_numeric_literal(body: &str) -> bool {
    let mut rest = body;
    while let Some(at) = rest.find("LogicalPx(") {
        let after = &rest[at + "LogicalPx(".len()..];
        let after = after.strip_prefix('(').unwrap_or(after);
        let after = after.trim_start();
        if after.starts_with(|c: char| c.is_ascii_digit()) {
            return true;
        }
        rest = &rest[at + "LogicalPx(".len()..];
    }
    false
}

fn verdict_of(body: &str) -> Verdict {
    if !holds_numeric_literal(body) {
        Verdict::Delegating
    } else if body.contains("self.ui_zoom") {
        Verdict::ZoomAware
    } else {
        Verdict::LiteralWithoutZoom
    }
}

/// `pub fn <name>(&self) -> LogicalPx` 본문을 훑어 (이름, 판정) 을 모은다.
/// 본문의 끝은 rustfmt 가 내는 4칸 들여쓰기 `}` 다.
fn scan(text: &str) -> Vec<(String, Verdict)> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let name = line
            .trim_start()
            .strip_prefix("pub fn ")
            .filter(|_| line.contains("(&self) -> LogicalPx"))
            .and_then(|r| r.split('(').next())
            .map(str::to_string);
        if let Some(name) = name {
            let mut body = String::new();
            let mut j = i + 1;
            while j < lines.len() && lines[j] != "    }" {
                body.push_str(lines[j]);
                body.push('\n');
                j += 1;
            }
            out.push((name, verdict_of(&body)));
            i = j;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_all() -> Vec<(String, Verdict)> {
        let dir = src_dir();
        let mut all = Vec::new();
        for f in SCAN_FILES {
            let text = fs::read_to_string(dir.join(f)).unwrap_or_default();
            all.extend(scan(&text));
        }
        all
    }

    /// 수집한 숫자 접근자에 self.ui_zoom이 포함되는지 확인한다.
    #[test]
    fn every_literal_bearing_length_accessor_follows_ui_zoom() {
        let all = scan_all();
        let literal_bearing: Vec<_> = all
            .iter()
            .filter(|(_, v)| *v != Verdict::Delegating)
            .collect();
        assert!(
            literal_bearing.len() >= ACCESSOR_FLOOR,
            "숫자를 담은 접근자 {}개가 하한 {}개보다 적다. 파일과 파서 결과를 확인한다. 전체 접근자 {}개",
            literal_bearing.len(),
            ACCESSOR_FLOOR,
            all.len()
        );
        let violations: Vec<&str> = literal_bearing
            .iter()
            .filter(|(_, v)| *v == Verdict::LiteralWithoutZoom)
            .map(|(n, _)| n.as_str())
            .collect();
        assert!(
            violations.is_empty(),
            "길이 접근자 {}개에서 self.ui_zoom을 찾지 못했다: {:?}\n\
             (숫자를 담은 접근자 {}개 · 위임 접근자 {}개)",
            violations.len(),
            violations,
            literal_bearing.len(),
            all.len() - literal_bearing.len()
        );
    }

    /// 숫자와 배율 참조 유무에 따라 세 결과가 구분되는지 합성 입력으로 확인한다.
    #[test]
    fn the_verdict_discriminates_the_three_shapes() {
        assert_eq!(
            verdict_of("        LogicalPx((340.0 * self.ui_zoom).round())"),
            Verdict::ZoomAware
        );
        assert_eq!(
            verdict_of("        LogicalPx(340.0)"),
            Verdict::LiteralWithoutZoom
        );
        assert_eq!(
            verdict_of("        LogicalPx((340.0f32 * self.ui_zoom).round())"),
            Verdict::ZoomAware
        );
        assert_eq!(
            verdict_of("        self.item_height_interactive"),
            Verdict::Delegating
        );
        assert_eq!(
            verdict_of("        self.autocomplete_max_height()"),
            Verdict::Delegating
        );
        // 곱셈이 있어도 `ui_zoom` 이 아니면 위반이다(다른 배율 축을 곱한 경우).
        assert_eq!(
            verdict_of("        LogicalPx((340.0 * self.view_zoom).round())"),
            Verdict::LiteralWithoutZoom
        );
    }

    /// 알려진 실제 접근자를 읽어 수집이 의도한 파일과 함수를 대상으로 하는지 확인한다.
    #[test]
    fn the_scanner_finds_a_known_accessor_in_the_real_source() {
        let all = scan_all();
        let found = all
            .iter()
            .find(|(n, _)| n == "file_picker_note_max_width")
            .expect("실제 file_picker_note_max_width 접근자를 수집하지 못했다");
        assert_eq!(found.1, Verdict::ZoomAware);
        let delegating = all
            .iter()
            .find(|(n, _)| n == "input_height")
            .expect("input_height 를 못 찾았다");
        assert_eq!(delegating.1, Verdict::Delegating);
    }
}

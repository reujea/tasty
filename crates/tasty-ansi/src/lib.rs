#![forbid(unsafe_code)]

//! 터미널 버퍼와 출력 파서가 공유하는 ANSI escape 제거 함수.
//! 두 소비자의 정규식과 의존성을 별도로 관리하지 않도록 공용 크레이트에 둔다.

use std::sync::LazyLock;

use regex::Regex;

/// CSI 파라미터는 0x30–0x3F, intermediate는 0x20–0x2F, final은 0x40–0x7E다.
/// private mode와 콜론형 하위 파라미터도 포함한다.
static ANSI_ESCAPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-9:;<=>?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
        .expect("static regex is valid")
});

/// ANSI escape(CSI `\x1b[...`, OSC `\x1b]...\x07` / `\x1b]...\x1b\\`)를 제거한
/// 문자열을 돌려준다.
///
/// raw 문자열의 byte offset 매핑은 보존되지 않는다 — offset 은 결과 기준이다.
pub fn strip_ansi(s: &str) -> String {
    ANSI_ESCAPE_RE.replace_all(s, "").into_owned()
}

/// [`strip_ansi`]가 지우는 escape의 바이트 범위를 앞에서부터 겹치지 않게 돌려준다. 같은 정규식의 매치라
/// 이 범위를 지운 결과가 [`strip_ansi`]의 결과와 같다. 지운 바이트가 원문 어디서 왔는지 따로 대응시킬
/// 소비자(스캔 읽기의 출처 정보)가 쓴다.
pub fn ansi_escape_ranges(s: &str) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    ANSI_ESCAPE_RE.find_iter(s).map(|m| m.range())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_mode_sequences_are_stripped() {
        assert_eq!(strip_ansi("\x1b[?25lhidden\x1b[?25h"), "hidden");
        assert_eq!(strip_ansi("\x1b[?1049halt"), "alt");
        assert_eq!(strip_ansi("\x1b[?2004hpaste\x1b[?2004l"), "paste");
    }

    #[test]
    fn a_csi_with_a_non_alphabetic_final_byte_is_stripped() {
        assert_eq!(strip_ansi("\x1b[1@x"), "x");
        assert_eq!(strip_ansi("\x1b[2`y"), "y");
    }

    #[test]
    fn a_csi_with_an_intermediate_byte_is_stripped() {
        assert_eq!(strip_ansi("\x1b[2 qbar"), "bar");
    }

    #[test]
    fn csi_parameter_bytes_beyond_digits_are_stripped() {
        assert_eq!(strip_ansi("\x1b[4:3mwavy\x1b[0m"), "wavy");
        assert_eq!(strip_ansi("\x1b[38:2::255:0:0mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("\x1b[58:2::255:0:0mu\x1b[0m"), "u");
        assert_eq!(strip_ansi("\x1b[>4;1mx"), "x");
        assert_eq!(strip_ansi("\x1b[=1cy"), "y");
        assert_eq!(strip_ansi("\x1b[<0;12;3Mz"), "z");
    }

    #[test]
    fn sgr_and_osc_are_still_stripped() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("\x1b]0;title\x07after"), "after");
        assert_eq!(strip_ansi("\x1b]8;;http://x\x1b\\link"), "link");
    }

    #[test]
    fn csi_and_osc_in_one_line_are_both_stripped() {
        assert_eq!(
            strip_ansi("\x1b[31mred\x1b[0m \x1b]0;title\x07after"),
            "red after"
        );
    }

    /// Given 여러 종류의 escape가 섞인 문자열, When 매치 범위를 지우면, Then 정렬·비겹침이고 `strip_ansi`와 같다.
    #[test]
    fn removing_the_escape_ranges_gives_the_stripped_text() {
        let inputs = [
            "\x1b[31mred\x1b[0m \x1b]0;title\x07after",
            "API \x1b[2JError",
            "\x1b[?25l\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\",
            "plain? [0-9] text",
            "\x1b[\x1b[1m\x1b]unterminated",
            "",
        ];
        for input in inputs {
            let mut kept = String::new();
            let mut prev_end = 0;
            for r in ansi_escape_ranges(input) {
                assert!(r.start >= prev_end && r.start < r.end, "{input:?}: {r:?}");
                kept.push_str(&input[prev_end..r.start]);
                prev_end = r.end;
            }
            kept.push_str(&input[prev_end..]);
            assert_eq!(kept, strip_ansi(input), "{input:?}");
        }
    }

    #[test]
    fn plain_text_including_question_marks_is_untouched() {
        assert_eq!(strip_ansi("really? yes [0-9] ok"), "really? yes [0-9] ok");
        assert_eq!(strip_ansi("a<b >c =d ratio 3:4"), "a<b >c =d ratio 3:4");
    }
}

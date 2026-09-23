#![forbid(unsafe_code)]

//! ANSI escape 제거 — **레포에 하나뿐인 거처**.
//!
//! 이 함수는 원래 `tasty-terminal`(`strip_ansi_escapes`)과
//! `tasty-output`(`strip_ansi`)에 각각 하나씩 있었고, **이름이 달라서 갈라지는 것이
//! grep 으로도 안 보였다.** 실제로 갈라졌다 — 한쪽 문자군이 `[0-9;]` 라
//! `\x1b[?25l` 같은 private mode 가 그대로 남았고, `--strip-ansi` 를 켠 호출자가
//! escape 섞인 텍스트를 성공 코드와 함께 받았다. 실패하지 않고 약속을 절반만
//! 지키는 부류다.
//!
//! 두 소비자 크레이트는 서로를 흡수할 수 없다 — `tasty-output` 에 합치면
//! `tasty-terminal` 이 `serde`/`serde_json` 을, 반대로 합치면 `tasty-output` 이
//! `termwiz`/`portable-pty` 를 딸려온다. `tasty-utils` 는 `regex` 를 쓰는 크레이트보다
//! 훨씬 많은 크레이트가 의존해서, 거기에 `regex` 를 넣으면 일부만 쓰는 의존이 그 전부로
//! 퍼진다(두 수는 `crates/*/Cargo.toml` 에서 `^tasty-utils\s*=` 와 `^regex\s*=` 를 `git grep -l`
//! 로 센다). 그래서
//! 크기가 아니라 의존 방향으로 판정해 별도 크레이트로 세웠다 —
//! 근거는 `docs/adr/0089-crate-split-follows-dependency-direction.md`.

use std::sync::LazyLock;

use regex::Regex;

/// CSI 파라미터 바이트는 ECMA-48 이 `0x30-0x3F` 로 정한 **16 글자 전부**다 —
/// `0-9 : ; < = > ?`. 문자군은 그 범위를 그대로 적는다(`[0-9:;<=>?]`).
///
/// - `?` — private mode(`\x1b[?25l` 커서 숨김, `\x1b[?1049h` alt 스크린,
///   `\x1b[?2004h` bracketed paste)
/// - `:` — 서브파라미터(곱슬 밑줄 `\x1b[4:3m`, 콜론형 truecolor `\x1b[38:2::R:G:Bm`)
/// - `< = >` — private parameter prefix(SGR 마우스 `\x1b[<0;12;3M`, DA 질의
///   `\x1b[=1c`, modifyOtherKeys `\x1b[>4;1m`)
///
/// 종결 바이트도 알파벳만이 아니다 — CSI 의 final byte 대역은 `@-~` 이고
/// intermediate 바이트(`[ -/]`)가 그 앞에 올 수 있다(`\x1b[2 q` 커서 모양).
/// 문자군이나 종결 대역이 좁으면 그 시퀀스들이 **그대로 남는다.**
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `strip_ansi` 를 켜고도 escape 가 남아 있던 형태들. 전부 **private mode**(`?`)라
    /// 예전 문자군 `[0-9;]` 에 걸리지 않았다. 에이전트가 화면을 되읽어 문구를 찾을 때
    /// 이 잔재가 섞이면 매칭이 어긋난다 — 실제로 그렇게 잃은 적이 있다.
    #[test]
    fn private_mode_sequences_are_stripped() {
        assert_eq!(strip_ansi("\x1b[?25lhidden\x1b[?25h"), "hidden");
        assert_eq!(strip_ansi("\x1b[?1049halt"), "alt");
        assert_eq!(strip_ansi("\x1b[?2004hpaste\x1b[?2004l"), "paste");
    }

    /// 종결 바이트가 알파벳이 아닌 CSI 도 있다(`@-~`). 예전 `[a-zA-Z]` 는 못 잡았다.
    #[test]
    fn a_csi_with_a_non_alphabetic_final_byte_is_stripped() {
        assert_eq!(strip_ansi("\x1b[1@x"), "x");
        assert_eq!(strip_ansi("\x1b[2`y"), "y");
    }

    /// intermediate 바이트(`[ -/]`)가 파라미터와 종결 사이에 오는 형태.
    #[test]
    fn a_csi_with_an_intermediate_byte_is_stripped() {
        assert_eq!(strip_ansi("\x1b[2 qbar"), "bar");
    }

    /// ECMA-48 의 CSI 파라미터 바이트는 `0x30-0x3F` **전체**다 — `0-9 : ; < = > ?`.
    /// 최근 rustc/gcc 진단은 곱슬 밑줄 `\x1b[4:3m` 을 쓰고 nvim 은 `\x1b[>4;1m` 을
    /// 방출한다 — 남으면 파서의 plain text 매칭이 그만큼 어긋난다.
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

    /// 두 사본 시절 `tasty-output` 쪽에만 있던 형태 — CSI 와 OSC 가 한 줄에 섞인다.
    #[test]
    fn csi_and_osc_in_one_line_are_both_stripped() {
        assert_eq!(
            strip_ansi("\x1b[31mred\x1b[0m \x1b]0;title\x07after"),
            "red after"
        );
    }

    /// 본문의 같은 글자는 건드리지 않는다 — 문자군을 넓힌 대가로 평문을 먹으면 안 된다.
    #[test]
    fn plain_text_including_question_marks_is_untouched() {
        assert_eq!(strip_ansi("really? yes [0-9] ok"), "really? yes [0-9] ok");
        assert_eq!(strip_ansi("a<b >c =d ratio 3:4"), "a<b >c =d ratio 3:4");
    }
}

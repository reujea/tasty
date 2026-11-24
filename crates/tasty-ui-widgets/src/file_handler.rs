//! 본체와 갤러리가 공유하는 파일 핸들러 ID·경로의 말줄임 규칙.

use crate::tokens::{
    FH_ID_ELIDE_MAX, FH_ID_ELIDE_TAIL, FH_TARGET_ELIDE_FALLBACK, FH_TARGET_LINE_BOX,
    FH_TARGET_MONO_ADVANCE,
};
use tasty_type_geometry::length::LogicalPx;

/// 반복되는 벤더 접두사보다 핸들러 이름이 있는 끝을 보존하도록 앞부분을 줄인다.
pub fn elide_id_front(id: &str) -> String {
    let n = id.chars().count();
    if n <= FH_ID_ELIDE_MAX {
        return id.to_string();
    }
    let tail: String = id.chars().skip(n - FH_ID_ELIDE_TAIL).collect();
    format!("…{tail}")
}

/// 경로 한 줄에 들어갈 글자 수. mono_advance는 한 글자를 추가했을 때 늘어나는 폭이다.
/// 공칭 글리프 폭과 실제 배치 폭은 다를 수 있으므로 문자열을 배치해 측정해야 한다.
/// 값이 유효하지 않거나 한 글자도 들어가지 않으면 고정 fallback을 사용한다.
pub fn target_budget_chars(mono_advance: LogicalPx) -> usize {
    let w = mono_advance.value();
    if !w.is_finite() || w <= 0.0 {
        return FH_TARGET_ELIDE_FALLBACK;
    }
    let fits = (FH_TARGET_LINE_BOX.value() / w).floor();
    if !fits.is_finite() || fits < 1.0 {
        return FH_TARGET_ELIDE_FALLBACK;
    }
    fits as usize
}

/// 경로 상자를 잴 수 없을 때 추정에 쓰는 원격 표지 내용.
#[derive(Debug, Clone, Copy)]
pub struct UnmeasuredMark<'a> {
    /// 호스트 글자. 비면(낱말 Remote) 호스트 상한으로 본다.
    pub host: &'a str,
    /// 재연결 중이면 상태 단어.
    pub reconnecting: Option<&'a str>,
    /// 상태 단어의 언어. 그 언어의 label 글꼴이 고정폭이면 칸 수로, 아니면 글자 수로 폭을 추정한다.
    pub language: &'a str,
    /// 표지 글리프 크기(icon-glyph-size-xs 12).
    pub glyph: LogicalPx,
    /// 글리프 ↔ 호스트, 호스트 ↔ 상태 단어 간격(`fh-remote-gap` 4).
    pub mark_gap: LogicalPx,
    /// 호스트 글자 폭 상한(`fh-remote-max-width` 160). 빈 호스트와 폭을 모르는 호스트에 쓴다.
    pub host_cap: LogicalPx,
    /// UI sans 로 그리는 상태 단어(ja · zh-Hans)의 글자당 폭. 호출 쪽은 [`FH_STATUS_WORD_EM_UNMEASURED`](crate::tokens::FH_STATUS_WORD_EM_UNMEASURED)(12)를 넘긴다.
    pub em: LogicalPx,
}

/// 원격 표지 옆 경로 상자(경로 줄 − 표지 − `gap`)에 들어갈 글자 수.
/// 글자 폭이나 표지 폭을 잴 수 없으면 [`target_budget_chars_unmeasured`]로 추정한다.
pub fn target_budget_chars_beside(
    mono_advance: LogicalPx,
    mark_w: LogicalPx,
    gap: LogicalPx,
    unmeasured: &UnmeasuredMark<'_>,
) -> usize {
    let w = mono_advance.value();
    let room = (FH_TARGET_LINE_BOX - mark_w - gap).value();
    if !w.is_finite() || w <= 0.0 || !room.is_finite() {
        return target_budget_chars_unmeasured(unmeasured, gap);
    }
    let fits = (room / w).floor();
    if fits < 1.0 {
        return target_budget_chars_unmeasured(unmeasured, gap);
    }
    fits as usize
}

/// 잴 수 없을 때의 경로 글자 수(정본 Meta 'path cap, unmeasured'):
/// floor((366 − min(6 × 호스트 글자 수, 160) − 재연결이면 (4 + 상태 단어 폭)) / 6).
/// 366 은 경로 줄 390 − 글리프 12 − 간격 4 − 경로 앞 `gap`(space-sm 8)이다. 빈 호스트는 상한 160 이다.
/// 상태 단어 폭은 [`status_word_width_unmeasured`]로 추정한다.
pub fn target_budget_chars_unmeasured(mark: &UnmeasuredMark<'_>, gap: LogicalPx) -> usize {
    let host = if mark.host.is_empty() {
        mark.host_cap
    } else {
        (FH_TARGET_MONO_ADVANCE * mark.host.chars().count() as f32).min(mark.host_cap)
    };
    let word = mark.reconnecting.map_or(LogicalPx(0.0), |word| {
        mark.mark_gap + status_word_width_unmeasured(word, mark.language, mark.em)
    });
    let room = FH_TARGET_LINE_BOX - mark.glyph - mark.mark_gap - gap - host - word;
    (room.value() / FH_TARGET_MONO_ADVANCE.value())
        .floor()
        .max(1.0) as usize
}

/// 재연결 상태 단어의 폭을 잴 수 없을 때의 추정. 그 언어의 label 글꼴이 고정폭(en · ko)이면 칸당 6 이고
/// 한글 음절은 두 칸이다(reconnecting 72, 재연결 중 54). UI sans(ja · zh-Hans)면 글자당 12 다
/// (再接続中 48, 重新连接中 60).
pub fn status_word_width_unmeasured(word: &str, language: &str, em: LogicalPx) -> LogicalPx {
    if crate::label_font_family_for(language) == egui::FontFamily::Monospace {
        let cells: usize = word
            .chars()
            .map(|c| {
                if ('\u{AC00}'..='\u{D7A3}').contains(&c) {
                    2
                } else {
                    1
                }
            })
            .sum();
        FH_TARGET_MONO_ADVANCE * cells as f32
    } else {
        em * word.chars().count() as f32
    }
}

/// 파일명 끝을 보존하도록 경로 앞부분을 줄인다.
/// 먼저 디렉터리를 통째로 제거하고 …/를 붙인다. 마지막 조각도 길면 문자 단위로 줄인다.
pub fn elide_target_front(s: &str, budget_chars: usize) -> String {
    if s.chars().count() <= budget_chars {
        return s.to_string();
    }
    let parts: Vec<&str> = s.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    for keep in (1..parts.len()).rev() {
        let tail = parts[parts.len() - keep..].join("/");
        if tail.chars().count() + 2 <= budget_chars {
            return format!("…/{tail}");
        }
    }
    elide_chars_front(s, budget_chars)
}

/// 문자 단위 앞자름 — 맨 앞 `…` 한 글자를 포함해 `max` 문자가 된다.
fn elide_chars_front(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max == 0 {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::FH_TARGET_ELIDE_FALLBACK_REMOTE;

    /// 디자인에서 받은 경로 표본. 실제 잘리는 위치는 현재 글자 수 상한으로 검사한다.
    const A_FITS: &str = "packages/design-system/src/components/navigation/federated/Bar.ts";
    const B_SEGMENT: &str = "packages/design-system/src/components/navigation/federation/SidebarCategoryHeaderContrast.tsx";
    const C_ONE_SEGMENT: &str =
        "navigation-federation-sidebar-category-header-contrast-exploration-v2.md";

    /// 상한 안에 들어가는 짧은 경로는 그대로 반환한다.
    #[test]
    fn a_path_inside_the_budget_is_untouched() {
        let short = "docs/architecture.md";
        assert!(short.chars().count() <= FH_TARGET_ELIDE_FALLBACK);
        assert_eq!(elide_target_front(short, FH_TARGET_ELIDE_FALLBACK), short);
    }

    /// 표본 A는 상한과 같은 길이라 줄이지 않는다.
    #[test]
    fn the_first_design_sample_fits_the_budget_exactly() {
        assert_eq!(FH_TARGET_ELIDE_FALLBACK, 65);
        assert_eq!(A_FITS.chars().count(), FH_TARGET_ELIDE_FALLBACK);
        assert_eq!(elide_target_front(A_FITS, FH_TARGET_ELIDE_FALLBACK), A_FITS);
    }

    #[test]
    fn a_long_path_loses_whole_leading_segments() {
        assert_eq!(B_SEGMENT.chars().count(), 93);
        let out = elide_target_front(B_SEGMENT, FH_TARGET_ELIDE_FALLBACK);
        assert_eq!(
            out,
            "…/navigation/federation/SidebarCategoryHeaderContrast.tsx"
        );
        assert_eq!(out.chars().count(), 57);
        // 조각 경계에서 잘렸다 — `…/` 뒤는 온전한 디렉토리 이름이다.
        assert!(out.starts_with("…/navigation/"));
    }

    #[test]
    fn one_segment_longer_than_the_line_falls_to_a_character_cut() {
        assert_eq!(C_ONE_SEGMENT.chars().count(), 72);
        let out = elide_target_front(C_ONE_SEGMENT, FH_TARGET_ELIDE_FALLBACK);
        assert_eq!(out.chars().count(), 65);
        assert!(out.starts_with('…'));
        assert!(!out.starts_with("…/"));
        assert!(out.ends_with("-exploration-v2.md"));
    }

    /// …/ 접두사 두 글자도 상한에 포함되는지 경계 입력으로 확인한다.
    #[test]
    fn the_ellipsis_prefix_is_paid_for_out_of_the_budget() {
        let budget = FH_TARGET_ELIDE_FALLBACK;
        let mid = "m".repeat(budget - 1 - "/file.rs".chars().count());
        let path = format!("aaa/{mid}/file.rs");
        assert!(path.chars().count() > budget);
        // 조각 하나를 버린 꼬리는 예산에 들어가지만(`budget - 1`), 접두 두 글자까지
        // 세면 안 들어간다 — 그러면 조각을 하나 **더** 버려야 한다.
        assert_eq!(format!("{mid}/file.rs").chars().count(), budget - 1);
        let out = elide_target_front(&path, budget);
        assert!(
            out.chars().count() <= budget,
            "{} 자: {out}",
            out.chars().count()
        );
        assert_eq!(out, "…/file.rs");
    }

    /// 디자인 표본: 표지 = 12 + 4 + 호스트(6px 글자). 상자 = 390 − 표지 − 8.
    #[test]
    fn the_path_beside_a_remote_mark_gets_what_the_mark_leaves() {
        let mark = |host_px: f32| LogicalPx(12.0 + 4.0 + host_px);
        let unmeasured = UnmeasuredMark {
            host: "host",
            reconnecting: None,
            language: "en",
            glyph: LogicalPx(12.0),
            mark_gap: LogicalPx(4.0),
            host_cap: LogicalPx(160.0),
            em: LogicalPx(12.0),
        };
        let budget = |host_px| {
            target_budget_chars_beside(LogicalPx(6.0), mark(host_px), LogicalPx(8.0), &unmeasured)
        };
        assert_eq!(budget(24.0), 57, "host");
        assert_eq!(budget(108.0), 43, "work-laptop-attach");
        assert_eq!(budget(90.0), 46, "127.0.0.1:54321");
        assert_eq!(budget(160.0), 34, "host at the cap");
        // 글자 폭을 잴 수 없으면 호스트 글자 수로 추정한다('host' 57).
        assert_eq!(
            target_budget_chars_beside(LogicalPx(0.0), mark(24.0), LogicalPx(8.0), &unmeasured),
            57
        );
        // 93자 표본은 work-laptop-attach 옆에서 조각 경계로, 상한에서는 한 조각 안에서 잘린다.
        assert_eq!(
            elide_target_front(B_SEGMENT, budget(108.0)),
            "…/SidebarCategoryHeaderContrast.tsx"
        );
        assert_eq!(
            elide_target_front(B_SEGMENT, budget(160.0)),
            "…SidebarCategoryHeaderContrast.tsx"
        );
    }

    /// 잴 수 없을 때의 경로 글자 수(정본 Meta 'path cap, unmeasured'). 호스트는 6 × 글자 수(상한 160),
    /// 빈 호스트와 상한은 34 다. 재연결 중이면 4 + 상태 단어 폭을 더 빼고, 그 폭은 언어별로 추정한다.
    #[test]
    fn the_unmeasured_path_cap_follows_the_host_and_the_status_word_by_language() {
        let cap = |host, reconnecting, language| {
            let mark = UnmeasuredMark {
                host,
                reconnecting,
                language,
                glyph: LogicalPx(12.0),
                mark_gap: LogicalPx(4.0),
                host_cap: LogicalPx(160.0),
                em: LogicalPx(12.0),
            };
            // 글자 폭이 유효하지 않은 실제 경로로도 같은 값이 나온다.
            let beside = target_budget_chars_beside(
                LogicalPx(f32::NAN),
                LogicalPx(40.0),
                LogicalPx(8.0),
                &mark,
            );
            assert_eq!(
                beside,
                target_budget_chars_unmeasured(&mark, LogicalPx(8.0))
            );
            beside
        };
        assert_eq!(cap("host", None, "en"), 57);
        assert_eq!(cap("work-laptop-attach", None, "en"), 43);
        assert_eq!(
            cap("a-very-long-attach-profile-name-for-the-cap", None, "en"),
            34,
            "at the cap"
        );
        assert_eq!(
            cap("", None, "en"),
            FH_TARGET_ELIDE_FALLBACK_REMOTE,
            "no host"
        );
        assert_eq!(
            cap("", None, "ja"),
            FH_TARGET_ELIDE_FALLBACK_REMOTE,
            "no host, ja"
        );

        // 상태 단어 폭: en·ko 는 칸당 6(한글 음절 두 칸), ja·zh-Hans 는 글자당 12.
        for (word, language, width) in [
            ("reconnecting", "en", 72.0),
            ("재연결 중", "ko", 54.0),
            ("再接続中", "ja", 48.0),
            ("重新连接中", "zh-Hans", 60.0),
        ] {
            assert_eq!(
                status_word_width_unmeasured(word, language, LogicalPx(12.0)),
                LogicalPx(width),
                "{language}"
            );
            // 상한 호스트 옆: floor((366 − 160 − 4 − 폭) / 6).
            let expected = ((366.0 - 160.0 - 4.0 - width) / 6.0_f32).floor() as usize;
            assert_eq!(cap("", Some(word), language), expected, "{language}");
        }
        assert_eq!(cap("", Some("reconnecting"), "en"), 21);
        assert_eq!(cap("", Some("재연결 중"), "ko"), 24);
        assert_eq!(cap("", Some("再接続中"), "ja"), 25);
        assert_eq!(cap("", Some("重新连接中"), "zh-Hans"), 23);
        // 'host' 옆 재연결: floor((366 − 24 − 4 − 72) / 6) = 44.
        assert_eq!(cap("host", Some("reconnecting"), "en"), 44);
    }

    #[test]
    fn the_budget_is_measured_when_a_glyph_width_is_available() {
        // 390 / 6.0 = 65. 좁은 글리프면 더 많이 들어간다 — 상수였다면 안 움직였을 값이다.
        assert_eq!(target_budget_chars(LogicalPx(6.0)), 65);
        assert_eq!(target_budget_chars(LogicalPx(3.9)), 100);
    }

    /// fallback이 문자열 배치 증가분으로 계산한 글자 수와 같은지 확인한다.
    #[test]
    fn the_derived_cap_is_what_the_measurement_would_have_given() {
        assert_eq!(FH_TARGET_LINE_BOX.value(), 390.0);
        assert_eq!(FH_TARGET_MONO_ADVANCE.value(), 6.0);
        assert_eq!(
            target_budget_chars(FH_TARGET_MONO_ADVANCE),
            FH_TARGET_ELIDE_FALLBACK,
            "fallback은 문자열 배치 증가분으로 계산한 글자 수와 같아야 한다"
        );
        // 공칭 글리프 폭을 쓰면 더 큰 글자 수가 나오는 차이도 확인한다.
        assert_eq!(target_budget_chars(LogicalPx(5.5556)), 70);
    }

    #[test]
    fn an_unmeasurable_glyph_width_falls_back_to_the_derived_cap() {
        assert_eq!(
            target_budget_chars(LogicalPx(0.0)),
            FH_TARGET_ELIDE_FALLBACK
        );
        assert_eq!(
            target_budget_chars(LogicalPx(-1.0)),
            FH_TARGET_ELIDE_FALLBACK
        );
        assert_eq!(
            target_budget_chars(LogicalPx(f32::NAN)),
            FH_TARGET_ELIDE_FALLBACK
        );
        // 글리프가 줄보다 넓으면 한 글자도 안 들어간다 — 그것도 못 잰 것으로 친다.
        assert_eq!(
            target_budget_chars(FH_TARGET_LINE_BOX + LogicalPx(1.0)),
            FH_TARGET_ELIDE_FALLBACK
        );
    }

    #[test]
    fn a_long_id_keeps_its_tail() {
        let id = "net.example.enterprise.documents.attachments/inline-preview-handler";
        let out = elide_id_front(id);
        assert_eq!(out.chars().count(), FH_ID_ELIDE_MAX);
        assert!(out.starts_with('…'));
        assert!(out.ends_with("inline-preview-handler"));
    }

    #[test]
    fn a_short_id_is_untouched() {
        assert_eq!(
            elide_id_front("com.tasty.text/editor"),
            "com.tasty.text/editor"
        );
    }
}

//! 콜아웃 종류 표(GFM 다섯 종 + Obsidian 확장·별칭)와 종류별 CSS·아이콘.

use pulldown_cmark::BlockQuoteKind;
use tasty_type_appearance::theme::Theme;

use super::percent_encode_fragment;

// ── callouts (GFM `> [!NOTE]` alerts + Obsidian-style `> [!type]+ Title`) ──────

/// 콜아웃 한 종류의 표시 이름, CSS 클래스와 아이콘.
pub(super) struct CalloutKind {
    /// 파서가 구분한 GFM 콜아웃 종류. 확장 종류는 None이다.
    pub(super) gfm_kind: Option<BlockQuoteKind>,
    /// Lowercase Obsidian tag text this entry answers to (e.g. `"note"`, `"info"`) — matched
    /// case-insensitively against the `[!type]` token via [`find_callout_kind`]. For the 5 GFM
    /// kinds this is simply their lowercase name, so a bare `[!note]` and an Obsidian-flavored
    /// `[!note]+ Title` resolve to the same entry either way.
    pub(super) type_key: &'static str,
    /// The literal class this kind renders as (mirrors pulldown-cmark's own `html.rs` naming
    /// for the 5 GFM kinds, kept identical so existing CSS/snapshots don't need to change).
    pub(super) class: &'static str,
    /// `Translator` key for the default header label (used whenever no custom title follows the
    /// tag) — must exist in `lang/{en,ko,ja}.toml`.
    pub(super) label_key: &'static str,
    /// [`tasty_icons`] glyph. Its `body` (no wrapping `<svg>`, no color baked in) and `filled`
    /// (`true` colors it via `fill`, `false` via `stroke`) feed [`alert_icon_data_uri`].
    pub(super) icon: tasty_icons::Icon,
    /// 테마에서 콜아웃 강조색을 가져온다.
    pub(super) accent: fn(&Theme) -> tasty_type_appearance::color::HexColor,
}

/// 지원하는 콜아웃 종류. 별칭은 CALLOUT_ALIASES에서 정규 이름에 연결한다.
pub(super) const CALLOUT_KINDS: &[CalloutKind] = &[
    CalloutKind {
        gfm_kind: Some(BlockQuoteKind::Note),
        type_key: "note",
        class: "markdown-alert-note",
        label_key: "markdown.alert.note",
        icon: tasty_icons::ALERT_CIRCLE,
        accent: Theme::accent_primary,
    },
    CalloutKind {
        gfm_kind: Some(BlockQuoteKind::Tip),
        type_key: "tip",
        class: "markdown-alert-tip",
        label_key: "markdown.alert.tip",
        icon: tasty_icons::STAR_FILL,
        accent: Theme::accent_success,
    },
    CalloutKind {
        gfm_kind: Some(BlockQuoteKind::Important),
        type_key: "important",
        class: "markdown-alert-important",
        label_key: "markdown.alert.important",
        icon: tasty_icons::BELL,
        accent: Theme::accent_agent,
    },
    CalloutKind {
        gfm_kind: Some(BlockQuoteKind::Warning),
        type_key: "warning",
        class: "markdown-alert-warning",
        label_key: "markdown.alert.warning",
        icon: tasty_icons::ALERT_TRIANGLE,
        accent: Theme::accent_warning,
    },
    CalloutKind {
        gfm_kind: Some(BlockQuoteKind::Caution),
        type_key: "caution",
        class: "markdown-alert-caution",
        label_key: "markdown.alert.caution",
        icon: tasty_icons::CLOSE,
        accent: Theme::accent_danger,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "abstract",
        class: "markdown-alert-abstract",
        label_key: "markdown.alert.abstract",
        icon: tasty_icons::LIST,
        accent: Theme::accent_primary,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "info",
        class: "markdown-alert-info",
        label_key: "markdown.alert.info",
        icon: tasty_icons::ALERT_CIRCLE,
        accent: Theme::accent_info,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "todo",
        class: "markdown-alert-todo",
        label_key: "markdown.alert.todo",
        icon: tasty_icons::CHECK,
        accent: Theme::accent_info,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "success",
        class: "markdown-alert-success",
        label_key: "markdown.alert.success",
        icon: tasty_icons::CHECK,
        accent: Theme::accent_success,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "question",
        class: "markdown-alert-question",
        label_key: "markdown.alert.question",
        icon: tasty_icons::HELP_CIRCLE,
        accent: Theme::accent_attention,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "failure",
        class: "markdown-alert-failure",
        label_key: "markdown.alert.failure",
        icon: tasty_icons::ALERT_TRIANGLE,
        accent: Theme::accent_danger,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "danger",
        class: "markdown-alert-danger",
        label_key: "markdown.alert.danger",
        icon: tasty_icons::CLOSE,
        accent: Theme::accent_danger,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "bug",
        class: "markdown-alert-bug",
        label_key: "markdown.alert.bug",
        icon: tasty_icons::CLOSE,
        accent: Theme::accent_agent,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "example",
        class: "markdown-alert-example",
        label_key: "markdown.alert.example",
        icon: tasty_icons::SCRIPT,
        accent: Theme::accent_agent,
    },
    CalloutKind {
        gfm_kind: None,
        type_key: "quote",
        class: "markdown-alert-quote",
        label_key: "markdown.alert.quote",
        icon: tasty_icons::TEXT_LEFT,
        accent: Theme::accent_primary,
    },
];

/// 콜아웃 별칭과 정규 이름.
const CALLOUT_ALIASES: &[(&str, &str)] = &[
    ("summary", "abstract"),
    ("tldr", "abstract"),
    ("hint", "tip"),
    ("check", "success"),
    ("done", "success"),
    ("help", "question"),
    ("faq", "question"),
    ("fail", "failure"),
    ("missing", "failure"),
    ("error", "danger"),
    ("cite", "quote"),
];

/// 소문자로 정규화된 이름을 정규 이름 및 별칭과 비교한다.
pub(super) fn find_callout_kind(type_key: &str) -> Option<&'static CalloutKind> {
    if let Some(found) = CALLOUT_KINDS.iter().find(|k| k.type_key == type_key) {
        return Some(found);
    }
    let canonical = CALLOUT_ALIASES
        .iter()
        .find(|(alias, _)| *alias == type_key)?
        .1;
    CALLOUT_KINDS.iter().find(|k| k.type_key == canonical)
}

/// 강조색 채움의 알파 바이트. design `--tasty-tint-fill-alpha` 를 CSS `#rrggbbaa` 의 알파로 옮긴다.
/// 콜아웃 채움과 diff 추가·삭제 줄 배경이 같이 쓴다.
pub(super) fn tint_fill_alpha(theme: &Theme) -> u8 {
    (theme.tint_fill_alpha() * f32::from(u8::MAX)).round() as u8
}

/// 콜아웃 종류별 테마 색과 아이콘 CSS를 만든다.
pub(super) fn alert_css(theme: &Theme) -> String {
    let bg_alpha = tint_fill_alpha(theme);
    let mut rules = String::new();
    for kind in CALLOUT_KINDS {
        let color = (kind.accent)(theme);
        let hex = color.to_hex();
        let icon_uri = alert_icon_data_uri(kind.icon.body, kind.icon.filled, &hex);
        // 접힘 마커는 summary 배경의 chevron 이다. 닫히면 오른쪽, 열리면 아래를 가리킨다.
        let closed_uri = alert_icon_data_uri(
            tasty_icons::CHEVRON_RIGHT.body,
            tasty_icons::CHEVRON_RIGHT.filled,
            &hex,
        );
        let open_uri = alert_icon_data_uri(
            tasty_icons::CHEVRON_DOWN.body,
            tasty_icons::CHEVRON_DOWN.filled,
            &hex,
        );
        rules.push_str(&format!(
            ".{class}{{border-left-color:{hex};background:{bg};}}.{class}>.tasty-callout-label::before,.{class}[data-label]::before,.{class}>summary::before{{color:{hex};background-image:url(\"{icon_uri}\");}}.{class}>summary{{background-image:url(\"{closed_uri}\");}}.{class}[open]>summary{{background-image:url(\"{open_uri}\");}}\n",
            class = kind.class,
            bg = color.with_alpha(bg_alpha).to_hex(),
        ));
    }
    rules
}

/// 지정한 색의 SVG를 data URI로 만든다.
pub(super) fn alert_icon_data_uri(icon_body: &str, filled: bool, color_hex: &str) -> String {
    let (fill, stroke) = if filled {
        (color_hex, color_hex)
    } else {
        ("none", color_hex)
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="{fill}" stroke="{stroke}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{icon_body}</svg>"#,
    );
    format!("data:image/svg+xml,{}", percent_encode_fragment(&svg))
}

#[cfg(test)]
mod tests {
    use tasty_type_appearance::theme::Theme;

    /// 토큰으로 옮기기 전 stylesheet 가 쓰던 CSS 알파 바이트(`#rrggbb1f`). 출력 불변을 확인하는 기준값이다.
    const PREVIOUS_CSS_ALPHA: u8 = 31;

    /// 토큰 0.12 는 CSS 알파 31 이다. 콜아웃 채움과 diff 줄 배경의 출력이 그대로여야 한다.
    #[test]
    fn tint_fill_alpha_is_the_31_byte_the_stylesheet_writes() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        assert_eq!(super::tint_fill_alpha(&theme), PREVIOUS_CSS_ALPHA);
        let css = super::alert_css(&theme);
        let note = theme
            .accent_primary()
            .with_alpha(PREVIOUS_CSS_ALPHA)
            .to_hex();
        assert!(css.contains(&format!("background:{note};")), "{css}");
    }

    /// 접힘 마커는 종류 색 chevron 이다. 닫힘은 오른쪽, 열림은 아래 글리프로 서로 다르다.
    #[test]
    fn foldable_marker_is_a_kind_coloured_chevron_that_turns_when_open() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let css = super::alert_css(&theme);
        let hex = theme.accent_primary().to_hex();
        let closed = super::alert_icon_data_uri(
            tasty_icons::CHEVRON_RIGHT.body,
            tasty_icons::CHEVRON_RIGHT.filled,
            &hex,
        );
        let open = super::alert_icon_data_uri(
            tasty_icons::CHEVRON_DOWN.body,
            tasty_icons::CHEVRON_DOWN.filled,
            &hex,
        );
        assert_ne!(closed, open);
        assert!(
            css.contains(&format!(
                ".markdown-alert-note>summary{{background-image:url(\"{closed}\");}}"
            )),
            "{css}"
        );
        assert!(
            css.contains(&format!(
                ".markdown-alert-note[open]>summary{{background-image:url(\"{open}\");}}"
            )),
            "{css}"
        );
    }
}

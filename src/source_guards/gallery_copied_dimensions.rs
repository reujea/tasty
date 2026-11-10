//! 본체와 갤러리가 따로 정의한 치수를 등록해 기본값을 비교한다.
//! 갤러리가 본체 바이너리를 직접 참조할 수 없어 남긴 복사본을 검사한다. 공용 정의를 사용할 수 있게 되면
//! 양쪽을 그 정의로 옮기고 비교 항목을 제거한다. 어느 치수가 서로 대응하는지는 사람이 판단한다.
//!
//! 리터럴·생성 토큰·Theme 기본값의 합을 읽는다. ThemeSum은 파일에 필드 이름이 있는지도 확인하지만
//! 실제로 그 치수를 계산하는 표현식인지까지 증명하지 않는다. 기본 테마의 값이 같아도 다른 테마에서
//! 고정값과 Theme 계산 결과는 달라질 수 있다. 실패하면 디자인 의도에 따라 사용할 값을 정한다.
//!
//! 본체를 언급한 상수 주석과 공용 토큰 경로도 명부와 대조한다. 등록되지 않고 본체도 언급하지 않는
//! 새 복사본은 놓칠 수 있다. 출력 표는 --nocapture로 볼 수 있으며 통과한 시험의 기본 출력에는 숨겨진다.

use tasty_doc_guards::source_text::mask_non_code;

const THEME: &str = "crates/tasty-type-appearance/src/theme.rs";
const SEMANTIC: &str = "crates/tasty-design-tokens/src/generated/semantic.rs";
const PRIMITIVE: &str = "crates/tasty-design-tokens/src/generated/primitive.rs";

/// 양쪽 모두 리터럴·토큰·Theme 합 중 하나로 표현한다.
#[derive(Clone, Copy)]
enum Side {
    /// 직접 적은 LogicalPx 또는 숫자 값.
    Lit(&'static str, &'static str),
    /// 생성 토큰을 가리키는 상수.
    Alias(&'static str, &'static str),
    /// 등록된 Theme 필드 기본값의 합.
    ThemeSum(&'static str, &'static [&'static str]),
}

const COPIED: &[(&str, Side, Side)] = &[
    (
        "Plugins 세그먼트 탭 라벨 폰트(primitive 12 — semantic role 없음)",
        Side::Lit("src/view/plugins/ui.rs", "SEGMENT_TAB_LABEL_PRIMITIVE_12"),
        Side::Lit(GALLERY_PLUGINS_WINDOW, "SEGMENT_TAB_LABEL_PRIMITIVE_12"),
    ),
    (
        "Plugins Attention 본문 폰트(primitive 12 — semantic role 없음)",
        Side::Lit("src/view/plugins/ui/attention.rs", "ATTN_PRIMITIVE_12"),
        Side::Lit(GALLERY_PLUGINS_ATTENTION, "ATTN_PRIMITIVE_12"),
    ),
    (
        "popup 제목바 높이",
        Side::ThemeSum("src/adapters/ui/popup.rs", &["item_height_interactive"]),
        Side::Lit(GALLERY_POPUP_FRAME, "TITLE_BAR_HEIGHT"),
    ),
    (
        "popup 콘텐츠 상하 여백",
        Side::ThemeSum("src/adapters/ui/popup.rs", &["spacing_xs"]),
        Side::Alias(GALLERY_POPUP_FRAME, "CONTENT_MARGIN"),
    ),
    (
        "상태바 좌우 여백",
        Side::Lit("crates/tasty-ui-widgets/src/status_bar.rs", "BAR_PAD_X"),
        Side::Lit(
            "crates/tasty-gallery/src/catalog/components/status_bar_theme_cell.rs",
            "BAR_PAD_X",
        ),
    ),
    (
        "종료 확인 창 폭",
        Side::Lit("src/app/modal/quit.rs", "WINDOW_W"),
        Side::Lit(GALLERY_QUIT_MODAL, "WINDOW_W"),
    ),
    (
        "종료 확인 창 높이",
        Side::Lit("src/app/modal/quit.rs", "WINDOW_H"),
        Side::Lit(GALLERY_QUIT_MODAL, "WINDOW_H"),
    ),
    (
        "프리셋 leaf 요약 표시 폭 임계",
        Side::Lit(
            "src/adapters/ui/preset/demo_layout.rs",
            "LEAF_SUMMARY_MIN_W",
        ),
        Side::Lit(GALLERY_PRESET_EDITOR, "LEAF_SUMMARY_MIN_W"),
    ),
    (
        "프리셋 leaf 요약 표시 높이 임계",
        Side::Lit(
            "src/adapters/ui/preset/demo_layout.rs",
            "LEAF_SUMMARY_MIN_H",
        ),
        Side::Lit(GALLERY_PRESET_EDITOR, "LEAF_SUMMARY_MIN_H"),
    ),
    // 본체를 언급하지 않은 상수도 대응 관계를 확인해 수동으로 등록한다. 이름·값이 같다는 사실만으로 짝을 자동 결정하지 않는다.
    (
        "프리셋 편집기 탭 추가 버튼 폭",
        Side::Lit(HOST_PRESET_DEMO, "ADD_TAB_W"),
        Side::Lit(GALLERY_PRESET_EDITOR, "ADD_TAB_W"),
    ),
    (
        "프리셋 leaf 본문 여백",
        Side::Lit(HOST_PRESET_DEMO, "BODY_PAD"),
        Side::Lit(GALLERY_PRESET_EDITOR, "BODY_PAD"),
    ),
    (
        "프리셋 탭 닫기 히트 영역",
        Side::Lit(HOST_PRESET_DEMO, "CLOSE_HIT"),
        Side::Lit(GALLERY_PRESET_EDITOR, "CLOSE_HIT"),
    ),
    (
        "프리셋 탭 닫기 버튼 바깥 여백",
        Side::Lit(HOST_PRESET_DEMO, "CLOSE_MARGIN"),
        Side::Lit(GALLERY_PRESET_EDITOR, "CLOSE_MARGIN"),
    ),
    (
        "프리셋 탭 라벨↔닫기 사이 여백",
        Side::Lit(HOST_PRESET_DEMO, "CLOSE_TAB_PAD"),
        Side::Lit(GALLERY_PRESET_EDITOR, "CLOSE_TAB_PAD"),
    ),
    (
        "프리셋 leaf 사이 간격",
        Side::Lit(HOST_PRESET_DEMO, "LEAF_GAP"),
        Side::Lit(GALLERY_PRESET_EDITOR, "LEAF_GAP"),
    ),
    (
        "프리셋 leaf 아이콘만 표시 임계",
        Side::Lit(HOST_PRESET_DEMO, "LEAF_ICON_ONLY_MIN"),
        Side::Lit(GALLERY_PRESET_EDITOR, "LEAF_ICON_ONLY_MIN"),
    ),
    (
        "프리셋 leaf 탭 스트립 높이",
        Side::Lit(HOST_PRESET_DEMO, "STRIP_H"),
        Side::Lit(GALLERY_PRESET_EDITOR, "STRIP_H"),
    ),
    (
        "프리셋 탭 좌우 여백",
        Side::Lit(HOST_PRESET_DEMO, "TAB_PAD_X"),
        Side::Lit(GALLERY_PRESET_EDITOR, "TAB_PAD_X"),
    ),
    (
        "원격 attach 좌측 프로필 열 폭",
        Side::Lit(HOST_REMOTE_ATTACH, "LEFT_W"),
        Side::Lit(GALLERY_REMOTE_ATTACH, "LEFT_W"),
    ),
    (
        "원격 attach 헤더 높이",
        Side::Lit(HOST_REMOTE_ATTACH, "HEADER_H"),
        Side::Lit(GALLERY_REMOTE_ATTACH, "HEADER_H"),
    ),
    (
        "원격 attach 헤더 좌측 여백",
        Side::Lit(HOST_REMOTE_ATTACH, "HEADER_PAD_L"),
        Side::Lit(GALLERY_REMOTE_ATTACH, "HEADER_PAD_L"),
    ),
    (
        "원격 attach 푸터 높이",
        Side::Lit(HOST_REMOTE_ATTACH, "FOOTER_H"),
        Side::Lit(GALLERY_REMOTE_ATTACH, "FOOTER_H"),
    ),
    (
        "원격 attach 프로필 행 높이",
        Side::Lit(HOST_REMOTE_ATTACH, "PROFILE_ROW_H"),
        Side::Lit(GALLERY_REMOTE_ATTACH, "PROFILE_ROW_H"),
    ),
    (
        "원격 attach 워크스페이스 행 높이",
        Side::Lit(HOST_REMOTE_ATTACH, "WS_ROW_H"),
        Side::Lit(GALLERY_REMOTE_ATTACH, "WS_ROW_H"),
    ),
    (
        "파일 picker 경로 구분 글리프 크기",
        Side::Lit(HOST_FILE_PICKER, "CRUMB_GLYPH"),
        Side::Lit(GALLERY_FILE_PICKER, "CRUMB_GLYPH"),
    ),
    (
        "튜토리얼 말풍선 폭",
        Side::Lit(HOST_TUTORIAL_CALLOUT, "CALLOUT_W"),
        Side::Lit(GALLERY_TUTORIAL, "CALLOUT_W"),
    ),
    (
        "튜토리얼 말풍선 꼬리 크기",
        Side::Lit(HOST_TUTORIAL_CALLOUT, "TAIL"),
        Side::Lit(GALLERY_TUTORIAL, "TAIL"),
    ),
    (
        "explorer 사이드바 폭",
        Side::Lit(HOST_EXPLORER, "SIDEBAR_W"),
        Side::Lit(GALLERY_EXPLORER_SIDEBAR, "SIDEBAR_W"),
    ),
    (
        "explorer grid 셀 폭",
        Side::Lit(HOST_EXPLORER, "CELL_W"),
        Side::Lit(GALLERY_EXPLORER_CELLS, "CELL_W"),
    ),
    (
        "훅 추가 행 라벨 폭",
        Side::Lit(HOST_HOOK_HANDLERS, "HOOK_ADD_LABEL_W"),
        Side::Lit(GALLERY_SETTINGS_HANDLER, "HOOK_ADD_LABEL_W"),
    ),
    (
        "훅 명령 라벨 폭",
        Side::Lit(HOST_HOOK_HANDLERS, "HOOK_CMD_LABEL_W"),
        Side::Lit(GALLERY_SETTINGS_HANDLER, "HOOK_CMD_LABEL_W"),
    ),
    (
        "단축키 가져오기 그룹 헤더 chevron 간격",
        Side::Lit(HOST_KB_IMPORT_EXPORT, "GROUP_CHEVRON_GAP"),
        Side::Lit(GALLERY_KB_IMPORT_EXPORT, "GROUP_CHEVRON_GAP"),
    ),
    (
        "단축키 가져오기 plugin 점 간격",
        Side::Lit(HOST_KB_IMPORT_EXPORT, "PLUGIN_DOT_GAP"),
        Side::Lit(GALLERY_KB_IMPORT_EXPORT, "PLUGIN_DOT_GAP"),
    ),
    (
        "파일 피커 크기 열 폭",
        Side::Lit(HOST_FILE_PICKER, "SIZE_COL_W"),
        Side::Lit(GALLERY_FILE_PICKER, "SIZE_COL_W"),
    ),
    (
        "파일 피커 수정일 열 폭",
        Side::Lit(HOST_FILE_PICKER, "MOD_COL_W"),
        Side::Lit(GALLERY_FILE_PICKER, "MOD_COL_W"),
    ),
];

const GALLERY_QUIT_MODAL: &str = "crates/tasty-gallery/src/catalog/components/quit_modal.rs";
const GALLERY_REMOTE_ATTACH: &str = "crates/tasty-gallery/src/catalog/components/remote_attach.rs";
const GALLERY_FILE_PICKER: &str = "crates/tasty-gallery/src/catalog/components/file_picker.rs";
const GALLERY_TUTORIAL: &str = "crates/tasty-gallery/src/catalog/widgets/tutorial.rs";
const GALLERY_EXPLORER_SIDEBAR: &str =
    "crates/tasty-gallery/src/catalog/components/explorer_sidebar.rs";
const GALLERY_EXPLORER_CELLS: &str =
    "crates/tasty-gallery/src/catalog/components/explorer_view_cells.rs";
const GALLERY_SETTINGS_HANDLER: &str =
    "crates/tasty-gallery/src/catalog/components/settings_handler.rs";

const HOST_PRESET_DEMO: &str = "src/adapters/ui/preset/demo_layout.rs";
const HOST_REMOTE_ATTACH: &str = "src/adapters/ui/popup/remote_attach.rs";
const HOST_FILE_PICKER: &str = "src/adapters/ui/popup/file_picker.rs";
const HOST_TUTORIAL_CALLOUT: &str = "src/adapters/ui/tutorial/callout.rs";
const HOST_EXPLORER: &str = "src/adapters/ui/surface/explorer.rs";
const HOST_HOOK_HANDLERS: &str = "src/view/settings/ui/file_handler_tab/hook_handlers.rs";
const HOST_KB_IMPORT_EXPORT: &str = "src/view/settings/ui/keybindings_tab/import_export.rs";
const GALLERY_KB_IMPORT_EXPORT: &str =
    "crates/tasty-gallery/src/catalog/components/kb_import_export.rs";
const GALLERY_PRESET_EDITOR: &str = "crates/tasty-gallery/src/catalog/components/preset_editor.rs";
const GALLERY_POPUP_FRAME: &str = "crates/tasty-gallery/src/catalog/popup_frame.rs";
const GALLERY_PLUGINS_WINDOW: &str =
    "crates/tasty-gallery/src/catalog/components/plugins_window.rs";
const GALLERY_PLUGINS_ATTENTION: &str =
    "crates/tasty-gallery/src/catalog/components/plugins_window/attention.rs";

/// 상수의 줄 번호(1부터 시작)와 숫자 값.
fn const_site(masked: &str, name: &str) -> Option<(usize, f32)> {
    let (line_no, line) = find_const_line(masked, name)?;
    // LogicalPx 생성자와 일반 숫자 초기화식을 모두 허용한다.
    let rest = match line.rfind("LogicalPx(") {
        Some(at) => &line[at + "LogicalPx(".len()..],
        None => line.split('=').nth(1)?.trim(),
    };
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    Some((line_no, num.parse().ok()?))
}

fn theme_site(masked: &str, name: &str) -> Option<(usize, f32)> {
    let needle = format!("{name}: LogicalPx(");
    let (idx, line) = masked
        .lines()
        .enumerate()
        .find(|(_, l)| l.trim_start().starts_with(&needle))?;
    let open = line.find(&needle)? + needle.len();
    let num: String = line[open..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    Some((idx + 1, num.parse().ok()?))
}

/// 가시성 접두사를 허용하고 상수 이름 전체를 맞춘다. MIN_HEIGHT와 MIN_HEIGHT_LG를 구별해야 한다.
fn find_const_line<'a>(masked: &'a str, name: &str) -> Option<(usize, &'a str)> {
    let needle = format!("const {name}:");
    masked.lines().enumerate().find_map(|(i, l)| {
        let t = l
            .trim_start()
            .strip_prefix("pub(crate) ")
            .or_else(|| l.trim_start().strip_prefix("pub(super) "))
            .or_else(|| l.trim_start().strip_prefix("pub "))
            .unwrap_or(l.trim_start());
        t.starts_with(&needle).then_some((i + 1, l))
    })
}

fn alias_target(masked: &str, name: &str) -> Option<(usize, String)> {
    let (line_no, line) = find_const_line(masked, name)?;
    let rhs = line.split('=').nth(1)?.trim().trim_end_matches(';').trim();
    let token = rhs.rsplit("::").next()?.trim();
    (!token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()))
    .then(|| (line_no, token.to_string()))
}

fn token_value(semantic: &str, primitive: &str, token: &str) -> Option<(String, f32)> {
    let (sem_line, line) = find_const_line(semantic, token)?;
    let prim = line
        .split('=')
        .nth(1)?
        .trim()
        .trim_end_matches(';')
        .rsplit("::")
        .next()?
        .trim()
        .to_string();
    let (prim_line, value) = const_site(primitive, &prim)?;
    Some((
        format!("{token}@{SEMANTIC}:{sem_line} → {prim}={value}@{PRIMITIVE}:{prim_line}"),
        value,
    ))
}

fn read(rel: &str) -> String {
    mask_non_code(&read_raw(rel))
}

/// 상수에 붙은 문서 주석을 읽을 때는 마스킹하지 않은 원문이 필요하다.
fn read_raw(rel: &str) -> String {
    std::fs::read_to_string(super::repo_root().join(rel)).unwrap_or_default()
}

fn resolve(side: &Side, theme: &str, semantic: &str, primitive: &str) -> (String, f32) {
    match side {
        Side::Lit(file, name) => {
            let (line, value) = const_site(&read(file), name).unwrap_or_else(|| {
                panic!("`{file}` 에서 `{name}` 을 못 읽었다 — 이름이 바뀌었으면 명부를 따라 고쳐라")
            });
            (format!("{file}:{line}  {name} = {value}"), value)
        }
        Side::Alias(file, name) => {
            let src = read(file);
            let (line, token) = alias_target(&src, name).unwrap_or_else(|| {
                panic!(
                    "`{file}:{name}`이 토큰을 가리키지 않는다. 리터럴로 바뀌었다면 의도를 확인하고 명부를 Lit으로 갱신한다."
                )
            });
            let (trace, value) = token_value(semantic, primitive, &token)
                .unwrap_or_else(|| panic!("토큰 `{token}`의 값을 읽지 못해 치수를 비교할 수 없다"));
            (format!("{file}:{line}  {name} = {trace}"), value)
        }
        Side::ThemeSum(file, names) => {
            // 기본값의 합만 같고 해당 필드 참조는 없어진 경우를 찾는다. 정확한 계산식까지 확인하지는 않는다.
            let src = read(file);
            let mut sum = 0.0;
            let mut terms = Vec::new();
            for n in *names {
                assert!(
                    src.contains(n),
                    "`{file}`에서 `{n}`을 찾지 못했다. Theme 계산이 고정값으로 바뀌었는지 확인하고 명부와 설계 근거를 갱신한다."
                );
                let (line, v) = theme_site(theme, n)
                    .unwrap_or_else(|| panic!("`{THEME}`에서 `{n}`의 기본값을 읽지 못했다"));
                sum += v;
                terms.push(format!("{n}={v}@{THEME}:{line}"));
            }
            (
                format!("{file}  테마 파생 {} = {sum}", terms.join(" + ")),
                sum,
            )
        }
    }
}

#[test]
fn the_gallery_still_agrees_with_the_dimensions_it_restates() {
    // 항목을 삭제해 불일치를 숨기지 않도록 하한을 둔다. 쌍을 더할 때는 고치지 않는다.
    assert!(
        COPIED.len() >= 35,
        "비교 명부가 {}쌍으로 하한 35보다 적다. 복사본이 실제로 사라졌다면 양쪽이 같은 토큰을 읽는지 확인한 뒤 하한을 낮춘다. 불일치를 숨기려고 쌍을 지우지 않는다.",
        COPIED.len()
    );
    let theme = read(THEME);
    let semantic = read(SEMANTIC);
    let primitive = read(PRIMITIVE);

    let mut listed = Vec::new();
    let mut split = Vec::new();
    for (what, host_side, gallery_side) in COPIED {
        let (left, host) = resolve(host_side, &theme, &semantic, &primitive);
        let (right, value) = resolve(gallery_side, &theme, &semantic, &primitive);
        let mark = if host == value { "=" } else { "≠" };
        listed.push(format!(
            "  [{mark}] {what}\n        본체   {left}\n        갤러리 {right}"
        ));
        if host != value {
            split.push((*what).to_string());
        }
    }

    let table = listed.join("\n");
    let headline = format!("갈라진 치수 {}: {}", split.len(), split.join(" · "));
    assert!(
        split.is_empty(),
        "{}\n\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        headline,
        table,
        "디자인 의도를 확인해 본체와 갤러리 중 바꿀 대상을 정한다.",
        "본체를 Theme 계산으로 바꾸면 기본값 외의 테마에서 동작이 달라질 수 있다.",
        "갤러리를 고정값으로 바꾸면 해당 치수는 테마를 따르지 않는다.",
        "값을 맞추는 방법이 기존 디자인 의도를 보존하는지 검토한다.",
        "비교 항목을 지우기만 하면 복사본의 불일치를 찾지 못한다.",
        "두 곳이 같은 공용 정의를 읽게 됐을 때는 비교 항목을 제거할 수 있다. 예: 본체 preset 편집기의 HANDLE_GAP과 갤러리 E_HANDLE_GAP은 tasty_ui_widgets::tokens::STRUCT_GAP_2를 함께 사용한다."
    );
    println!("[사본 치수] {} 쌍\n{table}", COPIED.len());
}

/// 공용 토큰을 함께 읽어 별도 값 비교가 필요 없는 항목. (갤러리 파일, 상수, 토큰, 본체 파일, 사유).
const SHARES_ONE_ITEM: &[(&str, &str, &str, &str, &str)] = &[(
    GALLERY_PRESET_EDITOR,
    "E_HANDLE_GAP",
    "tasty_ui_widgets::tokens::STRUCT_GAP_2",
    "src/adapters/ui/preset/demo_layout.rs",
    "선택 leaf 의 설정·삭제 핸들 사이 간격 — 본체 `HANDLE_GAP` 과 갤러리가 둘 다 \
         `tasty_ui_widgets::tokens::STRUCT_GAP_2` 를 읽는다",
)];

/// 의도적으로 다른 치수. (갤러리 파일, 상수, 본체에 생기면 재검토할 이름, 사유).
const DECLARED_DIFFERENT: &[(&str, &str, &str, &str)] = &[
    (
        "crates/tasty-gallery/src/catalog/components/script_manager.rs",
        "FRAME_MAX_W",
        "FRAME_MAX_W",
        "본체는 설정 콘텐츠 영역의 폭을 쓰고 갤러리는 전시용 카드의 최대 폭을 따로 정한다.",
    ),
    (
        GALLERY_KB_IMPORT_EXPORT,
        "SPECIMEN_W",
        "SETTINGS_CONTENT_W",
        "본체 콘텐츠 폭은 창의 남은 영역에 따라 달라진다. 갤러리는 기본 창 크기로 계산한 폭을 전시 입력으로 쓴다.",
    ),
    (
        "crates/tasty-gallery/src/catalog/components/status_bar.rs",
        "WIDE",
        // 본체 상태바가 고정 폭을 갖게 되면 차이의 근거를 다시 검토한다.
        "BAR_W",
        "본체 상태바는 현재 작업 영역의 rect.width()를 받는다. 갤러리는 축소 단계를 보여주기 위한 입력 폭을 사용한다.",
    ),
];

/// 본체를 언급한 연속 /// 주석이 붙은 LogicalPx 상수를 찾는다. 주석을 읽어야 하므로 리터럴만 가린다.
fn sites_that_claim_a_host_counterpart(src: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start().trim_start_matches("pub ");
        if !t.starts_with("const ") || !t.contains(": LogicalPx") {
            continue;
        }
        let Some(name) = t
            .trim_start_matches("const ")
            .split(':')
            .next()
            .map(str::trim)
        else {
            continue;
        };
        let mut doc = String::new();
        let mut j = i;
        while j > 0 && lines[j - 1].trim_start().starts_with("///") {
            doc.insert_str(0, lines[j - 1].trim_start());
            j -= 1;
        }
        if doc.contains("본체") {
            out.push((i + 1, name.to_string()));
        }
    }
    out
}

/// 수집한 상수가 COPIED·SHARES_ONE_ITEM·DECLARED_DIFFERENT 중 하나에 있는지 확인한다.
/// 명부 사유와 주석 삭제 여부는 이 모듈의 다른 시험에서 검사한다. 한 시험만 실행하면 그 검증은 빠진다.
/// 주석에서 본체를 언급하지 않은 새 복사본은 이 수집에 잡히지 않는다.
#[test]
fn every_gallery_constant_that_claims_a_host_dimension_is_accounted_for() {
    let mut claims: Vec<(String, usize, String)> = Vec::new();
    let mut files = 0usize;
    for (rel_path, raw) in super::rust_sources() {
        if !rel_path.starts_with("crates/tasty-gallery/src") {
            continue;
        }
        files += 1;
        let rel = rel_path.to_string_lossy().to_string();
        let src = tasty_doc_guards::source_text::mask_literals(&raw);
        for (line, name) in sites_that_claim_a_host_counterpart(&src) {
            claims.push((rel.clone(), line, name));
        }
    }
    assert!(
        files >= 80,
        "갤러리 소스를 {files}개만 수집했다(하한 80). 경로 접두사와 수집 범위를 확인한다."
    );

    assert!(
        claims.len() >= 8,
        "본체를 언급한 갤러리 상수를 {}개만 찾았다(하한 8). 실제 선언·주석과 수집 결과를 대조한다.",
        claims.len()
    );

    let mut unaccounted = Vec::new();
    for (rel, line, name) in &claims {
        let in_roster = COPIED.iter().any(|(_, _, g)| match g {
            Side::Lit(f, n) | Side::Alias(f, n) => f == rel && n == name,
            Side::ThemeSum(f, _) => f == rel,
        });
        let shared = SHARES_ONE_ITEM
            .iter()
            .any(|(f, n, ..)| *f == rel && *n == name);
        let different = DECLARED_DIFFERENT
            .iter()
            .any(|(f, n, ..)| *f == rel && *n == name);
        if !in_roster && !shared && !different {
            unaccounted.push(format!("  {rel}:{line}  {name}"));
        }
    }

    assert!(
        unaccounted.is_empty(),
        "본체를 언급했으나 분류되지 않은 갤러리 상수가 {}개다:\n{}\n대응하는 본체 치수를 COPIED에 등록한다. 공용 항목을 함께 읽으면 SHARES_ONE_ITEM에, 의도적으로 다른 치수면 DECLARED_DIFFERENT에 근거를 적는다. 본체 값에 이름이 없다면 먼저 상수로 정의한다. 주석의 본체 언급만 지워 비교를 피하지 않는다.",
        unaccounted.len(),
        unaccounted.join("\n")
    );

    println!(
        "[본체 언급 상수] {}개 · 비교 {}쌍 · 공유 {}개 · 의도적 차이 {}개",
        claims.len(),
        COPIED.len(),
        SHARES_ONE_ITEM.len(),
        DECLARED_DIFFERENT.len()
    );
}

fn initializer_of(masked: &str, name: &str) -> Option<String> {
    let (_, line) = find_const_line(masked, name)?;
    Some(
        line.split_once('=')?
            .1
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string(),
    )
}

/// 등록된 상수의 주석에서 본체 언급만 지워 검사 대상에서 빠지는 경우를 확인한다.
fn still_claims_a_host_counterpart(rel: &str, name: &str) -> bool {
    sites_that_claim_a_host_counterpart(&tasty_doc_guards::source_text::mask_literals(&read_raw(
        rel,
    )))
    .iter()
    .any(|(_, n)| n == name)
}

/// 공유 항목은 갤러리 초기화식·본체의 토큰 이름·토큰 값 존재를 대조한다.
/// 의도된 차이는 본체에 대응하는 상수 이름이 없는지와 갤러리 값이 리터럴인지만 확인한다.
/// 두 치수의 역할이 실제로 다른지, 등록한 본체와 갤러리가 올바른 짝인지는 사람이 검토해야 한다.
#[test]
fn the_checkable_roster_premises_still_hold() {
    assert!(
        !SHARES_ONE_ITEM.is_empty() && !DECLARED_DIFFERENT.is_empty(),
        "공유 또는 의도적 차이의 명부가 비어 해당 분류를 검증할 수 없다"
    );

    for (gallery_file, name, token_path, host_file, reason) in SHARES_ONE_ITEM {
        let short = token_path.rsplit("::").next().expect("토큰 경로");

        let init = initializer_of(&read(gallery_file), name).unwrap_or_else(|| {
            panic!("`{gallery_file}` 에서 `{name}` 의 초기화식을 못 읽었다 — 이름이 바뀌었으면 명부를 따라 고쳐라")
        });
        assert!(
            init.contains("::") && init.ends_with(short),
            "`{gallery_file}:{name}`이 `{token_path}`를 가리키지 않는다(초기화식: `{init}`). 기존 공유 근거: {reason}. 값 복사본으로 바뀌었다면 COPIED에 옮겨 비교한다."
        );

        // 주석의 토큰 언급만으로 공유한다고 판단하지 않도록 마스킹한 코드에서 찾는다.
        assert!(
            read(host_file).contains(token_path),
            "`{host_file}` 코드에 `{token_path}`가 없어 양쪽이 같은 항목을 쓴다는 근거가 달라졌다"
        );

        let tokens = read("crates/tasty-ui-widgets/src/tokens.rs");
        let (_, value) = const_site(&tokens, short)
            .unwrap_or_else(|| panic!("위젯 토큰 `{short}`의 선언 또는 값을 읽지 못했다"));
        assert!(
            value > 0.0,
            "`{short}`의 값이 {value}다. 양수 토큰을 올바르게 읽었는지 확인한다."
        );

        assert!(
            still_claims_a_host_counterpart(gallery_file, name),
            "`{gallery_file}:{name}`의 주석에 본체 언급이 없다. 실제 상수는 남겨 두고 주석만 지워 검사에서 빠진 것은 아닌지 확인한다."
        );
    }

    for (gallery_file, name, falsifier, reason) in DECLARED_DIFFERENT {
        let gallery = read(gallery_file);
        assert!(
            const_site(&gallery, name).is_some(),
            "`{gallery_file}:{name}`이 리터럴 치수가 아니다. 의도적 차이의 근거를 다시 검토한다: {reason}"
        );

        let mut host_hits = Vec::new();
        let mut host_files = 0usize;
        for (rel_path, raw) in super::rust_sources() {
            if !rel_path.starts_with("src/") {
                continue;
            }
            host_files += 1;
            if let Some((line, _)) = const_site(&mask_non_code(&raw), falsifier) {
                host_hits.push(format!("  {}:{line}", rel_path.to_string_lossy()));
            }
        }
        assert!(
            host_files >= 200,
            "본체 소스를 {host_files}개만 수집했다(하한 200). 빈 검색 결과를 판단하기 전에 수집 범위를 확인한다."
        );
        assert!(
            host_hits.is_empty(),
            "본체에 `{falsifier}` 라는 치수가 생겼다:\n{}\n\n\
             `DECLARED_DIFFERENT` 의 사유(\"{reason}\")는 본체에 대응하는 이름이 없다는 \
             것에 기대고 있었다. 같은 치수면 `COPIED` 로 옮기고, 여전히 다른 치수면 \
             사유를 그 자리에 맞게 다시 써라",
            host_hits.join("\n")
        );

        assert!(
            still_claims_a_host_counterpart(gallery_file, name),
            "`{gallery_file}:{name}` 의 doc 이 더 이상 본체를 지목하지 않는다 — 명부 행만 \
             남고 계상 판정은 그 자리를 못 보게 된다"
        );
    }

    // 있는 상수도 찾지 못하는 파서가 빈 결과로 통과하지 않도록 대조한다.
    let known = read("src/view/plugins/ui.rs");
    assert!(
        const_site(&known, "SEGMENT_TAB_LABEL_PRIMITIVE_12").is_some(),
        "기존 상수 SEGMENT_TAB_LABEL_PRIMITIVE_12를 찾지 못했다. 다른 상수가 없다는 결과를 판단하기 전에 파서를 확인한다."
    );
}

/// 주석의 본체 언급을 지워 대상을 줄이지 못하도록 이름을 고정한다. 주석과 명부를 함께 지우는 변경은 별도 검토가 필요하다.
const CONFESSED: &[(&str, &str)] = &[
    (GALLERY_POPUP_FRAME, "TITLE_BAR_HEIGHT"),
    (GALLERY_POPUP_FRAME, "CONTENT_MARGIN"),
    (GALLERY_QUIT_MODAL, "WINDOW_W"),
    (GALLERY_PRESET_EDITOR, "LEAF_SUMMARY_MIN_W"),
    (GALLERY_PRESET_EDITOR, "E_HANDLE_GAP"),
    (
        "crates/tasty-gallery/src/catalog/components/script_manager.rs",
        "FRAME_MAX_W",
    ),
    (GALLERY_KB_IMPORT_EXPORT, "SPECIMEN_W"),
    (GALLERY_PLUGINS_WINDOW, "SEGMENT_TAB_LABEL_PRIMITIVE_12"),
    (GALLERY_PLUGINS_ATTENTION, "ATTN_PRIMITIVE_12"),
    (
        "crates/tasty-gallery/src/catalog/components/status_bar.rs",
        "WIDE",
    ),
    (
        "crates/tasty-gallery/src/catalog/components/status_bar_theme_cell.rs",
        "BAR_PAD_X",
    ),
];

/// 초기화식에서 공용 토큰 경로 문자열을 찾는다. 전체 표현식의 의미를 해석하는 검사는 아니다.
fn points_at_a_shared_item(init: &str) -> bool {
    init.contains("tasty_design_tokens::") || init.contains("tasty_ui_widgets::")
}

fn gallery_length_constants() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (rel_path, raw) in super::rust_sources() {
        if !rel_path.starts_with("crates/tasty-gallery/src") {
            continue;
        }
        let rel = rel_path.to_string_lossy().to_string();
        let masked = mask_non_code(&raw);
        for line in masked.lines() {
            let t = line
                .trim_start()
                .strip_prefix("pub(crate) ")
                .or_else(|| line.trim_start().strip_prefix("pub "))
                .unwrap_or(line.trim_start());
            let Some(rest) = t.strip_prefix("const ") else {
                continue;
            };
            let Some((name, ty)) = rest.split_once(':') else {
                continue;
            };
            if !ty.contains("LogicalPx") {
                continue;
            }
            let name = name.trim().to_string();
            let init = initializer_of(&masked, &name).unwrap_or_default();
            out.push((rel.clone(), name, init));
        }
    }
    out
}

/// 이름이나 값이 같다는 사실로 대응 관계를 정하지 않는다. 다른 컴포넌트도 같은 이름을 쓸 수 있고,
/// 값이 같은 항목만 고르면 값이 달라지는 순간 검사에서 빠진다.
/// 본체 언급의 삭제와 공용 토큰 경로의 미등록을 찾되, 언급도 등록도 없는 복사본은 찾지 못한다.
#[test]
fn the_confessed_population_is_pinned_by_name_not_by_prose() {
    let consts = gallery_length_constants();
    assert!(
        consts.len() >= 120,
        "갤러리 LogicalPx 상수를 {}개만 수집했다(하한 120). 실제 선언과 수집 범위를 확인한다.",
        consts.len()
    );

    let mut found: Vec<(String, String)> = Vec::new();
    for (rel, name, _) in &consts {
        if still_claims_a_host_counterpart(rel, name) {
            found.push((rel.clone(), name.clone()));
        }
    }
    found.sort();
    let mut pinned: Vec<(String, String)> = CONFESSED
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    pinned.sort();

    let gone: Vec<&(String, String)> = pinned.iter().filter(|p| !found.contains(p)).collect();
    let extra: Vec<&(String, String)> = found.iter().filter(|p| !pinned.contains(p)).collect();
    assert!(
        gone.is_empty(),
        "명부에 있으나 본체 언급을 찾지 못한 상수가 {}개다:\n{}\n상수 자체가 사라졌다면 명부도 갱신한다. 상수는 남아 있고 주석의 본체 언급만 빠졌다면 복원한다.",
        gone.len(),
        gone.iter()
            .map(|(f, n)| format!("  {f}  {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        extra.is_empty(),
        "본체를 언급했으나 명부에 없는 상수가 {}개다:\n{}\n대응 관계를 확인해 명부에 등록한다.",
        extra.len(),
        extra
            .iter()
            .map(|(f, n)| format!("  {f}  {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let shared: Vec<&(String, String, String)> = consts
        .iter()
        .filter(|(_, _, init)| points_at_a_shared_item(init))
        .collect();
    assert!(
        !shared.is_empty(),
        "공용 토큰 경로를 가리키는 갤러리 상수를 찾지 못했다. 실제 선언과 검색 조건을 확인한다."
    );
    for (rel, name, init) in &shared {
        let accounted = SHARES_ONE_ITEM
            .iter()
            .any(|(f, n, ..)| f == rel && n == name)
            || COPIED.iter().any(|(_, _, g)| match g {
                Side::Alias(f, n) | Side::Lit(f, n) => f == rel && n == name,
                Side::ThemeSum(f, _) => f == rel,
            });
        assert!(
            accounted,
            "`{rel}:{name}` 초기화식 `{init}`에 공용 토큰 경로가 있으나 명부에 없다. 공유 항목이면 SHARES_ONE_ITEM에, 생성 토큰을 가리키는 비교 항목이면 COPIED의 Alias로 등록한다."
        );
    }

    println!(
        "[갤러리 상수] LogicalPx {}개 · 본체 언급 {}개 · 공용 토큰 경로 {}개",
        consts.len(),
        found.len(),
        shared.len()
    );
}

#[cfg(test)]
mod detector {
    use super::*;

    #[test]
    fn it_reads_a_named_length_constant() {
        let src = "const A: LogicalPx = LogicalPx(440.0);\nconst B: LogicalPx = LogicalPx(12.5);";
        assert_eq!(const_site(src, "A"), Some((1, 440.0)));
        assert_eq!(const_site(src, "B"), Some((2, 12.5)));
    }

    #[test]
    fn a_longer_name_is_not_the_name_asked_for() {
        let src = "const MIN_HEIGHT_LG: LogicalPx = LogicalPx(999.0);";
        assert_eq!(const_site(src, "MIN_HEIGHT"), None);
    }

    #[test]
    fn a_theme_field_default_is_read_by_name() {
        let src =
            "            spacing_lg: LogicalPx(16.0),\n            spacing_xs: LogicalPx(4.0),";
        assert_eq!(theme_site(src, "spacing_lg"), Some((1, 16.0)));
        assert_eq!(theme_site(src, "spacing_xs"), Some((2, 4.0)));
    }

    /// 생성 상수의 pub(crate) 접두사도 읽어야 한다.
    #[test]
    fn a_visibility_prefix_does_not_hide_the_constant() {
        assert_eq!(
            const_site(
                "pub(crate) const SIZE_4: LogicalPx = LogicalPx(4.0);",
                "SIZE_4"
            ),
            Some((1, 4.0))
        );
        assert_eq!(
            const_site("pub const W: LogicalPx = LogicalPx(9.0);", "W"),
            Some((1, 9.0))
        );
    }

    #[test]
    fn an_alias_names_the_token_it_points_at() {
        let src =
            "const CONTENT_MARGIN: LogicalPx = tasty_design_tokens::generated::semantic::SPACE_XS;";
        assert_eq!(
            alias_target(src, "CONTENT_MARGIN"),
            Some((1, "SPACE_XS".to_string()))
        );
    }

    #[test]
    fn a_literal_is_not_an_alias() {
        assert_eq!(
            alias_target(
                "const CONTENT_MARGIN: LogicalPx = LogicalPx(4.0);",
                "CONTENT_MARGIN"
            ),
            None
        );
    }

    #[test]
    fn a_token_is_followed_two_hops_to_its_value() {
        let semantic = "pub const SPACE_XS: LogicalPx = super::primitive::SIZE_4;";
        let primitive = "pub(crate) const SIZE_4: LogicalPx = LogicalPx(4.0);";
        let (trace, v) =
            token_value(semantic, primitive, "SPACE_XS").expect("두 단을 따라가야 한다");
        assert_eq!(v, 4.0);
        assert!(
            trace.contains("SIZE_4"),
            "추적 문자열에 중간 primitive 토큰이 없다"
        );
    }

    #[test]
    fn a_token_whose_primitive_is_missing_is_not_guessed() {
        let semantic = "pub const SPACE_XS: LogicalPx = super::primitive::SIZE_4;";
        assert_eq!(token_value(semantic, "", "SPACE_XS"), None);
    }

    #[test]
    fn a_name_that_is_not_there_is_not_invented() {
        assert_eq!(
            const_site("const A: LogicalPx = LogicalPx(1.0);", "B"),
            None
        );
        assert_eq!(
            theme_site("spacing_lg: LogicalPx(16.0),", "spacing_md"),
            None
        );
    }
}

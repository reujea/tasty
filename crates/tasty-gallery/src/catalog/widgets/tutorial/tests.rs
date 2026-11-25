//! 갤러리 튜토리얼 표본이 본체 basics 주제와 같은 단계 수·순서를 보이는지 본체 소스를 읽어 대조한다.

use super::{BASICS_STEPS, COMPOSITE_NOTE, COMPOSITE_WHEN};

fn app_source(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()))
}

const APP_CATALOG: &str = "src/adapters/ui/tutorial/catalog.rs";

/// `src[start..]`의 원소를 최상위 쉼표로 나눠 `(첫 글자 바이트, 주석을 뺀 원소 글)`로 돌려준다. 여는 괄호
/// 없이 시작해 짝 없는 닫는 `]`에서 멈춘다. 문자열 리터럴 안의 괄호·쉼표는 세지 않고, 줄 주석과
/// (중첩 포함) 블록 주석은 원소 글과 위치에서 뺀다. 위치는 주석 밖 첫 글자가 원문에 놓인 바이트다.
fn top_level_elements(src: &str, start: usize) -> Vec<(usize, String)> {
    let b = src.as_bytes();
    let (mut i, mut depth) = (start, 0usize);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut first: Option<usize> = None;
    let flush = |cur: &mut String, first: &mut Option<usize>, out: &mut Vec<(usize, String)>| {
        if let Some(at) = first.take() {
            out.push((at, cur.trim().to_owned()));
        }
        cur.clear();
    };
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut nest = 1;
            i += 2;
            while i < b.len() && nest > 0 {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    nest += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    nest -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            cur.push(' ');
            continue;
        }
        if c == b']' && depth == 0 {
            flush(&mut cur, &mut first, &mut out);
            return out;
        }
        if c == b',' && depth == 0 {
            flush(&mut cur, &mut first, &mut out);
            i += 1;
            continue;
        }
        let from = i;
        if c == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i += 1;
        } else {
            match c {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        while i < b.len() && !src.is_char_boundary(i) {
            i += 1;
        }
        let i_end = i.min(b.len());
        if first.is_none() && !c.is_ascii_whitespace() {
            first = Some(from);
        }
        cur.push_str(&src[from..i_end]);
    }
    panic!("{APP_CATALOG}: basics steps 목록이 닫히지 않았다");
}

/// 본체 catalog의 basics 주제에서 `step!("<id>", <Target>, …)` 원소를 순서대로 꺼낸다. 이 꼴이 아닌
/// 원소(구조체 리터럴, 다른 괄호의 매크로 등)가 하나라도 있으면 원소와 위치를 담아 실패한다.
fn app_basics_steps() -> Vec<(String, String)> {
    parse_basics_steps(&app_source(APP_CATALOG))
}

/// `src`(본체 catalog 꼴의 소스)에서 basics 단계를 꺼낸다. 규칙은 `app_basics_steps`와 같다.
fn parse_basics_steps(src: &str) -> Vec<(String, String)> {
    let topic = src
        .find("id: \"basics\"")
        .expect("본체 catalog에 basics 주제가 있다");
    let open =
        src[topic..].find("steps: &[").expect("basics steps 시작") + topic + "steps: &[".len();
    let mut steps = Vec::new();
    for (at, element) in top_level_elements(src, open) {
        let line = src[..at].matches('\n').count() + 1;
        let args = element
            .strip_prefix("step!(")
            .and_then(|e| e.strip_suffix(')'))
            .unwrap_or_else(|| {
                panic!("{APP_CATALOG}:{line}: basics 단계가 step!(…) 꼴이 아니다: {element}")
            });
        let mut parts = args.splitn(3, ',');
        let id = parts.next().unwrap_or("").trim();
        let target = parts.next().unwrap_or("").trim();
        let id = id
            .strip_prefix('"')
            .and_then(|i| i.strip_suffix('"'))
            .unwrap_or_else(|| {
                panic!("{APP_CATALOG}:{line}: 단계 id가 문자열 리터럴이 아니다: {element}")
            });
        assert!(
            !target.is_empty()
                && target
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{APP_CATALOG}:{line}: 단계 대상이 MarkerTarget 이름이 아니다: {element}"
        );
        steps.push((id.to_owned(), target.to_owned()));
    }
    steps
}

#[test]
fn basics_steps_follow_the_app_topic() {
    let app = app_basics_steps();
    let ids: Vec<&str> = app.iter().map(|(id, _)| id.as_str()).collect();
    let gallery: Vec<&str> = BASICS_STEPS.iter().map(|s| s.id).collect();
    assert_eq!(
        gallery, ids,
        "갤러리 BASICS_STEPS가 본체 basics 순서와 다르다"
    );

    // 마지막 단계만 Summary 대상이고, 본체는 Summary에 마커 사각형을 두지 않는다.
    let summaries: Vec<usize> = app
        .iter()
        .enumerate()
        .filter(|(_, (_, target))| target == "Summary")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(summaries, vec![app.len() - 1]);
    assert!(
        app_source("src/adapters/ui/tutorial/mod.rs").contains("MarkerTarget::Summary => None"),
        "본체 summary 단계가 마커를 그리면 COMPOSITE_NOTE의 summary 서술을 고친다"
    );
}

#[test]
fn basics_step_keys_exist_in_app_lang() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lang/en.toml");
    let value: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut flat = std::collections::HashMap::new();
    tasty_i18n::flatten_catalog_toml("", &value, &mut flat);
    for step in &BASICS_STEPS {
        for key in [step.title_key, step.body_key] {
            assert!(flat.contains_key(key), "{key}");
        }
    }
}

#[test]
fn composite_texts_state_the_app_step_count_and_order() {
    let n = BASICS_STEPS.len();
    assert!(
        COMPOSITE_WHEN.starts_with(&format!("Step 1/{n} ")),
        "{COMPOSITE_WHEN}"
    );
    let middle: Vec<&str> = BASICS_STEPS[1..n - 1].iter().map(|s| s.id).collect();
    let order = middle.join(" → ");
    assert!(
        COMPOSITE_NOTE.contains(&format!("move the marker {order}:")),
        "{COMPOSITE_NOTE}"
    );
    // 탭 헤더(페인 상단 띠)와 서피스(탭 바 아래)는 서로를 담지 않는다. 마커는 옮겨 갈 뿐이다.
    assert!(!COMPOSITE_NOTE.contains("containment"), "{COMPOSITE_NOTE}");
    assert!(
        COMPOSITE_NOTE.contains(&format!("last step ({})", BASICS_STEPS[n - 1].id)),
        "{COMPOSITE_NOTE}"
    );
}

/// 합성 표본을 GPU 없이 한 프레임 그려 글자 모양을 모은다.
fn composite_texts() -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_owned()),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let theme = tasty_themes::mocha_fallback();
    let ctx = egui::Context::default();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| super::draw_composite(ui, &theme));
    });
    let mut out = Vec::new();
    for cs in &output.shapes {
        collect(&cs.shape, &mut out);
    }
    out
}

#[test]
fn composite_draws_the_first_basics_step_callout() {
    let texts = composite_texts();
    let progress = format!("1 / {}", BASICS_STEPS.len());
    assert!(texts.contains(&progress), "{progress} 없음: {texts:?}");
    let title = crate::i18n::t(BASICS_STEPS[0].title_key);
    assert!(texts.iter().any(|t| t == title), "{title} 없음: {texts:?}");
}

/// 갤러리 본문 열과 같은 꼴로 합성 표본을 한 프레임 그린다. host_shell의 main_ui처럼 열 폭 `col_w`를
/// 정하고 본문 열 폭으로 저장한 뒤, `widen`이면 그 앞에 주제 팝업 표본(고정 폭 무대)을 그려 열의 Ui를
/// 넓힌다. (표본 결과, 보이는 열의 오른쪽 끝 x)를 돌려준다.
fn composite_frame(
    ctx: &egui::Context,
    theme: &tasty_type_appearance::theme::Theme,
    col_w: f32,
    widen: bool,
) -> (super::CompositeLayout, f32) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(col_w + 400.0, 3000.0),
        )),
        ..Default::default()
    };
    let mut got = None;
    let _frame = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(col_w);
                ui.data_mut(|d| d.insert_temp(crate::catalog::spec::body_column_width_id(), col_w));
                let right = ui.max_rect().left() + col_w;
                if widen {
                    super::draw_topics(ui, theme);
                }
                got = Some((super::composite_specimen(ui, theme), right));
            });
        });
    });
    got.expect("합성 표본을 그렸다")
}

/// `composite_frame`을 그리고, `to_end`면 가로 스크롤을 끝으로 옮겨 한 번 더 그린다.
fn composite_in_column(col_w: f32, widen: bool, to_end: bool) -> (super::CompositeLayout, f32) {
    let theme = tasty_themes::mocha_fallback();
    let ctx = egui::Context::default();
    let first = composite_frame(&ctx, &theme, col_w, widen);
    if !to_end {
        return first;
    }
    let layout = &first.0;
    let mut state =
        egui::scroll_area::State::load(&ctx, layout.scroll_id).expect("가로 스크롤 상태");
    state.offset.x = (layout.content.x - layout.viewport.width()).max(0.0);
    state.store(&ctx, layout.scroll_id);
    composite_frame(&ctx, &theme, col_w, widen)
}

fn floor() -> f32 {
    tasty_themes::mocha_fallback()
        .tutorial_step_specimen_min_width()
        .value()
}

#[test]
fn composite_stage_keeps_its_floor_and_holds_the_callout() {
    // 바닥보다 좁은 열: 무대는 바닥 폭이고 말풍선과 꼬리가 무대 안에 든다.
    let (l, _) = composite_in_column(floor() / 2.0, false, false);
    assert_eq!(l.stage.width(), floor(), "{:?}", l.stage);
    assert!(
        l.stage.contains_rect(l.bubble),
        "{:?} ⊅ {:?}",
        l.stage,
        l.bubble
    );
    // 넓은 열: 무대는 최대 폭에서 멈춘다.
    let (l, _) = composite_in_column(2000.0, false, false);
    assert_eq!(
        l.stage.width(),
        super::COMPOSITE_STAGE_MAX_W.value(),
        "{:?}",
        l.stage
    );
    assert!(
        l.stage.contains_rect(l.bubble),
        "{:?} ⊅ {:?}",
        l.stage,
        l.bubble
    );
}

#[test]
fn composite_stage_scrolls_horizontally_in_a_narrow_column() {
    let (l, _) = composite_in_column(floor() / 2.0, false, false);
    assert!(
        l.stage.width() > l.viewport.width() + 0.5,
        "무대 {:?}가 보이는 영역 {:?}보다 넓어야 한다",
        l.stage,
        l.viewport
    );
    assert_eq!(l.content.x, floor(), "스크롤 내용 폭 {:?}", l.content);
}

#[test]
fn composite_tail_is_visible_after_scrolling_to_the_end_under_a_widening_specimen() {
    // 주제 팝업 표본이 열의 Ui를 넓힌 뒤에도 보이는 영역은 열 안에 있고, 끝까지 스크롤하면 꼬리 끝이 보인다.
    let col_w = 300.0;
    assert!(col_w < floor());
    let (l, right) = composite_in_column(col_w, true, true);
    assert!(
        l.viewport.right() <= right + 0.5,
        "보이는 영역 {:?}가 열 끝 {right}을 넘는다",
        l.viewport
    );
    assert!(
        l.bubble.right() <= l.viewport.right() + 0.5,
        "끝까지 스크롤한 말풍선 {:?}의 꼬리가 보이는 영역 {:?} 밖이다",
        l.bubble,
        l.viewport
    );
}

/// 진단 메시지의 `catalog.rs:<줄>:`을 꺼내 줄 번호를 돌려준다.
fn rejected_line(src: &str) -> usize {
    let err = std::panic::catch_unwind(|| parse_basics_steps(src)).expect_err("거부해야 한다");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    let rest = msg
        .strip_prefix(&format!("{APP_CATALOG}:"))
        .unwrap_or_else(|| panic!("진단에 파일·줄이 없다: {msg}"));
    rest.split(':').next().unwrap().parse().unwrap()
}

const FIXTURE_HEAD: &str = "    Topic {\n        id: \"basics\",\n        steps: &[\n            step!(\"workspace\", ContentArea, Read, None),\n";

#[test]
fn block_comments_before_a_step_are_skipped() {
    let src = format!(
        "{FIXTURE_HEAD}            /* summary는 말풍선만, ( [ , 를 담아도 */\n            step!(\"summary\", Summary, Read, None),\n        ],\n"
    );
    let ids: Vec<String> = parse_basics_steps(&src)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids, ["workspace", "summary"]);
}

#[test]
fn a_rejected_step_under_a_block_comment_reports_its_own_line() {
    // 5행: 블록 주석, 6행: 거부할 원소.
    let src = format!(
        "{FIXTURE_HEAD}            /* 여러 줄\n            블록 주석 */ step![\"extra\", Summary, Read, None],\n        ],\n"
    );
    assert_eq!(rejected_line(&src), 6);
    let src = format!(
        "{FIXTURE_HEAD}            /* 한 줄 블록 주석 */\n            step![\"extra\", Summary, Read, None],\n        ],\n"
    );
    assert_eq!(rejected_line(&src), 6);
}

#[test]
fn composite_edge_fades_stay_on_the_stage() {
    // 바닥보다 좁은 열의 첫 프레임: 오른쪽으로 더 스크롤할 수 있어 오른쪽 페이드가 있다. 페이드(메시)는
    // 무대의 세로 범위를 그대로 덮는다(무대 아래로 넘지도, 높이 0으로 줄지도 않는다). 세로로 남은 가용
    // 높이가 큰 첫 번째 경우와, 긴 갤러리 페이지처럼 남은 높이가 0인 두 번째 경우를 함께 본다.
    let theme = tasty_themes::mocha_fallback();
    let col_w = floor() / 2.0;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(col_w + 400.0, 3000.0),
        )),
        ..Default::default()
    };
    for no_height_left in [false, true] {
        edge_fades_cover_the_stage(&theme, col_w, input.clone(), no_height_left);
    }
}

fn edge_fades_cover_the_stage(
    theme: &tasty_type_appearance::theme::Theme,
    col_w: f32,
    input: egui::RawInput,
    no_height_left: bool,
) {
    let ctx = egui::Context::default();
    let mut stage = None;
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(col_w);
                if no_height_left {
                    ui.add_space(ui.available_height());
                }
                ui.data_mut(|d| d.insert_temp(crate::catalog::spec::body_column_width_id(), col_w));
                stage = Some(super::composite_specimen(ui, theme).stage);
            });
        });
    });
    let stage = stage.expect("합성 표본을 그렸다");
    let meshes: Vec<egui::Rect> = output
        .shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::epaint::Shape::Mesh(m) => Some(m.calc_bounds()),
            _ => None,
        })
        .collect();
    assert!(!meshes.is_empty(), "오른쪽 페이드가 없다");
    for m in meshes {
        assert!(
            (m.top() - stage.top()).abs() <= 0.5 && (m.bottom() - stage.bottom()).abs() <= 0.5,
            "남은 높이 0={no_height_left}: 페이드 {m:?}의 세로 범위가 무대 {stage:?}와 같아야 한다"
        );
    }
}

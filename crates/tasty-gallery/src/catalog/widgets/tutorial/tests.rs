//! 갤러리 튜토리얼 표본이 본체 basics 주제와 같은 단계 수·순서를 보이는지 본체 소스를 읽어 대조한다.

use super::{BASICS_STEPS, COMPOSITE_NOTE, COMPOSITE_WHEN};

fn app_source(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()))
}

/// 본체 catalog의 basics 주제에서 `step!("<id>", <Target>, …)`를 순서대로 꺼낸다.
fn app_basics_steps() -> Vec<(String, String)> {
    let src = app_source("src/adapters/ui/tutorial/catalog.rs");
    let topic = src
        .find("id: \"basics\"")
        .expect("본체 catalog에 basics 주제가 있다");
    let rest = &src[topic..];
    let open = rest.find("steps: &[").expect("basics steps 시작");
    let close = rest[open..].find("\n        ],").expect("basics steps 끝") + open;
    let mut steps = Vec::new();
    let mut body = &rest[open..close];
    while let Some(at) = body.find("step!(") {
        body = &body[at + "step!(".len()..];
        let mut args = body.splitn(3, ',');
        let id = args.next().unwrap().trim().trim_matches('"').to_owned();
        let target = args.next().expect("step target").trim().to_owned();
        steps.push((id, target));
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

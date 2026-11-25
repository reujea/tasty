//! host popup의 draw가 plugin popup보다 앞에 있는지 소스 순서를 확인한다.
//! plugin은 같은 프레임에서 host의 hit-test rect를 읽고, 반대 방향은 이전 프레임 값을 사용한다.
//! 최종 페인트 순서와 별개로 이 데이터의 갱신 순서를 지켜야 한다.
//! 텍스트의 호출 위치를 비교하며 조건 분기의 실제 실행 여부는 판단하지 않는다.

use super::{fn_body, repo_root, strip_comments};

const HOME: &str = "src/gfx/gpu/egui_bridge.rs";
const FRAME_FN: &str = "fn run_egui_frame";
const HOST_DRAW: &str = "ui::draw_popups(";
const PLUGIN_DRAW: &str = "popup_render::draw_plugin_popups(";

fn frame_body() -> String {
    let path = repo_root().join(HOME);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{HOME} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, FRAME_FN).unwrap_or_else(|| {
        panic!("{HOME}에서 `{FRAME_FN}` 본문을 읽지 못했다. 이름·이동 여부와 함수 추출을 확인한다.")
    });
    strip_comments(&body)
}

#[test]
fn the_host_popups_are_drawn_before_the_plugin_popups() {
    let body = frame_body();
    let host = body.find(HOST_DRAW).unwrap_or_else(|| {
        panic!("`{FRAME_FN}` 안에 `{HOST_DRAW}` 가 없다 — host popup draw 가 사라졌다")
    });
    let plugin = body.find(PLUGIN_DRAW).unwrap_or_else(|| {
        panic!("`{FRAME_FN}` 안에 `{PLUGIN_DRAW}` 가 없다 — plugin popup draw 가 사라졌다")
    });
    assert!(
        host < plugin,
        "`{FRAME_FN}`에서 `{PLUGIN_DRAW}`가 `{HOST_DRAW}`보다 앞에 있다. plugin은 이전 프레임의 host rect를 읽고 host는 예상보다 이른 plugin rect를 읽을 수 있다. {HOME}의 두 호출 순서를 확인한다."
    );
}

#[test]
fn the_guard_reads_the_frame_closure_and_not_some_other_function() {
    // 본문 추출이 다른 함수를 반환하지 않았는지 이웃 호출도 확인한다.
    let body = frame_body();
    for neighbour in ["enforce_foreground_z_order(", "draw_plugin_banners("] {
        assert!(
            body.contains(neighbour),
            "잘라 온 본문이 `{FRAME_FN}` 이 아니다 — `{neighbour}` 가 안 보인다"
        );
    }
}

#[test]
fn child_scope_is_resolved_before_the_first_host_paint() {
    let body = frame_body();
    let inherit = body
        .find("popup_scope::sync_plugin_popup_scopes(")
        .expect("child scope resolution");
    let host = body.find(HOST_DRAW).expect("host draw");
    assert!(
        inherit < host,
        "child scope must be applied before host paint and hit testing"
    );
}

/// host 팝업이 위일 때 포커스 띠는 egui 패스 안에서 합성될 plugin 팝업 본문의 clip으로 구멍을 내고, 합성은
/// egui 패스 뒤에 한다. 둘이 같은 프레임의 같은 clip을 쓰려면 본문 디코드가 egui 패스보다 앞이어야 한다.
/// 디코드가 합성 단계에만 있으면 첫 프레임과 clip이 바뀌는 프레임에 띠 구멍이 한 프레임 늦는다. 합성 단계가
/// 다시 디코드하면 그 사이 plugin이 쓴 새 프레임으로 띠 구멍과 다른 본문을 합성한다.
#[test]
fn the_popup_bodies_are_decoded_before_the_edge_reads_them() {
    let body = frame_body();
    let prepare = body
        .find("self.prepare_mesh_popups(")
        .expect("`run_egui_frame` 이 egui 패스 전에 팝업 본문을 디코드해야 한다");
    let bodies = body
        .find("popup_scope::bodies_to_decode(")
        .expect("사전 디코드 대상은 직전 프레임 합성 영역에서 골라야 한다(접힌 팝업 제외)");
    assert!(bodies < prepare, "디코드 대상을 고른 뒤 디코드한다");
    let composed = body
        .find("self.composed_popups()")
        .expect("띠가 읽는 합성 본문");
    let pass = body.find("self.run_main_pass(").expect("egui 패스");
    let redraw = body
        .find("popup_scope::redraw_for_undecoded_bodies(")
        .expect("본문 없이 합성 영역을 받은 팝업은 다음 프레임을 요청해야 한다");
    assert!(
        pass < redraw,
        "다음 프레임 요청은 이번 패스의 합성 영역을 본 뒤다"
    );
    assert!(
        prepare < composed && composed < pass,
        "팝업 본문 디코드 → 합성 본문 읽기 → egui 패스 순서가 아니다"
    );
    let path = repo_root().join("src/gfx/gpu/egui_mesh_prepare.rs");
    let src = strip_comments(
        &std::fs::read_to_string(&path)
            .expect("egui_mesh_prepare.rs")
            .replace("\r\n", "\n"),
    );
    // 합성 함수의 시그니처부터 다음 함수 정의 앞까지를 본문으로 읽는다.
    let start = src
        .find("fn render_egui_mesh_popups(")
        .expect("egui_mesh_prepare.rs 에 팝업 합성 함수가 있어야 한다");
    let rest = &src[start + 1..];
    let end = rest
        .find("\n    pub(super) fn ")
        .or_else(|| rest.find("\n    fn "));
    let composite = &rest[..end.unwrap_or(rest.len())];
    assert!(
        composite.contains("composite_mesh_target(")
            && !composite.contains("decode_mesh_popup(")
            && !composite.contains("decode_and_track("),
        "팝업 합성은 egui 패스 전에 디코드한 본문을 그대로 합성하고 다시 디코드하지 않는다"
    );
}

/// 소스의 `name` 표기 하나. 정의(`fn name(`), `self.name(` 호출, 다른 수신자의 메서드 호출(`a.b.name(`)이다.
#[derive(Debug, PartialEq, Eq)]
enum Site {
    Def,
    Call { caller: String },
    Method { caller: String, receiver: String },
}

/// [`sites_in`]이 찾은 표기 하나: 파일, 줄 번호, 그 줄(메서드 사슬을 이은 논리 줄), 표기.
struct Found {
    file: String,
    line: usize,
    text: String,
    site: Site,
}

/// `src` 아래(가드 폴더 제외) 모든 .rs 에서 `name` 의 표기를 찾는다([`sites_in`]).
fn sites_of(name: &str) -> Vec<(String, Site)> {
    sites_in(name, |_| true)
        .into_iter()
        .map(|f| (f.file, f.site))
        .collect()
}

/// `src` 아래(가드 폴더 제외) .rs 중 상대 경로가 `keep`을 만족하는 파일에서 `name` 의 표기를 찾는다. 주석은 뺀다.
/// `.`으로 시작하는 줄은 앞 줄에 이어 읽는다(rustfmt가 나눈 메서드 사슬). 정의·메서드 호출이 아닌 표기(경로 호출,
/// 함수 값 등)는 file:line 과 함께 '지원하지 않는 표기'로 실패한다. 호출은 감싼 가장 가까운 `fn` 이름을 함께
/// 돌려준다.
fn sites_in(name: &str, keep: impl Fn(&str) -> bool) -> Vec<Found> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).expect("src 폴더").flatten() {
            let p = e.path();
            if p.is_dir() {
                if !p.ends_with("source_guards") {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let root = repo_root();
    let mut files = Vec::new();
    walk(&root.join("src"), &mut files);
    files.sort();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    for path in files {
        let raw = std::fs::read_to_string(&path)
            .expect("소스")
            .replace("\r\n", "\n");
        let masked = tasty_doc_guards::source_text::mask_comments(&raw);
        let rel =
            tasty_doc_guards::source_text::repo_relative(path.strip_prefix(&root).unwrap_or(&path))
                .display()
                .to_string();
        if !keep(&rel) {
            continue;
        }
        let mut lines: Vec<(usize, String)> = Vec::new();
        for (no, line) in masked.lines().enumerate() {
            match lines.last_mut() {
                Some((_, prev)) if line.trim_start().starts_with('.') => {
                    prev.push_str(line.trim());
                }
                _ => lines.push((no + 1, line.to_string())),
            }
        }
        let mut caller = String::new();
        for (no, line) in &lines {
            let line = line.as_str();
            if let Some(i) = line.find("fn ") {
                let rest = &line[i + 3..];
                let end = rest.find(|c: char| !is_word(c)).unwrap_or(rest.len());
                if end > 0 && (i == 0 || !line[..i].ends_with(is_word)) {
                    caller = rest[..end].to_string();
                }
            }
            let mut at = 0;
            while let Some(k) = line[at..].find(name) {
                let start = at + k;
                let end = start + name.len();
                at = end;
                let before = &line[..start];
                if before.ends_with(is_word) || line[end..].starts_with(is_word) {
                    continue;
                }
                let call = line[end..].starts_with('(');
                let receiver = before
                    .strip_suffix('.')
                    .map(|b| {
                        let from = b
                            .rfind(|c: char| !is_word(c) && c != '.')
                            .map_or(0, |i| i + 1);
                        &b[from..]
                    })
                    .filter(|r| !r.is_empty());
                let site = match receiver {
                    _ if before.ends_with("fn ") && call => Site::Def,
                    Some("self") if call => Site::Call {
                        caller: caller.clone(),
                    },
                    Some(r) if call => Site::Method {
                        caller: caller.clone(),
                        receiver: r.to_string(),
                    },
                    _ => panic!("{rel}:{no}: 지원하지 않는 표기: {}", line.trim()),
                };
                out.push(Found {
                    file: rel.clone(),
                    line: *no,
                    text: line.trim().to_string(),
                    site,
                });
            }
        }
    }
    out
}

/// 팝업 본문 디코드 경로는 하나다. `decode_mesh_popup` 을 부르는 곳은 `prepare_mesh_popups` 하나뿐이고,
/// `prepare_mesh_popups` 를 부르는 곳은 `run_egui_frame`(egui 패스 전) 하나뿐이다. 합성 본문이 어느 이름으로든
/// 다시 디코드하면 같은 프레임의 띠 구멍과 다른 본문을 합성할 수 있다.
#[test]
fn the_popup_body_decode_has_one_caller_chain() {
    let gpu = "src/gfx/gpu/egui_mesh_prepare.rs".to_string();
    assert_eq!(
        sites_of("decode_mesh_popup"),
        vec![
            (
                gpu.clone(),
                Site::Call {
                    caller: "prepare_mesh_popups".into()
                }
            ),
            (gpu.clone(), Site::Def),
        ],
        "decode_mesh_popup 의 정의 하나와 prepare_mesh_popups 안 호출 하나만 있어야 한다"
    );
    assert_eq!(
        sites_of("prepare_mesh_popups"),
        vec![
            (
                HOME.to_string(),
                Site::Call {
                    caller: FRAME_FN
                        .trim_start_matches("fn ")
                        .trim_end_matches('(')
                        .into()
                }
            ),
            (gpu, Site::Def),
        ],
        "prepare_mesh_popups 의 정의 하나와 run_egui_frame 안 호출 하나만 있어야 한다"
    );
}

/// 제품 디코더(`decode_mesh_bytes_into_target`)는 디코드한 도형을 대상의 `target.shapes.adopt` 로 채운다. 시험
/// `the_edge_hole_and_the_composite_clip_match_in_each_frame` 은 GPU 없이 `MeshShapes::adopt` 를 부르므로, 제품
/// 디코더가 이 호출을 잃으면 그 시험은 보지 못한다. `adopt` 는 egui_mesh_prepare 안에서만 쓰인다.
#[test]
fn the_product_decoder_fills_the_target_shapes() {
    let sites = sites_in("adopt", |f| f.starts_with("src/gfx/gpu/egui_mesh_prepare"));
    let product = Site::Method {
        caller: "decode_mesh_bytes_into_target".into(),
        receiver: "target.shapes".into(),
    };
    assert_eq!(
        sites.iter().filter(|f| f.site == product).count(),
        1,
        "제품 디코더가 target.shapes.adopt 를 한 번 불러야 한다"
    );
    assert_eq!(
        sites.iter().filter(|f| f.site == Site::Def).count(),
        1,
        "MeshShapes::adopt 정의"
    );
}

/// `file` 에서 패턴에 `name` 을 묶는 `let` 문장의 줄 번호와 공백을 뺀 문장 전체(괄호 짝을 따라 문장 끝 `;`
/// 까지). 주석은 뺀다. 패턴은 `let` 부터 깊이 0의 `=`(또는 `;`)까지이므로 `let name`, `let mut name`, 타입 명시
/// (`let name: T =`), 구조 분해(`let (name, ..) =`), `if let Some(name) =` 같은 꼴을 모두 돌려준다.
fn bindings_of(file: &str, name: &str) -> Vec<(usize, String)> {
    let raw = std::fs::read_to_string(repo_root().join(file))
        .unwrap_or_else(|e| panic!("{file} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let src = tasty_doc_guards::source_text::mask_comments(&raw);
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = src[from..].find("let ") {
        let start = from + k;
        from = start + 4;
        if src[..start].ends_with(is_word) {
            continue;
        }
        let mut depth = 0i32;
        let mut pattern_end = src.len();
        for (i, c) in src[from..].char_indices() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '=' | ';' if depth <= 0 => {
                    pattern_end = from + i;
                    break;
                }
                _ => {}
            }
        }
        let pattern = &src[from..pattern_end];
        let binds = pattern.match_indices(name).any(|(i, _)| {
            !pattern[..i].ends_with(is_word) && !pattern[i + name.len()..].starts_with(is_word)
        });
        if !binds {
            continue;
        }
        let mut depth = 0i32;
        let mut end = src.len();
        for (i, c) in src[start..].char_indices() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ';' if depth == 0 => {
                    end = start + i + 1;
                    break;
                }
                _ => {}
            }
        }
        let line = src[..start].matches('\n').count() + 1;
        let stmt = src[start..end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        out.push((line, stmt));
    }
    out
}

/// egui 패스 전 디코드 대상은 `bodies_to_decode` 가 고른 것이고, 그 결과가 그대로 `prepare_mesh_popups` 인자로
/// 간다. `bodies` 초기화는 문장 끝(닫는 괄호·세미콜론)까지 대조한다. 대조에 맞지 않는 초기화(덧붙인 원소, 타입
/// 명시 바인딩 등)와 다른 인자 표기는 file:line 과 '지원하지 않는 표기'로 실패한다.
#[test]
fn the_chosen_bodies_are_what_the_frame_decodes() {
    const BODIES: &str = "letbodies=plugin_manager.map_or_else(Vec::new,|mgr|{crate::plugin_bridge::popup_scope::bodies_to_decode(&state.plugin_mesh_popup_regions,mgr.popup_instances().map(|(id,_)|id),)});";
    let bindings = bindings_of(HOME, "bodies");
    for (line, stmt) in &bindings {
        assert!(stmt == BODIES, "{HOME}:{line}: 지원하지 않는 표기: {stmt}");
    }
    assert_eq!(
        bindings.len(),
        1,
        "{HOME} 에서 bodies 를 한 번 묶는다: {bindings:?}"
    );
    let calls = sites_in("prepare_mesh_popups", |f| f == HOME);
    assert_eq!(calls.len(), 1, "{HOME} 의 prepare_mesh_popups 호출 하나");
    let call = &calls[0];
    let args = call
        .text
        .split_once("prepare_mesh_popups(")
        .map_or("", |(_, rest)| rest);
    assert!(
        args.starts_with("bodies.iter().copied(),"),
        "{}:{}: 지원하지 않는 표기: {}",
        call.file,
        call.line,
        call.text
    );
}

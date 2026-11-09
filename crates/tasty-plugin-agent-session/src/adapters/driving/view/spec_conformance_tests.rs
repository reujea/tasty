//! 사용자 문서(supervisor plugin 가이드,
//! "탭에서 직접" 절)가 **약속한 것**이 실제로 나오는지 대조한다.
//!
//! 단위 시험은 각 조각을 보지만, 그 조각이 **문서가 말한 대로인지**는
//! 안 본다. 문서가 먼저 늙는 것이 이 프로젝트의 반복 함정이라
//! (`spec-figures-aged-out-of-source` 5회), 약속을 코드로 고정한다.

use super::*;

fn prof(
    name: &str,
    kind: &str,
    cwd: Option<&str>,
    env: &[(&str, &str)],
) -> crate::adapters::driven::store::agents::AgentProfile {
    crate::adapters::driven::store::agents::AgentProfile {
        name: name.into(),
        kind: kind.into(),
        cwd: cwd.map(str::to_string),
        env: env
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        ..Default::default()
    }
}

/// 문서: *"agent 프로필 — select box, 등록된 프로필이 kind 로 묶여
/// **전부** 나온다. 각 항목에 그 프로필의 **환경변수가 함께** 적힌다"*
#[test]
fn the_agent_field_is_a_select_grouped_by_kind_with_env() {
    let roster = [
        prof("cc", "claude", None, &[("CLAUDE_CONFIG_DIR", "/p/dev")]),
        prof("cc-sdk", "claude", None, &[("CLAUDE_CONFIG_DIR", "/p/sdk")]),
        prof("pi-dev", "pi", None, &[]),
    ];
    let h = render_start(&[], &[], &roster, "", None);

    assert!(
        h.contains(r##"<select id="a" name="a""##),
        "select 가 아니다"
    );
    assert!(
        h.contains(r##"<optgroup label="claude">"##),
        "claude 묶음이 없다"
    );
    assert!(h.contains(r##"<optgroup label="pi">"##), "pi 묶음이 없다");
    // **전부** — 하나라도 빠지면 고를 수 없다.
    for p in &roster {
        assert!(
            // **끝을 단정하지 않는다** — `option` 에 속성이 더 붙을 수
            // 있다(0.1.18 이 `data-cwd` 를 더했고 이 단정이 깨졌다).
            h.contains(&format!(r##"<option value="{}""##, p.name)),
            "프로필 {} 이 목록에 없다",
            p.name
        );
    }
    // env 가 라벨에 있어야 cc 와 cc-sdk 를 가른다.
    assert!(
        h.contains("/p/dev") && h.contains("/p/sdk"),
        "env 가 안 적혀 둘을 못 가른다"
    );
}

/// 문서: *"작업 디렉토리 — 직접 치거나, 프로필이 쓰는 폴더를 눌러
/// 넣거나, 폴더 찾기… 로 고른다. **비우면 agent 설정을 따른다**"*
#[test]
fn the_cwd_field_offers_all_three_ways_the_doc_promises() {
    let roster = [prof("cc", "claude", Some("/w/one"), &[])];
    let h = render_start(&[], &[], &roster, "", None);

    assert!(
        h.contains(r##"<input id="c" name="c""##),
        "직접 치는 칸이 없다"
    );
    assert!(h.contains("/w/one"), "프로필이 쓰는 폴더 단추가 없다");
    assert!(h.contains("선택…"), "찾기가 없다");
    assert!(
        h.contains("비우면 에이전트 설정을 따릅니다"),
        "비웠을 때의 뜻이 안 적혀 있다"
    );
}

/// 문서: *"그 아래에 **이미 있는 세션에 붙기** 목록이 나란히 있다 —
/// 세션 id 를 **손으로 옮겨 적지 않아도** 된다"*
#[test]
fn running_sessions_are_listed_so_no_id_is_typed_by_hand() {
    let att = Attachable {
        id: "aaaaaaaa-1111".into(),
        agent: "cc".into(),
        state: "running".into(),
        reclaimed: false,
        cwd: Some("/w/proj".into()),
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let h = render_start(&[], &[att], &[], "", None);
    assert!(h.contains("세션 목록"), "그 절이 없다");
    assert!(h.contains("aaaaaaaa"), "세션이 안 보인다");
    assert!(h.contains("이 탭에서 열기"), "누를 자리가 없다");
}

/// 문서: *"확장자 필터를 **안 걸어 둔 것도 그래서다** — 걸면 그 확장자
/// 파일이 없는 폴더를 고를 수 없다"*
#[test]
fn the_picker_asks_for_no_extension_filter_as_documented() {
    let p = crate::adapters::driving::folder_pick::trigger_params(1, None, "");
    assert_eq!(
        p["filters"].as_array().map(Vec::len),
        Some(0),
        "필터를 걸면 그 확장자가 없는 폴더를 못 고른다"
    );
}

/// 문서: *"프로필은 **고르기만** 한다. 추가·수정은 CLI 의 몫이다"*
///
/// 폼에 편집 수단이 생기면 같은 설정을 두 곳에서 관리하게 된다.
///
/// ⚠️ **세는 것을 바꿨다**(0.1.51). 종전에는 *"입력칸이 하나뿐"* 으로
/// 셌는데, 그러면 **세션당 값**을 더해도 걸린다(model·name·첫 메시지 —
/// 그 셋은 프로필을 고치지 않는다). 숫자를 올리면 가드가 헐거워지니,
/// **허용 목록**으로 바꾼다 — 모르는 칸이 생기면 그때 걸린다.
///
/// 프로필 편집은 **관리 절**(`manage`, 문서 끝의 `id="manage"`)에만 있다 — 사용자가 시작 화면에서
/// 프로필 · 역할을 관리하기로 정했다(2026-10-30). 시작 폼 자체(관리 절 앞)는 여전히 세션당 값만 받는다.
#[test]
fn the_form_offers_no_way_to_edit_profiles() {
    let page = render_start(&[], &[], &[prof("cc", "claude", None, &[])], "", None);
    let cut = page
        .find(r#"id="manage""#)
        .expect("관리 절이 없다 — 프로필을 고칠 자리가 화면에 없다");
    assert!(
        page[cut..].contains(crate::adapters::driving::manage::PROFILE_SAVE),
        "관리 절에 프로필 저장이 없다"
    );
    let h = &page[..cut];
    // 이 폼이 받는 것은 **세션당 값**뿐이다. 프로필에 속한 것
    // (env·kind·바이너리 경로)은 여기서 못 고친다.
    // `ad`(이어받을 좌표)도 **세션당 값**이다 — 어느 대화를 이어서
    // 띄울지는 그 세션 하나의 성질이고, 프로필(env·kind·경로)은
    // 그대로다. 목록에 더할 때는 **그것이 왜 세션당 값인지**를
    // 적는다. 안 적으면 다음 사람이 프로필 칸도 같은 자리에 넣는다.
    let allowed = ["c", "m", "n", "p", "ad"];
    let ids: Vec<&str> = h
        .match_indices(r##"<input id=""##)
        .map(|(i, m)| {
            let rest = &h[i + m.len()..];
            &rest[..rest.find('"').unwrap_or(0)]
        })
        .collect();
    // 분모를 단언한다 — 칸을 하나도 못 찾았으면 "모르는 칸 0건" 은
    // 통과가 아니라 아무것도 안 본 것이다.
    assert!(
        !ids.is_empty(),
        "입력칸을 하나도 못 찾았다 — 세는 방식이 깨졌다"
    );
    let unknown: Vec<&&str> = ids.iter().filter(|i| !allowed.contains(i)).collect();
    assert!(
        unknown.is_empty(),
        "모르는 입력칸이 생겼다(프로필 편집?): {unknown:?} / 전체 {ids:?}"
    );
    assert!(h.contains("profile-set"), "고치는 법 안내가 없다");
}

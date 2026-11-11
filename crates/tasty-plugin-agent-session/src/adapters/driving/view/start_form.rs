//! 세션이 없는 탭의 **시작 화면** — 연결 · 에이전트 관리 · 역할 관리 세 탭을 한 문서로 그린다.
//!
//! 대화창(`render`)과 독자가 다르다. 시작 화면은 세션을 고르기 전 한 번 그려지고, 대화창은
//! 턴마다 스크립트로 고쳐진다. 그래서 나눠 둔다.
//!
//! 세 탭의 목록(세션 · 에이전트 · 역할)은 같은 표([`super::table`])를 쓴다.

use super::table::{self, Col};
use super::*;
use crate::domain::convo::{Attachable, PickField};

/// 세션이 없는 탭 — agent 를 골라 시작하는 폼. 받아 둔 모델 없이 그린다 — 시험이 쓰는 모양이다.
#[cfg(test)]
pub fn render_start(
    known: &[String],
    attachable: &[Attachable],
    profiles: &[crate::adapters::driven::store::agents::AgentProfile],
    cwd: &str,
    start_error: Option<&str>,
) -> String {
    render_start_with(
        &Default::default(),
        known,
        attachable,
        profiles,
        cwd,
        start_error,
        &Extras::default(),
    )
}

/// 시작 화면에 덧붙는 것 — 역할 명부 · 관리 결과 · 이 탭의 surface id · 관리 칸에 고른 폴더.
#[derive(Default)]
pub struct Extras<'a> {
    pub roles: &'a [crate::adapters::driven::store::roles::Role],
    pub manage_note: Option<&'a crate::adapters::driving::manage::Note>,
    /// 이 탭의 surface id — 세션 표의 탭 칸이 "이 탭" 을 가른다.
    pub this_tab: Option<u32>,
    /// 관리 탭의 폴더 칸(`pw` · `rp` · `rs`)에 피커로 고른 값 — 한 번만 실린다.
    pub field_pick: Option<(PickField, &'a str)>,
}

/// [`render_start`] 에 데몬에서 받아 둔 프로필별 모델(`models`)을 함께 싣는다 — 없는 프로필은 별칭이다.
pub fn render_start_with(
    models: &std::collections::BTreeMap<String, crate::domain::models::Choices>,
    known: &[String],
    attachable: &[Attachable],
    profiles: &[crate::adapters::driven::store::agents::AgentProfile],
    cwd: &str,
    start_error: Option<&str>,
    extras: &Extras<'_>,
) -> String {
    let mut h = String::with_capacity(16 * 1024);
    // 관리 탭에서 한 일(저장 · 역할 추가)의 결과는 그 탭에서 보여야 한다 — 문서가 다시 그려져도
    // 그 탭으로 연다. 그 밖에는 스크립트가 마지막으로 고른 탭을 되살린다.
    let forced = extras.manage_note.map(|n| n.tab.id());
    let on = |tab: &str| match forced {
        Some(f) if f == tab => " on",
        None if tab == "connect" => " on",
        _ => "",
    };
    h.push_str(r#"<!doctype html><meta charset="utf-8">"#);
    h.push_str(super::start_style::CSS);
    h.push_str(r#"<nav class="seg" role="tablist">"#);
    for (tab, label) in [
        ("connect", crate::domain::i18n::t("sup.form.tab_connect")),
        ("agents", crate::domain::i18n::t("sup.form.tab_agents")),
        ("roles", crate::domain::i18n::t("sup.form.tab_roles")),
    ] {
        h.push_str(&format!(
            r#"<button type="button" class="tab{on}" data-tab="{tab}" role="tab">{label}</button>"#,
            on = on(tab),
            label = escape(&label),
        ));
    }
    h.push_str("</nav>\n");
    h.push_str(&format!(
        r##"<div id="starting"><div class="spin"></div><div class="starting-msg" id="starting-msg"></div></div>
<section class="pane{on}" id="tab-connect">
"##,
        on = on("connect"),
    ));
    // 시작 폼과 역할 표를 나란히 둔다 — 좁으면 표가 폼 아래로 내려간다(CSS `.connect-split`).
    // 제목은 **왼쪽 칸 안**에 둔다. 두 칸 바깥에 두면 오른쪽 역할 제목이 제목 · 설명 높이만큼
    // 내려가 두 칸의 머리가 어긋난다.
    h.push_str(r#"<div class="connect-split"><div class="connect-main">"#);
    h.push_str(&format!(
        r#"<h1>{title}</h1>
<p class="sub">{sub}</p>
"#,
        title = escape(&crate::domain::i18n::t("sup.form.title")),
        sub = escape(&crate::domain::i18n::t("sup.form.subtitle")),
    ));
    // 직전 시작이 실패했으면 **그 사유를 남긴다**. 이것이 없으면 화면이 직전과 똑같아 재적재가
    // 건너뛰어지고, spinner 가 영영 남는다.
    if let Some(e) = start_error {
        h.push_str(&format!(
            r##"<div class="start-err">{}: {}</div>"##,
            escape(&crate::domain::i18n::t("sup.form.start_failed")),
            escape(e)
        ));
    }
    h.push_str(&connect_form(profiles, cwd));
    h.push_str(&known_or_hint(profiles, known));
    h.push_str("</div>");
    h.push_str(&crate::adapters::driving::manage::role_table(extras.roles));
    h.push_str("</div>");
    h.push_str(&session_section(attachable, extras.this_tab));
    // 스크립트는 **요소 뒤에** 붙인다 — 앞에 있으면 `getElementById` 가 전부 `null` 이라 직접
    // 바인딩이 아무것도 안 붙는다(WKWebView 실측).
    h.push_str(&connect_script(profiles, models));
    h.push_str(crate::adapters::driving::manage::ROLE_SCRIPT);
    h.push_str("</section>");
    // 관리 탭 둘 — 자기 스크립트를 가지므로 연결 스크립트 **뒤**다. 연결 스크립트가 문서의 첫
    // 스크립트로 남는다(그 스크립트를 재는 시험들이 첫 `<script>` 를 본다).
    let pick_for = |f: PickField| extras.field_pick.filter(|(p, _)| *p == f).map(|(_, v)| v);
    h.push_str(&format!(
        r#"<section class="pane{}" id="tab-agents">"#,
        on("agents")
    ));
    h.push_str(&crate::adapters::driving::manage::render_agents(
        profiles,
        extras.manage_note,
        pick_for(PickField::AgentCwd),
    ));
    h.push_str("</section>");
    h.push_str(&format!(
        r#"<section class="pane{}" id="tab-roles">"#,
        on("roles")
    ));
    h.push_str(&crate::adapters::driving::manage::render_roles(
        extras.roles,
        extras.manage_note,
        pick_for(PickField::RolePath),
        pick_for(PickField::RoleScan),
    ));
    h.push_str("</section>");
    h.push_str(crate::adapters::driving::manage::FORM_SCRIPT);
    h.push_str(table::SCRIPT);
    h.push_str(&tab_script(forced));
    h
}

/// 연결 탭의 폼 — 에이전트 · 작업 폴더 · 접힌 옵션 · 시작. 역할은 옆 표([`role_table`])에서 고른다.
///
/// [`role_table`]: crate::adapters::driving::manage::role_table
fn connect_form(
    profiles: &[crate::adapters::driven::store::agents::AgentProfile],
    cwd: &str,
) -> String {
    let t = |k: &'static str| escape(&crate::domain::i18n::t(k));
    let mut h = format!(
        r##"<form method="get" action="#" class="fgrid" id="sform">
 <label for="a">{agent}</label><div class="ctl">{field}</div>
 <label for="c">{cwd_label}</label><div class="ctl"><div class="with-btn"><input id="c" name="c" autocomplete="off" value="{cwd}" class="mono" placeholder="{cwd_hint}"><a class="btn" href="#{BROWSE_MARK}=1">{browse}</a></div><p class="help">{browse_note}</p></div>
"##,
        agent = t("sup.form.agent_label"),
        field = agent_field(profiles),
        cwd_label = t("sup.form.cwd_label"),
        // 고른 폴더를 되싣는다 — 단추를 누르면 이 함수가 다시 돌고, 그때 칸이 비어 있으면 누른
        // 것이 사라진 것처럼 보인다.
        cwd = escape(cwd),
        cwd_hint = t("sup.form.cwd_hint"),
        browse = t("sup.form.browse"),
        browse_note = t("sup.form.browse_note"),
    );
    // 모델 · 이름 · 이어받기 · 권한 모드는 접는다(사용자 지정). 접는 블록에는 스타일이 같은
    // 변경에 따라와야 한다 — 브라우저 기본 `summary` 는 어두운 바탕에서 안 보인다.
    h.push_str(&format!(
        r##" <details class="more"><summary>{options}</summary><div class="fgrid">
 <label for="m">{model_label}</label><div class="ctl">{model_pick}<input id="m" name="m" autocomplete="off" class="mono" placeholder="{model_hint}"><p class="help" id="mnote"></p></div>
 <label for="n">{name_label}</label><div class="ctl"><input id="n" name="n" autocomplete="off" placeholder="{name_hint}"></div>
 <label for="ad">{adopt_label}</label><div class="ctl"><input id="ad" name="ad" autocomplete="off" class="mono" placeholder="{adopt_hint}"><p class="help">{adopt_note}</p></div>
 <label for="pm">{pm_label}</label><div class="ctl"><select id="pm" name="pm">{pm_options}</select><p class="help">{pm_note}</p></div>
 </div></details>
 <input type="hidden" name="{START}" value="1">
 <div class="factions"><button type="submit" class="primary">{start}</button></div>
</form>
"##,
        options = t("sup.form.section_options"),
        model_label = t("sup.form.model_label"),
        model_hint = t("sup.form.model_hint"),
        // 프로필을 고를 수 있을 때만 목록을 놓는다 — 직접 치는 프로필 칸이면 무엇을 보일지 모른다.
        model_pick = if profiles.is_empty() {
            ""
        } else {
            r#"<select id="ms" class="models"></select>"#
        },
        name_label = t("sup.form.name_label"),
        name_hint = t("sup.form.name_hint"),
        adopt_label = t("sup.form.adopt_label"),
        adopt_hint = t("sup.form.adopt_hint"),
        adopt_note = t("sup.form.adopt_note"),
        pm_label = t("sup.form.permission_label"),
        pm_options = permission_options(),
        pm_note = t("sup.form.permission_note"),
        START = START_MARK,
        start = t("sup.form.start"),
    ));
    h
}

/// 에이전트 칸 — 명부를 읽었으면 select(종류로 묶음), 못 읽었으면 직접 입력이다. 빈 select 는
/// 아무것도 못 고르게 만들어 폼 자체를 막는다.
fn agent_field(profiles: &[crate::adapters::driven::store::agents::AgentProfile]) -> String {
    if profiles.is_empty() {
        return format!(
            r##"<input id="a" name="a" autofocus autocomplete="off" placeholder="{}">"##,
            escape(&crate::domain::i18n::t("sup.form.agent_hint"))
        );
    }
    let mut h = String::from(r##"<select id="a" name="a" autofocus>"##);
    for kind in crate::adapters::driven::store::agents::kinds(profiles) {
        h.push_str(&format!(r##"<optgroup label="{}">"##, escape(&kind)));
        for a in crate::adapters::driven::store::agents::of_kind(profiles, &kind) {
            // env 를 라벨에 싣는다 — 이름만으로는 `cc` 와 `cc-sdk` 가 무엇이 다른지 알 수 없다.
            let env: Vec<String> = a.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            let tail = if env.is_empty() {
                String::new()
            } else {
                format!("  ({})", env.join(" "))
            };
            // cwd 를 option 에 싣는다 — 고르면 스크립트가 그 값을 입력칸에 채운다. 별도 표를 두면
            // 프로필이 바뀔 때 한쪽만 고쳐져 **틀린 폴더로 세션이 뜬다**(실제 사고가 있던 축).
            h.push_str(&format!(
                r##"<option value="{}" data-cwd="{}">{}{}</option>"##,
                escape(&a.name),
                escape(a.cwd.as_deref().unwrap_or_default()),
                escape(&a.name),
                escape(&tail)
            ));
        }
        h.push_str("</optgroup>");
    }
    h.push_str("</select>");
    h
}

/// 명부가 있으면 고치는 곳을 알리고, 없으면 지금 도는 세션의 agent 를 단추로 놓는다.
fn known_or_hint(
    profiles: &[crate::adapters::driven::store::agents::AgentProfile],
    known: &[String],
) -> String {
    if !profiles.is_empty() {
        return format!(
            r##"<p class="hint">{}</p>"##,
            crate::domain::i18n::t("sup.form.edit_profiles_hint")
        );
    }
    if known.is_empty() {
        return String::new();
    }
    let mut h = format!(
        r##"<p class="known">{}: "##,
        escape(&crate::domain::i18n::t("sup.form.running_now"))
    );
    for a in known {
        // 단추도 같은 폼 표식을 쓴다 — 누르면 그 agent 로 바로 시작한다.
        h.push_str(&format!(
            r##"<a href="#a={}&amp;{}=1">{}</a>"##,
            escape(a),
            START_MARK,
            escape(a)
        ));
    }
    h.push_str("</p>");
    h
}

/// 세션 상태 → (점 클래스, 화면 글). 도는 중 · 재워짐 · 그 밖을 색으로 가르되 글을 늘 함께 둔다
/// (색만으로 가르지 않는다). 모르는 상태는 데몬이 준 글을 그대로 보인다.
fn state_label(a: &Attachable) -> (&'static str, String) {
    let t = crate::domain::i18n::t;
    match a.state.as_str() {
        "running" => ("run", t("sup.state.running")),
        "stopped" if a.reclaimed => ("slept", t("sup.state.slept")),
        "stopped" => ("dead", t("sup.state.stopped")),
        "starting" => ("dead", t("sup.state.starting")),
        "finished" => ("dead", t("sup.state.finished")),
        "failed" => ("dead", t("sup.state.failed")),
        "blocked_precondition" => ("dead", t("sup.state.blocked_precondition")),
        _ if a.reclaimed => ("slept", t("sup.state.slept")),
        other => ("dead", other.to_string()),
    }
}

/// 세션 목록 — 검색 · 새로고침 · 표 · 일괄 삭제 바.
fn session_section(attachable: &[Attachable], this_tab: Option<u32>) -> String {
    let t = |k: &'static str| escape(&crate::domain::i18n::t(k));
    let mut h = format!(
        r##"<div class="sect"><h2>{}</h2>"##,
        t("sup.form.attach_title")
    );
    if attachable.is_empty() {
        h.push_str(&format!(
            r##"<p class="none">{}</p></div>"##,
            t("sup.form.no_attachable")
        ));
        return h;
    }
    // 검색은 **클라이언트**가 한다 — 서버 왕복이 곧 문서 재적재이고, 그것이 깜빡임이다.
    h.push_str(&format!(
        r##"<div class="tools">
 <input id="q" autocomplete="off" placeholder="{search}">
 <span class="count" id="count"></span>
 <a class="btn refresh" id="refresh" href="#{REFRESH}=1">{refresh}</a>
</div>"##,
        search = t("sup.form.search_placeholder"),
        REFRESH = REFRESH_MARK,
        refresh = t("sup.form.refresh"),
    ));
    let rows: String = attachable
        .iter()
        .map(|a| session_row(a, this_tab))
        .collect();
    // 전체 선택은 머리글의 체크 칸이다 — **보이는 행만** 고른다(검색이 숨긴 것은 뺀다).
    let all = format!(
        r##"<input type="checkbox" id="allpick" aria-label="{s}" title="{s}">"##,
        s = t("sup.form.select_all")
    );
    // 폭을 정하지 않은 칸은 폴더 하나다 — 행을 가르는 것은 폴더 뒷부분이라 남는 폭을 몰아 준다.
    // id 두 칸은 가운데 말줄임이 앞뒤를 남기므로 고정 폭이면 충분하다.
    let cols = [
        Col::raw(all, "pickcol", "32px"),
        Col::new(&crate::domain::i18n::t("sup.form.col_state"), "", "96px"),
        Col::new(&crate::domain::i18n::t("sup.form.col_agent"), "", "112px"),
        Col::new(&crate::domain::i18n::t("sup.form.col_folder"), "", ""),
        Col::new(&crate::domain::i18n::t("sup.form.col_changes"), "", "120px"),
        Col::new(
            &crate::domain::i18n::t("sup.form.col_session_id"),
            "",
            "160px",
        ),
        Col::new(
            &crate::domain::i18n::t("sup.form.col_agent_session"),
            "",
            "136px",
        ),
        Col::new(&crate::domain::i18n::t("sup.form.col_tab"), "num", "48px"),
        Col::raw(String::new(), "acts", "184px"),
    ];
    h.push_str(&table::table("rows", &cols, &rows, 960));
    // 여러 개 고르기 — 고른 것은 스크립트의 집합(`picks`)이 쥔다. 검색은 행을 숨길 뿐이라 DOM
    // 질의로 뽑으면 숨은 행이 섞이거나 빠진다. 삭제는 단건과 같은 두 단계다.
    h.push_str(&format!(
        r##"<div class="bulk" id="bulk" style="display:none">
 <span id="picked"></span>
 <a class="btn danger del" id="delsel" href="#">{}</a>
</div>
<p class="none" id="nomatch" style="display:none">{}</p></div>"##,
        t("sup.form.delete_selected"),
        t("sup.form.no_match")
    ));
    h
}

/// 세션 한 행 — 고르기 · 상태 · 에이전트 · 폴더 · 변경 · 세션 id · 에이전트 세션 · 탭 · 조작.
fn session_row(a: &Attachable, this_tab: Option<u32>) -> String {
    let t = |k: &'static str| escape(&crate::domain::i18n::t(k));
    let (cls, label) = state_label(a);
    let running = a.state == "running";
    // 폴더 — 같은 agent 로 띄운 세션이 여럿이면 이것이 유일하게 가르는 값이다. `None` 은
    // "agent 설정을 따른다" 이지 *"폴더가 없다"* 가 아니다 — 빈 칸으로 두지 않는다.
    let cwd = match a.cwd.as_deref() {
        Some(c) => table::path(Some(c)),
        None => format!(
            r#"<span class="inherit">{}</span>"#,
            t("sup.form.inherit_cwd")
        ),
    };
    // 남은 일 — 결정 대기 · 미커밋 · 마지막 활동. 모르면 회색 `—`.
    let rem = crate::adapters::driven::remaining::label(&a.remaining);
    let rem = if rem == "-" {
        table::nil()
    } else {
        format!(r#"<span title="{r}">{r}</span>"#, r = escape(&rem))
    };
    // 에이전트 쪽 세션 id 와 이름 — 이어받은 대화는 supervisor id 와 달라서, 다른 곳에서 쓰던
    // 대화가 어느 행인지는 이 칸으로만 찾는다. 이름은 알 때만 괄호로 붙인다.
    let agent_session = match (
        a.agent_session_id.as_deref(),
        a.agent_session_name.as_deref(),
    ) {
        (Some(id), Some(n)) => format!(
            r#"<span class="pair">{} <span class="aname">({})</span></span>"#,
            table::id(Some(id)),
            escape(n)
        ),
        (id, _) => table::id(id),
    };
    // 탭 — 물린 세션은 surface 가 세션 하나만 들 수 있어(`surface.meta` 1:1) 고르면 앞 탭의 짝이
    // 조용히 끊긴다. 그래서 목록에 두되 **어느 탭에 있는지** 적고 열기 단추를 안 낸다.
    let tab = match a.taken_by {
        Some(n) if Some(n) == this_tab => t("sup.form.this_tab"),
        Some(n) => format!(
            r#"<span class="inuse" title="{}">{n}</span>"#,
            escape(&crate::domain::i18n::t1("sup.form.in_use", &n.to_string()))
        ),
        None => table::nil(),
    };
    let open = match a.taken_by {
        Some(_) => String::new(),
        None => format!(
            r##"<a class="btn" href="#s={}&amp;{}=1">{}</a>"##,
            escape(&a.id),
            ATTACH_MARK,
            t("sup.form.attach_here")
        ),
    };
    // 도는 세션은 삭제 링크를 안 낸다 — 데몬이 거부하고(`invalid_state`), 누를 수 있게 두면
    // 사용자는 눌러 보고 실패를 본다. 고르기 칸도 같은 규칙이다.
    let del = if running {
        format!(
            r##"<span class="btn danger off" aria-disabled="true" title="{}">{}</span>"##,
            t("sup.form.delete_running"),
            t("sup.form.delete")
        )
    } else {
        format!(
            r##"<a class="btn danger del" href="#s={}&amp;{}=1">{}</a>"##,
            escape(&a.id),
            DELETE_MARK,
            t("sup.form.delete")
        )
    };
    let pick = if running {
        String::new()
    } else {
        format!(
            r##"<input type="checkbox" class="pick" value="{}">"##,
            escape(&a.id)
        )
    };
    // 검색 키 — agent · 폴더 · id · 에이전트 세션 id · 이름을 한 줄로 모은다.
    let key = [
        Some(a.agent.as_str()),
        a.cwd.as_deref(),
        Some(a.id.as_str()),
        a.agent_session_id.as_deref(),
        a.agent_session_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|v| !v.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase();
    // 세션 id 는 **통째로** 싣는다(2026-10-02 · 상태줄과 같은 축). 오류 문면이 전체 id 로 오므로
    // 앞 8자만 보이면 대조하려고 CLI 를 따로 띄워야 한다. 칸이 좁으면 가운데만 줄인다.
    format!(
        r##"<tr data-k="{key}"><td class="pickcol">{pick}</td><td><span class="dot {cls}"></span>{label}</td><td title="{agent}">{agent}</td><td class="path mono">{cwd}</td><td class="rem">{rem}</td><td class="mono">{sid}</td><td class="mono">{agent_session}</td><td class="num">{tab}</td><td class="acts">{open}{del}</td></tr>"##,
        key = escape(&key),
        label = escape(&label),
        agent = escape(&a.agent),
        sid = table::id(Some(&a.id)),
    )
}

/// 연결 탭의 스크립트 — 에이전트 → 폴더 · 모델 채우기, 시작 가로채기(+spinner), 검색, 여러 개
/// 고르기, 두 단계 삭제. JS 가 꺼져도 제출 · 열기 · 삭제는 링크라 그대로 간다.
fn connect_script(
    profiles: &[crate::adapters::driven::store::agents::AgentProfile],
    models: &std::collections::BTreeMap<String, crate::domain::models::Choices>,
) -> String {
    format!(
        r##"<script>
(function(){{
var $=function(id){{return document.getElementById(id);}};

/* agent -> folder */
var sel=$('a'), cwd=$('c');
function fillCwd(){{
  if(!sel||!cwd)return;
  var o=sel.options[sel.selectedIndex];
  if(!o)return;
  var d=o.getAttribute('data-cwd')||'';
  if(cwd.dataset.touched==='1')return;
  cwd.value=d;
  cwd.placeholder=d?d:'{ph}';
}}
if(sel){{
  sel.addEventListener('change',fillCwd);
  if(cwd && !cwd.value) fillCwd();
}}
if(cwd)cwd.addEventListener('input',function(){{cwd.dataset.touched='1';}});

/* agent -> models */
var ms=$('ms'), mi=$('m'), mn=$('mnote'), MODELS={models_json};
var CUSTOM='\u0001custom';
function showInput(on){{if(mi)mi.style.display=on?'':'none';}}
function fillModels(){{
  if(!ms)return;
  var o=sel&&sel.options[sel.selectedIndex];
  var e=MODELS[o?o.value:'']||{{l:[],n:''}};
  var keep=mi?mi.value:'';
  ms.innerHTML='';
  var add=function(v,t){{var x=document.createElement('option');x.value=v;x.textContent=t;ms.appendChild(x);}};
  add('','{model_default}');
  for(var i=0;i<e.l.length;i++)add(e.l[i],e.l[i]);
  add(CUSTOM,'{model_custom}');
  if(!keep)ms.value='';
  else if(e.l.indexOf(keep)>=0)ms.value=keep;
  else ms.value=CUSTOM;
  showInput(ms.value===CUSTOM);
  if(mn)mn.textContent=e.n||'';
}}
if(ms){{
  ms.addEventListener('change',function(){{
    if(ms.value===CUSTOM){{showInput(true);if(mi){{mi.value='';mi.focus();}}}}
    else{{showInput(false);if(mi)mi.value=ms.value;}}
  }});
  if(sel)sel.addEventListener('change',function(){{if(mi&&ms.value!==CUSTOM)mi.value='';fillModels();}});
  fillModels();
}}
window.supSetModels=function(name,e){{
  MODELS[name]=e;
  var o=sel&&sel.options[sel.selectedIndex];
  if(o&&o.value===name)fillModels();
}};

/* start: GET submit leaves the page (white screen); a fragment stays in the document */
var f=$('sform'), ov=$('starting'), om=$('starting-msg');
if(f)f.addEventListener('submit',function(e){{
  e.preventDefault();
  if(om)om.textContent='{starting}';
  if(ov)ov.classList.add('on');
  var a=sel?sel.value:(document.getElementById('a')||{{}}).value||'';
  var c=cwd?cwd.value:'';
  var g=function(id){{var e=$(id);return e&&e.value?e.value.trim():'';}};
  var h='{START}=1&a='+encodeURIComponent(a)+'&c='+encodeURIComponent(c);
  var extra=[['m','m'],['n','n'],['ad','ad'],['pm','pm']];
  for(var i=0;i<extra.length;i++){{
    var v=g(extra[i][1]);
    if(v)h+='&'+extra[i][0]+'='+encodeURIComponent(v);
  }}
  var rs=document.querySelectorAll('input[name=r]:checked');
  for(var j=0;j<rs.length;j++)h+='&r='+encodeURIComponent(rs[j].value);
  location.hash=h;
}});
/* opening a session can take a while too: same cover */
document.addEventListener('click',function(e){{
  var t=e.target; if(!t||!t.closest)return;
  var a=t.closest('a[href^="#s="]');
  if(a&&ov&&!a.classList.contains('del')){{if(om)om.textContent='{starting}';ov.classList.add('on');}}
}});

/* search: hide rows, scroll instead of pages */
var q=$('q'), tbl=$('rows');
if(q&&tbl){{
  var all=[].slice.call(tbl.tBodies[0].rows);
  function view(){{
    var t=(q.value||'').trim().toLowerCase();
    var n=0;
    all.forEach(function(r){{
      var hit=!t||(r.getAttribute('data-k')||'').indexOf(t)>=0;
      r.style.display=hit?'':'none';
      if(hit)n++;
    }});
    var c=$('count'); if(c)c.textContent='{count}'.replace('{{n}}',n);
    var nm=$('nomatch'); if(nm)nm.style.display=(n?'none':'');
    syncAll();
    if(window.supFit)window.supFit();
  }}
  q.addEventListener('input',view);
  view();
}}

/* multi-select. NOT `sel` - that name is the agent <select> above, in the same IIFE */
var picks={{}};
function picked(){{var k=[];for(var i in picks)if(picks[i])k.push(i);return k;}}
function bulkview(){{
  var k=picked(), b=$('bulk'), pk=$('picked');
  if(b)b.style.display=(k.length?'flex':'none');
  if(pk)pk.textContent='{picked_n}'.replace('{{n}}',k.length);
  var da=$('delsel'); if(da)da.textContent='{del_sel}';
}}
function mark(cb){{
  var tr=cb.closest('tr');
  if(tr)tr.classList.toggle('on',cb.checked);
}}
/* select-all acts on visible rows only: picking rows the user cannot see would delete sessions they never saw */
function visiblePicks(){{
  var out=[], rows=document.querySelectorAll('#rows tbody tr');
  for(var i=0;i<rows.length;i++){{
    if(rows[i].style.display==='none')continue;
    var cb=rows[i].querySelector('.pick');
    if(cb)out.push(cb);
  }}
  return out;
}}
function syncAll(){{
  var ap=$('allpick'); if(!ap)return;
  var v=visiblePicks();
  var on=0;
  for(var i=0;i<v.length;i++)if(v[i].checked)on++;
  ap.checked=(v.length>0&&on===v.length);
  ap.indeterminate=(on>0&&on<v.length);
  ap.disabled=(v.length===0);
}}
document.addEventListener('change',function(e){{
  var t=e.target;
  if(!t||!t.classList||!t.classList.contains('pick'))return;
  picks[t.value]=t.checked;
  mark(t);
  bulkview();
  syncAll();
}});
/* clicking the row toggles its checkbox */
document.addEventListener('click',function(e){{
  var t=e.target; if(!t||!t.closest)return;
  if(t.closest('a')||t.closest('button')||t.closest('.btn'))return;
  if(t.classList&&t.classList.contains('pick'))return;
  var tr=t.closest('#rows tr'); if(!tr||tr.parentNode.tagName!=='TBODY')return;
  var cb=tr.querySelector('.pick'); if(!cb)return;
  cb.checked=!cb.checked;
  picks[cb.value]=cb.checked;
  mark(cb);
  bulkview();
  syncAll();
}});
document.addEventListener('change',function(e){{
  var t=e.target;
  if(!t||t.id!=='allpick')return;
  var v=visiblePicks(), on=t.checked;
  for(var i=0;i<v.length;i++){{
    v[i].checked=on;
    picks[v[i].value]=on;
    mark(v[i]);
  }}
  bulkview();
  syncAll();
}});
syncAll();

/* bulk delete: ask first, then send with ok=1 */
var asked=false;
document.addEventListener('click',function(e){{
  var t=e.target; if(!t||!t.closest)return;
  var ds=t.closest('#delsel'); if(!ds)return;
  e.preventDefault();
  var k=picked(); if(!k.length)return;
  if(!asked){{
    asked=true;
    ds.textContent='{ask_many}'.replace('{{n}}',k.length);
    return;
  }}
  var qs=k.map(function(i){{return 's='+encodeURIComponent(i);}}).join('&');
  if(om)om.textContent='{starting}';
  if(ov)ov.classList.add('on');
  /* the host reads the query first and ignores the fragment when a `?` is present */
  history.replaceState(null,'','about:blank');
  location.hash=qs+'&{DEL}=1&ok=1';
}});

/* single delete (sessions, agents, roles): the first click asks in place, the second sends with ok=1 */
document.addEventListener('click',function(e){{
  var t=e.target; if(!t||!t.closest)return;
  var a=t.closest('a.del'); if(!a)return;
  /* the bulk button shares .del but has its own handler */
  if(a.id==='delsel')return;
  e.preventDefault();
  var td=a.parentNode, href=a.getAttribute('href'), saved=td.innerHTML;
  td.classList.add('asking');
  td.innerHTML='<span class="confirm">{ask}'
    +'<a class="yes" href="'+href+'&amp;ok=1">{yes}</a>'
    +'<a class="no" href="#">{no}</a></span>';
  td.querySelector('.no').addEventListener('click',function(ev){{
    ev.preventDefault(); td.innerHTML=saved; td.classList.remove('asking');
  }});
}});
}})();
</script>
"##,
        START = START_MARK,
        models_json = crate::domain::models::initial_json(profiles, models),
        model_default = escape(&crate::domain::i18n::t("sup.form.model_default")),
        model_custom = escape(&crate::domain::i18n::t("sup.form.model_custom")),
        ph = escape(&crate::domain::i18n::t("sup.form.inherit_cwd")),
        starting = escape(&crate::domain::i18n::t("sup.form.starting")),
        count = escape(&crate::domain::i18n::t("sup.form.count")),
        ask = escape(&crate::domain::i18n::t("sup.form.delete_confirm")),
        yes = escape(&crate::domain::i18n::t("sup.form.delete_yes")),
        no = escape(&crate::domain::i18n::t("sup.form.delete_no")),
        DEL = DELETE_MARK,
        picked_n = escape(&crate::domain::i18n::t("sup.form.picked")),
        del_sel = escape(&crate::domain::i18n::t("sup.form.delete_selected")),
        ask_many = escape(&crate::domain::i18n::t("sup.form.delete_ask_many")),
    )
}

/// 탭 전환 — 문서를 다시 받지 않는다. 주소의 `#` 은 plugin 으로 가는 신호라 탭은 단추로 바꾼다.
///
/// 고른 탭은 `sessionStorage` 에 둔다 — 열기 · 삭제 같은 조작마다 문서가 다시 그려져도 같은 탭이
/// 남는다. 저장소를 못 쓰면 서버가 정한 탭이다. 관리 결과(`forced`)가 있으면 그 탭이 이긴다.
/// `data-goto` 단추(연결 탭의 "역할 관리" 등)도 같은 전환을 쓴다.
fn tab_script(forced: Option<&str>) -> String {
    let forced = forced.map_or_else(|| "null".to_string(), |f| format!("'{f}'"));
    format!(
        r##"<script>(function(){{
var tabs=document.querySelectorAll('.seg .tab');
function show(name){{
  for(var i=0;i<tabs.length;i++){{
    var t=tabs[i], on=t.getAttribute('data-tab')===name;
    t.classList.toggle('on',on); t.setAttribute('aria-selected',on?'true':'false');
    var p=document.getElementById('tab-'+t.getAttribute('data-tab')); if(p)p.classList.toggle('on',on);
  }}
  try{{sessionStorage.setItem('agent-start-tab',name);}}catch(e){{}}
  if(window.supFit)window.supFit();
}}
for(var i=0;i<tabs.length;i++)tabs[i].addEventListener('click',function(){{show(this.getAttribute('data-tab'));}});
document.addEventListener('click',function(e){{
  var g=e.target&&e.target.closest&&e.target.closest('[data-goto]'); if(!g)return;
  e.preventDefault(); show(g.getAttribute('data-goto'));
}});
var forced={forced};
var kept=null; try{{kept=sessionStorage.getItem('agent-start-tab');}}catch(e){{}}
if(forced){{show(forced);}}else if(kept&&document.getElementById('tab-'+kept)){{show(kept);}}else{{show('connect');}}
}})();</script>"##
    )
}

/// 기동 권한 모드 — 데몬이 받는 6종(protocol 13 · consumer-contract §7-B). 맨 앞의 빈 값은 프로필의 모드다.
pub(super) const PERMISSION_MODES: [&str; 6] = [
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "manual",
    "dontAsk",
    "plan",
];

/// 권한 모드 선택지 — 빈 값(프로필 기본)이 먼저다. 빈 값은 폼이 보내지 않는다(`parse_start` 가 거른다).
fn permission_options() -> String {
    let mut h = format!(
        r#"<option value="">{}</option>"#,
        escape(&crate::domain::i18n::t("sup.form.permission_default"))
    );
    for m in PERMISSION_MODES {
        h.push_str(&format!(r#"<option value="{m}">{m}</option>"#));
    }
    h
}

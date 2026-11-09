//! 에이전트 관리 · 역할 관리 탭과 연결 탭의 역할 칸을 그린다 — 요청 해석과 반영은 상위 모듈이다.

use serde_json::Value;

use super::{Note, PROFILE_DELETE, PROFILE_SAVE, ROLE_ADD, ROLE_DELETE, ROLE_SCAN, Tab};
use crate::adapters::driven::store::agents::{self, AgentProfile};
use crate::adapters::driven::store::roles::Role;
use crate::adapters::driving::html::escape;
use crate::adapters::driving::view::table::{self, Col};
use crate::domain::convo::PickField;
use crate::domain::i18n::t;

/// 연결 탭의 역할 표 — 시작 폼 옆에 늘 그린다. 명부가 비면 회색 안내와 역할 관리 탭으로 가는 단추를 둔다.
///
/// 칸은 고르기 · 이름 · 경로다. 검색과 쪽 넘김은 **클라이언트**가 한다([`ROLE_SCRIPT`]) — 서버 왕복이 곧
/// 문서 재적재라 깜빡인다. 행은 숨기기만 하므로 다른 쪽에서 고른 체크 칸도 제출에 실린다.
pub fn role_table(roles: &[Role]) -> String {
    let head = format!(
        r##"<aside class="rolebox" aria-labelledby="rl"><h2 id="rl">{}</h2>"##,
        escape(&t("sup.form.roles_label"))
    );
    if roles.is_empty() {
        return format!(
            r##"{head}<p class="roles-empty">{} <button type="button" class="linkbtn" data-goto="roles">{}</button></p></aside>
"##,
            escape(&t("sup.form.no_roles")),
            escape(&t("sup.form.tab_roles"))
        );
    }
    let mut h = format!(
        r##"{head}<div class="tools">
 <input id="rq" autocomplete="off" placeholder="{search}">
 <span class="count" id="rcount" data-fmt="{count}"></span>
</div>"##,
        search = escape(&t("sup.form.roles_search")),
        count = escape(&t("sup.form.roles_count")),
    );
    let rows: String = roles
        .iter()
        .map(|r| {
            let missing = if r.guide().is_none() {
                format!(
                    r##" <span class="noguide">({})</span>"##,
                    escape(&t("sup.manage.no_guide"))
                )
            } else {
                String::new()
            };
            format!(
                r##"<tr data-k="{key}"><td class="pickcol"><input type="checkbox" name="r" value="{name}" aria-label="{name}"></td><td>{name}{missing}</td><td class="mono">{path}</td></tr>"##,
                key = escape(&format!("{} {}", r.name, r.path).to_lowercase()),
                name = escape(&r.name),
                path = table::path(Some(&r.path)),
            )
        })
        .collect();
    let cols = [
        Col::raw(String::new(), "pickcol", "32px"),
        Col::new(&t("sup.form.roles_col_name"), "", "36%"),
        Col::new(&t("sup.form.roles_col_path"), "", ""),
    ];
    h.push_str(&table::table("rpick", &cols, &rows, 280));
    h.push_str(&format!(
        r##"<p class="none" id="rnomatch" hidden>{nomatch}</p>
<div class="pager"><button type="button" class="btn" id="rprev" aria-label="{prev}" title="{prev}">&#8249;</button><span class="count" id="rpage"></span><button type="button" class="btn" id="rnext" aria-label="{next}" title="{next}">&#8250;</button></div>
<p class="help">{note}</p></aside>
"##,
        nomatch = escape(&t("sup.form.roles_no_match")),
        prev = escape(&t("sup.form.roles_prev")),
        next = escape(&t("sup.form.roles_next")),
        note = escape(&t("sup.form.roles_note")),
    ));
    h
}

/// 역할 표의 검색 · 쪽 넘김 · 행 클릭 — 연결 스크립트 **뒤**에 붙인다(그 스크립트가 문서의 첫
/// `<script>` 로 남아야 한다). 한 쪽은 8행이다(docs `index.md` 연결 탭).
pub const ROLE_SCRIPT: &str = r#"<script>(function(){
var tbl=document.getElementById('rpick'); if(!tbl)return;
var PAGE=8, page=0;
var q=document.getElementById('rq'), cnt=document.getElementById('rcount'), pg=document.getElementById('rpage');
var prev=document.getElementById('rprev'), next=document.getElementById('rnext'), nm=document.getElementById('rnomatch');
var all=[].slice.call(tbl.tBodies[0].rows);
function view(){
  var t=(q&&q.value||'').trim().toLowerCase();
  var hits=all.filter(function(r){return !t||(r.getAttribute('data-k')||'').indexOf(t)>=0;});
  var pages=Math.max(1,Math.ceil(hits.length/PAGE));
  if(page>=pages)page=pages-1;
  all.forEach(function(r){r.style.display='none';});
  hits.slice(page*PAGE,(page+1)*PAGE).forEach(function(r){r.style.display='';});
  var k=tbl.querySelectorAll('input[name=r]:checked').length;
  if(cnt)cnt.textContent=(cnt.getAttribute('data-fmt')||'').replace('{n}',hits.length).replace('{k}',k);
  if(pg)pg.textContent=(page+1)+' / '+pages;
  if(prev)prev.classList.toggle('off',page===0);
  if(next)next.classList.toggle('off',page>=pages-1);
  if(nm)nm.hidden=hits.length>0;
  if(window.supFit)window.supFit();
}
if(q)q.addEventListener('input',function(){page=0;view();});
if(prev)prev.addEventListener('click',function(){if(page>0){page--;view();}});
if(next)next.addEventListener('click',function(){page++;view();});
tbl.addEventListener('click',function(e){
  var r=e.target.closest&&e.target.closest('tbody tr'); if(!r)return;
  var c=r.querySelector('input[name=r]');
  if(c&&e.target!==c)c.checked=!c.checked;
  view();
});
view();
})();</script>
"#;

/// 편집할 때 칸을 채우는 데이터 — 이름 → 칸 값. `</` 를 끊어 `<script>` 안에서 안전하게 둔다.
fn fill_data(profiles: &[AgentProfile]) -> String {
    let map: serde_json::Map<String, Value> = profiles
        .iter()
        .map(|p| {
            let env: Vec<String> = p.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            (
                p.name.clone(),
                serde_json::json!({
                    "pt": p.kind, "pc": p.command, "pw": p.cwd.clone().unwrap_or_default(),
                    "pmo": p.model.clone().unwrap_or_default(), "pe": env.join("\n"),
                }),
            )
        })
        .collect();
    Value::Object(map).to_string().replace("</", r"<\/")
}

/// 관리 결과 한 줄 — 그 탭의 맨 위. 실패는 빨갛게 둔다.
fn note_line(note: Option<&Note>, tab: Tab) -> String {
    match note.filter(|n| n.tab == tab) {
        Some(n) => format!(
            r##"<p class="mg-note {}" role="status">{}</p>"##,
            if n.ok { "ok" } else { "err" },
            escape(&n.text)
        ),
        None => String::new(),
    }
}

/// 실패한 요청의 폼이면 스크립트가 직전 입력을 되살린다(`data-restore`).
fn restore(note: Option<&Note>, tab: Tab) -> &'static str {
    match note {
        Some(n) if n.tab == tab && !n.ok => " data-restore",
        _ => "",
    }
}

/// 폴더 칸 — 입력칸 옆에 폴더 선택 단추. 피커가 고른 값(`picked`)이 있으면 싣고 표시한다.
fn folder_input(id: &str, form: &str, field: PickField, picked: Option<&str>) -> String {
    let picked = match picked {
        Some(v) => format!(r#" value="{v}" data-picked="{v}""#, v = escape(v)),
        None => String::new(),
    };
    format!(
        r##"<div class="with-btn"><input id="{id}" name="{id}" autocomplete="off" class="mono"{picked}><a class="btn" href="#{mark}=1&amp;pf={pf}" data-draft="{form}">{browse}</a></div>"##,
        mark = crate::adapters::driving::url::BROWSE_MARK,
        pf = field.param(),
        browse = escape(&t("sup.form.browse")),
    )
}

/// 종류 칸 — 실행 명령이 종류와 같으면 한 번만, 다르면 둘 다.
pub(super) fn kind_cell(p: &AgentProfile) -> String {
    if p.command.is_empty() || p.command == p.kind {
        escape(&p.kind)
    } else {
        format!(
            r#"<span title="{c}">{} · <span class="mono">{c}</span></span>"#,
            escape(&p.kind),
            c = escape(&p.command)
        )
    }
}

/// 환경 칸 — `KEY=값` 을 한 줄로, 값의 홈 폴더는 `~` 로 줄인다. 전체는 툴팁.
pub(super) fn env_cell(p: &AgentProfile) -> String {
    if p.env.is_empty() {
        return table::nil();
    }
    let full: Vec<String> = p.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let shown: Vec<String> = p
        .env
        .iter()
        .map(|(k, v)| format!("{k}={}", table::tilde(v)))
        .collect();
    format!(
        r#"<span title="{}">{}</span>"#,
        escape(&full.join("\n")),
        escape(&shown.join("  "))
    )
}

/// 에이전트 관리 탭 — 만들기 · 편집 폼과 에이전트 표.
pub fn render_agents(
    profiles: &[AgentProfile],
    note: Option<&Note>,
    picked: Option<&str>,
) -> String {
    let types: String = agents::AGENT_TYPES
        .iter()
        .map(|k| format!(r##"<option value="{k}">{k}</option>"##))
        .collect();
    let rows: String = profiles
        .iter()
        .map(|p| {
            let name = escape(&p.name);
            format!(
                r##"<tr><td title="{name}">{name}</td><td>{kind}</td><td class="mono">{cwd}</td><td class="mono">{model}</td><td class="mono">{env}</td><td class="acts"><button type="button" class="btn mg-edit" data-name="{name}">{edit}</button><a class="btn danger del" href="#pn={name}&amp;{PROFILE_DELETE}=1">{delete}</a></td></tr>"##,
                kind = kind_cell(p),
                cwd = table::path(p.cwd.as_deref()),
                model = p.model.as_deref().map_or_else(table::nil, escape),
                env = env_cell(p),
                edit = escape(&t("sup.manage.edit")),
                delete = escape(&t("sup.manage.delete")),
            )
        })
        .collect();
    let list = if profiles.is_empty() {
        format!(
            r##"<p class="none">{}</p>"##,
            escape(&t("sup.manage.no_profiles"))
        )
    } else {
        let cols = [
            Col::new(&t("sup.manage.name"), "", "140px"),
            Col::new(&t("sup.manage.agent_type"), "", "140px"),
            Col::new(&t("sup.manage.cwd"), "", ""),
            Col::new(&t("sup.manage.model"), "", "120px"),
            Col::new(&t("sup.manage.env"), "", ""),
            Col::raw(String::new(), "acts", "112px"),
        ];
        table::table("agents", &cols, &rows, 720)
    };
    let new_title = escape(&t("sup.manage.create"));
    let create = escape(&t("sup.manage.create_button"));
    format!(
        r##"<div id="manage">{note}
<h2 id="pf-title" data-new="{new_title}" data-edit="{edit_title}">{new_title}</h2>
<form class="mg fgrid" id="pform" method="get" action="#"{restore}>
 <label for="pn">{name}</label><div class="ctl"><input id="pn" name="pn" autocomplete="off" required></div>
 <label for="pt">{kind}</label><div class="ctl"><select id="pt" name="pt">{types}</select></div>
 <label for="pc">{command}</label><div class="ctl"><input id="pc" name="pc" autocomplete="off" class="mono" placeholder="claude"><p class="help">{command_hint}</p></div>
 <label for="pw">{cwd}</label><div class="ctl">{cwd_input}</div>
 <label for="pmo">{model}</label><div class="ctl"><input id="pmo" name="pmo" autocomplete="off" class="mono"></div>
 <label for="pe">{env}</label><div class="ctl"><textarea id="pe" name="pe" class="mono" placeholder="KEY=VALUE"></textarea><p class="help">{env_hint}</p></div>
 <input type="hidden" id="pmode" name="pmode" value="new">
 <input type="hidden" name="{PROFILE_SAVE}" value="1">
 <div class="factions"><button type="button" class="btn tall" id="pcancel" hidden>{cancel}</button><button type="submit" class="primary" id="psave" data-new="{create}" data-edit="{save}">{create}</button></div>
</form>
<h2>{list_title}</h2>
{list}
<script>window.supProfiles={data};</script>
</div>"##,
        note = note_line(note, Tab::Agents),
        edit_title = escape(&t("sup.manage.edit_title")),
        restore = restore(note, Tab::Agents),
        name = escape(&t("sup.manage.name")),
        kind = escape(&t("sup.manage.agent_type")),
        command = escape(&t("sup.manage.command")),
        command_hint = escape(&t("sup.manage.command_hint")),
        cwd = escape(&t("sup.manage.cwd")),
        cwd_input = folder_input("pw", "pform", PickField::AgentCwd, picked),
        model = escape(&t("sup.manage.model")),
        env = escape(&t("sup.manage.env")),
        env_hint = escape(&t("sup.manage.env_hint")),
        cancel = escape(&t("sup.manage.cancel")),
        save = escape(&t("sup.manage.save")),
        list_title = escape(&t("sup.manage.list")),
        data = fill_data(profiles),
    )
}

/// 역할 관리 탭 — 역할 추가 폼 · 하위 폴더 가져오기 · 역할 표.
pub fn render_roles(
    roles: &[Role],
    note: Option<&Note>,
    picked_path: Option<&str>,
    picked_scan: Option<&str>,
) -> String {
    let rows: String = roles
        .iter()
        .map(|r| {
            let guide = if r.guide().is_some() {
                escape(&t("sup.manage.guide_yes"))
            } else {
                format!(
                    r#"<span class="guide-no">{}</span>"#,
                    escape(&t("sup.manage.no_guide"))
                )
            };
            format!(
                r##"<tr><td title="{name}">{name}</td><td class="mono">{path}</td><td>{guide}</td><td class="acts"><a class="btn danger del" href="#rd={name}&amp;{ROLE_DELETE}=1">{delete}</a></td></tr>"##,
                name = escape(&r.name),
                path = table::path(Some(&r.path)),
                delete = escape(&t("sup.manage.delete")),
            )
        })
        .collect();
    let list = if roles.is_empty() {
        format!(
            r##"<p class="none">{}</p>"##,
            escape(&t("sup.manage.no_roles"))
        )
    } else {
        let cols = [
            Col::new(&t("sup.manage.name"), "", "160px"),
            Col::new(&t("sup.manage.role_path"), "", ""),
            Col::new(&t("sup.manage.guide"), "", "136px"),
            Col::raw(String::new(), "acts", "72px"),
        ];
        table::table("roles", &cols, &rows, 520)
    };
    let restore = restore(note, Tab::Roles);
    format!(
        r##"{note}
<h2>{add_title}</h2>
<form class="mg fgrid" id="rform" method="get" action="#"{restore}>
 <label for="rn">{name}</label><div class="ctl"><input id="rn" name="rn" autocomplete="off" required></div>
 <label for="rp">{path}</label><div class="ctl">{path_input}<p class="help">{path_hint}</p></div>
 <input type="hidden" name="{ROLE_ADD}" value="1">
 <div class="factions"><button type="submit" class="primary">{add}</button></div>
</form>
<h2>{scan_title}</h2>
<form class="mg fgrid" id="rscan" method="get" action="#"{restore}>
 <label for="rs">{scan_label}</label><div class="ctl">{scan_input}<p class="help">{scan_hint}</p></div>
 <input type="hidden" name="{ROLE_SCAN}" value="1">
 <div class="factions"><button type="submit" class="primary">{scan}</button></div>
</form>
<h2>{list_title}</h2>
{list}"##,
        note = note_line(note, Tab::Roles),
        add_title = escape(&t("sup.manage.role_new")),
        name = escape(&t("sup.manage.name")),
        path = escape(&t("sup.manage.role_path")),
        path_input = folder_input("rp", "rform", PickField::RolePath, picked_path),
        path_hint = escape(&t("sup.manage.role_path_hint")),
        add = escape(&t("sup.manage.add")),
        scan_title = escape(&t("sup.manage.scan_title")),
        scan_label = escape(&t("sup.manage.scan_label")),
        scan_input = folder_input("rs", "rscan", PickField::RoleScan, picked_scan),
        scan_hint = escape(&t("sup.manage.scan_hint")),
        scan = escape(&t("sup.manage.scan")),
        list_title = escape(&t("sup.manage.roles_list")),
    )
}

/// 관리 폼 스크립트 — 편집 모드 · Esc 취소 · 제출 가로채기 · 입력 보존.
///
/// 폴더 선택과 실패한 제출은 문서를 다시 그리므로 친 값이 사라진다. 그래서 떠나기 직전 폼 값을
/// `sessionStorage` 에 두고, 다시 그린 문서가 피커 값(`data-picked`)을 실었거나 실패를 알리면
/// (`data-restore`) 되살린다. 성공한 제출 뒤에는 버린다.
pub const FORM_SCRIPT: &str = r##"<script>(function(){
var $=function(i){return document.getElementById(i);};
var P=window.supProfiles||{}, pf=$('pform');
function mode(edit){
  var m=$('pmode'), n=$('pn'), tt=$('pf-title'), sv=$('psave'), cc=$('pcancel');
  if(!m||!n)return;
  m.value=edit?'edit':'new';
  n.readOnly=edit;
  if(tt)tt.textContent=tt.getAttribute(edit?'data-edit':'data-new');
  if(sv)sv.textContent=sv.getAttribute(edit?'data-edit':'data-new');
  if(cc)cc.hidden=!edit;
}
function fill(v){for(var k in v){var e=$(k); if(e)e.value=v[k];}}
function cancel(){if(!pf)return; pf.reset(); mode(false);}
document.addEventListener('click',function(e){
  var b=e.target&&e.target.closest&&e.target.closest('.mg-edit'); if(!b||!pf)return;
  e.preventDefault();
  var nm=b.getAttribute('data-name'), p=P[nm]; if(!p)return;
  pf.reset(); $('pn').value=nm; fill(p); mode(true);
  pf.scrollIntoView({block:'nearest'});
  var f=$('pc'); if(f)f.focus();
});
var cc=$('pcancel'); if(cc)cc.addEventListener('click',cancel);
if(pf)pf.addEventListener('keydown',function(e){
  if(e.key==='Escape'&&$('pmode')&&$('pmode').value==='edit'){e.preventDefault();cancel();}
});
function save(f){
  if(!f||!f.id)return;
  try{sessionStorage.setItem('agent-draft:'+f.id,JSON.stringify([].slice.call(new FormData(f).entries())));}catch(e){}
}
document.addEventListener('click',function(e){
  var a=e.target&&e.target.closest&&e.target.closest('[data-draft]'); if(a)save($(a.getAttribute('data-draft')));
});
var fs=document.querySelectorAll('form.mg');
for(var i=0;i<fs.length;i++){
  var f=fs[i], d=null;
  try{d=sessionStorage.getItem('agent-draft:'+f.id);sessionStorage.removeItem('agent-draft:'+f.id);}catch(e){}
  var picked=f.querySelectorAll('[data-picked]');
  if(d&&(f.hasAttribute('data-restore')||picked.length)){
    var kv=[]; try{kv=JSON.parse(d);}catch(e){}
    for(var j=0;j<kv.length;j++){var el=f.elements[kv[j][0]]; if(el&&el.type!=='hidden')el.value=kv[j][1];}
    for(var j2=0;j2<kv.length;j2++)if(f===pf&&kv[j2][0]==='pmode'&&kv[j2][1]==='edit')mode(true);
  }
  for(var k=0;k<picked.length;k++)picked[k].value=picked[k].getAttribute('data-picked');
  f.addEventListener('submit',function(e){
    e.preventDefault();
    save(e.target);
    location.hash=new URLSearchParams(new FormData(e.target)).toString()+'&nonce='+Date.now();
  });
}
})();</script>"##;

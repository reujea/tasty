//! 시작 화면의 공용 표 — 세션 · 에이전트 · 역할 목록이 같은 모양을 쓴다.
//!
//! 머리글 줄이 있고 칸은 한 줄이다(줄바꿈 없이 말줄임). 경로와 id 는 가운데를 줄이고(`.mid`,
//! 스크립트 [`SCRIPT`]) 전체 값은 툴팁에 둔다. 빈 값은 회색 `—` 다. 행이 많으면 표 상자 안에서
//! 세로로 스크롤하고, 폭이 좁으면 가로로 스크롤한다. 조작 칸은 맨 오른쪽 고정 폭이며 행에 마우스를
//! 올리거나 행 안에 포커스가 있을 때 보인다 — 키보드로도 닿게 `visibility` 대신 투명도로 숨긴다.

use super::escape;

/// 열 하나 — 머리글(이미 이스케이프한 HTML) · 칸 클래스 · 폭(`""` 이면 남는 폭을 나눈다).
pub struct Col {
    pub head: String,
    pub class: &'static str,
    pub width: &'static str,
}

impl Col {
    pub fn new(head: &str, class: &'static str, width: &'static str) -> Self {
        Self {
            head: escape(head),
            class,
            width,
        }
    }

    /// 머리글이 HTML 인 열(전체 선택 체크 칸 등).
    pub fn raw(head: String, class: &'static str, width: &'static str) -> Self {
        Self { head, class, width }
    }
}

/// 표를 그린다 — `rows` 는 `<tr>…</tr>` 들이다. `min_width` 보다 좁으면 상자가 가로로 스크롤한다.
pub fn table(id: &str, cols: &[Col], rows: &str, min_width: u32) -> String {
    let colgroup: String = cols
        .iter()
        .map(|c| {
            if c.width.is_empty() {
                "<col>".to_string()
            } else {
                format!(r#"<col style="width:{}">"#, c.width)
            }
        })
        .collect();
    let head: String = cols
        .iter()
        .map(|c| format!(r#"<th class="{}" scope="col">{}</th>"#, c.class, c.head))
        .collect();
    format!(
        r#"<div class="tbl-wrap"><table class="tbl" id="{id}" style="min-width:{min_width}px"><colgroup>{colgroup}</colgroup><thead><tr>{head}</tr></thead><tbody>{rows}</tbody></table></div>"#
    )
}

/// 빈 값 — 회색 `—`.
pub fn nil() -> String {
    r#"<span class="nil">—</span>"#.to_string()
}

/// 가운데를 줄이는 값 — 화면에는 `shown`, 툴팁에는 `full`. 스크립트가 칸 폭에 맞춰 가운데를 줄인다.
pub fn mid(full: &str, shown: &str) -> String {
    format!(
        r#"<span class="mid" title="{}" data-full="{s}">{s}</span>"#,
        escape(full),
        s = escape(shown)
    )
}

/// 경로 칸 — 홈 폴더를 `~` 로 줄이고 가운데 말줄임. 비면 `—`.
pub fn path(p: Option<&str>) -> String {
    match p.filter(|p| !p.is_empty()) {
        Some(p) => mid(p, &tilde(p)),
        None => nil(),
    }
}

/// id 칸 — 전체 값을 싣고 가운데 말줄임. 비면 `—`.
pub fn id(v: Option<&str>) -> String {
    match v.filter(|v| !v.is_empty()) {
        Some(v) => mid(v, v),
        None => nil(),
    }
}

/// 홈 폴더 아래 경로를 `~` 로 줄인다 — 홈을 모르거나 밖이면 그대로.
pub fn tilde(p: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    tilde_with(p, &home)
}

fn tilde_with(p: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.is_empty() {
        return p.to_string();
    }
    match p.strip_prefix(home) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with(['/', '\\']) => format!("~{rest}"),
        _ => p.to_string(),
    }
}

/// 가운데 말줄임 — `.mid` 마다 칸 폭에 들어갈 만큼 앞 4할 · 뒤 6할을 남긴다(경로는 뒤가 가른다).
/// 숨은 탭의 칸은 폭이 0 이라 건너뛴다 — 탭을 열 때 `supFit()` 을 다시 부른다.
pub const SCRIPT: &str = r#"<script>(function(){
function cut(f,k){var a=Math.ceil(k*0.4);return f.slice(0,a)+'\u2026'+f.slice(f.length-(k-a));}
function fit(el){
  var f=el.getAttribute('data-full')||'';
  el.textContent=f;
  if(el.scrollWidth<=el.clientWidth)return;
  var lo=0,hi=f.length-1;
  while(lo<hi){var k=Math.ceil((lo+hi)/2);el.textContent=cut(f,k);if(el.scrollWidth<=el.clientWidth)lo=k;else hi=k-1;}
  el.textContent=cut(f,lo);
}
function all(){var m=document.querySelectorAll('.mid');for(var i=0;i<m.length;i++)if(m[i].offsetParent!==null)fit(m[i]);}
window.supFit=all;
var tm=null;
window.addEventListener('resize',function(){clearTimeout(tm);tm=setTimeout(all,80);});
all();
})();</script>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_folder_becomes_a_tilde() {
        assert_eq!(tilde_with("/Users/me/dev/p", "/Users/me"), "~/dev/p");
        assert_eq!(tilde_with("/Users/me", "/Users/me/"), "~");
        assert_eq!(
            tilde_with("/Users/meo/p", "/Users/me"),
            "/Users/meo/p",
            "이름이 홈으로 시작할 뿐인 폴더를 줄였다"
        );
        assert_eq!(tilde_with("/w/p", ""), "/w/p");
        assert_eq!(tilde_with(r"C:\Users\me\p", r"C:\Users\me"), r"~\p");
    }

    #[test]
    fn a_table_has_a_header_row_and_one_col_per_column() {
        let h = table(
            "t",
            &[Col::new("이름", "", "120px"), Col::new("폴더", "", "")],
            "<tr><td>a</td><td>b</td></tr>",
            400,
        );
        assert!(h.contains(r#"<thead><tr><th class="" scope="col">이름</th>"#));
        assert_eq!(
            h.matches("<col").count() - h.matches("<colgroup").count(),
            2
        );
        assert!(h.contains(r#"<col style="width:120px">"#));
        assert!(h.starts_with(r#"<div class="tbl-wrap">"#));
    }

    #[test]
    fn empty_values_are_a_grey_dash_and_full_values_go_to_the_tooltip() {
        assert_eq!(path(None), nil());
        assert_eq!(id(Some("")), nil());
        let c = mid("/a/<b>", "~/<b>");
        assert!(c.contains(r#"title="/a/&lt;b&gt;""#), "{c}");
        assert!(c.contains(r#">~/&lt;b&gt;</span>"#), "{c}");
    }
}

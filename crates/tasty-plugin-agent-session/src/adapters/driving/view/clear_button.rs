//! 비우기 단추 — 입력 폼의 단추 · 결과 줄 자리 · 두 번 눌러야 보내는 스크립트. 누른 것을 읽는 쪽은
//! `url::parse_clear_click`, 비우는 일은 `application::clear` 다.

use super::escape;

/// 비우기 단추 — 입력 폼의 중지 단추 옆에 둔다.
pub fn button_html() -> String {
    format!(
        r#"<button type="button" id="clear" class="clear" title="{}" data-confirm="{}">{}</button>"#,
        escape(&crate::domain::i18n::t("sup.chat.clear_hint")),
        escape(&crate::domain::i18n::t("sup.chat.clear_confirm")),
        escape(&crate::domain::i18n::t("sup.chat.clear_button")),
    )
}

/// 결과 한 줄 자리 — 입력 폼 바로 위. 거절 · 실패 사유를 보인다.
pub fn note_html() -> String {
    r#"<div id="clearnote" hidden></div>"#.to_string()
}

/// 단추 모양과 동작 — 문서 끝에 한 번 싣는다.
///
/// 첫 누름은 단추 글자를 확인 문구로 바꾸기만 하고, 3초 안에 다시 눌러야 보낸다 — 되돌릴
/// 수 없는 일이라 한 번의 오조작으로 일어나지 않게 한다. 턴이 도는 동안은 CSS 가 단추를
/// 숨긴다(중지 단추와 같은 두 신호).
pub fn script_html() -> String {
    format!(
        r#"<style>
 button.clear{{color:#8b93a1}}
 button.clear.armed{{color:#d8a33c}}
 body:has(.turn.pending) button.clear,body:has(#act:not([hidden])) button.clear{{display:none}}
 #clearnote{{padding:0 16px 8px;color:#e06c75}}
 #clearnote[hidden]{{display:none}}
</style>
<script>
(function(){{
var b=document.getElementById('clear'), t=document.getElementById('t');
if(!b)return;
var label=b.textContent, timer=null, cn=0;
function disarm(){{ b.classList.remove('armed'); b.textContent=label; timer=null; }}
b.addEventListener('click',function(e){{
  e.preventDefault();
  if(!b.classList.contains('armed')){{
    b.classList.add('armed'); b.textContent=b.getAttribute('data-confirm');
    timer=setTimeout(disarm,3000);
    return;
  }}
  if(timer)clearTimeout(timer);
  disarm();
  cn=cn+1; location.hash='{MARK}='+cn;
  if(t)t.focus();
}});
}})();
</script>"#,
        MARK = crate::adapters::driving::url::CLEAR_MARK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_needs_a_second_press() {
        let s = script_html();
        assert!(s.contains("classList.contains('armed')"));
        assert!(s.contains("location.hash='agent-clear='+cn"));
        // 턴이 도는 동안은 숨는다 — 중지 단추가 보이는 바로 그 신호로.
        assert!(s.contains("body:has(.turn.pending) button.clear"));
    }
}

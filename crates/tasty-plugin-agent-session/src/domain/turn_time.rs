//! 대화 턴의 시각 표시 — 날짜가 바뀌는 자리에 `[YYYY-MM-DD]` 구분선을 넣고, 그 아래 턴마다 보낸 말 앞에
//! `[HH:MM:ss]` 를 붙인다.
//!
//! # 누가 무엇을 만드나
//!
//! - 서버(이 plugin)는 턴이 열린 시각(`Turn::at`, 유닉스 초)을 페이지 상태(`client::log_state`)에 싣기만 한다.
//! - 날짜·시각 글자는 페이지 앱(`assets/app.js`)이 **webview 의 지역 시간대**로 그린다 — plugin 은 시간대
//!   데이터를 갖지 않는다(`budget::epoch` 는 UTC 초까지만 푼다).
//! - 앱이 붙인 턴(보낸 직후의 낙관적 턴 · 에이전트가 이어 간 턴)은 붙는 순간의 시각을 쓴다. 다시 실으면 서버가
//!   같은 턴에 자기 시각을 싣는다(같은 기계라 차이는 초 단위).
//! - 시각을 모르는 턴(옛 스냅샷)은 시각도 날짜 줄도 안 붙는다 — 지금 시각을 붙이면 지난 대화가 오늘 한 말처럼
//!   보인다.
//!
//! 여기에는 그 표시의 모양(CSS)만 있다.

pub fn style_html() -> &'static str {
    r#"<style>
 .day{display:flex;align-items:center;gap:8px;color:#5a6070;margin:4px 0 10px}
 .day::before,.day::after{content:"";flex:1;border-top:1px solid #3a3f4b}
 .ts{color:#5a6070}
 .sent:has(> .ts)::before{content:none}
 .sent > .ts::after{content:"> ";color:#5ac17d}
 .sent.auto > .ts::after{content:"\21BB  ";color:#5a6070}
</style>"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_draws_day_lines_and_the_time_prefix_from_the_state() {
        let app = include_str!("../../assets/app.js");
        assert!(app.contains(r#"<div class="day""#), "날짜 줄을 안 만든다");
        assert!(
            app.contains("getFullYear() + '-'"),
            "날짜가 YYYY-MM-DD 꼴이 아니다"
        );
        assert!(
            app.contains(">[${d}]</div>"),
            "날짜 줄이 [YYYY-MM-DD] 꼴이 아니다"
        );
        assert!(
            app.contains("'[' + p2(d.getHours()) + ':'"),
            "시각 칸이 [HH:MM:ss] 꼴이 아니다"
        );
        assert!(
            style_html().contains(".day::before,.day::after{content:\"\";flex:1;border-top"),
            "날짜 줄 양쪽에 선이 없다"
        );
    }
}

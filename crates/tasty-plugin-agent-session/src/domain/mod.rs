//! 도메인 — 대화 상태와 그 전이 · 판정 규칙. 호스트 · 데몬 · 파일을 직접 부르지 않는다.

pub(crate) mod commands;
pub(crate) mod convo;
pub(crate) mod i18n;
pub(crate) mod models;
pub(crate) mod overdue;
pub(crate) mod own_turns;
pub(crate) mod rate_block;
pub(crate) mod retry_429;
pub(crate) mod send_queue;
pub(crate) mod sync_rules;
pub(crate) mod sys_note;
pub(crate) mod turn_time;
pub(crate) mod usage;

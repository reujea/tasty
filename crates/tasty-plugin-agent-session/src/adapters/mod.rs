//! 어댑터 — 바깥에서 들어오는 쪽(`driving`: IPC · navigation · 화면)과 이 plugin 이 부르는 쪽(`driven`: 데몬 · 파일 · 프로세스).

pub(crate) mod driven;
pub(crate) mod driving;

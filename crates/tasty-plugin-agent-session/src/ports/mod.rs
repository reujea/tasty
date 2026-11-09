//! 포트 — 로직이 바깥(데몬 · 호스트)에 닿는 trait. 실물은 `adapters` 와 조립 루트(`main.rs`)에 있다.

pub(crate) mod host;
pub(crate) mod supervisor;

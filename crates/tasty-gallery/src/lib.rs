#![forbid(unsafe_code)]

//! `tasty-gallery` — Tasty UI 컴포넌트 단독 시각 검증 도구.
//!
//! Storybook 류 별도 바이너리. 메인 앱(`tasty`) IPC/CLI 표면에 어떤 영향도
//! 주지 않으며, 본체와 동일한 lib crate (`tasty-egui-theme` 등) 의 함수를
//! 직접 호출해 "데모 = 메인 = 같은 코드" 를 보장한다.
//!
//! 카탈로그(`catalog/`)는 Theme 만 의존하는 foundation 항목(color swatches,
//! typography, spacing 등)부터 popup / sidebar / tab_bar 계열까지 본체 UI
//! 컴포넌트를 폭넓게 포괄한다 — cut 금지, gallery-first(ADR-0510).

pub mod catalog;
pub mod fonts;
pub mod host_shell;

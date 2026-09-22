//! Vendored DTCG 디자인 토큰 + 코드 생성.
//!
//! 디자인 시스템(claude design 산출물)의 W3C DTCG export 를 `dtcg/tasty.tokens.json`
//! 으로 vendor 하고(3-tier: primitive → semantic → component, 총 832 토큰),
//! `src/bin/generate.rs` 가 치수 계열($type: dimension/duration/number/fontWeight)을
//! `src/generated/` 의 Rust const 로 생성한다. **생성물은 커밋**되며, freshness
//! 테스트(`tests/freshness.rs`)가 vendor json ↔ 생성물 텍스트 일치를 강제한다 — 통합
//! 테스트라 **자동 실행은 헤드리스 잡뿐**이다(`check-headless` 가 전체 스위트를 돌린다).
//! 기본 조합 잡은 `--lib --bins` 라 못 보고, 자동 잡은 push 된 커밋만 본다
//! (`docs/dev-guide/ci-gates.md`). 토큰을 다시 vendor 했으면 커밋 전에 직접 돌려라.
//!
//! # Tier 규율 (컴파일 타임 강제)
//!
//! 디자인 계약("primitives are referenced only by the semantic layer")에 따라
//! `generated::primitive` 는 **`pub(crate)`** 다 — 외부 crate 는 `semantic`/
//! `component` 만 읽을 수 있고, primitive 직접 참조는 컴파일 에러가 된다.
//!
//! # zoom 우회 금지 (필수)
//!
//! 생성된 raw const 를 위젯/뷰가 직접 소비하면 `Theme::with_colors_and_zoom()` 의
//! host UI zoom 적용·반올림과 zoom 제외 정책을 우회한다. 제외 갈래는 hairline / 탭바 /
//! 상태바 / CSD 타이틀바 / 렌더 콘텐츠 폰트이고, **정본 집합은 `tasty-type-appearance`
//! 의 zoom 면제 가드가 이름 단위로 든다** — 여기 적힌 갈래 이름은 요약이다.
//! **generated const 의 역할은 `SIZING` 초기값 공급과 정합 테스트 까지다 — 런타임
//! 소비는 반드시 `&Theme` 필드/접근자를 경유한다.**
//!
//! # 색 토큰
//!
//! 색의 SSoT 는 런타임 테마 시스템(`tasty-themes` 의 `theme_base` ▷
//! `theme_overrides`)이다. 여기서는 색 const 를 생성하지 않고(시리즈 04/05),
//! 드리프트 테스트(`tests/color_drift.rs`)로 DTCG ↔ 임베드 테마 값 일치만 고정한다.
//!
//! # vendor 갱신
//!
//! 절차는 crate `README.md` 참조 — 디자인 폴더 위치는 매번 바뀌므로 사용자에게
//! 물어서 복사한다 (경로를 코드/문서에 박지 않는다).

pub mod dtcg;
pub mod generated;

/// vendor 된 DTCG 토큰 파일 원문. 파서/생성기/테스트가 공유하는 단일 입력.
pub const DTCG_JSON: &str = include_str!("../dtcg/tasty.tokens.json");

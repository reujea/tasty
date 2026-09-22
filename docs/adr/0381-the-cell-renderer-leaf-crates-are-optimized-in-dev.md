# ADR-0381: 셀 렌더러의 잎 크레이트 셋은 dev 에서도 최적화한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: build, dev-profile, opt-level, renderer, selection, terminal-link, cell-width, measurement, adr-0342
- **Group**: architecture

## Context

루트 `Cargo.toml` 의 dev 프로필은 본체를 `opt-level = 0` 으로 두고 의존성을 `"*"` glob 으로
`opt-level = 3` 에 둔다. 워크스페이스 멤버는 glob 에 안 걸리므로 `[profile.dev.package.<이름>]`
으로 **하나씩 등재**돼야 opt 3 을 받는다.

셀 렌더러가 부르던 선택·링크·폭 계산이 잎 크레이트 셋으로 올라갔다 — `tasty-selection` ·
`tasty-terminal-link` · `tasty-cell-width`([ADR-0342](0342-the-cell-renderer-names-the-crates-not-the-host-re-exports.md)).
셋 다 등재되지 않아 dev 에서 opt 0 이었다. 추출 전에도 이 코드는 본체(opt 0) 안에 있었으므로
추출이 느리게 만든 것은 아니다. 다만 렌더러 티켓은 "새 경계의 opt-level 은 편집/실행 비용을 함께
보고 결정하며 일괄 opt-level=3 을 강제하지 않는다" 를 요구했고, 그 결정과 근거가 어디에도 없었다.

세 크레이트의 함수 중 렌더러가 **셀마다** 부르는 것은 `unicode_width`(렌더러 안 여러 자리) ·
`is_selected`(선택이 있을 때) · `LinkHighlight::covers`(링크 위에 포인터가 있을 때)다. 링크 검출
(`link_at` → `detect_*_line`)은 포인터가 움직일 때마다 한 줄씩 돈다.

## Decision

**셋 다 등재한다.** 크레이트마다 실행 이득과 편집 비용을 따로 재고, 셋 모두 실행 이득이 편집
비용을 넘는다고 판단했다. 값은 `Cargo.toml` 의 등재 줄 위 주석에 적는다.

실측 2026-09-21, 이 머신(20 코어 · 부하 평균 약 58 — 다른 lane 이 동시에 빌드 중).

| 크레이트 | 실행: opt 0 → opt 3 (240×70 한 화면, 두 빌드 번갈아 20 회 · 중앙값) | 편집: 크레이트 하나 재컴파일 opt 0 → opt 3 (3 회 중앙값) |
|---|---|---|
| `tasty-cell-width` | 폭 판정(셀마다 2 회) 2.12 ms → 0.24 ms / 프레임 (8.8×) | 0.24 s → 0.33 s |
| `tasty-terminal-link` | `covers` 0.41 ms → 0.18 ms / 프레임 · 한 줄 검출 84 µs → 5 µs / 포인터 이동 | 0.60 s → 1.97 s |
| `tasty-selection` | `is_selected` 0.23 ms → 0.17 ms / 프레임 | 0.51 s → 1.07 s |

판단: 폭 판정은 화면 전체를 다시 그리는 프레임에서 16.7 ms 의 약 11% 를 되돌려 받는다 —
망설일 자리가 아니다. 링크와 선택은 프레임 이득이 작지만, 편집 비용이 초 단위이고 그 크레이트를 고치면 **본체가 어차피 다시 링크된다**
(수십 초). 편집 비용 증가분이 그 링크 시간에 묻힌다.

## Consequences

- **얻은 것**: dev 빌드의 전체 화면 redraw 가 폭 판정에서 프레임당 약 1.9 ms 싸진다. 링크 hover 검출이
  포인터 이동마다 약 80 µs 싸진다.
- **잃은 것**: 세 크레이트를 고쳤을 때의 재컴파일이 0.1~1.4 s 길어진다. 첫 빌드도 그만큼 길어진다.
- **운영 비용 / 유지 부담**: 새 워크스페이스 크레이트를 만들 때마다 등재 여부를 다시 정해야 한다 —
  glob 이 멤버를 안 덮는 한 그대로다.

## Alternatives Considered

- **등재하지 않고 주석으로 사유만 적기** — 폭 판정의 프레임 비용이 너무 크다. 기각.
- **`tasty-cell-width` 만 등재** — 나머지 둘의 편집 비용 증가분은 본체 재링크 시간에 묻혀 실익이
  작고, "렌더러 hot path 크레이트는 opt 3" 이라는 한 줄 규칙이 깨진다. 기각.
- **셀 폭 함수에 `#[inline]` 을 달아 호출자에서 펼치기** — 호출자(본체)가 opt 0 이라 펼쳐도
  최적화되지 않는다. 기각.

## Reconsideration Triggers

**채널이 붙는 것**

- 세 크레이트 중 하나가 없어지거나 이름이 바뀌어 `Cargo.toml` 의 등재 줄이 가리키는 패키지가 없어지면
  (없는 패키지를 가리키는 프로필 오버라이드는 빌드를 안 깬다 — `--config` 로 준 것은 경고도
  없이 통과했다, 실측 2026-09-21. 그래서 등재 줄이 죽어도 아무도 모른다).

**원리적으로 안 붙는 것**

- 이 크레이트들을 자주 고치는 작업이 생겨 편집 비용이 체감되면. 재는 법: 아래 측정 절차의 편집 쪽.
- 렌더러가 셀마다 부르는 함수가 바뀌면(예: 폭 판정이 캐시된 값을 읽게 되면). 재는 법: 아래 측정
  절차의 실행 쪽을 새 호출 형태로.

## 측정 절차 — 다시 재는 법

레포 밖 임시 디렉토리에 bin 크레이트 하나를 만든다. 세 크레이트와 `tasty-type-appearance`
(`default-features = false`) · `termwiz` 를 path/버전 의존으로 걸고, 프로필은 레포와 같게
`[profile.dev] opt-level = 0` · `[profile.dev.package."*"] opt-level = 3` 으로 둔다. `Cargo.lock` 은
레포 것을 복사한다.

- 본문: 240×70 격자(ASCII 80% · 한글 20%)를 만들고, 한 프레임을 흉내 내 셀마다 `unicode_width` 2 회,
  `is_selected`(화면 전체를 덮는 Normal 선택), `covers`(한 줄 짜리 링크)를 부른다. 링크 검출은 URL
  하나와 경로 하나가 든 한 줄로 `detect_scrollback_line(.., mirror = true)` 를 부른다. 각 작업을 7 회
  돌려 최솟값을 ns 로 찍는다. 입력은 `std::hint::black_box` 로 감싼다.
- 빌드 둘: `CARGO_TARGET_DIR` 를 둘로 나누고, 한쪽만 `--config profile.dev.package.<크레이트>.opt-level=0`
  을 세 크레이트에 준다. `cargo build -v` 의 `-C opt-level` 로 실제로 먹었는지 확인한다.
- 실행: 두 바이너리를 **번갈아**(홀수 회 A→B, 짝수 회 B→A) 20 회씩 돌리고 작업별 중앙값을 견준다.
  공유 머신에서는 부하가 시간에 따라 움직이므로 한쪽을 몰아서 돌리면 부하 궤적이 그대로 편향이 된다.
- 편집 쪽: 크레이트의 `lib.rs` 를 `touch` 하고 `cargo build -p <크레이트>` 를 두 설정으로 번갈아 3 회씩 잰다.

## References

- 루트 `Cargo.toml` — `[profile.dev.package.tasty-cell-width]` 등 세 등재와 그 위 주석(결정이 실현된 현재 위치)
- [ADR-0342](0342-the-cell-renderer-names-the-crates-not-the-host-re-exports.md) — 세 크레이트를 렌더러가 직접 부르는 결정
- `docs/dev-guide/build.md` — 빌드 프로필

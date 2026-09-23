# ADR-0074: Explorer root 는 항상 절대경로 — 상대 경로는 채택하지 않고 홈으로 폴백한다

- **Status**: Accepted
- **Date**: 2026-08-18
- **Tags**: explorer, surface-cwd, invariant, fallback, attach, path
- **Group**: bundled-plugins

## Context

Explorer surface 의 root 는 `params["path"]` → `SurfaceKindDef::create` 의 carry cwd 순으로 결정되고, 둘 다 없으면 폴백이 걸린다. 그 폴백값이 문자 그대로 `"."` 였다(생성 · snapshot 복원 · 빈 탭 목록 복원 세 곳 모두).

상대 root 는 그대로 밖으로 흘러나간다 — 주소창에 `.` 이 표시되고, 항목 경로가 `./<name>` 이 되어 경로 복사 결과가 상대경로가 되며, attach mirror 에서는 `list_dir` 요청의 `dir` 이 `"."` 로 나가 **원격 프로세스의 cwd** 가 나열된다. 나열 대상 자체도 사용자가 의도한 적 없는 디렉토리(앱을 띄운 셸의 위치)가 된다.

이는 [surface cwd 불변식](../design/policies/cwd.md#surface-cwd-invariant) 의 취지("호스트 시작 cwd 가 root 행세 못 하게 — `std::env::current_dir()` 폴백 제거")에 정면으로 어긋난다. `"."` 를 root 로 두는 것은 `current_dir()` 을 **지연 평가**하는 것과 동작상 같으면서, 그 문자열이 UI·wire·클립보드로 새어나가므로 오히려 더 나쁘다.

동시에 그 불변식 문서의 해당 절은 존재하지 않는 `tasty-plugin-explorer` 크레이트를 전제로 서술돼 있었다 — explorer 는 본체 builtin surface 로 승격된 지 오래다. 즉 문서에는 `$HOME` 단계가 있는데 구현에는 없었고, 문서가 가리키는 코드 위치도 죽어 있었다.

cwd 를 상속하지 않는 생성 경로(비-terminal split, `workspace.create`, mirror convert)는 각자 별도의 결함이지만, 그 조합이 무엇이든 **root 만은 절대경로로 확정**되는 방어선이 필요하다.

## Decision

**Explorer root 는 어떤 생성·복원 경로에서도 절대경로다.** 결정 순서는 `params["path"]` → carry cwd → `$HOME`/`%USERPROFILE%` → (홈 조회 실패 시) 절대경로로 확정한 프로세스 cwd → 파일시스템 루트다.

앞 두 단계의 값이 **상대경로면 채택하지 않고** 홈 단계로 내려간다. snapshot 복원도 같은 규칙을 쓰므로, 과거 폴백이 `"."` 로 저장해 둔 기존 `layout.json` 은 재시작 시 홈으로 교정된다. 정책의 단일 진실원천은 `tasty_model::explorer_panel::{default_root, resolve_root}` 이며, 생성 · snapshot 복원 · 빈 탭 목록 복원 세 경계가 모두 이를 호출한다.

## Consequences

- **얻은 것**: root 가 UI(주소창·표시명)·클립보드(경로 복사)·attach `list_dir` wire 중 어디로 나가든 항상 절대경로다. 호스트 프로세스 cwd 가 root 로 승격되는 경로가 사라졌다. 기존에 오염된 `layout.json` 이 별도 마이그레이션 코드 없이 복원 시점에 자동 교정된다. 폴백 정책이 한 곳에 모여 세 경계가 갈라질 수 없다.
- **잃은 것**: 상대 `--path`(예 `--path .`)를 넘기면 그 값이 조용히 무시되고 홈이 열린다. CLI 프로세스의 cwd 는 호스트가 알 수 없으므로 어차피 사용자가 의도한 디렉토리로 해석될 수 없었지만, "무시" 라는 사실 자체는 사용자에게 보이지 않는다.
- **운영 비용 / 유지 부담**: 없음에 가깝다 — 순수 함수 2 개 + 회귀 테스트. `tasty-model` 이 `directories` 에 직접 의존하게 됐다(이미 `tasty-utils` 경유 간접 의존이었다).

## Alternatives Considered

- **상대 경로를 프로세스 cwd 기준으로 절대화**: 이 불변식이 금지한 "호스트 시작 cwd 가 root 행세" 를 그대로 되살린다. 지연 평가를 즉시 평가로 바꿀 뿐 결과는 같고, 오히려 그 값이 절대경로 얼굴로 저장·전파돼 추적이 어려워진다.
- **상대 경로를 오류로 거부(생성 실패)**: 이미 `"."` 로 오염된 `layout.json` 이 존재하므로, 거부는 복원 시 explorer 탭이 사라지는 결과가 된다. 사용자 데이터 손실 대비 이득이 없다.
- **폴백값만 `"."` → 홈으로 바꾸고 명시 상대값은 그대로 수용**: 저장된 `"."` 스냅샷이 재시작 후에도 그대로 살아나 문제의 절반이 남는다.
- **CLI `--path` 를 `--cwd` 처럼 `normalize_cwd_arg` 로 정규화**: CLI 프로세스 cwd 기준 절대화라 의미론이 맞고 UX 도 낫다. 다만 이는 호스트 측 방어선과 **직교**하는 별도 개선이며(다른 클라이언트·IPC 직접 호출은 여전히 호스트 규칙에 걸린다), 본 ADR 은 호스트 경계의 불변식만 확정한다.

## Reconsideration Triggers

- CLI/IPC 입구에서 상대 `path` 를 호출자 cwd 기준으로 정규화하게 되면, 호스트가 상대값을 "무시" 하는 것과 "정규화된 값을 받는 것" 의 역할 분담을 다시 정리한다.
- 상대 root 를 사용자가 의도적으로 쓰는 UX(예: 프로젝트 상대 즐겨찾기)가 생기면, 그 상대성의 기준점을 명시적으로 갖는 표현으로 재설계한다.
- 홈 조회 실패 환경(HOME 없는 컨테이너 등)이 실제 배포 대상이 되면, 최후 수단인 "프로세스 cwd" 대신 더 나은 앵커(예: 명시 설정값)를 도입할지 재검토한다.

## References

- [`docs/design/policies/cwd.md`](../design/policies/cwd.md#5-explorer-root-fallback-host-builtin) §5 — 현행 규칙 본문
- [`docs/features/explorer/index.md`](../features/explorer/index.md) — root 결정 규칙
- `src/core/surface_registry/builtins.rs` (`register_explorer`, `explorer_tab_from_json`) · `crates/tasty-model/src/explorer_panel.rs` (`default_root`, `resolve_root`)

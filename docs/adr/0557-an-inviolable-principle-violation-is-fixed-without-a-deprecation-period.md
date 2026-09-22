# ADR-0557: 불가침 원칙 위반은 deprecation 유예 없이 고친다 — 위반을 이루는 부분만, 가장 적게 깨는 형태로

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: api-stability, deprecation, compatibility, changelog, identity, identity-principle-1, identity-principle-3, adr-0113, adr-0115, adr-0122, adr-0497, adr-0502, adr-0504, adr-0526, adr-0532, adr-0533
- **Group**: ipc-contract

## Context

[`dev-guide/api-conventions.md`](../dev-guide/api-conventions.md) 「안정성 정책」 은 0.x 의 break 를
"한 minor 이상 deprecation 우선" 으로 두고, 유예를 건너뛰어도 되는 경우를 **두 자리에** 적었다.
버전 단계 표의 0.x 행은 "보안 예외" 하나를, 「Deprecation 절차」 끝 문장은 "보안·심각 버그" 둘을
적었다 — 같은 물음(무엇이 유예를 건너뛰는가)에 답하는 두 자리가 이미 서로 다른 집합을 적고 있었다.
어느 쪽에도 **불가침 원칙 위반**([`identity.md`](../identity.md) §2)은 없다.

그런데 원칙 위반을 고친 외부 동작 변경은 전부 유예 없이 나갔다. 실측(2026-09-23, `main`
`705da30b4`). 술어: `docs/adr/` 에서 원칙 1·3 을 근거로 인용하면서 외부 동작(응답 · 기록 값 ·
표면 존재)을 바꾼 결정을 고르고, `CHANGELOG.md` 의 대응 항목을 대조했다.

모수는 "원칙을 인용한 ADR 전부" 가 아니라 **아래 패턴에 걸린 ADR** 이다. `docs/adr/` 에서
`git grep -lE '원칙 ?[123]|원칙 [0-9]·[0-9]|포커스 독립성|불가침|identity-principle-[13]'`
(index · template · 이 ADR 제외)에 걸린 74 편과, `CHANGELOG.md` 가 인용하는
66 편(`grep -oE 'ADR-[0-9]{4}' CHANGELOG.md | sort -u`)의 교집합 **22 편**을 한 편씩 읽었다. 여기에 CHANGELOG 항목이 없는 ADR-0143 을 더했다.
이 패턴은 원칙을 `[identity §2.1 ①](../identity.md)` 같은 링크나 `user-agent-separation` 태그로만
인용한 ADR 을 놓친다. 그래서 `identity\.md|user-agent-separation` 을 더해 넓혀 보았고, CHANGELOG 가
인용하는 것 중 22 편 밖이 4 편 더 나왔다(0113 · 0512 · 0514 · 0531). 그중 ADR-0113 만 술어에 든다.
0512 · 0514 는 동작을 더하기만 했고(0514 는 "생략 시 동작은 이 결정 전과 같다"), 0531 은 옛 동작에
기대던 호출자가 없다. CHANGELOG 가 인용하지 않는 원칙 인용 ADR 은 두 패턴 어느 쪽이든 읽지 않았다.

| 결정 | 원칙 | CHANGELOG | 유예 | 인용한 예외 |
|---|---|---|---|---|
| [ADR-0113](0113-close-preserves-the-focused-target.md) 에이전트의 `surface.close` 뒤 `active_workspace` · `active_tab` 이 가리키는 대상 | 1 ① | 항목 있음(`### Fixed`), `(BREAK)` 표기 없음 — 항목이 스스로 "불가침 원칙 1 위반" 이라 적었다 | 없음 | 없음 |
| [ADR-0115](0115-input-reproduction-ipc-debug-isolation.md) `surface.raw_key` · `switch_input_source` | 1 ② | `(BREAK)` | 없음 | 보안 |
| 같은 ADR `surface.ime_*` | 1 ② · 3 | `(BREAK)` | 없음 | "같은 이유로"(앞 항목을 잇는다) |
| [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md) `window.create` 결과 동기화 | 1 — 위반은 에이전트 발 실패를 사용자 toast 로 낸 것이고, `(BREAK)` 인 응답 형태는 그 toast 를 대신해 실패를 요청자에게 돌려줄 자리다 | `(BREAK)` | 없음 | 없음 |
| [ADR-0143](0143-a-named-target-is-checked-before-the-engine-in-headless.md) 헤드리스의 지목 대상 검사 | 3 | 항목 없음 | 없음 | 없음 |
| [ADR-0497](0497-an-agent-created-window-does-not-take-the-users-focus.md) 에이전트가 만든 창의 포커스 | 1 · 3 | 항목 있음, `(BREAK)` 표기 없음 | 없음 | 없음 |
| [ADR-0502](0502-an-agent-created-tab-does-not-take-the-users-tab.md) 에이전트가 만든 탭의 선택 · 응답 `active_tab` | 1 · 3 | 항목 있음, `(BREAK)` 표기 없음 | 없음 | 없음 |
| [ADR-0504](0504-the-file-picker-trigger-answers-only-a-plugin-caller.md) CLI · 에이전트의 `file_picker.trigger` 가 popup 대신 `-32016` | 1 ① · ② · 3 | `(BREAK)` | 없음 — ADR 이 "한 minor deprecation 을 거친다" 를 원칙 2.1 ① · 2.3 위반이 남는다는 이유로 기각했다 | 없음 — 사유만 적었다("불가침 원칙의 위반이라 deprecation 기간 없이 바꿨다") |
| [ADR-0526](0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md) origin 없는 `file_handler.dispatch` 의 탭 선택 | 1 · 3 | 항목 있음, `(BREAK)` 표기 없음 | 없음 | 없음 |
| [ADR-0532](0532-workspace-create-inherits-cwd-from-the-surface-that-names-its-window.md) `workspace.create` cwd 상속 | 3 | `(BREAK)` | 없음 | 없음 — 사유만 적었다("유예를 두면 그 기간 동안 위반이 그대로 남는다") |
| [ADR-0533](0533-an-omitted-target-keeps-its-focus-default-only-where-nothing-names-one.md) `approval.request` 의 기록 귀속 | 3 | 항목 있음, `(BREAK)` 표기 없음 | 없음 | 없음 |

- 원칙 위반을 고친 변경 11 건(ADR 10 편) 중 유예를 거친 것 **0 건**, 문서의 예외 이름을 댄 것 **1 건**
  (ADR-0115 의 보안 — 원칙 위반이 아니라 보안 표면이라는 사실을 댄 것이다).
- 같은 기간 `CHANGELOG.md` 의 `(BREAK)` 항목은 12 건이고(`grep -c '^- (BREAK)' CHANGELOG.md` = 12 —
  범례 줄은 `- (BREAK)` 로 시작하지 않아 빠진다) `### Deprecated` 절은 **한 번도 없다**
  (`grep -c '^### Deprecated' CHANGELOG.md` = 0).
- ADR-0504 는 유예를 두는 안을 대안으로 적고 원칙 위반을 이유로 기각했다 — 이 결정을 가장 곧게
  앞서간 선례다.
- ADR-0532 는 Consequences 에 "원칙 3 이 호환보다 앞선다" 고 적었다. 즉 이 판단은 개별 결정 안에서는
  이미 내려졌고, 그것을 받는 규칙 문장만 없었다. 독자는 그 `(BREAK)` 가 "심각 버그" 에 해당한다고
  **추론**해야 했다.

반대 방향의 선례도 있다. 원칙 위반을 고칠 때 **형태**는 호환을 따라 골랐다.

- ADR-0533 은 대상 생략을 거절하는 안(원칙 3 에 가장 가깝다)을 "지금 `workspace_id` 없이 부르는 모든
  호출이 깨진다" 는 이유로 기각하고, 호환이 가장 많이 남는 쪽을 골랐다.
- [ADR-0480](0480-a-forwarded-close-carries-who-asked-for-it.md) 은 wire 칸이 없을 때 "원칙 1 쪽으로 더
  안전한" 해석 대신 옛 클라이언트의 동작을 보존하는 해석을 골랐다.

그러니 관행은 "원칙 위반은 호환을 무시한다" 가 아니다. **유예 기간은 건너뛰되, 고치는 형태는 기존
호출자를 가장 적게 깨는 쪽**이다.

## Decision

**불가침 원칙 위반을 고치는 변경은 deprecation 유예를 건너뛸 수 있는 세 번째 사유다** — 보안 ·
심각 버그와 나란히, 「Deprecation 절차」 의 예외 목록에 이름으로 적는다. 조건 셋이 붙는다.

1. **유예를 건너뛰는 범위는 위반을 이루는 부분뿐이다.** 같은 변경에 끼어 가는 위반과 무관한 break 는
   정상 절차(한 minor 이상 유예)를 따른다.
2. **고치는 형태가 여럿이면 기존 호출자를 가장 적게 깨는 쪽을 고른다**(ADR-0533 · ADR-0480 의 선택
   기준). 예외는 유예를 건너뛸 근거이지 호환을 따지지 않아도 된다는 근거가 아니다.
3. **CHANGELOG 의 `(BREAK)` 항목에 어느 원칙을 어겼는지와, 유예를 건너뛴 사유가 이 예외라는 것을
   적는다.** 독자가 사유를 추론하지 않게 한다.

예외 목록은 **「Deprecation 절차」 한 자리에만** 열거한다. 버전 단계 표의 0.x 행은 목록을 다시 적지
않고 그 절을 가리킨다.

## Consequences

- **얻은 것**: 원칙 위반 수정의 `(BREAK)` 가 문서에 있는 사유를 댄다. "심각 버그" 로 읽을지를 사람마다
  다르게 판단하지 않는다 — 판정의 좌변이 identity.md 의 원칙 조항이라 리뷰가 그 조항을 짚으면 된다.
- **얻은 것**: 두 자리가 서로 다른 집합을 적던 상태가 끝난다. 열거가 하나뿐이라 둘을 대조하는 판정기가
  필요 없다([duplicated-sets](../dev-guide/duplicated-sets.md) — 소비자가 같은 물음에 답하는 자리는
  하나로 줄이는 것이 먼저다).
- **잃은 것**: 원칙 위반 수정을 받는 외부 호출자에게는 경고 기간이 없다. 옛 동작에 기대던 호출이 다음
  릴리스에서 바로 다른 값을 받는다(ADR-0532 의 cwd, ADR-0533 의 기록 귀속, ADR-0502 의 응답 `active_tab` 이 그 형태였다).
- **잃은 것**: "원칙 위반" 은 "보안" 보다 넓게 읽힐 수 있다 — 넓게 읽으면 유예 규칙 전체가 비어 버린다.
  조건 1·3(위반 부분만 · 원칙을 이름으로)이 그 경계를 잡지만 판정하는 것은 사람이다.
- **운영 비용 / 유지 부담**: `(BREAK)` 항목을 쓸 때 원칙 조항을 한 번 더 적는다. 강제 채널은 두지
  않는다 — 항목의 문장이 원칙을 댔는지는 산문 판단이다.

## Alternatives Considered

- **원칙 위반을 "심각 버그" 로 분류한다고 적는다**: 목록은 그대로 두고 분류 규칙 한 줄만 더하는 안.
  안 고른 이유: 원칙 위반 중에는 기능적으로 버그가 아닌 것이 있다 — ADR-0532 의 cwd 상속은 설계대로
  동작했고, ADR-0122 의 fire-and-forget 도 의도된 형태였다. "심각" 의 문턱을 원칙 위반에 빌려 쓰면
  같은 낱말이 두 기준을 갖게 되고, 진짜 심각 버그 쪽 판단이 흐려진다(`fs.pick_file` 을 뺀 CHANGELOG 항목은 호스트 전체가 멈추는 결함을 "안정성 정책의 예외" 로 유예 없이 뺐다 — [ADR-0162](0162-a-host-blocking-native-dialog-is-not-an-agent-surface.md)).
- **원칙 위반도 유예를 둔다**(한 minor 동안 옛 동작 + `tracing::warn!`): 규칙을 바꾸지 않는 안. 안 고른
  이유: 그 기간 동안 위반이 출하된다. 원칙 3 을 어기는 옛 동작을 "경고와 함께 유지" 하는 것은 원칙을
  한 minor 동안 끄는 것과 같다. 실측으로도 이 레포는 그렇게 한 적이 한 번도 없다.
- **원칙 위반이면 호환을 따지지 않고 가장 곧은 형태로 고친다**: 조건 2 를 빼는 안. 안 고른 이유:
  ADR-0533 · ADR-0480 이 실제로 호환을 기준으로 형태를 골랐고, 그 선택이 틀렸다는 관측이 없다.
- **두 자리에 같은 목록을 적고 가드로 대조한다**: 지금 배치를 유지하는 안. 안 고른 이유: 두 자리가 같은
  물음에 답하므로 다중 정본이고, 한쪽을 다른 쪽의 포인터로 줄이면 대조할 것 자체가 없어진다. 가드는
  자리를 줄일 수 없을 때의 수단이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 버전 단계가 0.x 를 벗어난다(루트 `Cargo.toml` 의 `version` 이 1 이상, 또는 api-conventions 버전 단계
  표에서 "(현재)" 가 다른 행으로 옮겨진다). 안정선의 "SemVer 엄격" 과 이 예외가 양립하는지 다시 묻는다.
- identity.md §2 의 불가침 원칙에 조항이 더해지거나 빠진다. 예외의 범위가 그 목록을 따라 움직이므로
  새 조항에도 같은 무게를 줄지 묻는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 이 예외를 인용한 `(BREAK)` 가 원칙 조항을 짚지 못한다(원칙과 무관한 변경을 이 사유로 즉시 낸다).
  재는 법: `grep -n '(BREAK)' CHANGELOG.md` 로 이 예외를 댄 항목을 모아, 각 항목이 인용한 ADR 에 원칙
  번호와 위반의 서술이 있는지 대조한다.
- 외부(서드파티) plugin · 클라이언트 소비자가 생겨, 원칙 위반 수정의 즉시 break 가 실제 피해 보고로
  돌아온다. 재는 법: 이슈 트래커와 plugin 레지스트리에서 `(BREAK)` 이후의 호환 문의를 센다.
- 사용자(레포 소유자)가 원칙 위반을 고치는 어떤 break 에 유예를 요구한다. 그 요구 하나가 이 예외의
  전제(원칙 위반은 한 minor 도 출하하지 않는다)를 뒤집으므로, 그 break 에만 유예를 주고 넘어가지 말고
  이 결정 전체를 다시 묻는다. 재는 법: 그 break 의 리뷰 · 이슈 · 커밋 논의에서 유예 요구가 나왔는지를
  본다 — 레포에 기록되는 값이 아니다.

## References

- 운영 문서: [dev-guide/api-conventions](../dev-guide/api-conventions.md) 「안정성 정책」 — 버전 단계 표 ·
  「Deprecation 절차」
- 원칙 정의: [identity.md](../identity.md) §2
- 표의 결정: [ADR-0113](0113-close-preserves-the-focused-target.md) ·
  [ADR-0115](0115-input-reproduction-ipc-debug-isolation.md) ·
  [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md) ·
  [ADR-0143](0143-a-named-target-is-checked-before-the-engine-in-headless.md) ·
  [ADR-0497](0497-an-agent-created-window-does-not-take-the-users-focus.md) ·
  [ADR-0502](0502-an-agent-created-tab-does-not-take-the-users-tab.md) ·
  [ADR-0504](0504-the-file-picker-trigger-answers-only-a-plugin-caller.md) ·
  [ADR-0526](0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md) ·
  [ADR-0532](0532-workspace-create-inherits-cwd-from-the-surface-that-names-its-window.md) ·
  [ADR-0533](0533-an-omitted-target-keeps-its-focus-default-only-where-nothing-names-one.md) ·
  형태 선택 선례 [ADR-0480](0480-a-forwarded-close-carries-who-asked-for-it.md) ·
  예외 이름 없이 "안정성 정책의 예외" 를 댄 선례 [ADR-0162](0162-a-host-blocking-native-dialog-is-not-an-agent-surface.md)
- 선행 결정 없음 — 탐색: `git grep -nE '보안 예외|보안·심각|유예 없이|deprecation' -- docs/adr/`
  (예외 목록 자체를 정한 ADR 은 없다. ADR-0026 · ADR-0510 은 "0.x 정책상" 유예 없이 제거했다고 적었지만
  목록을 정하지 않았다)

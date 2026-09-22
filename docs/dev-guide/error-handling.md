# 에러 처리 정책

`Result` 를 무음 무시하면 실패가 흔적 없이 사라져 회귀 추적이 불가능해진다. **모든 `Result` 는 처리하거나 로그를 남긴다.** 강제 정책은 [`../../CLAUDE.md`](../../CLAUDE.md) "에러 처리".

## 원칙

`Result` 를 `let _ =` 로 무시하지 않는다. 무시는 *명시적 정책 결정* 이어야 하고 그 이유가 주석으로 남는다.

```rust
// ✅ 실패 시 로그 (기본 패턴)
if let Err(e) = self.state.split_surface(SplitDirection::Vertical) {
    tracing::warn!("split_surface failed: {e}");
}

// ✅ 상위로 전파 (호출자가 처리 가능)
self.state.split_surface(SplitDirection::Vertical)?;

// ❌ 무음 무시 — 금지
let _ = self.state.split_surface(SplitDirection::Vertical);
```

## 로그 레벨

| 레벨 | 시점 |
|------|------|
| `tracing::error!` | 복구 불가, 사용자 작업이 의미를 잃음 (설정 저장 실패, layout 파싱 실패) |
| `tracing::warn!` | 무시해도 동작은 계속되는 부분 실패 (옵션 hook 실패, 텔레메트리 전송 실패) |
| `tracing::debug!` | 정상 흐름의 한 분기로 실패가 예상됨 (optional feature 미설치) |

기준선: 사용자가 "방금 왜 안 됐지?" 라고 물었을 때 답을 찾을 로그가 남아 있어야 한다.

## 의도적 무시

진짜로 무시해야 하는 극소수만 `let _ =` 를 허용하되 **왜 무시하는지 한 줄 주석**을 단다. 근거 없는 `let _ =` 는 리뷰에서 차단.

```rust
// 채널 receiver 가 이미 drop 된 정상 종료 케이스 — 송신 실패 무시.
let _ = tx.send(msg);
```

### 주석 위치

사유 주석은 **그 문장 옆에서 읽혀야** 한다. 인정하는 위치는 셋이다.

| 위치 | 예 |
|------|-----|
| 같은 줄 | `let _ = out.flush(); // 직후 exit — 전달할 호출자가 없다.` |
| 바로 윗줄 (빈 줄과 `#[..]` 속성은 사이에 있어도 된다) | `// 이유`<br>`let _ = f();` |
| 문장 범위 안 또는 바로 다음 줄 | 멀티라인 호출의 인자 사이, rustfmt 가 trailing 주석을 밀어낸 다음 줄 |

**블록 상단에 한 번 적은 설명은 사유로 인정하지 않는다.** 몇 줄 위의 주석을 인정하려면
"어디까지 거슬러 올라가는가" 를 정해야 하는데, 함수 doc 주석까지 닿을 만큼 넓히면 사실상
모든 `let _ =` 가 통과해 검사가 검사를 그만둔다. 위에 이미 적었더라도 한 줄을 더 적는다.

전수 강제는 [`crates/tasty-doc-guards/tests/let_underscore_documented.rs`](../../crates/tasty-doc-guards/tests/let_underscore_documented.rs)
가 한다(`cargo test --workspace`). 텍스트 스캔은 타입을 알 수 없으므로 규칙은 `Result` 만이
아니라 **모든 `let _ =`** 에 적용된다 — 변수 바인딩 억제(`let _ = path;`)도 "왜 여기서 안
쓰는가" 가 궁금한 지점이라 한 줄 주석이 손해가 아니다.

테스트 코드는 대상이 아니다 — cargo 규약 디렉토리 `tests`·`benches` 와 `#[cfg(test)]` / `#[test]`
아이템 본문. 테스트에서 값을 버리는 것은 대개 의도가 자명하고, 여기까지 강제하면
통과시키기 위한 형식적 주석만 늘어난다. 정책이 지키려는 것은 **프로덕션에서 조용히
사라지는 실패**다.

**다만 그 제외에는 비용이 있다.** `crates/tasty-gallery/tests/specimen_smoke.rs` 의
`run_frames` 는 `let _ = ctx.run(..)` 로 egui 의 `FullOutput` 을 버렸는데, 그 안에 담긴
것이 이 스모크가 확인하려던 레이아웃 결과였다 — 값을 버린 탓에 테스트가 초록으로
통과하면서도 레이아웃 결함을 못 잡고 있었다. **값을 버리는 것이 검증 자체를 무력화한
실례**다. 그래도 강제 범위를 테스트까지 넓히지 않는 이유는, 형식 주석을 요구해도 이
결함은 안 잡히기 때문이다 — "왜 버리는가" 를 한 줄 적는 것과 "버리면 안 되는 값이었다"
를 알아채는 것은 다른 일이다. 테스트에서 값을 버릴 때는 주석보다 먼저 **그 값이 검증의
일부가 아닌지**를 본다.

`.githooks/pre-commit` C.6 이 staged diff 의 추가 라인에 대해 같은 판정을 먼저 한다. 위쪽
주석을 찾을 때 빈 줄과 `#[..]` 속성 줄을 건너뛰는 것까지 **위 표와 같은 규칙**이다 —
`#[cfg(windows)]` 이 사유 주석과 문장 사이에 오는 것은 러스트 관용이고, 훅만 그것을 막으면
그 줄을 손대는 커밋이 `--no-verify` 로 도망간다. 한 줄 때문에 훅 전체를 끄게 만드는 게이트는
그 자체로 결함이다.

한 자리만 좁다: 훅은 **문장 범위 안**(멀티라인 호출의 인자 사이)을 못 본다. 표의 셋째 항목
중 "바로 다음 줄" 은 보고 "범위 안" 은 안 본다 — 그 방향은 훅이 더 많이 잡는 쪽이라 훅이
통과시킨 코드를 CI 가 떨어뜨리지는 않는다.

그리고 훅의 위쪽 탐색은 **판정기(`mask-source`)가 만든 사본**을 줄 번호로 읽는다. diff
컨텍스트(`-U<n>`)로는 경계가 생기고 그 경계 밖이 조용히 위반이 되기 때문이다. 판정기가
없는 트리에서는 바로 윗줄 하나만 보고 물러나므로 그때는 다시 표보다 좁아진다 — 물러났다는
것을 훅이 말한다.

두 검사 다 렉서가 아니라 텍스트 스캔이지만, **어휘 마스킹은 한다** — 주석·문자열·문자
리터럴을 공백으로 덮은 사본 위에서 판정한다. 마스킹이 없으면 두 방향으로 틀렸다: 문장
안의 `//` 가 문자열 내용(URL 이 대표적)이어도 사유 주석으로 인정했고(미탐), 문자열 안의
금지 형태를 코드로 봤다(오탐). 두 오류의 원인이 하나였다.

**두 층의 정확도가 갈리는 방향은 "훅이 더 거칠다" 쪽이다.** 가드는 파일 전체를 한 번에
마스킹하므로 여러 줄에 걸친 문자열·블록 주석까지 본다. 훅은 awk 로 staged diff 의 줄
하나씩을 보므로 **한 줄 안에서 닫히는 문자열만** 지운다 — 여러 줄 문자열 리터럴은 훅이
여전히 원문으로 본다. 그 층에서는 피할 수 없는 근사고, 그래서 전수 판정의 정본은 가드다.

가드 쪽 마스커는 `crates/tasty-doc-guards/src/source_text.rs` 한 벌이고 다른 스캔 가드도 그것을 쓴다 —
사본이 둘이면 갈리고, 갈린 쪽은 조용하다. 한계 전문은 가드 파일 머리 주석에 있다.

### `clippy::let_underscore_must_use` 는 무엇을 세는가

세 번째 층인 clippy lint 는 **위 규칙을 집행하지 않는다.** 타입은 정확히 보지만 주석을
읽지 못하므로, 사유가 제대로 달린 정상 코드까지 똑같이 경고한다. 즉 이 lint 의 출력은
위반 목록이 아니라 **"프로덕션에서 `Result`/`#[must_use]` 값을 의도적으로 버리는 자리의
명부"** 다. 그 명부는 그것대로 값이 있다 — 정기적으로 훑어 "이 무시가 아직도 옳은가" 를
묻는 자리이고, 타입을 아는 층이라 텍스트 스캔인 가드가 못 보는 것을 본다.

명부가 쓸모 있으려면 **정책이 애초에 아무것도 요구하지 않는 자리**가 거기 섞이면 안 된다.
테스트 본문(위 제외 범위)이 그렇다 — 사유를 요구하지 않으니 그 경고는 영원히 조치 대상이
아니고, 테스트가 늘 때마다 숫자만 흔든다. lint 레벨은 그대로 `warn` 이다 — 레벨을 낮추면
프로덕션에 새로 들어오는 자리까지 함께 사라진다.

**면제는 자리마다가 아니라 타깃마다 단다.** 두 형태뿐이다:

- **크레이트 루트**(그 크레이트의 lib 또는 main 소스)에
  `#![cfg_attr(test, allow(clippy::let_underscore_must_use))]`.
  `cfg_attr(test, ..)` 인 것이 핵심이다 — 라이브러리 타깃은 `cfg(test)` 없이 컴파일되므로
  **프로덕션 자리는 그대로 명부에 오른다.** 실측: `tasty-cli` 에 이 줄을 넣어도 명부 자리
  33 이 33 그대로였고, `tasty-terminal` 에서는 테스트 자리 2 가 0 이 됐다.
- **테스트 타깃**(테스트·벤치 디렉토리 아래 파일 — 각자 별개 크레이트다)의 머리에
  `#![allow(clippy::let_underscore_must_use)]`.

자리마다 다는 방식을 쓰지 않는 이유는 **텍스트로는 타입을 모르기 때문**이다. `let _ =`
하나하나에 면제를 요구하면 clippy 가 애초에 경고하지 않는 자리까지 요구한다 — 실측
(2026-09-05): 면제 없는 테스트 범위 자리 80 중 실제로 명부에 오른 것은 **5** 뿐이었고,
자리 단위로 요구하면 주석 48 개가 필요한데 그중 45 개는 아무것도 안 바꾼다. 타깃 단위로
올리면 필요한 줄이 **20**(크레이트 15 + 테스트 타깃 5)이고, 그 한 줄이 **그 타깃에 앞으로
생길 테스트까지 함께 덮는다.**

실측(2026-09-05, `cargo clippy --workspace --all-targets --locked`, **유니크 (파일,줄)
모수**): 면제를 타깃 단위로 정리한 뒤 이 lint 의 자리는 **83** 이고 전부 프로덕션이다.
정리 직전에는 88 이었고 그중 5 가 테스트 범위였다 — 없어진 것이 정확히 그 5 이고
프로덕션 자리는 하나도 안 줄었다(두 방향을 다 확인했다). 같은 자리가 여러 타깃에서 중복
보고되므로 원시 진단 수는 그보다 크다(171 → 107) — 인용할 때 어느 모수인지 함께 적는다.
전체 경고 총량은 다른 모든 변경과 함께 움직이므로 축으로 쓰지 않는다. 83 은 전부 사유
주석이 달려 있다(그러니 "고쳐야 할 83건" 이 아니다). 이 수가 움직였다면 **프로덕션이
움직인 것**이다.

**한때 이 성질은 저절로 유지되지 않았다.** 직전 측정(`81327419`, 유니크 91, 전부
프로덕션) 이후 하루 만에 테스트 범위 자리 18 개가 명부에 섞였고(테스트 파일 9 · 프로덕션
파일 안의 `#[cfg(test)]` 모듈 8 · `#![cfg(all(test, unix))]` 파일 1), 되돌리는 절차가
수동이었다. 명부가 오염되는 방향이 나쁘다 — 프로덕션에 새로 들어오는 자리가 그 안에
묻힌다.

**지금은 채널이 있다.** `crates/tasty-doc-guards/tests/let_underscore_documented.rs` 의
`test_scope_stays_out_of_the_lint_roster` 가 "테스트 범위에 `let _ =` 가 있으면 그 타깃에
위 면제가 달려 있다" 를 전수로 본다. 같은 파일의 사유 주석 가드와 **스캐너를 공유한다** —
사본이 둘이면 갈리고, 갈린 쪽은 조용하다. 실패 메시지가 **어느 파일의 어느 줄에 무엇을
적어야 하는지**를 그대로 낸다.

새 테스트를 막지는 않는다: 이미 면제가 달린 크레이트·타깃 안에서는 아무것도 요구하지
않고, 면제가 없는 새 크레이트나 새 테스트 타깃에서 처음 `let _ =` 를 쓸 때만 **한 줄**을
요구한다.

이 명부는 로컬 전용이 아니다. clippy 는 자동으로도 돌아서(`--all-targets` 라 테스트
타깃까지 본다) 프로덕션에 새로 들어오는 자리가 거기 **나타난다** — 다만 `-D warnings` 가
없어 **막지는 않는다**. 규칙을 실제로 막는 층은 위의 전수 가드인데, 그쪽은 통합 테스트라
**컴파일은 자동으로 검사되고 실행은 수동**이다. 어느 검사가 언제 도는지는
[ci-gates](ci-gates.md) 가 정본이다 — 여기에 채널 표를 복제하지 않는다.

## plugin 핸들러의 host 호출 — 전파와 최선노력

plugin 이 `host.call(...)` 로 호스트를 부르다 실패했을 때 그 실패를 IPC 응답으로
**전파할지**(`?`) 로그만 남기고 **계속 갈지**(`warn!`)는 취향이 아니다. 판정은 하나다.

> **그 호출 뒤에 이 핸들러가 지켜야 할 로컬 상태가 있는가.** 있으면 최선노력, 없으면 전파.

`?` 로 끊으면 뒤따르는 로컬 작업이 **호출이 실패한 그 경우에만** 건너뛰어진다. 그 로컬
작업이 "대상이 사라졌을 때의 정리" 라면, 정리가 필요한 유일한 경우에만 정리가 안 돈다.

저장소의 두 실례가 이 규칙의 양끝이다.

| 핸들러 | 호출 뒤 로컬 작업 | 판정 |
|---|---|---|
| `tasty-plugin-claude` 의 `handle_claude_hook` | `checklist::remove_state_for_session` · `profile_attach::mark_ended`/`store`/`sweep` — 파일시스템만 만져 surface 없이도 정의된다 | 전부 최선노력(`deliver` 가 `warn!` 하고 계속) |
| `tasty-plugin-codex` 의 `handle_hook` | 없다(이어지는 `fire_hook` 자체가 최선노력) | `terminal.set_state` 만 전파 |

그래서 **두 핸들러가 죽은 surface 에 다르게 답하는 것은 표류가 아니라 의도된 비대칭**
이다 — 같은 훅 이벤트에 claude 는 `ok` 를, codex 는 `-32602` 를 낸다.

죽은 surface 에 훅이 도착하는 것 자체는 close 순서 때문에 가능하다 — 호스트가
레이아웃에서 surface 를 먼저 지우고(`close_surface_by_id_inner`) 그 다음 PTY 를
떨구므로(`cleanup_surface` → `drop_terminal`), PTY 사망 뒤에 도착하는 훅은 이미 없는
surface 를 가리킨다.

**빈도는 판정 근거가 아니다 — 실측에서 오히려 드물었다**(8 일 연속 뜬 plugin 로그의
session-end 70 건 중 host 호출 실패 0 건). 그래도 claude 가 전파하지 않는 것은 대가가
비대칭이기 때문이다: 전파해도 훅 명령이 `|| true` 로 감싸여 있어 호출자에게 닿지 않는데,
대신 orphan 정리가 통째로 안 돈다. 수와 재는 명령은
[ADR-0172](../adr/0172-a-hook-handler-that-cleans-up-locally-does-not-propagate.md).

### 최선노력의 대가는 치르되 **값으로 노출한다**

최선노력에는 대가가 있다: 응답이 성공을 말하는 동안 그 아래에서 호출이 조용히 실패한다.
그 대가를 치르는 것과 **대가를 안 보이게 두는 것**은 다르다. 한때 두 훅 핸들러의 응답은
host 호출이 전부 실패해도 전부 성공했을 때와 바이트가 같았고, 유일한 흔적은 `warn!`
로그였다 — 로그는 그 인스턴스에만 남고 호출자에게는 안 간다.

그래서 최선노력 호출을 가진 핸들러는 **조용히 실패한 호출의 수를 응답에 싣는다.**

    { "ok": true, "surface_id": 100, "event": "session-end", "host_call_failures": 0 }

규약 셋을 함께 지킨다.

- **항상 있고 항상 수다.** "실패했을 때만 넣는" 형태는 침묵을 한 칸 옮길 뿐이다 —
  필드가 없는 것과 실패가 0 인 것이 다시 구별되지 않는다. `bool` 도 마찬가지로
  "몇 개가" 를 지운다.
- **센 것을 싣는다.** 삼키는 그 자리에서 센다. 그래서 삼키는 함수는 성공 여부를
  돌려주고, 호출부가 그 수를 모은다.
- **전파하는 호출은 이 수에 안 들어간다.** 그쪽은 실패가 곧 오류 응답이라 이미 보인다.
  세는 것은 **응답이 성공을 말하는 동안 실패할 수 있는 것**뿐이다.

이 값이 시험 가능하려면 핸들러가 호스트를 **트레이트로** 받아야 한다
(`tasty_plugin_agent_common::host_call::HostCall`). 구체 타입을 받으면 mock 을 못 먹여
그 수를 재는 시험을 쓸 수 없고, 결정은 주석으로만 남는다.

**관측 지점은 둘이다** — IPC 응답과, 그 응답을 그대로 찍는 CLI(`tasty claude hook …`).
다만 실사용에서 그 CLI 는 훅 명령 안에서 돌고 훅 명령은 출력을 버리므로, 0 이 아닌 수는
`hook-failures.log` 에도 함께 남긴다(위 "왜 IPC 가 아니라 파일인가" 와 같은 이유다).

## 락 poison (`Mutex` / `RwLock`)

poison 은 **"다른 스레드가 이 락을 든 채 패닉했다"** 는 사실만 알려준다. 보호 중인 데이터가
실제로 깨졌다는 뜻이 **아니다**. 그래서 `.unwrap()` / `.expect()` 로 일괄 패닉시키는 것도,
`Err(_) => return` 으로 일괄 무시하는 것도 틀렸다 — 지점마다 아래 두 질문에 답해서 고른다.

### 질문 1 — 임계구역이 불변식을 깨진 채 남길 수 있는가

| 임계구역이 하는 일 | 처리 |
|---|---|
| 자료구조 조작만 (`insert` / `remove` / `retain` / 필드 하나 갱신) | **복구** — `lock().unwrap_or_else(\|p\| p.into_inner())` |
| 여러 필드를 순서대로 갱신하거나, 콜백·trait object·외부 crate 등 **임의 코드**를 호출 | **에러 반환**하거나 그 항목만 폐기 후 재구성. 데이터를 신뢰하지 않는다 |

임계구역이 순수 자료구조 조작뿐이면 그 안에 패닉 지점이 사실상 없고, 있더라도(할당 실패 등)
컨테이너는 불변식을 유지한다. 그런 락을 poison 때문에 영구 사용 불가로 만드는 것은
**원래 패닉의 피해를 스스로 확대하는 것**이다.

### 질문 2 — 여기서 패닉하면 무엇이 죽는가

사망 범위가 클수록 패닉은 나쁜 선택이다.

| 범위 | 어디 | 방침 |
|---|---|---|
| **프로세스 전체** | winit 메인 스레드에서 도는 코드 — 이벤트 루프, 렌더, `AppEvent` 처리 | **패닉 금지.** 실행 중인 모든 창의 모든 터미널 세션이 사라진다 (창 생성 실패 절과 같은 근거) |
| **호스트 스레드 하나** | IPC 핸들러, agent runner, PTY 리더 | 그 요청/러너만 죽는다. 다만 **패닉이 자기 복구 경로까지 죽이지 않는지** 확인한다 — 재시작을 위해 읽어야 하는 레지스트리를 재시작 경로가 다시 패닉시키면 복구 설계가 무력해진다 |
| **플러그인 프로세스 하나** | `crates/tasty-plugin-sdk` 안 | 폭발 반경이 그 plugin 으로 한정된다. 패닉 허용 (plugin 스레드 spawn 정책과 같은 근거) |

### 어느 선택을 하든 로그를 남긴다

poison 은 **이미 어딘가에서 패닉이 있었다**는 신호다. 조용한 복구도 조용한 무시도 그 신호를
지운다. 레벨은 `tracing::error!` — 원인이 된 패닉은 이미 일어났고 복구 불가다.

poison 은 **sticky** 다(한 번 걸리면 그 락은 영구히 poisoned). 초당 여러 번 도는 경로에서
매번 로그를 내면 폭주하므로, 그런 지점은 `AtomicBool` 등으로 **첫 1 회만** 남긴다.

복구를 택한 지점의 공용 헬퍼는 `tasty_utils::poison` 이다(`recover_mutex` ·
`recover_read` · `recover_write` · `recover_try_write` — 각각 락 이름과 보고 플래그를
받는다). 헬퍼를 쓰지 않는 쪽이 맞는 경우도 있다: 한 파일 안에서 지점마다 답이 갈리고
그 판단이 이미 인라인으로 적혀 있다면, 그중 한 곳만 헬퍼로 바꾸는 것은 형태를 둘로
늘릴 뿐이다(`crates/tasty-plugin-agent-stream` 이 그 예다).

```rust
// ✅ 복구 + 관측 (자료구조 조작만 하는 임계구역)
let mut gates = self.targeted_gates.lock().unwrap_or_else(|p| {
    tracing::error!("targeted_gates mutex poisoned — recovering; a thread panicked while holding it");
    p.into_inner()
});

// ✅ 에러 반환 (데이터를 신뢰할 수 없는 임계구역)
let mut inner = self.inner.write().map_err(|e| {
    tracing::error!("registry write lock poisoned: {e}");
    RegistryError::Poisoned
})?;

// ❌ 무음 return — 등록이 반영되지 않았는데 관측 지점이 0 이다
let mut inner = match self.inner.write() { Ok(g) => g, Err(_) => return };

// ❌ 사망 범위를 안 따진 일괄 패닉
let mut inner = self.inner.lock().expect("poisoned");
```

위 **무음 `return`** ❌ 는 가드가 잡지 못한다 — 읽는 사람이 여기서 가장 자주 오해하는
자리라 적어 둔다. `crates/tasty-utils/src/poison.rs` 의 삼킴 판정기
(`silently_skipped_lock_lines`)는 `match` 와 `let Ok(..) else` 를 "poison 을 다루는
형태" 로 분류해 지나친다. 검사 대상은 락 verb 바로 뒤의 `.ok()`, 체인의
`.unwrap_or(..)`·`.unwrap_or_default()`, else 없는 `if let Ok(..)`·`while let Ok(..)`·
`&& let Ok(..)` 이다. 그 분류는 의도된 것이다: 두 형태는 `Err` 갈래를 **쓴** 자리라, 그 갈래에 로그나
사유가 들어 있는지를 모양만으로는 가릴 수 없다. 그래서 이 ❌ 는 자동으로 잡히는 금지가
아니라 **리뷰가 보는 금지**다. 판정기를 넓힐 생각이면 그 유닛 테스트가 두 형태를
"허용 — 못 잡는 것이 의도다" 로 못 박아 두었으니 거기서부터 시작한다.

### 이 방침이 덮는 범위와 덮지 않는 범위

**가드가 보는 축은 하나다** — `crates/tasty-utils/src/poison.rs` 의 `FORBIDDEN_LOCKS` 스캔은
**복구하면 안 되는 락을 복구하거나 조용히 지나치는 것**을 잡는다. 그 밖은 안 본다.

**보고 없는 복구는 별도 검사다.**
`crates/tasty-doc-guards/tests/no_silent_poison_recovery.rs` 가 `src`·`crates` 를
훑고, `poison_recovery` 판정기가 인라인 closure/Err arm의 `into_inner()` 에 보고가
있는지 본다. 테스트 전용 코드는 제외한다. 검사 실행 채널은 [ci-gates](ci-gates.md),
구체적인 인식 범위는 `crates/tasty-doc-guards/src/poison_recovery.rs` 에 있다.

**인식 밖의 복구가 초록에 섞일 수 있다.** `PoisonError::into_inner` 함수 포인터와
복구 헤드 없이 함수 인자로 받은 오류를 풀어내는 형태는 이 검사가 세지 못한다.
따라서 이 검사 통과가 모든 복구의 관측을 증명하지 않는다. 그런 자리도 같은 로그
규약을 따르며, 이미 받은 오류는 `tasty_utils::poison::recover_poisoned` 로 보고한다.
Condvar 대기 중 발생한 poison은 최초 lock 성공 뒤에 생기므로 **재획득 경로에서도**
같은 보고 플래그를 전달해야 한다. [timer-hub](timer-hub.md)의 waker가 이 계약을 따른다.


## stdout 쓰기 (CLI 클라이언트)

`println!` / `print!` 는 stdout 쓰기 실패를 panic 으로 승격한다. 읽는 쪽이 파이프를 먼저 닫으면
(`tasty list tree | head -1`, `| true`) EPIPE 가 돌아오고, 그 panic 이 종료 코드 101 + 가짜
crash report 가 된다. Rust 런타임은 SIGPIPE 를 무시하도록 두고 Windows 에는 SIGPIPE 가 없으므로,
**stdout 쓰기도 `Result` 로 받아 처리한다** — 근거와 대안은 [ADR-0101](../adr/0101-cli-stdout-broken-pipe-exit-zero.md).

- `crates/tasty-cli` 는 stdout 에 **`crate::out` 의 `outln!` / `out!`** 로만 쓴다(`println!` /
  `print!` 금지 — `tests/cli_stdout_broken_pipe.rs` 가 소스 스캔으로 강제). 값은
  `anyhow::Result<()>` 라 `?` 로 올린다. 개행 없는 출력 뒤의 flush 는 `crate::out::flush()?`,
  stdout 에 직접 쓰는 외부 코드(clap `print_help`)의 `io::Result` 는 `crate::out::from_io(..)?`.
- `ErrorKind::BrokenPipe` 는 `StdoutClosed` 로 구분돼 호출 스택을 타고 올라오고, CLI 진입점
  (`run_client` / `print_augmented_help` / `print_command_tree` / `try_run_plugin_cli`)이
  `quiet_if_stdout_closed` 로 **종료 코드 0** 으로 접는다. 실패가 아니라 "더 쓸 곳이 없다" 는
  신호다. 그 외 stdout 오류(EIO / ENOSPC 등)는 일반 에러로 전파된다.
- 폴링/follow 루프에서 읽는 쪽이 사라졌을 때 루프를 빠져나오는 지점은 **다음 출력**
  (`outln!`/`out!` 의 write)이다. `crate::out::flush()?` 는 버퍼에 남은 바이트가 있을 때만
  write(2) 를 내므로 **빈 버퍼 flush 는 EPIPE 를 감지하지 못한다** — flush 를 파이프 생존
  프로브로 쓰지 않는다. 출력이 더 없으면 `tail -f | head -1` 처럼 계속 대기한다.
- host(GUI / headless)는 stdout 에 쓰지 않는다(pre-commit C.11 이 `println!` 을 막는다). 이
  정책은 CLI 클라이언트 갈래에만 적용되고 `Routed::Gui` 의 동작은 바뀌지 않는다.
- **stderr 는 `crate::out` 의 `errln!` 으로만 쓴다**(`eprintln!` / `eprint!` 금지 — 같은 시험
  파일의 소스 스캔이 강제). `eprintln!` 도 쓰기 실패를 panic 으로 승격해 `2>&1 | head` 에서
  crash report 가 된다. stderr 는 실패를 알리는 마지막 채널이라 그 쓰기의 실패는 **버리고**,
  종료 코드는 명령이 원래 내던 값 그대로다(stdout 처럼 0 으로 접지 않는다 — 대개 이미 실패한
  명령이다). 근거는 [ADR-0513](../adr/0513-cli-stderr-broken-pipe-keeps-the-exit-code.md).
- 예외: `local/attach.rs` raw bridge 는 `std::io::stdout()` 핸들에 best-effort 미러하고 결과를
  주석과 함께 무시한다 — stdout 이 닫혀도 attach 세션은 계속돼야 한다.

```rust
// ✅ CLI 출력
outln!("{}", serde_json::to_string_pretty(&value)?)?;
out!("{chunk}")?;
crate::out::flush()?;

// ❌ panic 승격 — EPIPE 가 crash report 가 된다
println!("{}", serde_json::to_string_pretty(&value)?);
```

## 창·GPU·엔진·스레드 생성 실패

생성 실패는 패닉하지 않는다. 이미 터미널 세션이 떠 있는 상태에서 패닉하면 그 창 하나가
아니라 **실행 중인 모든 창의 작업**이 사라진다. 결정 전문과 근거는
[ADR-0117](../adr/0117-window-and-modal-creation-failure-policy.md).

| 지점 | 처리 |
|------|------|
| 부팅 창 생성 · 부팅 엔진 생성 · GPU 어댑터 부재 | 진단 3줄을 `tracing::error!` 한 이벤트로 내고 `exit(1)`. `eprintln!` 은 파일 로그에 안 남아 쓰지 않는다 |
| 부팅의 그 외 GPU 실패 | 패닉 유지 — 환경 문제가 아니라 버그이므로 크래시 리포팅 경로에 남긴다 |
| 새 창 · 설정 · 플러그인 모달 | 그 창만 취소하고 살아 있는 메인 창에 안내. 안내 문구는 지점별 i18n 키 |
| 종료 확인 모달 | 확인을 건너뛰고 `begin_shutdown()`. 생략 사실을 toast + `error!` 로 알린다 |
| 호스트 스레드 spawn | 에러 반환(`ObserverError::ThreadSpawn`) 또는 로그 후 미등록 |
| plugin 프로세스 스레드 spawn | 패닉 유지 — 폭발 반경이 그 plugin 프로세스로 한정된다 |

**안내 채널은 요청 origin 이 가른다.** 사용자 조작(메뉴 · 단축키 · dock · tray)발 실패는
`InfoModal`, 에이전트 IPC(`window.create`)발 실패는 **toast** 다. `InfoModal` 은 포커스를
가져가므로, 에이전트 행동의 부수효과가 사용자 포커스에 닿지 않는다는 핵심 원칙 1 을
어기게 된다.

이 경로들은 winit `ActiveEventLoop` 가 있어야 돌아가 행동 테스트로 감쌀 수 없다 —
`crates/tasty-doc-guards/tests/no_panic_in_window_creation.rs` 가 소스 형태로 패닉 재유입을 막는다.

## 로그 메시지 작성

**무엇이** 실패했는지 + **원인** + (가능하면) **영향**. 변수 보간으로 컨텍스트를 담는다.

```rust
tracing::warn!("failed");                                    // ❌ 컨텍스트 없음
tracing::warn!("hook {hook_id} failed for surface {surface_id}: {e}"); // ✅
```

## anyhow / thiserror

에러 타입 정의·`?` 전파·context 첨부 등 `anyhow`/`thiserror` 사용법은 상류 문서를 따른다 — 본 문서는 tasty 의 *정책* 만 다룬다.

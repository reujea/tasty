# 에러 처리 정책

`Result` 를 무음 무시하면 실패가 흔적 없이 사라져 회귀 추적이 불가능해진다. **모든 `Result` 는 처리하거나 로그를 남긴다.** 강제 정책은 [`../../CLAUDE.md`](../../CLAUDE.md) "에러 처리".

## 원칙

`Result` 를 `let _ =` 로 무시하지 않는다. 무시는 *명시적 정책 결정* 이어야 하고 그 이유가 주석과 문장 단위 `expect`의 `reason`으로 남는다([의도적 무시](#의도적-무시)).

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

실패를 무시해도 되는 경우에만 `let _ =` 를 허용하되 **왜 무시하는지 한 줄 주석**을 단다. 근거 없는 `let _ =` 는 리뷰에서 차단.

기준은 그 문장에서 `let_underscore_must_use`(warn)가 실제로 발동하는가다. 이 lint는 두 경우에
발동한다. 버리는 값의 타입이 `#[must_use]`인 경우(`Result` 등)와, 오른쪽 식의 마지막 호출 대상
함수·메서드에 `#[must_use]`가 붙은 경우다. 예를 들어 `let _ = f().is_ok();`는 버리는 값이
`bool`이지만 `Result::is_ok`가 `#[must_use]`라 진단된다.

- 발동하는 제품 코드 문장에는 주석과 함께 문장 단위
  `#[expect(clippy::let_underscore_must_use, reason = "...")]`를 단다. 주석만 있으면 lint가
  발동하고, CI clippy 단계와 pre-push B.4는 `-D warnings`로 돌아 그 경고에서 실패한다.
- 발동하지 않는 문장(변수 바인딩 억제 `let _ = path;`처럼 타입도 호출 대상도 `#[must_use]`가
  아닌 경우)에는 주석만 단다. 발동하지 않는 자리에 `expect`를 달면
  `unfulfilled_lint_expectations`로 같은 단계가 실패한다.

```rust
#[expect(
    clippy::let_underscore_must_use,
    reason = "The receiver may already be dropped during shutdown."
)]
let _ = tx.send(msg); // 채널 receiver 가 이미 drop 된 정상 종료 케이스 — 송신 실패 무시.
```

### 정상 종료와 의도된 반환값 버리기

수신자나 이벤트 루프가 이미 종료된 통지, 패닉 중의 테스트 임시 파일 정리,
종료 동작을 재현하는 시뮬레이터 출력은 실패를 전파할 대상이 없을 수 있다.
그 동작을 확인한 문장에는 `#[expect(clippy::let_underscore_must_use, reason = "...")]`로
사유를 고정한다. 실제 오류를 숨기기 위한 함수·크레이트 전체 면제는 추가하지 않는다.
경고가 더 이상 발생하지 않으면 `unfulfilled_lint_expectations`가 오래된 기대를 알린다.

유한 채널의 수신자 종료와 큐 포화는 다르다. 승인·작업 대기자 통지는 연결 종료를
정상으로 처리하되 포화된 일회용 큐는 경고한다. 저장소 정리 실패는 로그 후 재시도 기회를
유지하며, 오류 처리 도중 생긴 임시 파일 삭제 오류로 원래 저장 오류를 바꾸지 않는다.
스레드 join 실패는 패닉으로 기록하고 남은 정리를 계속한다. 이미 완료된 OnceLock에는
첫 값을 유지하는 `get_or_init`을 사용한다.

### 주석 위치

사유 주석은 **그 문장 옆에서 읽혀야** 한다. 인정하는 위치는 셋이다.

| 위치 | 예 |
|------|-----|
| 같은 줄 | `let _ = out.flush(); // 직후 exit — 전달할 호출자가 없다.` |
| 바로 윗줄 (빈 줄과 `#[..]` 속성은 사이에 있어도 된다) | `// 이유`<br>`let _ = f();` |
| 문장 범위 안 또는 바로 다음 줄 | 멀티라인 호출의 인자 사이, rustfmt 가 trailing 주석을 밀어낸 다음 줄 |

**블록 상단에 한 번 적은 설명은 사유로 인정하지 않는다.** 몇 줄 위의 주석을 인정하려면
"어디까지 거슬러 올라가는가" 를 정해야 하는데, 함수 doc 주석까지 닿을 만큼 넓히면 사실상
모든 `let _ =` 가 통과해 사유와 무관한 주석으로 통과할 수 있다. 위에 이미 적었더라도 한 줄을 더 적는다.

전수 강제는 [`crates/tasty-doc-guards/tests/let_underscore_documented.rs`](../../crates/tasty-doc-guards/tests/let_underscore_documented.rs)
가 한다(`cargo test --workspace`). 텍스트 스캔은 타입을 알 수 없으므로 규칙은 `Result` 만이
아니라 **모든 `let _ =`** 에 적용된다 — 변수 바인딩 억제(`let _ = path;`)도 "왜 여기서 안
쓰는가" 가 궁금한 지점이라 한 줄 주석이 손해가 아니다.

테스트 코드는 대상이 아니다 — cargo 규약 디렉토리 `tests`·`benches` 와 `#[cfg(test)]` / `#[test]`
아이템 본문. 테스트에서 값을 버리는 것은 대개 의도가 자명하고, 여기까지 강제하면
통과시키기 위한 형식적 주석만 늘어난다. 정책이 지키려는 것은 **프로덕션에서 조용히
사라지는 실패**다.

테스트에서는 사유 주석을 강제하지 않지만, 버리는 값이 검증 결과인지 먼저 확인한다.
예를 들어 egui의 `FullOutput`을 버리면 검사하려던 레이아웃 결과도 함께 버릴 수 있다.
주석을 추가하는 것만으로 이런 검증 누락을 고칠 수는 없다.

`.githooks/pre-commit` C.6 이 staged diff 의 추가 라인에 대해 같은 판정을 먼저 한다. 위쪽
주석을 찾을 때 빈 줄과 `#[..]` 속성 줄을 건너뛰는 것까지 **위 표와 같은 규칙**이다 —
`#[cfg(windows)]` 이 사유 주석과 문장 사이에 오는 것은 러스트 관용이고, 훅만 그것을 막으면
정상적인 코드까지 거절하게 된다.

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
여전히 원문으로 본다. 훅의 인식 범위는 더 좁으므로 전수 판정의 정본은 가드다.

가드 쪽 마스커는 `crates/tasty-doc-guards/src/source_text.rs` 한 벌이고 다른 스캔 가드도 그것을 쓴다 —
같은 규칙을 별도로 구현해 차이가 생기는 것을 피한다. 한계 전문은 가드 파일 머리 주석에 있다.

### `clippy::let_underscore_must_use` 는 무엇을 세는가

세 번째 층인 clippy lint 는 **위 규칙을 집행하지 않는다.** 타입은 정확히 보지만 주석을
읽지 못하므로, 사유를 주석으로만 남긴 정상 코드에도 lint 가 발동할 수 있다. 문장 단위
`#[expect(clippy::let_underscore_must_use, reason = "...")]` 가 있으면 lint 가 발동해 기대가
충족되고 그 진단은 억제된다. lint 가 더 이상 발동하지 않으면 `unfulfilled_lint_expectations` 가
보고된다. 즉 이 lint 의 출력은 위반 목록이 아니라 **"프로덕션에서 `Result`/`#[must_use]` 값을
버리는데 아직 억제되지 않은 자리의 명부"** 다. 그 명부는 그것대로 값이 있다 — 정기적으로 훑어 "이 무시가 아직도 옳은가" 를
묻는 자리이고, 타입을 아는 층이라 텍스트 스캔인 가드가 못 보는 것을 본다.

명부가 쓸모 있으려면 **정책이 애초에 아무것도 요구하지 않는 자리**가 거기 섞이면 안 된다.
테스트 본문(위 제외 범위)이 그렇다 — 사유를 요구하지 않으니 그 경고는 영원히 조치 대상이
아니고, 테스트가 늘 때마다 숫자만 흔든다. lint 레벨은 그대로 `warn` 이다 — 레벨을 낮추면
프로덕션에 새로 들어오는 자리까지 함께 사라진다.

**면제는 자리마다가 아니라 타깃마다 단다.** 두 형태뿐이다:

- **크레이트 루트**(그 크레이트의 lib 또는 main 소스)에
  `#![cfg_attr(test, allow(clippy::let_underscore_must_use))]`.
  `cfg_attr(test, ..)` 인 것이 핵심이다 — 라이브러리 타깃은 `cfg(test)` 없이 컴파일되므로
  **위치별 expect가 없는 프로덕션 자리는 명부에 오른다.** 실측: `tasty-cli` 에 이 줄을 넣어도 명부 자리
  33 이 33 그대로였고, `tasty-terminal` 에서는 테스트 자리 2 가 0 이 됐다.
- **테스트 타깃**(테스트·벤치 디렉토리 아래 파일 — 각자 별개 크레이트다)의 머리에
  `#![allow(clippy::let_underscore_must_use)]`.

텍스트 스캔은 값의 타입을 모르므로 모든 `let _ =`에 lint 면제를 요구하지 않는다.
위 두 형식으로 타깃 전체의 테스트 코드만 제외하고 제품 코드의 경고는 유지한다.
`test_scope_stays_out_of_the_lint_roster`
(`crates/tasty-doc-guards/tests/let_underscore_documented.rs`)가 테스트 범위의
`let _ =`에 해당 타깃 면제가 있는지 확인하며 사유 주석 검사와 같은 스캐너를 쓴다.

새 테스트를 막지는 않는다: 이미 면제가 달린 크레이트·타깃 안에서는 아무것도 요구하지
않고, 면제가 없는 새 크레이트나 새 테스트 타깃에서 처음 `let _ =` 를 쓸 때만 **한 줄**을
요구한다.

이 명부는 로컬 전용이 아니다. clippy 는 자동으로도 돌아서(`--all-targets` 라 테스트
타깃까지 본다) 프로덕션에 새로 들어오는 자리가 거기 **나타난다** — 자동 clippy 단계와
pre-push 는 `-D warnings` 로 돌므로 명부에 오를 자리가 생기면 **실패한다**. 매니페스트의
lint 레벨은 `warn` 그대로이고, `-D` 없이 돌린 로컬 clippy 에서는 경고로 보인다. 규칙을 실제로 막는 층은 위의 전수 가드다. 어느 검사가 언제 도는지는
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

그래서 **두 핸들러가 죽은 surface 에 다르게 답하는 것은 의도된 차이**
이다 — 같은 훅 이벤트에 claude 는 `ok` 를, codex 는 `-32602` 를 낸다.

죽은 surface 에 훅이 도착하는 것 자체는 close 순서 때문에 가능하다 — 호스트가
레이아웃에서 surface 를 먼저 지우고(`close_surface_by_id_inner`) 그 다음 PTY 를
떨구므로(`cleanup_surface` → `drop_terminal`), PTY 사망 뒤에 도착하는 훅은 이미 없는
surface 를 가리킨다.

호출 실패가 드물어도 뒤따르는 로컬 정리는 필요하다. 훅 명령은 `|| true`로 감싸져
있으므로 단순 오류 전파가 외부 호출자에게 전달되지 않을 수도 있다. 실패는 기록하고
정리를 끝낸다([훅 (Surface / Global hooks)](../features/hooks/index.md)).

### 최선노력의 대가는 치르되 **값으로 노출한다**

오류를 기록하고 계속하는 핸들러는 응답의 `host_call_failures`에 실패한 호출 수를 담는다.
호출자가 `ok:true`만 보고 모든 하위 호출이 성공했다고 오해하지 않게 한다.

    { "ok": true, "surface_id": 100, "event": "session-end", "host_call_failures": 0 }

규약 셋을 함께 지킨다.

- **필드를 항상 숫자로 넣는다.** 실패가 없으면 0을 넣어 필드 누락과 구분한다.
  `bool` 대신 실패한 호출 수를 전달한다.
- **오류를 기록하고 계속하는 지점에서 센다.** 해당 함수가 성공 여부를 반환하고,
  호출부가 실패 수를 합산한다.
- **오류 응답으로 전파하는 실패는 제외한다.** 이 수에는 전체 응답이 성공이더라도
  내부에서 실패할 수 있는 호출만 포함한다.

핸들러는 `tasty_plugin_agent_common::host_call::HostCall` 트레이트로 호스트를 받는다.
시험에서 mock으로 실패를 일으켜 집계가 맞는지 확인할 수 있다.

집계는 IPC 응답과 그 응답을 출력하는 CLI(`tasty claude hook …`)에서 확인한다.
실제 훅 명령은 CLI 출력을 버리므로, 실패 수가 0이 아니면
`hook-failures.log`에도 기록한다([CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md)).

## 락 poison (`Mutex` / `RwLock`)

poison 은 **"다른 스레드가 이 락을 든 채 패닉했다"** 는 사실만 알려준다. 보호 중인 데이터가
실제로 깨졌다는 뜻이 **아니다**. 그래서 `.unwrap()` / `.expect()` 로 일괄 패닉시키는 것도,
`Err(_) => return` 으로 일괄 무시하는 것도 틀렸다 — 지점마다 아래 두 질문에 답해서 고른다.

### 질문 1 — 임계구역이 불변식을 깨진 채 남길 수 있는가

| 임계구역이 하는 일 | 처리 |
|---|---|
| 단일 갱신 등, 패닉 뒤에도 보호하는 데이터의 불변식이 유지된다고 확인한 조작 | **복구** — `lock().unwrap_or_else(\|p\| p.into_inner())` |
| 여러 필드를 순서대로 갱신하거나, 콜백·trait object·외부 crate 등 **임의 코드**를 호출 | **에러 반환**하거나 그 항목만 폐기 후 재구성. 데이터를 신뢰하지 않는다 |

프레임을 쓰는 TcpStream·HandleStream·HandleClient 락은 poison 뒤 이어 쓰면 메시지 경계가
손상될 수 있으므로 복구하지 않는다. 일부 줄 손실을 허용하는 로그 fs::File과는 구분한다.

복구 여부는 `insert` 같은 메서드 이름만으로 정하지 않는다. 사용자 정의 Hash·Eq·Drop이나
콜백도 실행할 수 있으므로, 보호하는 데이터가 중간 상태로 남을 가능성을 확인한다.
불변식이 유지되는 단순 상태 락은 poison만을 이유로 영구 사용 불가로 만들지 않는다.

### 질문 2 — 여기서 패닉하면 무엇이 죽는가

사망 범위가 클수록 패닉은 나쁜 선택이다.

| 범위 | 어디 | 방침 |
|---|---|---|
| **프로세스 전체** | winit 메인 스레드에서 도는 코드 — 이벤트 루프, 렌더, `AppEvent` 처리 | **패닉 금지.** 실행 중인 모든 창의 모든 터미널 세션이 사라진다 (창 생성 실패 절과 같은 근거) |
| **호스트 스레드 하나** | IPC 핸들러, agent runner, PTY 리더 | 그 요청/러너만 죽는다. 다만 **패닉이 자기 복구 경로까지 죽이지 않는지** 확인한다 — 재시작을 위해 읽어야 하는 레지스트리를 재시작 경로가 다시 패닉시키면 복구 설계가 무력해진다 |
| **플러그인 프로세스 하나** | `crates/tasty-plugin-sdk` 안 | 영향 범위가 그 plugin 으로 한정된다. 패닉 허용 (plugin 스레드 spawn 정책과 같은 근거) |

### 어느 선택을 하든 로그를 남긴다

poison 은 **이미 어딘가에서 패닉이 있었다**는 신호다. 조용한 복구도 조용한 무시도 그 신호를
지운다. 레벨은 `tracing::error!` — 원인이 된 패닉은 이미 일어났고 복구 불가다.

poison 상태는 명시적으로 해제하지 않는 한 유지된다. 초당 여러 번 도는 경로에서
매번 로그를 내면 폭주하므로, 그런 지점은 `AtomicBool` 등으로 **첫 1 회만** 남긴다.

복구를 택한 지점의 공용 헬퍼는 `tasty_utils::poison` 이다(`recover_mutex` ·
`recover_read` · `recover_write` · `recover_try_write` — 각각 락 이름과 보고 플래그를
받는다). 헬퍼를 쓰지 않는 쪽이 맞는 경우도 있다: 한 파일 안에서 지점마다 답이 갈리고
그 판단이 이미 인라인으로 적혀 있다면, 그중 한 곳만 헬퍼로 바꾸는 것은 형태를 둘로
늘릴 뿐이다(`tasty-official-plugins/crates/tasty-plugin-agent-stream` 이 그 예다).

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

// ❌ 무음 return — 등록이 반영되지 않았는데 실패를 확인할 기록이 없다
let mut inner = match self.inner.write() { Ok(g) => g, Err(_) => return };

// ❌ 사망 범위를 안 따진 일괄 패닉
let mut inner = self.inner.lock().expect("poisoned");
```

위 **무음 `return`** ❌ 는 가드가 잡지 못한다 — 읽는 사람이 여기서 가장 자주 오해하는
자리라 적어 둔다. `crates/tasty-utils/src/poison.rs` 의 오류 무시 검사
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

**인식하지 못한 복구도 검사를 통과할 수 있다.** `PoisonError::into_inner` 함수 포인터와
복구 헤드 없이 함수 인자로 받은 오류를 풀어내는 형태는 이 검사가 세지 못한다.
따라서 이 검사 통과가 모든 복구의 관측을 증명하지 않는다. 그런 자리도 같은 로그
규약을 따르며, 이미 받은 오류는 `tasty_utils::poison::recover_poisoned` 로 보고한다.
Condvar 대기 중 발생한 poison은 최초 lock 성공 뒤에 생기므로 **재획득 경로에서도**
같은 보고 플래그를 전달해야 한다. [timer-hub](timer-hub.md)의 waker가 이 계약을 따른다.


## stdout 쓰기 (CLI 클라이언트)

`println!` / `print!` 는 stdout 쓰기 실패를 panic 으로 승격한다. 읽는 쪽이 파이프를 먼저 닫으면
(`tasty list tree | head -1`, `| true`) EPIPE 가 돌아오고, 그 panic 이 종료 코드 101 + 가짜
crash report 가 된다. Rust 런타임은 SIGPIPE 를 무시하도록 두고 Windows 에는 SIGPIPE 가 없으므로,
**stdout 쓰기도 `Result` 로 받아 처리한다** — 근거와 대안은 [CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md).

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
  명령이다). 근거는 [CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md).
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
[종료 시퀀스 — 종료 cascade + Drop tail](../architecture/shutdown-sequence.md).

| 지점 | 처리 |
|------|------|
| 부팅 창 생성 · GPU 어댑터 부재 | 진단 3줄을 `tracing::error!` 한 이벤트로 내고 `exit(1)`. `eprintln!` 은 파일 로그에 안 남아 쓰지 않는다 — 그릴 창·GPU 가 없어 로그가 유일한 채널이다 |
| 부팅 엔진 생성 | 같은 진단을 `tracing::error!` 로 내고, 이미 살아 있는 창에 실패 화면을 그려 둔다. 사용자가 닫을 때(종료 버튼 · Esc/Enter · 창 닫기) `exit(1)` |
| 부팅의 그 외 GPU 실패 | 패닉 유지 — 환경 문제가 아니라 버그이므로 크래시 리포팅 경로에 남긴다 |
| 새 창 · 설정 · 플러그인 모달 | 그 창만 취소한다. 사용자 조작발이면 살아 있는 메인 창에 안내하고(문구는 지점별 i18n 키), 에이전트 IPC 발이면 안내 없이 요청자에게 응답 에러로 돌려준다 — 아래 origin 절 |
| 종료 확인 모달 | 확인을 건너뛰고 `begin_shutdown()`. 생략 사실을 toast + `error!` 로 알린다 |
| 호스트 스레드 spawn | 에러 반환(`ObserverError::ThreadSpawn`) 또는 로그 후 미등록 |
| plugin 프로세스 스레드 spawn | 패닉 유지 — 영향 범위가 그 plugin 프로세스로 한정된다 |

**안내 채널은 요청 origin 이 가른다.** 사용자 조작(메뉴 · 단축키 · dock · tray)발 실패는
`InfoModal`, 에이전트 IPC(`window.create`)발 실패는 **요청자에게 IPC 응답 에러로만** 돌려주고
사용자 화면에는 아무것도 띄우지 않는다(toast 도 아니다). `InfoModal` 은 포커스를 가져가고
toast 도 요청하지 않은 일의 통지라, 어느 쪽이든 에이전트 행동의 부수효과가 사용자 상태에
닿지 않는다는 [포커스 정책](../design/policies/focus.md)을 어기게 된다([IPC 서버](../architecture/ipc-server.md)).

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

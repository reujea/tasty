# 에러 처리 정책

`Result` 를 무음 무시하면 실패가 흔적 없이 사라져 회귀 추적이 불가능해진다. **모든 `Result` 는 처리하거나 로그를 남긴다.** 강제 정책은 [`../../CLAUDE.md`](../../CLAUDE.md) "에러 처리".

## 원칙

`Result` 를 `let _ =` 로 무시하지 않는다. 무시는 *명시적 정책 결정* 이어야 하고 그 이유가 주석으로 남는다. 제품 코드에서 lint가 발동하는 문장이면 문장 단위 `expect`의 `reason`으로도 남는다([의도적 무시](#의도적-무시)).

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

기준은 그 문장에서 `let_underscore_must_use`(warn)가 실제로 발동하는가다. 이 lint는 주로 두
경우에 발동한다. 버리는 값의 타입이 must-use인 경우(`Result`, 그것을 담은 튜플·참조 등)와,
오른쪽 식 자체가 `#[must_use]`가 붙은 함수·메서드 호출인 경우다. 예를 들어 `let _ = f().is_ok();`는
버리는 값이 `bool`이지만 `Result::is_ok`가 `#[must_use]`라 진단된다. 이 둘은 요약이고 판정은
Clippy가 한다. 애매하면 그 문장을 clippy로 확인한다.

일부 빌드 조합(gui·headless·OS)에서만 발동하면 그 조건에만 기대를 건다
(`#[cfg_attr(<조건>, expect(clippy::let_underscore_must_use, reason = "..."))]`). 이 속성은
rustfmt가 여러 줄로 펼치기 쉽고, 윗줄 사유 주석 탐색은 여러 줄 속성의 안쪽 줄에서 멈춘다.
그래서 이때 사유 주석은 속성 위가 아니라 `let _ =`와 같은 줄이나 속성이 닫힌 뒤 문장 바로 위에 둔다.

- 발동하는 제품 코드 문장에는 주석과 함께 문장 단위
  `#[expect(clippy::let_underscore_must_use, reason = "...")]`를 단다. 주석만 있으면 lint가
  발동하고, CI clippy 단계와 pre-push B.4는 `-D warnings`로 돌아 그 경고에서 실패한다.
- 발동하지 않는 문장(`path`가 `&str`인 `let _ = path;`처럼 lint가 발동하지 않는 문장)에는
  주석만 단다. 발동하지 않는 자리에 `expect`를 달면
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
그 동작을 확인한 제품 코드 문장 중 lint가 발동하는 문장에는
`#[expect(clippy::let_underscore_must_use, reason = "...")]`로 사유를 고정한다. 실제 오류를 숨기기 위한 함수·크레이트 전체 면제는 추가하지 않는다.
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
| 같은 줄 | `#[expect(clippy::let_underscore_must_use, reason = "Exit leaves no caller.")]`<br>`let _ = out.flush(); // 직후 exit — 전달할 호출자가 없다.` |
| 바로 윗줄 (빈 줄과 `#[`·`#![`(`#` 뒤에 공백을 둔 `# [`·`# ! [` 포함)로 시작하는 속성 줄은 사이에 있어도 된다. 여러 줄 속성은 안쪽 줄에서 탐색이 멈추므로 그때는 속성이 닫힌 뒤 문장 바로 위나 같은 줄에 쓴다) | `// 이유`<br>`let _ = f();` |
| 문장 범위 안 또는 바로 다음 줄 | 멀티라인 호출의 인자 사이, rustfmt 가 trailing 주석을 밀어낸 다음 줄 |

표는 사유 주석의 위치만 다룬다. lint가 발동하는 제품 코드 문장에는 [의도적 무시](#의도적-무시)대로
문장 단위 `expect`도 단다.

**블록 상단에 한 번 적은 설명은 사유로 인정하지 않는다.** 몇 줄 위의 주석을 인정하려면
"어디까지 거슬러 올라가는가" 를 정해야 하는데, 함수 doc 주석까지 닿을 만큼 넓히면 사실상
모든 `let _ =` 가 통과해 사유와 무관한 주석으로 통과할 수 있다. 위에 이미 적었더라도 한 줄을 더 적는다.

전수 강제는 [`crates/tasty-doc-guards/tests/let_underscore_documented.rs`](../../crates/tasty-doc-guards/tests/let_underscore_documented.rs)
가 한다(`cargo test --workspace`). 텍스트 스캔은 타입을 알 수 없으므로 규칙은 `Result` 만이
아니라 **모든 `let _ =`** 에 적용된다 — 변수 바인딩 억제(`let _ = path;`)도 "왜 여기서 안
쓰는가" 가 궁금한 지점이라 한 줄 주석이 손해가 아니다.

테스트 코드는 대상이 아니다 — 경로 성분에 `tests`·`benches`가 든 파일(cargo 규약 디렉토리와
소스 트리 안의 하위 `tests` 디렉토리), 머리에 `#![cfg(test)]`·`#![cfg(all(test, unix))]`처럼
test를 요구하는 cfg(한 줄로 쓴 `cfg(test)`, 또는 `all(..)` 을 따라 `test` 가 나오는 cfg. `any(..)` 안의
`test` 는 테스트 필수 조건으로 인정하지 않는다)를 단 파일 전체(파일 머리의 안쪽 속성만 파일 전체로 본다. 인라인 모듈이나 함수 몸체 `{` 바로 안의 `#![cfg(test)]` 는 그 모듈·함수만 덮는다), `#[test]` 또는 그런 `#[cfg(..)]`가 붙은 아이템·문장·필드·인자·match 팔(표식은 요소와 같은 줄에 써도, 여러 줄에 걸쳐 써도 된다). `#[tokio::test]` 같은 매크로 속성은 표식이 아니므로
그런 함수는 이미 테스트 범위인 곳에 두거나 함수에 `#[cfg(test)]` 를 함께 단다. 가드는 이
범위를 텍스트로 근사한다 — 주석·리터럴을 가린 사본에서 속성을 괄호 짝으로 읽고, 표식이 붙은 요소의 끝을
감싸는 괄호의 종류로 정한다(아이템·문장은 `;` 나 뒤로 이어지지 않는 블록 `}`, 필드·인자·match 팔은 `,`).
따로 떨어진 모듈 파일은 그 파일의 경로와 표식으로 판정한다(부모의 `#[cfg(test)] mod x;` 선언은 따라가지 않고, 그 선언 뒤의 아이템은 테스트 범위가 아니다).
매크로 입력 안의 아이템은 일반 코드처럼 읽고, 아이템이 아닌 요소(식·문장·팔)의 테스트 표식과 면제는 그 속성만 덮는다(`(`·`[` 매크로 안의 `#![..]` 는 무시한다). 매크로 입력 안 아이템이라도 최상위 `$` 뒤에 다음 아이템이나 `let` 이 이어져 경계를 정할 수 없으면(`fn a() $body fn b() { .. }`) 속성만 덮는다. 구조체 리터럴은 `{` 앞 이름으로 근사한다 — 대문자 이름·`Self` 뒤의 `{` 는 쉼표 목록으로 읽고, 소문자 이름 뒤라 리터럴일 수 있는 문장 그룹(`s { .. }`. `if let Foo { a } = h {`·`while let`·let 체인(`if let A = x && let Foo { .. } = y {`)처럼 `if`·`while` 조건 뒤의 몸체는 아니다) 안 요소의 테스트 표식과 면제는 속성만 덮는다. 속성은 `#` 뒤(안쪽 속성은 `!` 앞뒤)에 공백을 둔 `# [cfg(test)]` 도 읽는다.
정확한 표기는 판정 함수(`scopes`·`Code::element_end`)가 정본이다. 테스트에서 값을 버리는 것은 대개 의도가 자명하고, 여기까지 강제하면
통과시키기 위한 형식적 주석만 늘어난다. 정책이 지키려는 것은 **프로덕션에서 조용히
사라지는 실패**다.

테스트에서는 사유 주석을 강제하지 않지만, 버리는 값이 검증 결과인지 먼저 확인한다.
예를 들어 egui의 `FullOutput`을 버리면 검사하려던 레이아웃 결과도 함께 버릴 수 있다.
주석을 추가하는 것만으로 이런 검증 누락을 고칠 수는 없다.

`.githooks/pre-commit` C.6 은 커밋 전에 staged diff 의 **추가 줄**만 본다. 대상은 staged `.rs` 파일 중
작업 트리에 있는 것 전부이고 `vendor/` 만 건너뛴다. 가드와 달리 테스트 경로·테스트 아이템·test cfg 범위를
빼지 않고 가드의 `ALLOWLIST` 도 쓰지 않는다. 그래서 **테스트 코드에 새로 쓴 `let _ =` 에도 훅은 사유 주석을
요구한다.** 반대로 추가 줄만 보므로 사유 주석만 지우고 `let _ =` 줄은 그대로 둔 diff 는 훅을 통과한다
(바꾸지 않은 `let _ =` 줄은 diff 의 컨텍스트 줄이라 검사하지 않는다). 그 문장이 가드의 제품 검사 대상(테스트
경로·테스트 범위·`ALLOWLIST` 밖)이고 인정 위치에 사유 주석이 하나도 남지 않으면 가드에서 걸린다. 같은 줄 끝 주석처럼 인정되는 사유 주석이 남아 있으면 가드도 통과한다(위쪽 주석만 지운 diff). 테스트 경로·테스트 범위의 문장은 가드도 사유를 보지 않는다. 위쪽 주석을 찾을 때 빈 줄과 `#[` 로 시작하는 속성 줄을 건너뛰는 것은 **위 표와 같다**(훅은 붙여 쓴 `#[` 줄만 건너뛰고, 가드는 `#![` 줄과 `#` 뒤에 공백을 둔 속성 줄도 건너뛴다) —
`#[cfg(windows)]` 이 사유 주석과 문장 사이에 오는 것은 러스트 관용이고, 훅만 그것을 막으면
정상적인 코드까지 거절하게 된다.

주석 위치를 보는 범위도 다르다. 훅은 `#![..]` 줄을 건너뛰지 않고 거기서 멈춘다. 훅의 "다음 줄" 은
`let _ =` 가 시작된 줄의 바로 다음 diff 줄 하나다(바꾸지 않은 `-U1` 컨텍스트 줄도 다음 줄이 되고, 삭제 줄은
건너뛴다). 가드는 시작 줄부터 문장 끝(괄호 깊이가 0 으로 돌아오며 `;` 가 나오는 줄, 최대 80줄까지 세고
못 찾으면 시작 줄)까지와 그다음 한 줄을 본다. 한 줄 문장이면 두 "다음 줄" 이 같다. 여러 줄 문장이면 훅은
범위 안 둘째 줄이 독립 주석 줄일 때만 인정하고, 셋째 줄부터와 `);` 다음 줄로 밀린 주석(표 셋째 칸의
rustfmt 형태)은 거절한다. 같은 후보를 완전한 판정기 사본으로 판정할 때는 훅이 인정하는 주석 자리를
가드도 모두 인정하므로, 주석 위치 때문에 훅을 통과한 코드가 가드에서 떨어지지는 않는다. 사본 없이
정규식으로 판정하는 경로는 아래처럼 예외가 있다.

그리고 훅의 위쪽 탐색은 **판정기(`mask-source`)가 만든 사본**을 줄 번호로 읽는다. diff
컨텍스트(`-U<n>`)로는 경계가 생기고 그 경계 밖이 조용히 위반이 되기 때문이다. 사본이 전혀 없거나
그 파일의 사본이 없으면 바로 앞 diff 줄 하나만 보고 물러나므로 그때는 다시 표보다 좁아진다. 읽어 온
사본에 필요한 줄이 없을 때만 거기서 탐색을 끝내고 주석이 없는 것으로 본다.

두 검사 다 렉서가 아니라 텍스트 스캔이지만, **어휘 마스킹은 한다**. 가드는 `let _ =` 를 찾을 때
주석·문자열·문자 리터럴을 모두 덮은 사본을, 사유 주석을 찾을 때 리터럴만 덮은 사본을 쓴다(둘 다
파일 전체를 한 번에 마스킹하므로 여러 줄 문자열·블록 주석까지 구분한다). 훅은 판정기를 찾으면 staged
파일 전체를 임시 폴더에 꺼내 `mask-source --keep-comments` 로 리터럴만 덮은 사본을 만들고, 각 diff 줄에서
사본의 같은 줄을 읽은 뒤 `//` 이후만 주석으로 지운다. 그래서 블록 주석 안의 `/* 예: let _ = x; */` 는
훅에서 위반이 되고 가드에서는 아니다. 마스킹이 없으면 두 방향으로 틀렸다: 문장
안의 `//` 가 문자열 내용(URL 이 대표적)이어도 사유 주석으로 인정했고(미탐), 문자열 안의
금지 형태를 코드로 봤다(오탐). 두 오류의 원인이 하나였다.

**두 층의 정확도가 갈리는 방향은 "훅이 더 거칠다" 쪽이다.** 훅은 diff 줄만 보고, `#![` 에서 멈추고,
`let _ =` 바로 다음 줄 하나만 본다. 판정기를 찾지 못했거나 판정기 실행이 실패하면 경고를 내고 모든 줄을
한 줄 안에서 닫히는 문자열만 지우는 정규식으로 판정한다(임시 폴더를 만들지 못하면 훅 자체가 중단된다).
그 파일·줄의 사본만 없으면 경고 없이 그 줄을 정규식으로 판정한다. 정규식 경로에서는 다음 줄에서 닫히는
문자열 안의 `//`(`let _ = "https://..` 처럼)를 같은 줄 사유로 받아 가드가 잡는 자리를 통과시킬 수 있고,
여러 줄 문자열 안의 금지 형태를 위반으로 잡을 수 있다. 훅의 인식 범위는 더 좁으므로 전수 판정의 정본은 가드다.

가드 쪽 마스커는 `crates/tasty-doc-guards/src/source_text.rs` 한 벌이고 다른 스캔 가드도 그것을 쓴다 —
같은 규칙을 별도로 구현해 차이가 생기는 것을 피한다. 한계 전문은 가드 파일 머리 주석에 있다.

### `clippy::let_underscore_must_use` 는 무엇을 세는가

세 번째 층인 clippy lint 는 **위 규칙을 집행하지 않는다.** 타입과 호출 대상의 `#[must_use]`는
보지만 주석을 읽지 못하므로, 사유를 주석으로만 남긴 정상 코드에도 lint 가 발동할 수 있다. 문장 단위
`#[expect(clippy::let_underscore_must_use, reason = "...")]` 는 lint 가 발동하면 그 진단을 억제하고,
활성 `expect` 가 있는데 lint 가 발동하지 않으면 `unfulfilled_lint_expectations` 가 보고된다. 즉 이 lint 의 출력은 위반 목록이 아니라 **"그 빌드에서 lint 가 발동했는데 아직 억제되지
않은 자리의 명부"** 다(발동 기준은 [의도적 무시](#의도적-무시)). 아래 범위 면제로 테스트 자리를
덜어 내면 프로덕션의 명부가 된다. 그 명부는 그것대로 값이 있다 — 정기적으로 훑어 "이 무시가 아직도 옳은가" 를
묻는 자리이고, 타입을 아는 층이라 텍스트 스캔인 가드가 못 보는 것을 본다.

명부가 쓸모 있으려면 **정책이 사유 주석을 요구하지 않는 자리**가 거기 섞이면 안 된다.
테스트 본문(위 제외 범위)이 그렇다 — 그 경고가 명부에 섞이면 테스트가 늘 때마다 숫자만
흔든다. 그래서 테스트의 경고는 아래 범위 면제로 명부에서 뺀다. lint 레벨은 그대로 `warn` 이다 — 레벨을 낮추면
프로덕션에 새로 들어오는 자리까지 함께 사라진다.

**테스트 코드의 면제는 문장마다가 아니라 범위마다 단다.** 다는 자리는 셋이다:

- **크레이트 루트**(그 코드를 컴파일하는 타깃의 루트 소스 — lib·main·`src/bin/*.rs`)에
  `#![cfg_attr(test, allow(clippy::let_underscore_must_use))]`.
  `cfg_attr(test, ..)` 인 것이 핵심이다 — `cfg(test)` 가 켜진 빌드에서는 이 면제가 크레이트
  전체(제품 함수 포함)에 적용되지만, lib·bin 타깃의 일반 빌드는 `cfg(test)` 없이 컴파일되므로
  **lint 가 발동하고 위치별 `expect`·`allow` 로 억제되지 않은 프로덕션 자리는 명부에 오른다.** 실측: `tasty-cli` 에 이 줄을 넣어도 명부 자리
  33 이 33 그대로였고, `tasty-terminal` 에서는 테스트 자리 2 가 0 이 됐다.
- **파일 머리**. 테스트 전용 파일은 `#![allow(clippy::let_underscore_must_use)]`, 제품 코드와
  테스트가 함께 있는 파일은 `#![cfg_attr(test, allow(clippy::let_underscore_must_use))]`.
  경로 성분에 `tests`·`benches`가 든 파일(통합 테스트 타깃 — 각자 별개 크레이트다 — 과
  `tests/common/mod.rs` 같은 공유 모듈, 소스 트리 안의 하위 `tests` 디렉토리 아래 모듈)은 `let _ =`가 있으면
  파일마다 이 줄을 단다(그 자리를 감싸는 아이템의 `#[allow(..)]` 로 덮어도 가드는 인정한다). 그런 경로 밖에서 부모가 `#[cfg(test)] mod x;` 로 선언한 모듈 파일은 그 선언만으로 가드의
  테스트 범위가 되지 않는다. 그 파일 안의 표식(`#[test]`·test cfg)으로도 테스트 범위에 들지 않는
  자리(표식 없는 헬퍼)에 `let _ =` 를 두면 가드가 사유 주석을 요구하므로, 파일 전체를 테스트 범위로 두려면 머리에 `#![cfg(test)]` 를 독립된 한 줄로 단다.
  부모의 OS·feature 조건은 부모 선언에 그대로 두고, 파일에도 필요하면 별도의 `#![cfg(..)]` 줄로 더한다.
- **테스트 아이템**(test를 요구하는 `#[cfg(..)]`를 단 모듈·함수·블록, `#[test]` 함수)의 속성 묶음에
  `#[allow(clippy::let_underscore_must_use)]`.

세 표기 모두 같은 줄이나 바로 위에 사유 주석(`// 이유: ...` 또는 `// reason: ...`)을 단다 —
`scripts/check-allow-reason.sh` 의 상한 래칫은 `crates/`·`src/` 아래 파일에서 `#[allow(`·`#![allow(`
줄과 `allow(` 를 같은 줄에 담은 `#[cfg_attr(`·`#![cfg_attr(` 줄 가운데 사유를 찾지 못한 줄을 센다. 아이템 면제의 사유 주석은 `allow` 와 같은 줄에
두거나, `allow` 바로 위에 둔다. 가드는 속성 사이의 주석·빈 줄을 건너뛰어 하나의 묶음으로 보므로
`#[cfg(test)]`·`// 사유`·`#[allow(..)]` 순서도 아이템 면제로 인정한다. 사유를 묶음 맨 위에 두고 `allow` 를
그 아래 다른 속성 뒤에 두면 래칫이 바로 윗줄의 속성(`#[cfg(test)]` 등)에서 멈춰 사유를 찾지 못한다.

제품 코드에는 이 면제를 달지 않는다. 제품 문장에 문장 단위 `expect`가 필요한지는 Clippy
진단으로 정한다([의도적 무시](#의도적-무시)). 반대로 가드가 테스트 범위에서 찾은 `let _ =`에는
값의 타입과 관계없이 범위 면제 표기가 필요하다 — 텍스트 스캔은 타입을 모르기 때문이다.

`test_scope_stays_out_of_the_lint_roster`
(`crates/tasty-doc-guards/tests/let_underscore_documented.rs`)가 테스트 범위의
`let _ =`에 면제 표기가 있는지 확인하며 사유 주석 검사와 같은 스캐너를 쓴다. 텍스트 스캔이라
면제 표기를 문자열로 찾고 범위를 괄호 짝으로 근사하므로 Clippy가 실제로 면제하는 범위와
어긋날 수 있다. 어긋날 때는 테스트 범위를 좁히는 쪽, 곧 그 문장을 제품 코드로 보고 사유 주석을 요구하는 쪽으로 기운다(표식이 붙은 요소를 끝까지 읽지 못하면, 곧 어느 갈래에서든 블록 식의 본문이나 괄호 짝·클로저 매개변수의 끝을 못 찾으면, 이미 읽은 앞부분도 범위에 넣지 않고 속성만 덮는다). 매크로 입력(`name!(..)`·`name! {..}`·`macro_rules!` 본문)은 Rust 문법이 아닌 토큰 열일 수 있어, 그 안에서 아이템이 아닌 요소(식·문장·팔)에 붙은 바깥 속성(`#[..]`)의 테스트 표식과 면제는 그 속성이 든 속성 묶음의 시작부터 그 속성의 끝까지만 덮는다. 매크로 안의 아이템(`proptest! { #[test] fn .. }`)은 일반 규칙으로 읽는다. 매크로 입력 안에서 최상위 `$` 때문에 경계가 불확실한 아이템과, 소문자 이름의 구조체 리터럴일 수 있는 그룹 안 요소도 속성만 덮는다. 판정 범위 가운데 이렇게 좁히는 것은 테스트 표식과 `allow` 뿐이고, `warn`·`deny`·`forbid` 는 다시 켠 진단을 놓치지 않도록 일반 규칙대로 요소 전체를 덮는다(안내에만 쓰는 `expect` 의 범위도 같은 조건에서 좁힌다). 같은 쪽으로 기우는 지원 한계로, 요소의 끝을 찾는 과정에서 구조체 패턴 클로저 매개변수(`|S { a }: S|`)를 직접 읽어야 하는 표식 요소(예: `#[cfg(test)] match |S { a }: S| { a } { .. }`)는 매개변수의 끝을 못 찾아 속성만 덮으므로 그 안의 테스트 자리가 제품 자리로 보고된다(보이는 실패). `#[test]` 함수 몸체 안의 그런 클로저는 몸체를 괄호 짝으로 건너뛰므로 함수의 테스트 범위가 유지된다. 안쪽 속성 `#![cfg(test)]`·`#![allow(..)]` 는 매크로 입력 안에서도 일반 규칙대로 그 속성을 직접 감싼 중괄호 그룹을 덮고, 그 그룹이 함수 몸체이면 시그니처를 포함한 함수 전체를 덮는다(직접 감싼 괄호가 `(`·`[` 이면 무시한다). 정확한 판정은 그 파일의 판정 함수가 정본이다. 가드는 Clippy 처럼 그 `let _ =`를
감싸는 모든 층의 lint 수준을 본다 — 파일 머리의 `#![..]`, 감싸는 블록·모듈·함수 안쪽의 `#![..]`,
감싸는 아이템·문장의 속성 묶음(테스트 표식이 없는 아이템 포함). 그래서 `#[cfg(test)]` 모듈에 단 `allow` 는 안쪽
`#[test]` 함수의 자리도 덮고, 위에서 권한 세 자리 밖의 `allow`(문장 하나에 단 것 등)도 그 범위만큼 인정한다.
가장 안쪽 층이 정하므로 안쪽의 `#[warn(clippy::let_underscore_must_use)]`·`deny`·`forbid` 는 바깥 `allow` 를
다시 켠다. `forbid` 는 그 안쪽의 `allow` 로도 풀 수 없으므로(E0453) 가드도 `forbid` 안쪽 자리를 면제로 보지 않는다 — 같은 `cfg_attr` 안에서 `forbid` 뒤에 둔 `allow` 와 크레이트 루트의 `forbid` 도 같다. `cfg_attr(<조건>, allow(..))` 는 조건이 테스트 빌드에서 거짓으로 정해질 때(`not(test)`, 그것을
담은 `all(..)`)만 무시하고, OS·feature 조건은 알 수 없어 적용되는 것으로 본다. 경로 성분에 `tests`·`benches`가
든 파일은 남은 자리를 그대로 보고한다. 그 밖의 파일은 테스트 범위 자리가 있으면 크레이트 루트 후보의 머리 수준을 읽는다(파일 안에서 면제한 자리에도 루트의 `forbid` 는 적용되기 때문이다). 루트 면제는 수준 표기가 없는 자리에만 적용한다 — 안쪽에서 다시 켠 자리는 루트 `allow` 가 있어도 남는다. 가드는 `warn`·`deny` 로 다시 켠 자리에는 그 속성보다 안쪽에 면제를 달라고, `forbid` 아래 자리에는 `forbid` 를 `deny` 로 바꾸라고 따로 보고한다. `expect(clippy::let_underscore_must_use)` 는 면제로 보지 않는다 — 그것만 덮은 자리는 면제 없는 자리로 보고하고, `allow` 를 따로 더하지 말고 그 `expect` 를 `allow` 로 바꾸라고 안내한다(따로 더하면 `expect` 가 남는다). 면제 없이 보고할 테스트 자리를 덮는 파일 안 `expect` 가 없고 크레이트 루트 후보의 머리에서 찾았으면, 그 후보가 이 파일을 컴파일하는지 따지지 않으므로 그 자리를 덮는다고 하지 않고 그 후보에 `expect` 가 있다고 알린다. 활성 `expect` 는 그것이 직접 억제한 발동이 없으면 미충족 기대 경고(`unfulfilled_lint_expectations`)를 내고, 이 프로젝트의 clippy 는 `-D warnings` 로 돌아 그 경고가 실패가 된다. 가드는 타입과 관계없이 테스트 범위의 모든 `let _` 에 면제를 요구하므로 lint 가 발동하지 않는 자리에도 면제가 필요하다. 루트 후보는 `crates/<이름>/` 아래 파일이면 그 디렉토리의
`src/lib.rs`·`src/main.rs`, 그 밖의 파일이면 저장소 루트의 `src/lib.rs`·`src/main.rs`뿐이고, 가드는
후보 중 한 곳의 표기만 있어도 인정한다(어느 타깃이 그 모듈을 컴파일하는지는 따지지 않는다). 후보가 면제와 `forbid` 로 갈리면 어느 쪽이 적용되는지 정할 수 없으므로 판정하지 않고 따로 보고한다. 그때는 `forbid` 루트를 `deny` 로 바꾸고 그 파일 머리나 감싸는 아이템에 면제를 단다(파일 안에서 `warn`·`deny` 로 다시 켠 자리는 그 속성보다 안쪽에 단다) — 바꾼 뒤 가드는 다른 후보의 면제로도 통과시키지만, 실제로 그 파일을 컴파일하는 루트에 면제가 있다는 보장은 없다. 루트 면제를 쓸 때 Clippy 에서 적용되려면 그 모듈을 실제로
컴파일하는 타깃의 루트에 달아야 한다(`main.rs` 전용 모듈이면 `main.rs`). 그래서 별도 바이너리
(`src/bin/*`)에 딸린 모듈은 파일 머리나 아이템 면제로 덮는다.

새 테스트를 막지는 않는다: 가드가 이미 면제를 인정하는 범위 안에서는 아무것도 요구하지
않고, 아직 인정하는 면제가 없는 테스트 범위에서 처음 `let _ =` 를 쓸 때만 **면제 속성 한 줄**을
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
- 예외: `local/attach.rs` raw bridge 는 `crate::out` 대신 `std::io::stdout()` 핸들에 직접 쓴다.
  오류 처리는 위 규칙과 같은 방향이다. 화면 데이터의 write·flush 가 `BrokenPipe` 면 세션을 정상 완료로
  끝내 `StdoutClosed` 를 거치지 않고 종료 코드 0 이 되고, 화면 데이터 write·flush 의 그 밖의 오류는 호출자에게
  반환한다. 세션이 종료(`Completed`·`Disconnected`)로 끝나면 SGR 초기화를 쓰고, 그 실패는 `BrokenPipe` 면 넘기고
  그 밖이면 `tracing::warn!` 으로 남긴 뒤 원래 종료 결과를 돌려준다. 다른 오류로 반환하거나 재동기화로 끝나면 초기화를 건너뛴다.
  동작 정본은 [attach — CLI 출력 종료](attach-behavior.md#cli-출력-종료), 시험은 `attach.rs` 의
  `closed_stdout_finishes_attach_but_other_output_errors_are_returned` 와
  `a_closed_stdout_at_the_end_of_a_session_is_not_an_error` 다.

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

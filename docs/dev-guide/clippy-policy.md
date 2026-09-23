# 워크스페이스 lint 정책

tasty 의 lint 경고는 **워크스페이스 단위로 lint 를 끄기보다 위치별 `#[allow(...)]` / 소스 수정** 을 선호한다. `clippy.toml` 의 `disallowed-methods` 주석이 원칙을 명시한다 — *예외가 필요한 곳은 파일/함수 단위 `#[allow(...)]`*.

대부분이 clippy 지만 매니페스트는 rustc lint 도 같은 절에 든다. 두 벌을 갈라 두지 않는 이유는 **집행 경로가 같기 때문**이다 — 멤버가 `[lints] workspace = true` 로 상속하고, 같은 컴파일 단계에서 발동하고, 같은 `#[allow]` 로 빠져나간다. lint 이름을 어느 도구가 소유하는지는 답의 구현 세부이지 "무엇이 막혀 있나" 라는 물음의 축이 아니다.

## 현재 워크스페이스 설정

| 위치 | 설정 | 의미 |
|------|------|------|
| `Cargo.toml [workspace.lints.clippy]` | `undocumented_unsafe_blocks = "deny"` | 모든 unsafe 블록 위 `// SAFETY:` 강제 ([아래 unsafe 절](#unsafe--safety-주석)) |
| 〃 | `cognitive_complexity = "deny"` | 함수 cognitive 복잡도 상한 강제 — 복잡도 게이트의 함수 축이고, 임계값은 `clippy.toml` 이 든다 ([complexity-gate](complexity-gate.md)) |
| 〃 | `multiple_unsafe_ops_per_block = "deny"` | 블록당 unsafe op 2개+ 차단 — 면제는 위치 단위로 동결하고 **근거는 그 자리에 붙어 있다**. 형태는 자리의 성격이 정하므로 여기서 세지 않는다 |
| 〃 | `result_large_err = "allow"` | 도메인 Error enum ↔ IPC fault path 1:1 매핑이라 일괄 allow ([documentation-model](../documentation-model.md)) — 사유가 자리 수에 안 기대므로 이 셀은 수를 적지 않는다 |
| 〃 | `let_underscore_must_use = "warn"` | `let _ = <Result>` 무음 무시를 표면화 — **이 lint 는 정책을 집행하지 않는다**(주석을 못 읽어 사유가 달린 정상 코드도 warn 한다). 전수 판정은 `let_underscore_documented` 가드의 몫 ([error-handling](error-handling.md)) |
| 〃 | `disallowed_methods = "deny"` | 아래 `clippy.toml` 목록에 든 함수의 호출을 막는 **레벨**. warn 이던 시절 위반이 조용히 유입된 전례로 승격했다 |
| 〃 | `missing_safety_doc = "deny"` | 모든 `pub unsafe fn` 에 `# Safety` 절 강제 — `undocumented_unsafe_blocks` 의 블록 축과 대칭인 **호출자 계약** 축 ([아래 unsafe 절](#unsafe--safety-주석)) |
| `Cargo.toml [workspace.lints.rust]` | `unsafe_op_in_unsafe_fn = "deny"` | `unsafe fn` 안에서도 명시 unsafe 블록 강제. edition 2024 의 기본값이지만 워크스페이스 일관성을 위해 못박는다 |
| 〃 | `function_casts_as_integer = "deny"` | `f as usize` 금지 → `f as *const () as usize` |
| 〃 | `unused_assignments = "deny"` | dead store 금지 → 값 합성(`.max` 등) 또는 제거 |
| 〃 | `unused_must_use = "deny"` | Result/must_use 무음 무시 금지 → 처리 또는 `tracing` 로그 ([error-handling](error-handling.md)) |
| 〃 | `dead_code = "deny"` | 죽은 코드 금지 → 삭제 또는 위치 단위 `#[allow(dead_code)]` + 사유 |

이 표의 lint 행들은 루트 `Cargo.toml` 의 `[workspace.lints]` 를 손으로 베낀 사본이다 — 매니페스트만 고치면 아무 채널도 안 운다(컴파일에 먹는 것은 매니페스트뿐이다). 그래서 정합을 `crates/tasty-doc-guards/tests/workspace_lint_table_matches_the_manifest.rs` 가 **양방향으로** 강제한다: 표에만 있는 lint 도, 매니페스트에만 있는 lint 도 위반이다. 그 타깃은 `doc-guards.yml` 이 main push · PR 마다 돌린다. 자동 잡은 push 된 커밋만 보므로 **커밋 전에 직접 돌리면 그 자리에서 잡힌다**([ci-gates](ci-gates.md)). 그 표에는 **모수 밖 행이 없다** — 매니페스트 절을 안 가리키는 행이 섞이면 그 자체가 위반이다. 레벨이 아닌 설정값은 아래 소절이 든다.

### `clippy.toml` — 레벨이 아니라 설정값

`[workspace.lints]` 가 정하는 것은 **레벨**이고, `clippy.toml` 이 정하는 것은 그 레벨이 무는 **대상과 임계값**이다. 이름이 겹쳐 보이는 자리가 있어 갈라 적는다 — 위 표의 `disallowed_methods`(밑줄)는 **레벨**이고, 아래 `disallowed-methods`(하이픈)는 그 레벨이 무는 **목록**이다. 서로 다른 것이고 한쪽만 있어도 다른 쪽은 아무 일도 안 한다.

- `disallowed-methods` — 색을 "디자인" 하는 함수의 목록. 우연한 호출을 차단한다 ([theme › 색 생성 정책](../design/systems/theme.md#색-생성-정책)). 옛 pre-commit C.8 이 이리로 이관됐다.
- `cognitive-complexity-threshold` — `cognitive_complexity` 가 발동하는 임계값. 복잡도 게이트의 함수 축이 쓴다 ([complexity-gate](complexity-gate.md)).

clippy 내장 threshold config(`type-complexity-threshold`, `too-many-arguments-threshold` 등)는 **위반을 봐주려고 느슨하게 풀지 않는다**(원칙 #3) — 위반은 소스 개선(struct/type alias) 또는 위치별 attr 로 처리한다. 이는 threshold 를 *올려 신규 위반까지 은폐하는* 것을 금하는 것이며, 복잡도 상한 강제 자체를 배제하지 않는다. 복잡도 상한은 별도의 **복잡도 게이트**(아래)가 담당한다 — 두 축은 독립이다.

## 복잡도 게이트

함수 cognitive 복잡도(clippy 내장 `cognitive_complexity` = `deny`, 임계 20)와 파일 SLOC(`tokei` + `scripts/check-file-size.sh`, 상한 1000)의 **신규/증가분**을 차단한다. **두 축은 강제 채널이 다르다** — cognitive 는 clippy `deny` 라 자동 잡의 컴파일 단계에서 막히고, 파일 SLOC 은 전용 워크플로(`complexity-check.yml`)가 담당한다 — **그 워크플로의 트리거와 실제 발사 여부는 시점마다 다르므로 여기 적지 않는다.** 판정이 필요하면 [ci-gates](ci-gates.md) 의 "트리거는 어느 ref 의 것인가" 를 보고, 거기 적힌 세 명령을 그 자리에서 다시 돌려라. 기존 초과분은 위치 단위로 동결한다 — 함수는 `#[allow(clippy::cognitive_complexity)] // complexity-exempt: <사유>`, 파일은 `.complexity-file-allowlist`. 여기서 threshold 를 *조이는* 것(cognitive 게이트 신설)은 위 원칙 #3(threshold 를 *푸는* 것 금지)과 모순이 아니라 계층 분리다. 상세: [complexity-gate.md](complexity-gate.md), [ADR-0037](../adr/0037-complexity-gate.md).

## 결정 원칙

1. **소스 수정이 본질 개선이면 소스를 고친다** — `derivable_impls`(manual Default→derive), `too_many_arguments`(인자→context struct), `should_implement_trait`(inherent `from_str`→`FromStr`).
2. **의도된 패턴이면 모듈/위치 단위 `#[allow]`** — 예: `intent` 의 `from_*` 메서드는 `From` 변환이 아니라 *intent dispatch source 부착* 의미라 `#![allow(clippy::wrong_self_convention)]`. 테스트 fixture 의 `type_complexity` 도 모듈 allow.
3. **워크스페이스 단위로 끄지 않는다** — 신규 코드의 정당한 위반까지 묻히기 때문. threshold 조정도 우회라 지양.

위치별 처리 비용이 과해지면(예: 비영어 doc 의 `doc_lazy_continuation` 빈발) 워크스페이스 allow 재검토.

## unsafe — SAFETY 주석

워크스페이스 lint 가 모든 `unsafe` 블록에 `// SAFETY:` 주석을 강제한다(`undocumented_unsafe_blocks = "deny"`). 본 문서는 그 주석을 무엇으로 채울지, 새 unsafe 전 무엇을 자문할지 정의한다.

### 형식

```rust
// SAFETY: <한 줄 요약 — "왜 안전한가">
// - <invariant 1>: 누가/어디서 보장하는지
// - <invariant 2>: 호출자 책임 / OS 문서 참조
unsafe { ... }
```

형식적이면 안 된다. **무엇이 안전을 보장하는지가 핵심** — 검토자가 SAFETY 주석만 보고 동의할 수 있어야 한다.

### 자가검토 7문

1. **OS thread affinity 가 있는가?** AppKit·Win32 GUI = main thread only. Xlib `Display*` = `XInitThreads()` 없이는 not thread-safe. POSIX 기본 syscall = thread-safe.
2. **raw pointer lifetime 이 어디까지 보장되나?** 호출 끝까지 / 함수 / `'static`? 누가 free/release 책임?
3. **이 FFI 가 panic safe 한가?** panic 시 invariant(lock 미해제 등) 깨지는가?
4. **Drop 순서가 의존성을 만족하나?** (예: webview controller 가 hwnd 보다 먼저 drop)
5. **같은 함수의 unsafe 블록 2개+ 를 합쳐도 되나?** (`multiple_unsafe_ops_per_block` lint 가 분할 여부를 가린다)
6. **NULL 이 오류인가 정상 반환값인가?** 둘은 다르다. 정상 반환값이면 그것을 `Result`/`Option` 으로 받아야 하고, **NULL 에서 `assert!` 하는 래퍼를 거치면 안 된다** — 그 래퍼는 "값이 없다" 를 호출자에게 알리는 유일한 경로를 프로세스 즉사로 바꾼다. glib 계열 바인딩(`from_glib_full`)이 이 형태다: 안에 `assert!(!ptr.is_null())` 가 있어, NULL 을 정상 반환하는 C 함수를 감싸면 그 자리가 즉사 지점이 된다. 그럴 때는 `*-sys` 의 원 함수를 직접 불러 NULL 을 값으로 받는다 (`crates/tasty-platform/src/x11_gdk_window.rs`).
7. **연결이 하나인가?** 같은 X 서버라도 **연결이 다르면 요청 순서가 보장되지 않는다.** 한 연결에서 만든 자원을 다른 연결에서 조회하기 전에는 `XFlush`(보내기)가 아니라 **`XSync`(왕복)** 가 필요하다 — `XFlush` 는 서버가 **처리했는지**를 안 기다리므로, 조회가 생성을 앞질러 "그런 자원 없다" 를 받는다. winit(창 생성)과 GDK/GTK(조회)는 서로 다른 연결이다.

### 의심스러우면 `unsafe fn`

caller 에게 안전성 책임을 명시적으로 넘기는 게 깔끔하다. `unsafe fn` 내부 op 도 명시적 unsafe 블록 강제(`unsafe_op_in_unsafe_fn = "deny"`).

```rust
/// # Safety
/// 호출자는 클립보드가 열린 상태에서 호출해야 한다.
unsafe fn read_clipboard_inner() -> Option<...> {
    // SAFETY: 호출자가 OpenClipboard 성공을 보증 (function-level Safety doc 참조).
    let handle = unsafe { GetClipboardData(...) };
}
```

### SAFETY 주석 예시

```rust
// macOS — main thread 강제
// SAFETY: setAction 은 AppKit main thread only. NSApplicationDelegate 콜백 시그니처로
// ObjC 런타임이 main thread 에서만 호출.
unsafe { item.setAction(Some(sel!(tastyNewWindow:))) };

// Win32 — open/close 시퀀스
// SAFETY: OpenClipboard → SetClipboardData* → CloseClipboard 를 한 함수 안에서 완결.
// SetClipboardData 성공 시 HGLOBAL 소유권 OS 이전, 모든 분기에서 CloseClipboard.
unsafe { OpenClipboard(None)?; ...; CloseClipboard()?; }

// Xlib — Display 단일 thread
// SAFETY: PlatformWebView 는 main thread(winit event loop)에서만 생성/조작.
// XInitThreads 미호출 환경이라 Display* 호출은 main thread 한정.
unsafe { (xlib.XMapWindow)(display, x11_window); }
```

### 새 unsafe 절차

1. 정말 필요한지 재검토(`bytemuck::cast_slice`, `std::mem::take`, `pin-project` 등 safe 대안).
2. 7문에 답한다 → SAFETY 주석으로 작성.
3. `cargo clippy --workspace --all-targets` 통과 확인.
4. OS 의존이면 `docs/dev-guide/` 관련 문서에도 invariant 를 적는다.

### lint 정책

| lint | 수준 |
|------|------|
| `clippy::undocumented_unsafe_blocks` | `deny` (모든 unsafe 블록 SAFETY 필수) |
| `clippy::multiple_unsafe_ops_per_block` | `deny` |
| `rust::unsafe_op_in_unsafe_fn` | `deny` (edition 2024 기본) |

리뷰어는 코드보다 **SAFETY 본문(invariant) 검증을 우선** 한다. OS 문서 링크가 있으면 SAFETY 에 포함(장기 변동 대응).

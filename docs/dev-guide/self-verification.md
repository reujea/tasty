# 수정 후 자체 검증 (Self-verification)

수정이 직접 확인 가능한 종류라면 **사용자에게 "확인해 보세요" 라고 떠넘기지 않고 본인이, 커밋 전에 확인한다.**

커밋해두고 검증을 떠넘기면 회귀가 나올 때마다 수정→커밋→재검증 사이클이 반복되고 히스토리에 "feat→fix→fix" 가 쌓인다. 사용자의 시간을 낭비하는 일이다.

## 원칙

1. **검증 가능 여부를 먼저 판단.** IPC/CLI/스크립트로 트리거 가능하면 검증 가능. 마우스 hover 같은 *사용자 입력이 있어야만 재현되는* 케이스만 사용자에게 부탁한다.
2. **검증은 커밋 전.** 빌드 통과 + 단위 테스트 통과는 *컴파일된다* 는 의미일 뿐 *기능이 동작한다* 는 보장이 아니다 — 별도 시나리오 재현이 필요.
3. **확인 안 됐으면 "확인 안 됨" 이라고 말한다.** 추측으로 "동작할 거예요" 보고 금지.
4. **검증 인스턴스는 자동 격리된다.** debug 빌드(`cargo run` / `target/debug/tasty`)는 `~/.tasty-debug/` 루트를, release 사용자 세션은 `~/.tasty/` 를 쓴다 — 따로 끄거나 환경변수를 줄 필요 없이 `cargo run` 만으로 release 인스턴스와 충돌하지 않는다. debug 인스턴스끼리는 `TASTY_HOME` 으로 루트를 나눈다. (상세 [아래 절](#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다))

### 독립 검증 — 개발도 Agent 가 스스로 확인할 수 있어야 한다

> dev-guide 의 **가장 핵심 원칙**. Tasty 정체성인 *동시성*([identity.md](../identity.md))이 **개발 환경 자체에 재귀적으로 적용된 것** 이다.

#### 원칙

**Tasty 의 모든 기능은, 그것을 개발하는 AI Agent 가 자기 수정을 독립적으로 띄워 검증할 수 있도록 만들어야 한다.**

Tasty 를 개발하는 환경이 곧 Tasty 다 (dogfooding). 보통 사용자·다른 Agent 는 **release** 빌드를 띄워 작업 중이다. 그 위에서 Agent 가 자기가 고친 것을 확인하려면 **debug** 빌드를 동시에 띄워야 하는데, 둘이 같은 자원(포트·상태 파일)을 공유하면 서로 간섭한다.

그래서 Tasty 는 **debug 빌드와 release 빌드의 환경을 격리** 한다 (0% 충돌은 불가능하지만 최대한). 덕분에 Agent 는 *자신이 release tasty 안에서 동작 중이어도*, 자기 debug 빌드를 따로 띄워 release(= 사용자·다른 작업)와 충돌 없이 검증할 수 있다.

> **Agent 는 이것을 반드시 인지한다**: 내가 tasty 안에서 돌고 있어도, 내 수정은 *별도 debug 인스턴스* 로 띄워 검증한다. 돌아가는 release 를 건드리지 않는다.

#### debug ↔ release 격리 (현재 구현)

> 이 표가 격리 경로의 **산문 SoT** 다. 다른 문서는 이 표를 풀-복제하지 말고 "debug 는 별도 루트(`~/.tasty-debug/`)로 격리됨" + 이 문서 링크로 참조한다.

**데이터 루트 자체가 갈린다** — debug 빌드는 `~/.tasty-debug/`, release 는 `~/.tasty/` 를 쓴다. 같은 디렉토리 안의 파일명 접미사가 아니라 **루트 디렉토리 분리**라, 포트·layout·scrollback·state.db·memory.db·plugins 가 통째로 별도다:

| 자원 | release (`~/.tasty/`) | debug (`~/.tasty-debug/`) |
|------|---------|-------|
| IPC 포트 파일 | `~/.tasty/tasty.port` | `~/.tasty-debug/tasty.port` |
| scrollback | `~/.tasty/scrollback/` | `~/.tasty-debug/scrollback/` |
| layout | `~/.tasty/layouts/NN.json` | `~/.tasty-debug/layouts/NN.json` |

- 파일명은 양쪽 모두 동일(`tasty.port` 등) — 구분은 **루트** 가 한다. (`-debug` 파일명 접미사는 쓰지 않는다.)
- `target/debug/tasty` (debug 바이너리)는 `~/.tasty-debug/` 루트를 읽으므로 CLI 조작이 **debug 인스턴스에만** 간다. 사용자의 release 인스턴스는 건드리지 않는다.
- **`TASTY_HOME` env override**: 비어있지 않으면 그 경로를 루트로 강제한다(테스트/샌드박스/다중 인스턴스용) — debug/release 자동 분기보다 우선.
- 구현(SoT): `crates/tasty-utils/src/path.rs` (`tasty_home()` — `TASTY_HOME` 우선, 없으면 `cfg!(debug_assertions)`→`.tasty-debug`), `crates/tasty-ipc/src/port_file.rs`, `src/store/scrollback.rs`, `src/core/layout_persistence.rs`.

#### 새 기능 추가 시 적용

- 영속 상태(파일/소켓/포트 등)를 새로 추가하면 **debug/release 분리 패턴을 따른다** (`tasty_home()` 루트 아래에 둔다). 안 그러면 debug 검증이 release 데이터를 오염시킨다.
- 동작은 IPC/CLI 로 트리거 가능하게 만든다 (headless 동작-우선) — 그래야 Agent 가 GUI 없이 검증한다. → [identity.md](../identity.md) §2.2.

#### 한계 / 주의

- 격리는 **tasty 의 상태**(루트 디렉토리)만 가른다. OS 열기(브라우저 · 파일 관리자)는 사용자 데스크톱의 이미 떠 있는 브라우저에 닿는다 — 검증 인스턴스는 `TASTY_DEBUG_OS_OPEN_LOG` 와 가짜 브라우저로 띄운다([아래 "tasty 에서 직접 검증"](#tasty-에서-직접-검증), [debug-ipc.md](debug-ipc.md)).
- 격리는 **debug ↔ release** 기준이다. 두 debug 인스턴스를 동시에 띄우면 같은 `~/.tasty-debug/` 루트(포트파일 `~/.tasty-debug/tasty.port`)를 공유하므로 충돌한다 — 이때는 `TASTY_HOME` 으로 루트를 분리하거나 별도 checkout/worktree 를 운용한다.

#### 관련

- [아래 "tasty 에서 직접 검증"](#tasty-에서-직접-검증) — 실제 검증 절차 (cargo run & + CLI 시나리오)
- [`debug-ipc.md`](debug-ipc.md) — debug 전용 IPC (사용자 입력 재현, release 미노출)
- [`e2e-tests.md`](e2e-tests.md) — 테스트 환경 격리 정책
- [identity.md](../identity.md) — 동시성 정체성 (이 원칙의 뿌리)

## tasty 에서 직접 검증

대부분의 동작은 tasty CLI 로 시나리오를 만들 수 있다.

```bash
cargo build                                              # 먼저 빌드한다 — `cargo run &` 을 쓰면
                                                         # $! 가 cargo 의 PID 라 kill 이 앱에 닿지 않는다
target/debug/tasty --launch &                            # 백그라운드 실행 (--launch 는 아래 문단 참고)
MY_APP=$!                                                # 띄운 즉시 PID 를 잡는다
until target/debug/tasty list info 2>/dev/null; do       # IPC 대기 (sleep 루프 금지, until 조건검사)
  kill -0 "$MY_APP" 2>/dev/null || { echo "기동 실패"; break; }   # 죽은 프로세스를 무한정 기다리지 않는다
  sleep 1
done

target/debug/tasty list surfaces                         # 상태 조회
target/debug/tasty list tree
target/debug/tasty send text "echo HELLO\r" --surface 2  # 시나리오 조작
target/debug/tasty read screen --surface 2               # 결과 확인
kill "$MY_APP"                                           # 종료 — 저장한 PID 로만
```

**`cargo test` 는 `target/debug/tasty` 를 만들지 않는다 — 라이브로 재기 전에 빌드 시각을 본다.**
`cargo test -p tasty --lib` 가 만드는 것은 **테스트 하네스**(`target/debug/deps/tasty-<해시>`)이고
하네스 이름이 실행 바이너리와 같은 줄기라 그것도 함께 갱신된 것처럼 보인다. 갱신되지 않는다.
(**`--bin tasty` 로 좁히지 마라** — 루트 패키지의 단위시험은 전부 lib 타깃에 살아서 그
조합은 0 건을 돌고 초록이 난다. 0 건은 통과가 아니라 미측정이다.)
그래서 코드를 고치고 테스트만 돌린 뒤 위 절차로 인스턴스를 띄우면 **직전 코드를 재게 되고,
그 오진은 양방향이다** — 고친 것이 안 고쳐진 것처럼도, 되돌린 것이 여전히 고쳐진 것처럼도
보인다. plugin 쪽에 이미 알려진 함정(`cargo build` 가 plugin 바이너리를 relink 하지 않는다,
`CLAUDE.md` "빌드")의 **본체 판**이고 성질이 같다. 라이브 프로브 전에 한 줄로 확인한다:

```bash
cargo build --locked --bin tasty                  # 라이브 검증용 바이너리는 이 명령이 만든다
ls -la --time-style=full-iso target/debug/tasty   # 방금 고친 시각보다 새것인지 눈으로 본다
```

**종료는 자기가 띄운 PID 로만 한다.** 이름이나 명령줄 패턴으로 찾아서 죽이면
**자기 것이 아닌 인스턴스까지 죽인다** — 사용자 release · 다른 검증 세션 · 병렬 lane 의
debug 인스턴스가 동시에 떠 있는 것이 이 레포의 일상이고, 실제로 그 형태가 다른 세션의
프로세스를 죽인 사고가 두 번 났다. 레포의 PreToolUse 훅도 같은 이유로 패턴 기반 프로세스
종료를 차단한다. PID 를 놓쳤을 때 소유자를 확인하는 방법은
[ai-verification/screenshot-methods](../ai-verification/screenshot-methods.md) "사용자 세션을
건드리지 않고 격리 실행".

**이미 다른 tasty 인스턴스(사용자의 release 등)가 떠 있어도 충돌하지 않는다.** `cargo run` 은 debug 빌드라 데이터 루트가 `~/.tasty-debug/`(포트파일 `~/.tasty-debug/tasty.port`)로 release 의 `~/.tasty/` 와 **완전히 분리**된다 — 포트·layout·scrollback 모두 별도. 그러니 release 인스턴스가 떠 있는지는 따지지 말고 `cargo run` 으로 자기 debug 인스턴스를 띄워 검증한다 — 다른 debug 인스턴스가 떠 있으면 `TASTY_HOME` 으로 루트를 나눈다. (격리 표·`TASTY_HOME` override: [위 "독립 검증"](#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다))

**격리 홈도 전용 디스플레이도 OS 열기를 격리하지 않는다 — 검증 인스턴스는 OS 열기를 기록만 하게 띄운다.** 디렉토리 dispatch · 링크 클릭 · "OS 기본 앱으로 열기" 는 `xdg-open`/브라우저를 부르고, 브라우저는 이미 떠 있는 자기 인스턴스에 URL 을 넘기는 원격 제어 채널(DBus · 소켓)을 가져서 `DISPLAY` 와 무관하게 **사용자 브라우저에 탭이 열린다**(격리 `TASTY_HOME` + 전용 Xvfb 로 띄운 인스턴스에서 실제로 났다). 그래서 두 겹으로 막는다:

```bash
SB=$(mktemp -d)                                   # 가짜 브라우저 자리
for n in xdg-open gio open sensible-browser x-www-browser firefox firefox-bin google-chrome chromium; do
  printf '#!/bin/sh\necho "%s $*" >> %s/fake-open.log\n' "$n" "$SB" > "$SB/$n"; chmod +x "$SB/$n"
done
TASTY_DEBUG_OS_OPEN_LOG="$SB/os-open.log" \
PATH="$SB:$PATH" BROWSER="$SB/firefox" \
  ./target/debug/tasty --launch &                 # 나머지(env -u … · PID 저장)는 위 절차 그대로
```

- **`TASTY_DEBUG_OS_OPEN_LOG`** — tasty 자신의 OS 열기 자리가 프로세스를 띄우지 않고 그 파일에 `<via>\t<대상>` 을 붙인다. "무엇이 열리려 했나" 의 판정은 이 줄로 한다. 덮는 자리와 성질은 [debug-ipc.md](debug-ipc.md) "OS 열기를 띄우지 않고 기록하기".
- **가짜 브라우저 `PATH` + `BROWSER`** — tasty 가 띄운 **다른 프로세스**(PTY 안 셸의 `xdg-open`, 스스로 OS 열기를 부르는 plugin — 번들 plugin 은 host 를 거쳐 위 스위치 안이다)는 위 스위치를 안 읽는다. 그쪽이 부르는 것을 기록만 하는 가짜로 가로챈다. 여기 기록이 생기면 그 프로세스가 연 것이다.
- **`BROWSER=`(빈 값)은 막지 않는다.** `webbrowser` 크레이트(1.2.4 소스)는 빈 항목을 건너뛰고 xdg 설정의 기본 브라우저 desktop entry 를 **직접** 실행한다 — `PATH` 앞의 가짜 `xdg-open` 도 거치지 않는다. `BROWSER` 에는 기록하는 가짜의 경로를 준다.

**`--headless` 는 기본 빌드에서 headless 로 동작하지 않는다.** 기본 빌드는 `gui` feature 가 켜져 있고 그 빌드에는 headless 모드가 들어 있지 않아, `--headless` 를 줘도 **GUI 로 폴백해 실제 창을 띄운다.** 로그에 이렇게 남는다:

```
--headless requested in gui build; gui build does not embed headless mode.
Build with --no-default-features to enable headless. Falling back to run_gui.
```

즉 `--headless` 만 믿고 "창은 안 뜬다" 고 가정하면 **세마포어 없이 공용 디스플레이에 창을 띄우게 된다**(실제로 그렇게 밟은 적이 있다). headless 검증에는 `cargo build --no-default-features` 로 만든 바이너리를 쓴다. GUI 가 떠도 되는 검증이면 폴백을 그대로 써도 되지만, 그때는 GUI 검증 규약(디스플레이 직렬화, 종료 확인)을 따른다.

#### 진짜 headless 로 검증하는 절차

위 문단은 함정만 적고 "그러면 무엇을 쓰나" 를 안 적었다. 아래가 그 절차다 —
**실측 2026-09-07 · x86_64 linux · 이 레포의 debug 프로필**.

```bash
cargo build --no-default-features          # 산출물은 target/debug/tasty — 기본 빌드와 같은 경로다
env -u TASTY_SURFACE_ID -u TASTY_SESSION_TOKEN -u TASTY_AGENT_ID -u TASTY_LOCALE \
    TASTY_HOME=<격리 홈> ./target/debug/tasty &
echo $! > <pid 파일>                        # 정리는 저장한 이 PID 로만 한다
```

- **빌드는 된다.** rc=0, 143 유닛 컴파일, 1m11s(이미 더운 `target/` 위에서). 산출물 `target/debug/tasty` 는 521,433,520 B(debug·비최적화·디버그정보 포함). `strings` 로 위 폴백 경고 문자열을 세면 **0 회** — 그것이 이 바이너리가 gui 빌드가 아니라는 확인이다.
- **`DISPLAY` 를 준 채로도 창이 안 뜬다.** 전용 Xvfb `:91` 을 띄우고 `DISPLAY=:91` 로 부팅한 뒤 `xwininfo -root -children` → `0 children`(부팅 전과 같다). 그 프로세스의 `/proc/<pid>/fd` 에 X11 소켓이 **0 개** — 연결 자체를 안 한다. 기본 빌드의 폴백과 갈리는 자리가 여기다.
- **★ 같은 경로를 두 feature 조합이 공유한다.** `cargo build` 와 `cargo build --no-default-features` 는 **둘 다 `target/debug/tasty` 를 쓴다.** 번갈아 돌리면 그때마다 의존 유닛이 다시 컴파일되고, 무엇보다 **먼저 만든 쪽 바이너리가 조용히 덮인다.** 헤드리스 바이너리를 계속 쓸 거면 빌드 직후 다른 이름으로 복사해 두고, 복사에 `cp -p` 를 썼으면 **`touch` 로 mtime 을 지금으로 올려라**(과거 mtime 은 cargo 가 재빌드를 건너뛰게 만든다).

#### headless 빌드에서 무엇이 없는가 — 재기 전에 알아야 할 세 가지

1. **plugin 은 부팅해도 안 뜬다.** 부팅 직후 `plugin list` 는 9 개 전부 `running=false` 다. 헤드리스는 매니저를 **디스크만 읽어** 세우고(`ensure_plugin_manager_metadata`), 프로세스 기동은 attach 세션이 mesh mirror 를 요구하거나, **plugin namespace 로 forward 되거나**, `plugin.enable` 로 **지목되거나**, **plugin 이 선언한 surface kind 를 지목한 생성 요청이 올 때**(`tab.create`·`pane.split`·`workspace.create` 의 `type`) 일어난다(아래 2·3).

   ★ **넷은 띄우는 수가 다르다 — 그것이 관측 오염의 크기다.** 실측(2026-09-10, 갓 만든 격리 홈):

   | 트리거 | 띄우는 것 | 첫 호출 소요 |
   |---|---|---|
   | `plugin enable <id>` | 지목한 1 개 | — |
   | kind 지목 생성 요청 (`--type markdown`) | 그 kind 의 **소유자 1 개** | 0.14 s (두 번째 0.09 s) |
   | plugin namespace 한 번 (`markdown recent`) | 활성 owner + 매칭 IPC hook의 active extension | 기존 전량 기동에서 범위 축소(ADR-0282) |
   | attach mesh mirror 세션 | 설치된 **9 개 전부** | — |

   kind 지목은 소속을 **매니페스트 ∩ `plugins.toml`** 로 먼저 묻는다([ADR-0259](../adr/0259-a-kind-request-starts-the-owner-that-declares-it.md)). 그래서 ① 없는 이름은 plugin 을 하나도 안 띄우고(실측 `--type nosuchkind` 0.09 s, `running` 0), ② **`plugin disable` 한 plugin 의 kind 도 안 띄운다**(실측 0.09 s, `running` 0). ②가 없으면 그 요청이 영영 안 뜰 plugin 을 기다려 **데몬 IPC 전체가 선다** — 고치기 전 실측이 그 요청 5.34 s · 무관한 `list info` 5.04 s · 덤으로 8 개 기동이었다.
2. **하나만 띄우려면 `plugin enable <id>` 를 쓴다.** 이 둘은 헤드리스에도 배선돼 있고(`plugin.enable` · `plugin.disable`), **지목한 하나만** 기동한다. 실측(2026-09-09, 격리 홈 데몬): 부팅 직후 9 개 전부 `running=false` → `tasty plugin enable com.tasty.image` → `{"enabled":"com.tasty.image"}` → `plugin list` 의 `running` 이 `["com.tasty.image"]` 하나다. 어느 `plugin.*` 이 헤드리스에 있고 없는지는 [headless-ipc-surface.md](headless-ipc-surface.md) 가 메서드별로 가른다 — `plugin.install`·`remove`·`grant`·`revoke`·`upgrade_builtins`·`audit_follow` 는 아직 없어서 `-32017 … gated out of this build combination (headless / release)` 로 답한다.

   namespace 호출은 공통 manager에서 owner만 준비한다. 매칭 pre/post IPC hook이 있는 active extension은 그 호출에 필요한 경우에만 함께 준비한다. 미등록 prefix·disabled owner·게이트 거부는 기동 0이며, 이미 running이면 재시작하지 않는다. 등록 prefix 안의 오타는 완전한 IPC 메서드 명부가 없어 owner가 판정한다. [ADR-0282](../adr/0282-namespace-invocation-starts-only-its-owner-and-matching-extension.md). 기동 수는 fresh 격리 홈의 fake plugin start 로그와 plugin.list를 전후 대조하고 GUI의 부팅 기동과 호출의 추가 기동을 구분한다.
3. **선언된 surface kind 는 plugin 이 뜨는 순간 전부 등록된다 — 조합에 따라 갈리지 않는다.** `register_one_surface_kind` 는 `rendering` 세 종류(`webview`/`remote`/`egui-mesh`)를 모두 등록한다([ADR-0259](../adr/0259-a-kind-request-starts-the-owner-that-declares-it.md)). 실측(2026-09-09, 갓 만든 격리 홈 헤드리스 데몬, 9 개 기동 후):

   | kind | 선언 (`plugin.show` 의 `declared_rendering`) | 등록됐나 (`registered`) | `new workspace --type <kind>` |
   |---|---|---|---|
   | `markdown` | `webview` | `true` (`effective_rendering: "webview"`) | 생성됨 |
   | `html` | `webview` | `true` (`effective_rendering: "webview"`) | 생성됨 (`--url` 필요 — 없으면 `-32602`) |
   | `image` | `egui-mesh` | `true` (`effective_rendering: "egui-mesh"`) | 생성됨 |
   | `mesh_demo` | `egui-mesh` | `true` (`effective_rendering: "egui-mesh"`) | 생성됨 |

   **한때 이 자리는 정반대였다** — `webview`/`remote` 를 skip 하고 `egui-mesh` 만 등록해, 위 표의 앞 두 줄이 `registered: false` + `-32603 unknown surface kind` 였다. 그 skip 의 사유("렌더가 창을 전제한다")는 [ADR-0255](../adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md) 로 무너졌다: mirror 가 나르는 것은 픽셀이 아니라 원문이고 그리는 것은 client 다. 그러니 **옛 회차의 그 표를 근거로 삼지 마라.** 그때 skip 의 증거로 쓰던 `debug-dev.log` 의 `skipped in headless` 줄도 이제 안 나온다 — 그 줄이 없는 것은 로그가 꺼진 것이 아니라 skip 이 없어진 것이다.

   ★ 표의 **선언 열과 등록 열은 여전히 서로 다른 물음**이다. `declared_rendering` 은 매니페스트가 요청한 것이고 `registered` 는 host 가 받아들였는지다 — 지금은 둘이 일치하지만, 일치가 보장이라서가 아니라 **그 plugin 이 떠 있어서**다. 안 뜬 plugin 의 kind 는 선언만 있고 사실이 없다. 사실만 묻고 싶으면 **만들어 보지 말고** `tasty list surface-kinds` 를 쓴다(`surface.kinds`): registry 를 그대로 내는 읽기 전용 조회라 부수효과가 없고, host 내장 kind 도 함께 나온다. 만들어 보는 술어는 이제 대조용이다.

   실측 진행(같은 회차): 부팅 직후 `['dag_graph','empty','explorer','terminal']` 넷 → `plugin enable com.tasty.markdown` 뒤 `markdown` 이 더해져 다섯 → 9 개 전부 뜬 뒤 여덟.

   ★ **kind 지목 트리거는 이 표를 한 칸씩 채운다.** 실측(2026-09-10, 같은 데몬을 kind 지목만으로 몰아본 회차): 부팅 직후 넷 → `--type markdown` 뒤 다섯(`running` 은 `com.tasty.markdown` 하나) → `--type image` 뒤 여섯(`running` 둘). 그래서 **한 kind 를 재려고 나머지 여덟을 띄우지 않아도 된다** — 위 2 의 `plugin enable` 과 같은 성질이고, namespace 를 부르는 쪽과 갈리는 자리다.

**tasty 터미널 내부(`TASTY_SURFACE_ID` 환경변수가 설정된 셸)에서 검증 인스턴스를 띄울 때는 `--launch` 플래그가 필수다.** `cargo run --bin tasty -- <플래그>` 를 `--launch` 없이 실행하면 `src/boot.rs` 의 GUI 부팅 skip 조건(`cli_routing::Routed::AugmentedHelp` 분기)(`TASTY_SURFACE_ID` 설정 + `--launch` 미지정)에 걸려 GUI 가 뜨지 않고 CLI 도움말만 출력한 채 조용히 종료된다 — 이 상태로 `until target/debug/tasty list info ...` 같은 readiness poll 을 돌리면 죽은 프로세스를 무한정 기다리게 된다. 즉 `cargo run &` 을 `--launch` 없이 tasty 터미널 안에서 실행했다면, poll 이 멈추지 않을 때 프로세스가 애초에 GUI 로 뜬 게 맞는지부터 의심한다.

### 자주 쓰는 시나리오

- **PTY 입출력**: `send text` → `read screen` 으로 echo/명령 결과 확인. `read screen`(`surface.screen_text`/`pty.read`)은 dim(ghost-suggestion, 예: Claude Code CLI 가 그리는 미제출 자동완성 제안) 셀을 기본 제외한다 — 제안 텍스트가 실제 입력된 것처럼 오독되는 걸 막기 위함. 제안까지 보고 싶으면 `--show-dim`.
  - `--lines N` 은 **내용 기준 마지막 N 줄**이다 — grid 하단 N 행이 아니다. 내용 아래의 공백 행은 건너뛰고, 화면 내용이 N 에 모자라면 스크롤백에서 채운다. 따라서 N 의 크기와 무관하게 의미가 같고, **살아 있는 터미널이 `--lines` 때문에 빈 결과를 내는 일은 없다.** 빈 결과가 나왔다면 실제로 출력이 없는 것이므로 "surface 가 죽었다" 로 넘어가기 전에 `--lines` 없는 전체 화면과 대조한다.
- **레이아웃 저장/복원**: dirty 트리거 발생 → 슬롯 파일 확인(debug 검증이면 `~/.tasty-debug/layouts/NN.json`) → kill → 재시작 → `read screen` 으로 복원 확인.
- **Surface meta**: `surface-meta set/get/list` 로 키-값 확인.
- **Hook/플러그인**: `tasty list hooks` · `tasty plugin list` 로 등록 상태, 호출 결과는 plugin 로그(`<tasty_home>/plugins-logs/`).
- **레이아웃 트리 변형**: split/close/new → `list tree` 로 구조 변화 확인.

### debug 전용 IPC 로만 가능한 검증

사용자 입력 재현(키/마우스 주입, popup 강제 open/close, 도구 메뉴 클릭)이나 렌더 셀 덤프는 release 표면에 없다 — debug 빌드의 `debug.*` 로 구동한다. [debug-ipc.md](debug-ipc.md) 참조.

### GUI 시각 검증

색상·정렬·폰트처럼 스크린샷이 필요한 변경은 CLI 만으로 잡지 못한다 — [`ai-verification/screenshot-methods` 시각 판정 체크리스트](../ai-verification/screenshot-methods.md#시각-판정-체크리스트) 체크리스트를 따른다.

**스크린샷은 OS 화면 캡처(`screencapture` / PowerShell `CopyFromScreen` 등)보다 tasty 자체 `ui.screenshot` IPC 를 먼저 쓴다.** OS 화면 캡처는 화면 녹화 권한이 필요해 *빌드할 때마다 사용자가 권한을 다시 풀어주지 않는 한 막힌다* — 자기검증 흐름이 권한 프롬프트에서 멈춘다. `ui.screenshot` 은 tasty 가 실제 렌더한 프레임을 권한 없이 PNG 로 떨구므로 자동 검증에 적합하다(다른 윈도우 가림·포커스 상태에도 영향 없음).

**다만 `ui.screenshot` 이 모든 화면의 상위 채널은 아니다 — 무엇을 그리느냐가 어느 캡처로 보이느냐를 정한다.** native WebView 로 그리는 surface(`markdown` · `html`)는 wgpu 스왑체인 **밖의 OS 자식 윈도우**라 그 캡처에 담기지 않는다(찍으면 그 자리에 host 가 그린 "WebView region" + URL chrome 만 나온다). 그쪽은 OS 화면 캡처가 폴백이 아니라 **유일 채널**이다. 대상별로 어느 채널에 있고 어느 채널에 없는지는 [`ai-verification/screenshot-methods`](../ai-verification/screenshot-methods.md) 맨 위 표가 정본이다 — **"OS 캡처는 최후 폴백" 으로만 읽으면 그 두 kind 의 시각 검증을 통째로 건너뛰게 된다.** 호출법·격리 실행도 같은 문서.

### Linux 개발 환경

tasty 를 개발하는 AI 에이전트용 Linux 환경 가이드. (경로 예시는 Linux dev 머신 기준 — 본인 환경에 맞춰 치환.)

#### 바이너리 / 실행

```
target/debug/tasty     # 디버그
target/release/tasty   # 릴리스
```

`tasty` 는 PATH 에 없다 — 직접 경로 또는 `cargo run`.

```bash
# GUI 실행 + 준비 대기 — PID 를 저장한다(이름·패턴으로 찾지 않는다)
./target/debug/tasty & TASTY_PID=$!
while [ ! -f ~/.tasty-debug/tasty.port ]; do sleep 0.2; done

# 실행 여부
kill -0 "$TASTY_PID" 2>/dev/null && echo running || echo "not running"

# 다중 인스턴스 — 루트를 분리한다
TASTY_HOME=/tmp/tasty-a ./target/debug/tasty &
```

종료: `system.shutdown` IPC — **debug 전용이고 CLI 는 없다**([api-conventions](api-conventions.md)
"debug 표에 있는데 CLI 가 없는 메서드"). 포트로 raw JSON-RPC 를 보낸다. `kill "$TASTY_PID"` 로 죽이면 포트 파일 수동 삭제(`rm -f ~/.tasty-debug/tasty.port`).

#### 빌드 후 재시작

Linux 는 실행 중 바이너리를 `cargo build` 로 교체해도 실행 프로세스에 영향 없다(inode 참조). 실행 중 인스턴스는 옛 바이너리로 계속 동작, 다음 실행부터 새 바이너리.

#### 스크린샷

GUI 모드 전용(headless 불가):

```python
call("ui.screenshot", {"path": "/tmp/tasty-capture.png"})   # PNG
```

hover/애니메이션 등 상태 의존 UI 는 정적 캡처로 확인 불가 — 조건(`if response.hovered()` 등)을 임시 제거해 항상 적용 → 빌드/재시작/캡처 → 확인 후 복원. (시각 검증 휴리스틱: [ai-verification/screenshot-methods 시각 판정 체크리스트](../ai-verification/screenshot-methods.md#시각-판정-체크리스트).)

## 가드를 검증할 때 — 세 가지 침묵은 **다른 물음**이다

가드(소스 스캔·명부 재검사)를 검증하다 보면 침묵이 세 가지 모양으로 온다. 셋 다
"표적에 결함이 없다" 로 읽히는데, 실제로는 셋 다 **내 모형이 틀렸다** 인 경우가 많다.
그리고 **틀렸을 때 남는 오류의 부호가 서로 다르다** — 한 규칙으로 합치면 안 된다.

### ① 양성 대조가 **안 죽는다** → 표적이 아니라 모형부터

변이를 심었는데 가드가 초록이면, "가드가 못 잡는다" 로 적기 전에 둘을 먼저 본다.

- **그 자극이 실제로 실행됐나.** 변이가 컴파일 오류로 죽었거나, 심은 자리가 그 가드의
  모수 밖이거나, 파일이 다시 안 빌드된 경우다.
- **모형이 표적의 규칙과 맞나.** 가드가 좁게 판정하는 것이 **옳아서** 통과한 경우다.

실측 사례 셋(2026-09-07, 하루):

| 심은 곳 | 왜 안 죽었나 |
|---|---|
| `test.yml` 의 `test-linux-x64` 에 `--ignored` | 그 잡이 `workflow_dispatch` 전용이라 **자동 잡이 아니다.** 가드의 물음이 "자동 채널" 이므로 옳게 통과 |
| 본체에 `const FRAME_MAX_W: LogicalPx = LogicalPx::new(100.0)` | `LogicalPx::new` 가 없어 **컴파일이 죽었다.** 변이가 실행된 적이 없다 |
| `ci-gates.md` 의 한 절에 통과 수만 적기 | 그 절의 **하위 절**에 표지가 있었다. 가드 규칙이 "그 절이나 하위 절" 이라 옳게 통과 |

셋 다 1 차에서 멈췄으면 **거짓 결함 보고**였다. 모형을 고치니 셋 다 죽었다.

### ② 술어가 **0 을 낸다** → 잔여가 아니라 술어부터

스캔이 "위반 0" 을 내면, 그 0 이 "정말 없어서 0" 인지 "안 보여서 0" 인지 먼저 가른다.
가르는 방법은 하나다 — **이미 아는 양성으로 술어를 쏜다.** 안 잡으면 술어가 고장이다.

실측 사례: mtime 해상도에 기댄 단정을 찾는 첫 술어(한 줄 안에서
`assert…!(… modified() …)`)가 **0** 을 냈는데, 그 0 은 **이미 알려진 양성조차 못 잡는**
0 이었다. 그 자리는 mtime 을 변수에 담아 다섯 줄 밖에서 비교한다. 술어를 함수 블록
단위로 고치니 그 하나를 잡았다(그리고 그것뿐이었다).

**0 만 의심하지 마라 — 0 이 아닌 값도 틀어져 있고, 어느 쪽으로 틀어졌는지는 미리 안다.**
철자로 찾는 술어가 자기가 찾는 것을 **덜 찾았다면**(형태 하나를 놓치면 그만큼 안
세어진다), 그 편향이 보고 값에 붙는 부호는 **바늘이 무엇을 찾느냐**로 뒤집힌다.

| 바늘이 찾는 것 | 덜 찾으면 | 보고 값은 |
|---|---|---|
| 그 자리의 **존재**("이런 자리가 있나") | 적게 센다 | **하한** — 실제는 이보다 많다 |
| 근거의 **부재**("사유가 안 붙었나") | 근거를 덜 찾으니 위반이 늘어난다 | **상한** — 실제는 이보다 적다 |

그래서 수를 적을 때 "약 N" 이 아니라 **"N 이상" 인지 "N 이하" 인지**를 적을 수 있다.
그 부호는 술어를 보면 도출되지, 값을 다시 재야 아는 것이 아니다.

★ **이건 부호 규칙이지 빈도 규칙이 아니다.** "철자 술어는 대체로 덜 잡는다" 는 **여기서
말하지 않는다** — 그 물음의 모수는 *정정된 술어* 가 아니라 **쓰인 술어 전수**여야 하고,
그것은 아직 아무도 안 쟀다. 정정된 것만 모으면 맞은 술어는 정정이 없어 표에 안 오므로,
그 표본으로 빈도를 말하는 것은 선택 편향을 값으로 쓰는 것이다. 빈도가 필요하면 그건
**새 측정**이고 모수를 다시 뽑아야 한다.

**정답을 알고 있어도 첫 술어가 놓친다.** 보통은 정답을 모르는 채로 스캔하므로, 이
절차가 없으면 그 0 을 "없다" 로 적게 된다. 그것이 이 부류의 실제 비용이다.

### ③ 실행 자체가 **안 일어난다** → 초록이 아니라 **판정 불가**

①·② 는 대조와 술어의 물음이다. 셋째 침묵은 그보다 앞에 있다 — **부르는 자리가 아무것도
안 불렀는데 결과줄이 초록으로 찍히는** 경우다. `rc=0` 과 "위반 0" 만 보면 **안 돈 것과 다
통과한 것이 글자 그대로 같은 줄을 만든다.**

실측 세 가지(2026-09-07, 같은 날):

| 어디서 | 무엇이 안 돌았나 | 무엇으로 보였나 |
|---|---|---|
| 게이트를 목록으로 도는 하네스 | 셸 확장 하나가 그 셸에서 안 먹어 **루프가 0 회** 돌았다 | `게이트빨강=0` — 판정줄이 초록 |
| 이름 필터로 부르는 카나리아 | 필터가 안 맞아 **0 건 선택** | `0 passed; 0 failed` · `rc=0` |
| 변이 대조 | 변이를 심었는데 타깃이 **재빌드가 안 됐다** | 가드 초록 — "안 잡았다"(결함)와 같은 모양 |

세 번째가 특히 나쁘다. 그 초록은 ①("대조가 안 죽는다")과 **구별되지 않으므로**, ① 의
절차를 아무리 성실히 밟아도 원인을 못 찾는다.

**처방은 하나다 — 실행 수를 판정에 넣는다.**

- 배치로 돌린 실행은 **대상 N · 결과줄 N · 통과 M** 세 수를 함께 남긴다. **앞의 두 N 이
  어긋나면 그 회차는 초록도 빨강도 아니고 판정 불가다.**
- 발견으로 목록을 만드는 자리는 **발견 수가 0 인 것 자체를 실패로 센다.** 판정 불가를
  통과로 세지 않는 규칙은 이 층에도 그대로 든다.
- 관문 성격의 실행(뒤의 판정 전부가 그것에 기대는 자리)은 수가 안 맞으면 **본 실행을 아예
  쏘지 않는다.** 관문이 조용해지면 뒤의 값이 근거 없이 살아난다.
- 변이 대조는 셋을 함께 확인한다: 결과줄 총계가 기대 수인가 · 변이가 실제로 파일에
  들어갔나 · **재빌드가 실제로 일어났나**(산출물이 **이름으로** 움직였는지 — 수가 아니라 이름).
- **새것을 들일 때는 무엇이 그것을 처음 보는지를 묻는다.** 새 bin · 통합 테스트 타깃 · 크레이트 ·
  소스 파일 · 문서 · 스크립트에는 조립 채널(컴파일 · 타깃 자동 발견)이 저절로 붙어 초록이 나지만,
  그것을 **판정하는** 채널은 저절로 안 붙는다. 판정기가 빌드 산출물이면 재sync 뒤에도 낡은 채로
  돈다 — 재sync·조립 직후 판정기부터 다시 짓는다. pre-commit 의 새 파일 경고(W.2)가 이 물음을
  종류별로 찍는다. 근거와 "판정이 없다 / 도달 안 했다" 의 갈래는 [ADR-0184](../adr/0184-assembly-follows-a-new-artifact-but-judgment-does-not.md).

### 무엇을 돌렸는지의 단위는 **(패키지 × 타깃 × 필터)** 다

"게이트를 돌렸다" 는 이 물음에 답이 안 된다. 셋 중 하나만 어긋나도 안 돈 것이다.

- 패키지가 맞아도 **`--bin` 은 `tests/*.rs` 를 아예 안 짓는다.**
- 타깃이 맞아도 **이름 필터가 걸려 있으면** 그 타깃의 나머지는 안 돈 것이다.
- 크레이트에 lib 타깃이 없으면 그 크레이트의 단위 시험은 **bin 안**에 있어 `--lib` 로 안 돈다.

그리고 고를 때 묻는 물음은 **"내가 무엇을 고쳤나" 가 아니라 "내가 고친 것을 무엇이 보나"**
다. 앞쪽으로 물으면 자기 패키지가 답으로 나오고, 그건 거의 항상 틀린다 — 고친 대상이
가드여도 **그 가드를 검사하는 것은 또 다른 패키지**에 있다. 셸 스크립트를 고쳤을 때
"셸 게이트를 돌렸다" 가 틀린 추론인 이유가 이것이다: **게이트는 레포를 보고, 그 게이트를
보는 가드는 시험 타깃에 있다. 인구가 반대다.**

찾는 갈래는 셋이고, 셋이 서로 다른 것을 잡는다.

1. 내가 고친 파일을 **이름으로** 읽는 타깃
2. 그 디렉토리를 **디렉토리 이름으로** 훑는 타깃
3. **확장자·성질로 레포 전체를 훑는** 타깃 — **이 판이 가장 안 보인다.** 파일 이름도
   디렉토리 이름도 그 가드 소스에 안 나오므로 1·2 로는 걸리지 않는다.

★ **발견 명령의 결과 수를 모수로 쓰지 마라.** 그 수는 술어가 내는 수이지 "그것을 보는
가드의 수" 가 아니고, 트리마다 다르며, **산문으로 오염된다** — 어떤 파일의 주석에 그
낱말을 적기만 해도 문자열로 묻는 발견 명령의 인구가 늘어난다. 게이트의 인구는 래칫이
지키지만 **발견 명령의 인구는 아무것도 지키지 않는다.** 수를 낼 때는 **술어 · 모수 ·
그 수가 상한인지 하한인지**를 함께 적는다.

### 왜 셋을 합치지 않는가

같은 처방("모형을 먼저 의심하라")으로 보이지만 **입력과 오류의 부호가 다르다.**

| | 입력 | 안 하면 남는 것 |
|---|---|---|
| ① | 대조가 **안 죽는다** | 없는 결함을 보고한다 (거짓 양성) |
| ② | 표적이 **안 잡힌다** | 있는 결함을 없다고 적는다 (거짓 음성) |
| ③ | **아무것도 안 돌았다** | 판정을 안 한 채 **판정했다고 적는다** — 그 뒤의 값이 전부 근거를 잃는다 |

물어야 할 것도 다르다 — ①은 *"내 자극이 저 규칙 안에 들어갔나"*, ②는 *"내 바늘이 저
모수의 실제 모양과 맞나"*, ③은 *"이 명령이 실제로 몇 개를 돌렸나"* 다. 합치면 셋 다
"모형을 봐라" 로 뭉개지고, 그러면 어느 쪽을 어떻게 봐야 하는지가 사라진다.

★ ③ 은 앞의 둘과 층이 다르다. ①·② 는 **판정의 내용**을 묻고, ③ 은 **판정이 있었는가**를
묻는다. 그래서 ③ 이 조용하면 ①·② 를 아무리 성실히 밟아도 소용이 없다 — 먼저 세야 한다.

## 시험을 골라 돌릴 때 — 술어를 **철자**로 쓰지 마라

앞 절이 다루는 것은 *결과가 안 나거나 잘못 나는* 자리다. 이 절은 그 앞 단계 —
**내가 쓴 술어가 집합을 잘못 정의하는** 자리다. 둘을 섞으면 둘 다 흐려진다.

### 왜 고르나, 그리고 고를 때 먼저 가려야 할 둘

빌드가 비싸거나 러너가 바쁠 때, 이미 빌드된 시험 바이너리
(`target/debug/deps/<타깃>-<해시>`)를 **직접 실행**하면 컴파일이 0 이다. 그때
두 물음을 먼저 가른다.

1. **낡은 바이너리의 초록이 유효한가.** 소스를 런타임에 훑는 가드는 유효하다 —
   판정 대상이 지금 트리의 파일이다. 크레이트를 링크해 그 코드를 부르는 시험은
   무효다 — 낡은 코드의 초록이다.
2. **그 시험이 앱 인스턴스를 띄우는가.** 띄우면 컴파일이 0 이어도 부하다.

**둘은 다른 물음이고, 하나에 답했다고 다른 하나가 닫히지 않는다.** 같은 모양이
판정기 신선도에서도 난다 — 그 검사는 축이 둘인데(라이브러리 지문 · 검사기 자신의
소스) 한 축만 읽으면 "이 검사는 자기 소스를 안 본다" 는 틀린 결론이 나온다.
축이 몇 개인지 먼저 세지 않으면, 한 축을 정성껏 답한 것이 **전체를 답했다는
느낌**을 준다.

### 소스 철자로 물으면 틀린다

"이 시험이 하네스를 끌어오는가" 를 이렇게 물으면 안 된다:

```bash
grep -q '^use tasty_\|^use crate::\|tasty_cli::' "$src"    # ← 소스의 철자
```

같은 뜻을 쓰는 방법이 여럿이기 때문이다 — `use` 로 가져오기 · 본문에서 **완전
경로로 한 번** 쓰기 · `mod common;` 한 줄로 딸려오기. 마지막 형태에는 크레이트
이름도 `use` 도 안 나온다. 그래서 하네스를 통해 앱을 띄우는 시험이 "순수 소스
훑기" 로 분류되고, 실행하면 인스턴스가 뜬다.

### 바른 술어 — cargo 가 이미 적어 둔 `.d`

cargo 는 타깃마다 `target/debug/deps/<타깃>-<해시>.d` 에 **그 바이너리가 실제로
끌어온 파일 전부**를 적어 둔다. 철자가 아니라 링크 결과다.

```bash
# 이 시험을 돌리면 하네스가 딸려 오는가 = 앱 인스턴스가 뜨는가
tr ' ' '\n' < "$d" | grep -q 'tests/common/mod.rs\|spawn_diag/mod.rs\|gui_common'
```

일반형: **같은 물음에 이미 답한 기계 산출물이 있으면 그것을 읽는다.** 처방은
"철자를 더 많이 잡아라" 가 아니다 — 사람이 외우는 인용 형태 목록은 외우는 순간
낡는다. 빌드 산출물은 공짜고, 빌드가 갱신하고, 인용 형태와 무관하다.

### ★ 이 술어의 경계 — **링크**를 답하지 **판정 대상**을 답하지 않는다

`.d` 는 "이 바이너리를 짓는 데 무엇이 들어갔나" 를 답한다. **그 시험이 무엇을
판정하는가는 답하지 않는다.** 레포 전수 가드는 판정 대상을 링크하지 않고
런타임에 순회해서 읽으므로, 그 가드가 실제로 무는 파일은 `.d` 에 **없다**.

⇒ 두 물음을 안 가르면 "내 크레이트를 보는 타깃이 0 개" 라는 답이 나오고, 그 0 을
보고 안심한다. 이 술어로 물어도 되는 것은 오직 *"이 바이너리를 돌리면 무엇이
딸려 도는가"* 뿐이다. *"무엇이 내 파일을 판정하는가"* 는 가드의 **순회 뿌리**를
읽어서 답한다.

### 그리고 `.d` 자신도 철자다

- `.d` 안의 경로는 `mod` 가 쓴 철자 그대로다(`tests/gui_common/../spawn_diag/mod.rs`
  같은 형태가 나온다). 문자 매치로 세면 부풀고, **정규화한 뒤에** 세야 한다.
- `target/debug/*.d` 는 절대경로, `target/debug/deps/*.d` 는 상대경로다.
  같은 트리에서 규약이 다르다.
- `deps/` 에는 한 타깃의 **낡은 해시본이 쌓인다.** 이름으로 접어야 타깃 수가 나온다.

### "안 돈다" 에는 두 층이 있다

루트 통합 타깃 중 어떤 것은 **이미 빌드돼 있어 지금 공짜로 돌릴 수 있는데 아무도
안 부르고**, 어떤 것은 부르려면 빌드가 필요하다. 두 층을 안 가르면 앞쪽을 뒤쪽과
함께 "못 돈다" 로 접는다. 세는 명령(수는 커밋마다 바뀌므로 적지 않는다):

```bash
ls tests/*.rs | wc -l                                    # 루트 통합 타깃
ls target/debug/deps/*.d | sed 's/-[0-9a-f]*\.d$//' \
  | sort -u | wc -l                                      # 이미 빌드된 타깃(해시 접음)
```

### ★ 좁은 술어는 언제 맞고 언제 틀리는가 — 두 줄

같은 좁은 술어가 어떤 조사에서는 거의 다 맞히고 어떤 조사에서는 절반을 놓친다.
갈리는 것은 술어의 품질도 조사자의 성실성도 아니라 **찾는 대상이 어떻게 분포하는가**다.

- 표류가 **균일**하면(크레이트·모듈 단위로 한 관례를 통째로 따른다) → 좁은 술어도 맞다.
  어느 표본에서나 잡히므로 파일 경계가 답을 안 바꾼다.
- 표류가 **흩어져 있으면**(자리 단위로 몇 개만 어긋난다) → 좁은 술어가 놓친다.

그리고 흩어진 자리가 곧 **강제되지 않는 자리**다. 결함이 남는 곳과 좁은 술어가
못 보는 곳이 **같은 곳**이라, 좁은 술어는 하필 가장 필요한 자리에서 실패한다.

⇒ 절차: 조사를 설계할 때 **"이 축의 표류가 균일한가 흩어졌는가" 를 먼저 묻고**
술어 폭을 정한다. 균일하다고 판단했으면 그 근거를 함께 적는다.

### 술어의 실패율은 **대상의 문체 다양성**에 비례한다

한 줄 arm 만 잡는 술어를 dispatch 표에 대면, 그 표가 한 가지 문체로만 쓰였을 때는
거의 다 맞고 여러 문체가 섞였을 때는 크게 빗나간다. 실제로 갈리는 문체는 이런
것들이다 — 호출을 경로로 쓰는가 맨 이름으로 쓰는가, arm 이 한 줄인가 여러 줄인가,
`=> match …` 로 여는가, 여러 리터럴을 `|` 로 묶는가.

**조사자는 이 차이를 지각할 수 없다.** 넓어서 틀리면 대상 아닌 것이 딸려 와 눈에
걸리지만, **좁아서 틀리면 아무 일도 안 일어난다.**

### 그래서 보고는 **세 칸**으로 한다

수를 하나만 적으면 그 수가 상한인지 하한인지가 안 남는다. 축을 조사했으면:

| 옛 값 (좁은 술어) | 새 값 (교정된 모수) | **차이 중 실제 결함** |
|---|---|---|

셋째 칸이 산출물이다. 앞의 두 칸은 조사 중간값이라 **문서에 적지 않는다** — 커밋마다
바뀐다([ADR-0139](../adr/0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md)).
셋째 칸이 **0 인 축도 값이다**: 그 축은 문체와 무관하다는 뜻이고, 왜 무관한지가
다음 조사의 술어 폭을 정한다.

실측(2026-09-07, 두 에이전트 plugin 의 dispatch 명부): 축 셋을 좁은 술어에서
명부 전수로 바꿔 다시 판정했을 때 셋째 칸은 각각 0 · 3 · 2 였다. 0 이 나온 축은
표류가 크레이트 단위로 균일했던 축이다.

### ★ 명시적 제외는 좁은 술어보다 나쁘다

술어가 좁은 것은 도구의 성질이지만, **"이건 범위 밖이다" 라고 손으로 빼는 것은
판단**이고 그 판단은 근거를 기록에 남긴다. 그래서 나중에 읽으면 **정당한 범위
결정으로 보인다.**

위 실측에서 실제로 그 일이 났다: 한 자리를 "다른 크레이트라 이 항의 인구가 아니다"
로 명시적으로 제외했는데, 명부로 보니 같은 축 위의 같은 자리였고 **짝의 한쪽만
고쳐진 상태**였다. 제외의 근거가 파일·크레이트 경계였던 것이 원인이다.

⇒ 범위 밖으로 빼려면 **경계가 아니라 축으로** 근거를 적는다. "다른 파일이라"·"다른
크레이트라" 는 축의 근거가 아니다.

### 멈추는 자리 — **두 수를 나란히 찍는다**

모수를 교정해도 그 다음 술어가 또 좁을 수 있다. 무한퇴행처럼 들리지만 아니다.
멈추는 방법은 "더 넓게" 가 아니라 **같은 대상을 두 방법으로 세고 값이 갈리는지
보는 것**이다.

- 명부를 세는 술어를 두 개 쓴다. 갈리면 **갈렸다는 사실 자체를 보고**한다 —
  한 술어만 썼으면 그 명부가 하한인 줄 몰랐을 것이다.
- 명부 수와 그것을 해석한 수를 **나란히 찍는다.** 두 수가 다르면 해석기가 좁다.
  (실제로 한 해석기가 문자열을 소비하는 바람에 짧은 항목이 연달아 있을 때
  **하나 걸러 사라졌다.** 두 수를 나란히 안 찍었으면 못 봤다.)

## 길이 가드의 사각 계수가 달라졌을 때

`on_scale_length_literal::the_blind_spots_are_still_the_size_they_say`는 출하 코드의
0과 테스트 전용 길이도 따로 센다. 이 값은 토큰 위반 수가 아니다. 실패하면 `scan(false)`와
`scan(true)`의 자리 목록을 도입 커밋 전후로 비교하고, 값·호출 위치·출하 여부를 확인한다.

파일 피커의 `entry_row`는 고정 열을 뺀 파일명 폭을 0 이상으로 제한한다. 이 0은
새 디자인 치수가 아니므로 기존 zero 분류에 남는다. 좁은 폭 회귀시험의 viewport는
테스트 분류에 남으며, `size-*`에 속하는 400만 계상되고 360은 이 가족에 없다.
제품 기하를 바꾸거나 토큰을 붙여 계수를 맞추지 않는다.

계수를 갱신할 때는 같은 위치의 0을 실제 스케일 값으로 바꾼 대조와 테스트 게이트를
제거한 대조도 실행한다. 전자는 출하 치수 판정에 나타나야 하고, 후자는 같은 기하가
테스트 사각에서 출하 쪽으로 이동해야 한다. detector의 두 회귀시험은 이 경계를 검사하며,
전체 source scan은 현재 카탈로그와 파일 단위 test-only 판정까지 포함한다.

## 안티패턴 / 패턴

- ❌ "빌드 통과했어요, 확인해 주세요" — 빌드는 검증이 아니다.
- ❌ "테스트 545개 통과, 커밋했어요" — 테스트가 cover 못 하는 통합 동작이 있다.
- ❌ "동작할 것으로 보입니다" — 직접 돌려본 결과를 보고한다.
- ✅ 수정 → 빌드 → 단위 테스트 → **시나리오 재현** → 결과 보고 → 커밋.
- ✅ "재현 시나리오를 못 만들어 확인 못 했습니다" 를 인정하면 사용자가 검증을 도울 수 있다.

## 관련

- [`documentation-model.md`](../documentation-model.md) §6 — 여기의 변이 절차를 **문서의
  채널 주장**("배선돼 있다 / 이것이 본다")에 적용하는 규칙의 정본. 변이를 못 붙일 때
  무엇을 대신 적는지도 거기다.
- [debug-ipc.md](debug-ipc.md) — debug 전용 IPC (사용자 입력 재현)
- [`ai-verification/screenshot-methods` 시각 판정 체크리스트](../ai-verification/screenshot-methods.md#시각-판정-체크리스트) — 시각 검증 · [`ai-verification/screenshot-methods`](../ai-verification/screenshot-methods.md) — `ui.screenshot`(markdown·html 은 OS 캡처)

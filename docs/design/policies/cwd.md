# CWD 정책

각 surface 가 자기 "현재 폴더"(cwd)를 **정의·갱신**하는 방식. 이 cwd 는 surface 전환·새 탭 cwd 상속·split carry·터미널 링크 해석·닫힌 항목 복원에 쓰인다. 생성 시 cwd 가 *손실 없이 carry* 되는 invariant 는 [surface-cwd invariant](../../architecture/invariants/surface-cwd.md) — 본 문서는 *각 surface 가 자기 cwd 를 어떻게 정의/갱신하는가* 다.

호스트는 `CoreState::surface_cwd(sid)`(`src/core/state/surface_cwd.rs`)로 조회 — terminal kind 면 `engine.terminals.get(sid).get_cwd()`(store 경유), 그 외는 `Surface::source_cwd()`. 반환값은 출처가 구분된 `SurfaceCwd`(`Local` / `Remote`)이고, mirror 워크스페이스의 surface 는 `Remote` 다 — 원격 경로가 로컬 실행 자리로 새지 않게 하는 규칙은 [surface-cwd §3-2](../../architecture/invariants/surface-cwd.md).

## Surface 별 cwd

| Surface | "현재 폴더" 정의 | 갱신 트리거 | host 가 아는 방법 |
|---------|------------------|-------------|------------------|
| **Terminal** | shell 의 `$PWD` | shell 이 OSC 7 송신 시 | termwiz parse → `cached_cwd`(store), `get_cwd()` 로 조회 |
| **Markdown** / **Image** | 열린 파일의 parent | *불변* | `source_cwd()` = `file.parent()` |
| **Empty** | 생성 시 carry 된 cwd | 불변 | `source_cwd()` = `self.cwd` |
| **Explorer**(RemoteSurface) | 주소바(root_path) 폴더 | root_path 바뀌는 모든 path | plugin → `surface.set_cwd` IPC → host `RemoteSurface.cwd` 갱신 |
| **기타 RemoteSurface** | 각 plugin 의미 | 각 plugin | `surface.set_cwd` 권장, 미구현 시 None(호환 OK) |

- **불변 surface**(Markdown/Image/Empty): 사용자가 그 안에서 위치를 이동할 수단이 없어 갱신 메커니즘 불요 — `source_cwd()` 정적 반환.
- **동적 surface**(Terminal/Explorer): 사용자가 안에서 이동(`cd`/주소바) → 갱신 메커니즘 필수.

## 터미널 OSC 7

셸이 프롬프트마다 `\e]7;file://hostname/path\e\\` 를 보내면 즉시 `cached_cwd` 반영(비용 0, 이벤트 기반, **모든 플랫폼 OSC 7 에만 의존**). 코드: `crates/tasty-terminal/src/vte_handler/osc.rs`(수신), `accessors.rs`(`get_cwd`/`set_cached_cwd`).

| 셸 | OSC 7 |
|----|-------|
| zsh / fish | 기본 지원 |
| bash | 수동 (`PROMPT_COMMAND='printf "\033]7;file://%s%s\033\\" "$HOSTNAME" "$PWD"'`) |
| PowerShell 7+ | 수동 (`prompt` 함수) |

OSC 7 이 오면 그 surface 가 속한 탭의 이름도 다시 매긴다 — 탭의 focused surface 의 cwd 마지막 이름(홈이면 `~`)이고, 명시 이름과 OSC 제목이 있으면 그쪽이 이긴다. 두 빌드 조합이 같다: gui 는 `TerminalCwdChanged` → `SurfaceCwdChanged` cascade 가, headless 는 PTY drain(`src/boot.rs`)이 `intent::headless::apply_terminal_cwd_changed` 로 같은 engine 갱신을 한다([headless-build-boundaries](../../dev-guide/headless-build-boundaries.md) "두 조합이 같게 하는 것"). 시험 `tests/e2e_tests.rs` 의 `an_osc7_cwd_becomes_the_tab_name` 이 두 조합에서 같은 단언을 본다.

셸이 OSC 7 을 안 보내면 `cached_cwd` 가 비어 새 분할 시 부모 cwd 상속이 동작하지 않는다(프롬프트 설정으로 해결). (Windows 는 합성 rcfile 로 bash 의 OSC 7 emit 강제 — [terminal](../../features/terminal/index.md).)

## `surface.set_cwd` IPC (RemoteSurface)

동적 cwd plugin surface 가 host 에 통보:

```jsonc
{ "method": "surface.set_cwd", "params": { "surface_id": 42, "cwd": "/foo/bar" } }  // cwd: null 도 허용(해제)
```

- 권한 `surface.write`. plugin 은 *사용자 인지 "현재 폴더" 가 바뀐 모든 path* 에 발사(단일 setter 로 모으는 패턴 권장 — 누락 방지). host 는 `RemoteSurface.cwd` 에 보관, `source_cwd()` 가 반환 → 기존 carry 경로에 자동 합류. 옛 SDK(이 IPC 모름)는 None 으로 남음(추가만 — 호환 유지).

## 관련

- [surface-cwd invariant](../../architecture/invariants/surface-cwd.md) — 생성 시 cwd carry · [`design/flows/split-command`](../flows/split-command.md) — split/새 탭 상속
- [terminal](../../features/terminal/index.md) · [terminal-link](../../features/terminal-link/index.md)(OSC 7 경로 해석)

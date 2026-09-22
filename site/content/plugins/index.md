# 플러그인

마크다운과 이미지 보기, AI 에이전트 연동처럼 필요한 도구를 플러그인으로 사용해 보세요. 기본 제공 플러그인의 기능을 살펴보고, 새 플러그인을 추가하거나 사용 여부와 권한을 관리할 수 있습니다.

## 플러그인이란

마크다운 뷰어, 이미지 뷰어, Claude Code 연동 같은 기능은 Tasty 본체가 아니라 **플러그인** 이 제공합니다. 플러그인은 별도 프로세스로 돌면서 자기가 무엇을 추가하는지(서피스 종류 · 도구 메뉴 항목 · `tasty` 하위 명령 · 설정 페이지 · 파일 핸들러)와 어떤 **권한** 이 필요한지를 선언하고, Tasty 는 허용된 권한 안에서만 그 요청을 받아 줍니다.

- 설치 위치는 `~/.tasty/plugins/<id>/`, 로그는 `~/.tasty/plugins-logs/<id>.log`.
- 기본 제공 플러그인은 첫 실행 때 자동 설치됩니다. 그 뒤로는 직접 설치한 플러그인과 똑같이 끄거나 제거할 수 있습니다. 제거한 기본 플러그인은 다음 실행 때 다시 설치되지 않습니다.
- 플러그인이 꺼져 있으면 그 플러그인이 추가한 서피스 종류 · 명령 · 메뉴 항목이 함께 사라집니다. 실행 중에 끄거나 제거해도 재시작 없이 바로 그렇게 되고, 그 종류로 새 서피스를 열려고 하면 "제공하던 플러그인이 꺼져 있다" 는 이유로 열리지 않습니다. 이미 열어 둔 서피스는 그대로 남고, 다시 켜면 이어집니다.

## 기본 제공 플러그인

| 플러그인 | id | 하는 일 | 쓰는 곳 |
|---------|-----|---------|---------|
| **Markdown Viewer** | `com.tasty.markdown` | `.md` 파일을 렌더해 보여주는 마크다운 서피스. 파일이 바뀌면 자동으로 다시 읽습니다 | 탭 스트립 우클릭 > **새 마크다운...** <!-- en: New Markdown... -->, 탐색기에서 `.md` 열기, `tasty markdown reload` · `recent` |
| **Image** | `com.tasty.image` | 이미지 뷰어 겸 간단한 그림판. 같은 폴더의 다음 · 이전 이미지로 넘기고 PNG 로 저장합니다 | **새 이미지** <!-- en: New Image -->, 이미지 파일 열기, `tasty image open` · `save` · `export` · `next` · `prev` · `paste` · `list` |
| **HTML Viewer** | `com.tasty.html` | HTML 파일과 URL 을 내장 웹뷰로 보여주는 서피스 | **새 HTML...** <!-- en: New HTML... -->, `.html` 열기, `tasty html open` |
| **Clipboard Viewer** | `com.tasty.clipboard-viewer` | 지금 클립보드에 든 내용을 텍스트 · 파일 · 이미지 · HTML 로 분류해 보여주는 팝업. 이력은 저장하지 않습니다 | **도구** <!-- en: Tools --> > **클립보드 뷰어** <!-- en: Clipboard Viewer -->, `Ctrl+Shift+H` |
| **Git Viewer** | `com.tasty.git-viewer` | 현재 디렉터리 저장소의 status · log · diff 를 읽기 전용으로 보여주는 팝업. worktree 가 여러 개면 왼쪽에서 고릅니다 | **도구** > **Git**, 단축키는 직접 지정 |
| **Claude Code** | `com.tasty.claude` | Claude Code 를 Tasty 안에서 띄우고, 자식 인스턴스를 만들어 메시지를 보내고 완료를 알려 받는 멀티에이전트 명령 | `tasty claude launch` · `spawn` · `tell` … — [Claude · Codex 와 함께 쓰기](../agents/claude-codex.md) |
| **Codex** | `com.tasty.codex` | Codex CLI 에 대해 위와 같은 일을 합니다 | `tasty codex launch` · `spawn` · `tell` … — 같은 페이지 |

서피스 종류별 사용법은 [파일 열기](../using/files.md). 이 밖에 개발 빌드에만 실리는 데모 · 실험용 플러그인이 있는데, 배포판에는 포함되지 않습니다.

### 플러그인이 추가하는 설정

**설정** <!-- en: Settings --> 윈도우에서 플러그인 페이지는 두 곳에 나타납니다.

- **외관** <!-- en: Appearance --> > **Markdown** — 마크다운 서피스 전용 폰트 override.
- **외관** > **HTML** — **기본 확대** <!-- en: Default zoom --> (%) · **색 구성표** <!-- en: Color scheme --> (테마 따름 / 라이트 / 다크) · **원격 콘텐츠 허용** <!-- en: Allow remote content --> (기본 꺼짐 — 외부 http/https 리소스 차단) · **스크립트 샌드박스** <!-- en: Sandbox scripts --> (기본 켜짐).
- **플러그인** <!-- en: Plugins --> > **Claude Code** — **Spawn child 경고 임계치** <!-- en: Spawn child warning threshold --> 등.
- **플러그인** > **Codex** — **Spawn child 경고 임계치** · **기본 승인 정책** <!-- en: Default approval policy --> · **기본 샌드박스 모드** <!-- en: Default sandbox mode -->.

플러그인 윈도우의 **구성** <!-- en: Configure --> 버튼도 이 페이지로 갑니다.

## 플러그인 윈도우

사이드바 맨 아래 **플러그인** <!-- en: Plugins --> 버튼을 누릅니다. 탭이 셋입니다.

### 설치된 플러그인

**설치된 플러그인** <!-- en: Installed --> 탭. 왼쪽 목록에서 하나를 고르면 오른쪽에 상세가 뜹니다. 목록 위 **설치된 플러그인 필터…** <!-- en: Filter installed… --> 로 이름을 걸러낼 수 있습니다.

- **상태** <!-- en: Status --> — **활성화** <!-- en: Enabled --> / **비활성** <!-- en: Disabled -->, **실행 중** <!-- en: Running -->. 활성인데 실행에 실패하면 빨간 표시와 함께 **연결에 실패했습니다. 설정에서 플러그인 구성을 확인하세요.** <!-- en: Failed to connect. Check the plugin's configuration in Settings. --> 안내가 붙습니다.
- 활성화 토글 — 끄면 프로세스가 정리되고, 켜면 다시 시작됩니다.
- **권한** <!-- en: Permissions --> — 이 플러그인이 받은 권한 목록. 여기서는 읽기만 되고 바꾸지는 못합니다.
- **명령** <!-- en: Commands --> — 플러그인이 추가한 단축키 명령. 키는 **설정** > **단축키** > **플러그인** 에서 바꿉니다.
- **로그** <!-- en: Log --> · **설치 경로** <!-- en: Install path --> · **폴더 열기** <!-- en: Open folder -->.
- **구성** <!-- en: Configure --> — 설정 윈도우의 플러그인 페이지로 이동.
- **제거** <!-- en: Uninstall --> — 설치 폴더가 삭제됩니다. **기본 제공** <!-- en: built-in --> 배지가 있는 플러그인은 제거하면 다음 실행에서도 자동 설치되지 않는다는 경고가 한 번 더 뜹니다.

### 확인 필요

**확인 필요** <!-- en: Attention --> 탭. 등록이 거부됐거나 실행에 실패한 플러그인이 이유와 함께 모입니다.

| 표시 | 뜻 | 할 일 |
|------|----|------|
| **서명을 신뢰할 수 없음** <!-- en: Signature not trusted --> | 신뢰 목록에 없는 키로 서명됨 | 출처를 확인한 뒤 **지문 복사** <!-- en: Copy fingerprint --> 로 대조하고, 믿을 수 있으면 **재승인** <!-- en: Re-approve --> |
| **서명이 유효하지 않음** <!-- en: Signature invalid --> | 서명이 없거나 검증 실패 | 배포자에게 올바른 패키지를 받습니다 |
| **권한이 변경됨** <!-- en: Permissions changed --> | 업데이트로 요구 권한이 바뀜 | **새로 요청됨** <!-- en: newly requested --> 목록을 보고 **재승인** |
| **실행 오류** <!-- en: Runtime error --> | 활성인데 실행 중 실패 | **로그** 를 확인합니다 |

### 플러그인 추가

**플러그인 추가** <!-- en: Add plugin --> 탭.

1. **플러그인 폴더 경로** <!-- en: Plugin folder path --> 에 `tasty-plugin.toml` 이 들어 있는 폴더를 입력하거나 **플러그인 폴더 찾기…** <!-- en: Find plugin folder… --> 로 고릅니다.
2. **확인** <!-- en: Verify --> 을 누르면 **플러그인 정보** <!-- en: Plugin information --> 에 이름 · 버전 · 설명과 **요구 권한** 이 미리 보입니다.
3. **추가** <!-- en: Add --> 를 누릅니다.
4. 검증된 키로 서명되지 않은 플러그인이면 **출처를 알 수 없는 플러그인** <!-- en: Unknown source plugin --> 확인 창이 뜹니다. 지문을 확인하고 진행하면 그 키가 신뢰 목록에 기록돼 다음부터는 묻지 않습니다. 서명 키 파일(`tasty-plugin.toml.pub`)이 없으면 등록할 수 없으므로 배포자에게 요청합니다.

설치하면 플러그인이 요청한 권한이 허용됩니다. 추가하기 전에 미리보기에서 권한 목록을 확인하세요.

## 권한

플러그인은 필요한 권한을 미리 선언하고, 허용된 권한이 없는 요청은 Tasty 가 거절합니다. 자주 보이는 이름과 뜻:

| 권한 | 허용되는 일 |
|------|-------------|
| `surface.read` · `surface.write` | 서피스 목록 · 상태 읽기, 서피스 만들기 · 바꾸기 |
| `fs.read` · `fs.write` | 파일 읽기 · 쓰기 |
| `clipboard.read` · `clipboard.write` | 클립보드 읽기 · 쓰기 |
| `terminal.spawn` · `terminal.write` · `terminal.read` | 터미널 만들기, 키 입력 보내기, 출력 읽기 |
| `notification` | 알림 띄우기 |
| `process.spawn` · `network` | 외부 프로세스 실행, 네트워크 |
| `ui.tool_item` · `ui.popup` · `ui.settings_page` | 도구 메뉴 항목, 팝업, 설정 페이지 추가 |
| `file_handler.define` · `file_handler.handle:<종류>` | 파일 종류 식별 규칙 정의, 그 종류의 파일 열기 담당 |
| `memory.read` · `memory.write` · `memory.secret` | 에이전트 메모리 저장소 접근 |
| `agent` · `approval` · `telemetry` | 에이전트 협업 · 승인 게이트 · 텔레메트리 |

기본 제공 플러그인이 받는 권한:

| 플러그인 | 권한 |
|---------|------|
| Markdown Viewer | `surface.read` `surface.write` `fs.read` `file_handler.define` `file_handler.handle:markdown` `ui.settings_page` `ui.popup` |
| Image | `surface.read` `surface.write` `clipboard.read` `fs.read` `fs.write` `file_handler.define` `file_handler.handle:image` |
| HTML Viewer | `surface.read` `surface.write` `file_handler.define` `file_handler.handle:html` `ui.settings_page` |
| Clipboard Viewer | `clipboard.read` `ui.popup` `ui.tool_item` |
| Git Viewer | `ui.popup` `ui.tool_item` `fs.read` |
| Claude Code | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.read` `fs.write` `notification` `telemetry` `agent` `ui.settings_page` `completion_strategy.define` `memory.read` `ipc.invoke:codex` |
| Codex | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.write` `notification` `ui.settings_page` `completion_strategy.define` |

권한을 개별로 빼거나 되돌리는 것은 CLI 로만 합니다(아래).

## `tasty plugin` 명령

Tasty 가 실행 중일 때 터미널에서 씁니다. 출력은 JSON 입니다.

| 명령 | 하는 일 |
|------|---------|
| `tasty plugin list` | 설치된 플러그인의 id · 버전 · 활성 · 실행 여부 |
| `tasty plugin show <id>` | 매니페스트 · 권한 · 명령 · 실행 상태 전체 |
| `tasty plugin install <폴더>` | `tasty-plugin.toml` 이 있는 폴더에서 설치. 매니페스트 권한을 그대로 허용합니다 |
| `tasty plugin remove <id>` | 제거 |
| `tasty plugin enable <id>` · `disable <id>` | 켜기 · 끄기 |
| `tasty plugin logs <id> [--follow]` | 로그 출력. `--follow` 는 새 줄을 계속 보여줍니다 (`Ctrl+C` 로 중단) |
| `tasty plugin permissions <id>` | 매니페스트가 요구하는 권한과 실제로 허용된 권한 |
| `tasty plugin grant <id> <권한>` · `revoke <id> <권한>` | 권한 하나 허용 · 회수. 매니페스트에 선언된 권한만 허용할 수 있습니다 |
| `tasty plugin doctor <id>` | 매니페스트 진단 — 이 버전의 Tasty 가 이해하지 못하는 규칙이 있는지 |
| `tasty plugin upgrade-builtins [--force] [--restore-removed <id>]` | 기본 제공 플러그인을 번들 버전으로 다시 맞춥니다. `--restore-removed` 는 제거했던 기본 플러그인을 되살립니다 |

`enable`과 `disable`은 설치된 플러그인의 ID만 받습니다. 설치되지 않은 ID를 지정하면 오류로 끝나고 설정은 바뀌지 않습니다. ID는 `tasty plugin list`에서 확인할 수 있습니다.

`disable`은 플러그인이 끝나기를 기다리지 않고 바로 돌아옵니다. 응답하지 않는 플러그인은 뒤에서 최대 2초 뒤에 강제로 종료됩니다. 그 사이에 `enable`하면 이전 프로세스가 완전히 끝나기를 기다렸다가 바로 다시 시작하므로, `enable`이 최대 2초 걸릴 수 있습니다. `enable`이 돌아오면 플러그인은 이미 실행 중이라 곧바로 사용할 수 있습니다.

```sh
tasty plugin list
tasty plugin permissions com.tasty.git-viewer
tasty plugin disable com.tasty.clipboard-viewer
tasty plugin logs com.tasty.markdown --follow
```

`tasty plugin permissions com.tasty.git-viewer` 의 출력 예:

```json
{
  "granted": ["ui.tool_item", "fs.read", "ui.popup"],
  "id": "com.tasty.git-viewer",
  "manifest": ["ui.popup", "ui.tool_item", "fs.read"]
}
```

## 문제 해결

| 증상 | 확인할 것 |
|------|-----------|
| 마크다운 · 이미지 · HTML 파일이 터미널 안에서 열리지 않습니다 | 플러그인 윈도우에서 해당 플러그인이 **활성화** 인지. 꺼져 있으면 서피스 종류 자체가 없습니다 |
| 도구 메뉴에 **클립보드 뷰어** · **Git** 이 없습니다 | 두 플러그인이 꺼져 있거나 **확인 필요** 에 들어가 있습니다 |
| 플러그인 상태가 빨갛습니다 | **로그** 버튼 또는 `tasty plugin logs <id>` |
| 제거한 기본 플러그인을 되살리고 싶습니다 | `tasty plugin upgrade-builtins --restore-removed <id>` |
| 업데이트 뒤 플러그인이 **확인 필요** 에 들어갔습니다 | 요구 권한이 바뀐 것입니다. 목록을 읽고 **재승인** |

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [파일 열기](../using/files.md) — 마크다운 · 이미지 · HTML 서피스 사용법.
- [Claude · Codex 와 함께 쓰기](../agents/claude-codex.md) — Claude Code · Codex 플러그인.
- [설정](../customize/settings.md) — 플러그인 설정 페이지 위치.

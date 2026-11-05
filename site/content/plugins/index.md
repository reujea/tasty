# 플러그인

마크다운·이미지·HTML 보기와 Claude Code·Codex 연동 등은 플러그인으로 제공합니다. 기본 제공 플러그인을 관리하거나 새 플러그인을 설치하고, 필요한 권한을 확인할 수 있습니다.

## 플러그인이란

마크다운 뷰어, 이미지 뷰어, Claude Code 연동 같은 기능은 Tasty 본체가 아니라 **플러그인** 이 제공합니다. 플러그인은 별도 프로세스로 돌면서 자기가 무엇을 추가하는지(서피스 종류 · 도구 메뉴 항목 · `tasty` 하위 명령 · 설정 페이지 · 파일 핸들러)와 어떤 **권한** 이 필요한지를 선언하고, Tasty 는 허용된 권한 안에서만 그 요청을 받아 줍니다.

- 설치 위치는 `~/.tasty/plugins/<id>/`, 로그는 `~/.tasty/plugins-logs/<id>.log`.
- 기본 제공 플러그인은 첫 실행 때 자동 설치됩니다. 그 뒤로는 직접 설치한 플러그인과 똑같이 끄거나 제거할 수 있습니다. 제거한 기본 플러그인은 다음 실행 때 다시 설치되지 않습니다.
- 플러그인이 꺼져 있으면 그 플러그인이 추가한 서피스 종류 · 명령 · 메뉴 항목이 함께 사라집니다. 실행 중에 끄거나 제거해도 재시작 없이 바로 그렇게 되고, 그 종류로 새 서피스를 열려고 하면 "제공하던 플러그인이 꺼져 있거나 아직 다시 연결되지 않았다" 는 이유로 열리지 않습니다. 이미 열어 둔 서피스는 그대로 남고, 다시 켜면 이어집니다. 다만 스크롤 위치나 저장하지 않은 내용은 잃습니다.

플러그인을 켜거나 끄고 권한을 바꾸면 저장 결과를 확인한 뒤 실행 상태에 반영합니다. 설치·신뢰 선택도 저장이 끝난 뒤 결과를 알려 줍니다. 저장에 실패했거나 결과를 확인할 수 없으면 성공으로 처리하지 않습니다. 종료 중 저장이 끝난 활성화는 다음 실행에 적용되며, 종료 과정에서 플러그인을 새로 시작하지 않습니다. 종료 기한까지 결과를 확인하지 못한 변경은 성공으로 알리거나 자동으로 다시 실행하지 않습니다.

## 기본 제공 플러그인

| 플러그인 | id | 하는 일 | 쓰는 곳 |
|---------|-----|---------|---------|
| **Markdown Viewer** | `com.tasty.markdown` | `.md` 파일을 렌더해 보여주는 마크다운 서피스. 파일이 바뀌면 자동으로 다시 읽습니다 | 탭 스트립 우클릭 > **새 마크다운...** <!-- en: New Markdown... -->, 탐색기에서 `.md` 열기, `tasty markdown reload` · `recent` |
| **Image** | `com.tasty.image` | 이미지 뷰어 겸 간단한 그림판. 같은 폴더의 다음 · 이전 이미지로 넘기고 PNG 로 저장합니다 | **새 이미지** <!-- en: New Image -->, 이미지 파일 열기, `tasty image open` · `save` · `export` · `next` · `prev` · `paste` · `list` |
| **HTML Viewer** | `com.tasty.html` | HTML·SVG 파일과 URL 을 내장 웹뷰로 보여주는 서피스 | **새 HTML...** <!-- en: New HTML... -->, `.html` · `.svg` 열기, `tasty html open` |
| **Clipboard Viewer** | `com.tasty.clipboard-viewer` | 지금 클립보드에 든 내용을 텍스트 · 파일 · 이미지 · HTML 로 분류해 보여주는 팝업. 이력은 저장하지 않습니다 | **도구** <!-- en: Tools --> > **클립보드 뷰어** <!-- en: Clipboard Viewer -->, `Ctrl+Shift+H` |
| **Git Viewer** | `com.tasty.git-viewer` | 현재 디렉터리 저장소의 status · log · diff 를 읽기 전용으로 보여주는 팝업. worktree 가 여러 개면 왼쪽에서 고릅니다 | **도구** > **Git**, 단축키는 직접 지정 |
| **Claude Code** | `com.tasty.claude` | Claude Code 를 Tasty 안에서 띄우고, 자식 인스턴스를 만들어 메시지를 보내고 완료를 알려 받는 멀티에이전트 명령 | `tasty claude launch` · `spawn` · `tell` … — [Claude·Codex와 함께 쓰기](../agents/claude-codex.md) |
| **Codex** | `com.tasty.codex` | Codex CLI 에 대해 위와 같은 일을 합니다 | `tasty codex launch` · `spawn` · `tell` … — 같은 페이지 |

서피스 종류별 사용법은 [파일 관리와 보기](../using/files.md). 이 밖에 개발 빌드에만 실리는 데모 · 실험용 플러그인이 있는데, 배포판에는 포함되지 않습니다.

### 플러그인이 추가하는 설정

**설정** <!-- en: Settings --> 윈도우에서 플러그인 페이지는 두 곳에 나타납니다.

- **외관** <!-- en: Appearance --> > **Markdown** — 마크다운 서피스에만 적용할 폰트 설정 · **원격 콘텐츠 허용** <!-- en: Allow remote content --> (기본 꺼짐 — 문서가 불러오는 외부 http/https 리소스 차단).
- **외관** > **HTML** — **기본 확대** <!-- en: Default zoom --> (%) · **색 구성표** <!-- en: Color scheme --> (테마 따름 / 라이트 / 다크) · **원격 콘텐츠 허용** <!-- en: Allow remote content --> (기본 꺼짐 — 외부 http/https 리소스 차단) · **스크립트 샌드박스** <!-- en: Sandbox scripts --> (기본 켜짐).
- **플러그인** <!-- en: Plugins --> > **Claude Code** — **Spawn child 경고 임계치** <!-- en: Spawn child warning threshold --> 등.
- **플러그인** > **Codex** — **Spawn child 경고 임계치** · **기본 승인 정책** <!-- en: Default approval policy --> · **기본 샌드박스 모드** <!-- en: Default sandbox mode -->.

웹 페이지로 그리는 서피스를 추가하는 플러그인은 직접 설치한 것이라도 HTML 과 같은 방식으로 자기 설정 페이지의 확대 · 색 구성표 · 원격 콘텐츠 허용 · 스크립트 샌드박스 값을 그 서피스에 적용합니다. 바꾸지 않은 값은 플러그인이 정한 기본값을 따르고, 확대 단축키를 지원하는 서피스에서는 단축키가 그 플러그인의 확대 값을 바꿉니다.

플러그인 윈도우의 **구성** <!-- en: Configure --> 버튼도 이 페이지로 갑니다.

라이트 테마의 신뢰됨 상자는 본문과 방패 아이콘을 짙은 초록색으로 표시합니다. 서명 판정을 뜻하며, 코드 검토나 실행 안전을 보장하는 표시는 아닙니다.

## 플러그인 윈도우

사이드바 맨 아래 **플러그인** <!-- en: Plugins --> 버튼을 누릅니다. 탭이 셋입니다.

### 설치된 플러그인

**설치된 플러그인** <!-- en: Installed --> 탭. 왼쪽 목록에서 하나를 고르면 오른쪽에 상세가 뜹니다. 목록 위 **설치된 플러그인 필터…** <!-- en: Filter installed… --> 로 이름을 걸러낼 수 있습니다.

- 상세 맨 위 이름 아래 줄에 작성자와 플러그인 id 가 보입니다. 홈페이지가 있으면 같은 줄 끝에 나옵니다. `http://` · `https://` 주소면 밑줄 링크라 누르면 브라우저로 열고, 그 밖의 값은 글자로만 보입니다. 브라우저를 실행하지 못하면 플러그인 윈도우에 열지 못했다는 알림이 뜹니다.
- 목록 행에는 **비활성** <!-- en: Disabled --> 또는 **실행 중** <!-- en: Running --> 이 붙습니다. 활성인데 실행에 실패하면 빨간 표시와 함께 **연결에 실패했습니다. 설정에서 플러그인 구성을 확인하세요.** <!-- en: Failed to connect. Check the plugin's configuration in Settings. --> 안내가 붙습니다.
- **권한** <!-- en: Permissions --> — 이 플러그인이 받은 권한 목록. 여기서는 읽기만 되고 바꾸지는 못합니다.
- **명령** <!-- en: Commands --> — 플러그인이 추가한 단축키 명령. 한 줄에 명령 하나와 그 단축키가 보입니다. macOS 에서는 실제로 누르는 키로 바꿔 Ctrl · Option · Shift · Command 순서로 보이고, Option · Shift · Command 는 **설정** > **일반** 의 수식키 표시 방식(낱말 또는 기호)을 따릅니다. 키는 **설정** > **단축키** > **플러그인** 에서 바꿉니다.
- **설치 경로** <!-- en: Install path --> · **로그** <!-- en: Log --> — 상세의 마지막 절입니다. 긴 경로는 줄바꿈되어 끝까지 보이고, 드래그해 선택·복사할 수 있습니다. 머리글 줄 오른쪽 **폴더 열기** <!-- en: Open folder --> 는 설치 폴더를 파일 관리자로 엽니다. 파일 관리자를 실행하지 못하면 플러그인 윈도우에 열지 못했다는 알림이 뜹니다.
- 상세 아래 바는 상세를 스크롤해도 늘 보입니다.
  - 왼쪽 스위치 — 옆 라벨이 **활성화** <!-- en: Enabled --> / **비활성** <!-- en: Disabled --> 를 보입니다. 라벨을 눌러도 바뀝니다. 끄면 프로세스가 정리되고, 켜면 다시 시작됩니다.
  - **구성** <!-- en: Configure --> — 설정 윈도우의 플러그인 페이지로 이동.
  - **제거** <!-- en: Uninstall --> — 누르면 이 바가 그 자리에서 확인으로 바뀝니다. **취소** <!-- en: Cancel --> 와 **제거** <!-- en: Uninstall --> 버튼이 나오고, 제거를 누르면 설치 폴더가 삭제됩니다. 플러그인 설정은 직접 지울 때까지 남습니다. **기본 제공** <!-- en: built-in --> 플러그인은 다음 실행에서 다시 설치되지 않는다는 안내가 나옵니다. 확인이 뜨면 **취소** 에 키보드 초점이 있어 Enter 로 바로 취소할 수 있고, Esc 를 눌러도 취소됩니다(다른 탭으로 옮긴 뒤에 눌러도 됩니다). 다른 플러그인을 고르면 확인이 취소되어 평소 바로 돌아갑니다.

### 확인 필요

**확인 필요** <!-- en: Attention --> 탭. 등록이 거부됐거나 실행에 실패한 플러그인이 이유와 함께 모입니다. 상세 맨 위에는 설치된 플러그인처럼 이름 · 작성자 · id 와 홈페이지 링크(`http://` · `https://` 주소일 때), 그 아래 설명이 나옵니다. 서명이 유효하지 않은 플러그인은 매니페스트 내용을 믿을 수 없으므로 설명과 홈페이지를 보이지 않고, 설명 자리에 **서명이 유효하지 않아 설명을 숨겼습니다.** <!-- en: Description hidden — the signature is invalid. --> 가 나옵니다.

| 표시 | 뜻 | 할 일 |
|------|----|------|
| **서명을 신뢰할 수 없음** <!-- en: Signature not trusted --> | 신뢰 목록에 없는 키로 서명됨 | 상세에 **검증되지 않은 플러그인** <!-- en: Unverified plugin --> 경고가 나옵니다. 출처를 확인한 뒤 Signature 절의 **지문 복사** <!-- en: Copy fingerprint --> 버튼으로 지문을 복사해 대조합니다. 믿을 수 있으면 **그래도 등록** <!-- en: Register anyway --> 을 누릅니다 |
| **서명이 유효하지 않음** <!-- en: Signature invalid --> | 서명이 없거나 검증 실패. 상세에 실패 원인(서명 파일 없음 등)이 한 줄로 나옵니다 | 배포자에게 올바른 패키지를 받는 것이 안전합니다. 출처를 신뢰한다면 **그래도 등록** <!-- en: Register anyway --> 으로 쓸 수 있습니다 |
| **권한이 변경됨** <!-- en: Permissions changed --> | 업데이트로 요구 권한이 바뀜 | **새로 요청됨** <!-- en: newly requested --> 목록을 보고 **재승인** |
| **실행 오류** <!-- en: Runtime error --> | 활성인데 실행 중 실패 | **로그** 를 확인합니다 |

### 플러그인 추가

**플러그인 추가** <!-- en: Add plugin --> 탭.

1. **플러그인 폴더** <!-- en: Plugin folder --> 입력칸에 `tasty-plugin.toml` 이 들어 있는 폴더를 입력하거나 **폴더 찾기…** <!-- en: Find folder… --> 로 고릅니다. Tasty 폴더 피커에서 하위 폴더를 선택하거나, 아무 행도 고르지 않고 현재 폴더를 확정할 수 있습니다. 더블클릭은 폴더 안으로 이동하고 Enter는 확정합니다. 취소하면 입력은 그대로입니다.
2. 폴더 찾기로 확정하면 즉시 검증합니다. 경로를 직접 입력했다면 **확인** <!-- en: Verify --> 을 누릅니다. 입력칸 바로 아래에 매니페스트 카드가 보입니다. 이름 · 버전 · id · 작성자 · 설명 · **요구 권한** · 서피스 종류 · 원본 경로가 나오고, 홈페이지가 `http://` · `https://` 주소면 밑줄 친 링크를 눌러 브라우저로 열 수 있고(브라우저를 실행하지 못하면 열지 못했다는 알림이 뜹니다), 그 밖의 값은 글자로만 보입니다. 작성자가 여럿이면 첫 작성자 뒤에 `+N` 이 붙고, 마우스를 올리면 전체 목록이 보입니다. 경로를 고치면 카드가 사라지므로 다시 **확인** 을 누릅니다. 폴더에 `tasty-plugin.toml` 이 없거나 형식이 틀려 읽지 못하면 카드 대신 **tasty-plugin.toml을 읽을 수 없습니다** <!-- en: Can't read tasty-plugin.toml --> 상자가 나오고 그 아래 줄에 읽기 오류 원문이 보입니다. 파일은 읽었지만 내용이 규칙에 맞지 않으면(예: 지정한 실행 파일이 폴더에 없음) 같은 자리에 **tasty-plugin.toml이 올바르지 않습니다** <!-- en: tasty-plugin.toml is not valid --> 상자가 나오고 아래 줄에 무엇이 틀렸는지 보입니다. 경로를 고친 뒤 다시 **확인** 을 누릅니다.
3. 카드 아래 상자가 서명 판정을 알려 줍니다 — **신뢰한 게시자가 서명했습니다** <!-- en: Signed by a trusted publisher -->, **권한 변경됨** <!-- en: Permissions changed -->, 또는 **검증되지 않은 플러그인** <!-- en: Unverified plugin -->. 검증되지 않은 플러그인은 원인(신뢰 목록에 없는 키, 공개 키 파일 없음, 서명 없음, 서명 검증 실패)과 함께, 플러그인이 내 계정 권한으로 실행되며 권한 목록은 Tasty에 요청할 수 있는 범위만 제한한다는 경고가 나옵니다. 키가 있는 판정이면 상자에 지문(fingerprint)이 나오고 복사 버튼으로 복사할 수 있습니다. 긴 지문은 앞뒤 8바이트만 보이고 가운데를 `…` 로 줄이며, 마우스를 올리거나 복사하면 전체 값을 얻습니다.
4. 아래 줄 왼쪽에 부여할 권한 수가 보입니다. **플러그인 추가** <!-- en: Add plugin --> 를 누릅니다. 권한 변경이면 같은 자리의 버튼이 **신뢰하고 추가** <!-- en: Trust & add --> 로 바뀌고, 누르면 새 권한 묶음이 신뢰 목록에 기록됩니다. 권한은 플러그인이 Tasty에 요청할 수 있는 범위만 제한할 뿐, 플러그인 프로그램을 내 파일·네트워크·다른 프로그램으로부터 격리하지 않습니다. 검증되지 않은 플러그인이면 버튼이 **그래도 추가** <!-- en: Add anyway --> (신뢰 목록에 없는 키면 **키 신뢰 후 그래도 추가** <!-- en: Trust key & add anyway -->)로 바뀌고, 처음에는 **취소** <!-- en: Cancel --> 에 포커스가 있어 Enter·Esc 는 설치하지 않고 돌아갑니다. 출처를 신뢰할 때만 누르세요. 이 선택은 매니페스트가 바뀌거나 플러그인을 제거할 때까지 기억되고, 설치한 뒤에는 목록의 표시 외에 다른 제한이 없습니다. 추가하는 동안에는 버튼이 **추가하는 중…** <!-- en: Adding… --> 으로 바뀝니다.
5. 이미 설치된 플러그인이면 버튼이 비활성으로 남고 왼쪽에 **이미 설치됨** <!-- en: Already installed --> 이 표시됩니다.

설치하면 플러그인이 요청한 권한이 허용됩니다. 추가하기 전에 미리보기에서 권한 목록을 확인하세요.

## 권한

플러그인은 필요한 권한을 미리 선언합니다. Tasty에 보내는 요청에 필요한 권한이 없으면 해당 요청을 거절합니다.

권한은 플러그인이 Tasty에 요청할 수 있는 기능의 범위를 정합니다. 플러그인 프로세스의 모든 파일·네트워크 접근을 격리한다는 뜻은 아닙니다. 설치하려는 플러그인의 출처와 설명을 확인하세요.

자주 보이는 이름과 뜻:

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
| `agent.turn_report` | 에이전트 작업의 턴 시작·끝 보고만(다른 에이전트 협업 기능은 열지 않음) |
| `completion_strategy.define` | 작업 러너가 자식 에이전트의 완료를 판정하는 방식 정의 |
| `ipc.invoke:<prefix>` | 다른 플러그인이 `<prefix>` 이름으로 제공하는 명령 호출(예: `ipc.invoke:codex` 는 Codex 플러그인 명령) |

기본 제공 플러그인이 받는 권한:

| 플러그인 | 권한 |
|---------|------|
| Markdown Viewer | `surface.read` `surface.write` `fs.read` `file_handler.define` `file_handler.handle:markdown` `ui.settings_page` `ui.popup` |
| Image | `surface.read` `surface.write` `clipboard.read` `fs.read` `fs.write` `file_handler.define` `file_handler.handle:image` |
| HTML Viewer | `surface.read` `surface.write` `file_handler.define` `file_handler.handle:html` `file_handler.handle:svg` `ui.settings_page` |
| Clipboard Viewer | `clipboard.read` `ui.popup` `ui.tool_item` |
| Git Viewer | `ui.popup` `ui.tool_item` `fs.read` |
| Claude Code | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.read` `fs.write` `notification` `telemetry` `agent` `agent.turn_report` `ui.settings_page` `completion_strategy.define` `memory.read` `ipc.invoke:codex` |
| Codex | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.write` `notification` `ui.settings_page` `completion_strategy.define` `agent.turn_report` |

권한을 개별로 빼거나 되돌리는 것은 CLI 로만 합니다(아래).

## `tasty plugin` 명령

Tasty 가 실행 중일 때 터미널에서 씁니다. 출력은 JSON 입니다.

| 명령 | 하는 일 |
|------|---------|
| `tasty plugin list` | 설치된 플러그인의 id · 버전 · 활성 · 실행 여부, 자동으로 꺼졌는지와 마지막 시작 실패 |
| `tasty plugin show <id>` | 매니페스트 · 권한 · 명령 · 실행 상태 전체 |
| `tasty plugin install <폴더> [--allow-unverified]` | `tasty-plugin.toml` 이 있는 폴더에서 설치. 매니페스트 권한을 그대로 허용합니다. 검증되지 않은 플러그인은 `--allow-unverified` 를 줄 때만 설치합니다 |
| `tasty plugin remove <id>` | 제거 |
| `tasty plugin enable <id> [--no-wait]` · `disable <id>` | 켜기 · 끄기. `enable`은 플러그인이 연결될 때까지(최대 15초) 기다리고, 시작하지 못하면 원인과 함께 실패로 끝납니다. 3초가 지나도 시작 중이면 진행 중이라는 안내를 한 번 보입니다 |
| `tasty plugin logs <id> [--follow]` | 로그 출력. `--follow` 는 새 줄을 계속 보여줍니다 (`Ctrl+C` 로 중단) |
| `tasty plugin permissions <id>` | 매니페스트가 요구하는 권한과 실제로 허용된 권한 |
| `tasty plugin grant <id> <권한>` · `revoke <id> <권한>` | 권한 하나 허용 · 회수. 매니페스트에 선언된 권한만 허용할 수 있습니다 |
| `tasty plugin doctor <id>` | 매니페스트 진단 — 이 버전의 Tasty 가 이해하지 못하는 규칙이 있는지 |
| `tasty plugin upgrade-builtins [--force] [--restore-removed <id>] [--restore-removed-all] [--restart-running]` | 기본 제공 플러그인을 번들 버전으로 다시 맞춥니다. `--restore-removed` 는 제거했던 기본 플러그인을 되살리고(여러 번 지정 가능), `--restore-removed-all` 은 제거했던 기본 플러그인을 모두 되살립니다. `--restart-running` 은 실행 파일이 바뀐 플러그인을 다시 시작합니다(그 플러그인의 서피스가 잠깐 닫힙니다) |
| `tasty plugin extension list` | 플러그인이 다른 플러그인에 덧붙인 확장(extension)과 상태 |
| `tasty plugin grant-agent-permission --agent <id> --permission <권한> [--ttl <초>]` · `revoke-agent-permission` | 실행 중인 에이전트 세션에 임시 권한을 주거나 회수합니다. `--ttl` 을 빼면 회수할 때까지 유지됩니다 |
| `tasty plugin list-agent-permissions [--agent <id>]` | 에이전트 세션의 기본 권한과 임시 권한 |
| `tasty plugin request-permission --agent <id> --permission <권한> [--reason <이유>]` | 에이전트에게 권한을 줄지 묻는 승인 팝업을 미리 띄웁니다. 에이전트가 직접 실행하면 자기 자신만 대상으로 할 수 있고, 이 팝업은 사용자만 승인·거절할 수 있습니다 |
| `tasty plugin audit-query` · `audit-summary` · `audit-follow` · `audit-clear` | Tasty 가 받은 요청(CLI·플러그인·에이전트)의 허용·거부 기록을 조회 · 집계 · 실시간 확인 · 삭제합니다 |

`install`은 폴더를 복사하기 전에 서명을 확인합니다. 검증되지 않은 플러그인이면 원인과 위험을 경고하고 설치하지 않으며, 출처를 신뢰할 때 쓸 `tasty plugin install --allow-unverified <폴더>` 명령을 알려 줍니다. 서명 확인에서 설치를 거부하면 실제 이유(서명 없음, 신뢰하지 않은 키, 공개 키 파일 없음 또는 불일치 등)를 알리고, 플러그인 폴더·권한·신뢰 목록에 아무것도 남기지 않습니다. 문제를 고친 뒤 같은 명령으로 다시 설치할 수 있습니다. 등록까지 끝난 뒤 플러그인을 실행하지 못한 경우에는 설치한 폴더가 남습니다. 이때는 `tasty plugin logs <id>`로 원인을 확인하세요.

플러그인을 시작하지 못하면 알림 목록에 경고가 남습니다. 짧은 시간에 여러 번 실패해 Tasty가 플러그인을 자동으로 끄면 오류 알림이 따로 남고, 다시 켤 때까지 시작하지 않습니다. 직접 다시 켜지 않았는데 이어지는 같은 플러그인의 실패는 한 건으로 묶습니다. 직접 켤 때마다 그 결과는 따로 알림으로 남습니다. 원인은 알림 본문과 `tasty plugin logs <id>`에서 확인할 수 있습니다.

`enable`과 `disable`은 설치된 플러그인의 ID만 받습니다. 설치되지 않은 ID를 지정하면 오류로 끝나고 설정은 바뀌지 않습니다. ID는 `tasty plugin list`에서 확인할 수 있습니다.

`disable`은 종료 처리를 시작하며 보통 백그라운드에서 진행합니다. 2초의 정상 종료 유예가 지나도 프로세스가 남아 있으면 강제 종료를 시도합니다. 이 2초는 명령 전체의 시간 상한이 아니며, OS에서 프로세스 종료를 확인하는 동안 더 기다릴 수 있습니다. 그 사이 `enable`하면 이전 프로세스의 종료를 기다린 뒤 새로 시작합니다. `tasty plugin enable`은 이어서 새 플러그인이 연결될 때까지(최대 15초) 기다리고 결과를 알려 줍니다. 바로 돌아오려면 `--no-wait`를 주세요. 이때와 에이전트가 IPC로 직접 요청할 때는 시작을 접수했다는 응답만 받으므로, 결과는 `tasty plugin show <id>`에서 확인합니다. 실행이나 연결이 실패하면 알림과 로그를 확인하세요.

플러그인이 멈춰 그 서피스를 닫는 데 시간이 걸리는 동안에도 다른 명령과 입력은 평소처럼 처리됩니다. 닫기를 기다리는 중에 그 플러그인을 `disable`하면 프로세스가 종료되는 대로 닫기가 성공으로 끝납니다. 그 사이 Tasty를 종료하면 종료는 그 닫기가 끝날 때까지(최대 5초) 기다립니다.

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

- [파일 관리와 보기](../using/files.md) — 마크다운 · 이미지 · HTML 서피스 사용법.
- [Claude·Codex와 함께 쓰기](../agents/claude-codex.md) — Claude Code · Codex 플러그인.
- [설정](../customize/settings.md) — 플러그인 설정 페이지 위치.

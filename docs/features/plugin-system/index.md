# 플러그인 관리 (Plugins)

- **Status**: Implemented
- **주체**: 로컬 사용자(플러그인 창) · AI Agent(`tasty plugin` CLI). 원격 접속 사용자는 mirror 로 봄.
- **설계**: 없음
- **코드**: `src/view/plugins.rs`, `src/view/plugins/ui/`, `crates/tasty-cli/src/commands/plugin_cmd.rs`
- **화면**: [아래 절](#화면)

> 이 문서는 *플러그인을 설치·관리* 하는 사용자/에이전트 기능이다. *플러그인을 제작* 하는 법은 [plugin-development](../../dev-guide/plugin-development.md) · [plugin-permissions](../../dev-guide/plugin-permissions.md) · [plugin-development 민감 데이터](../../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택).

## 목적

플러그인을 설치·활성/비활성·제거하는 기능. [사이드바](../sidebar/index.md) 플러그인 버튼이 여는 **관리 창**(GUI)과 **`tasty plugin` CLI** 두 가지로 제공한다 — tasty-특화 기능이라 [IPC·CLI 제공 기준](../../dev-guide/api-conventions.md#ipc-메서드를-cli로-제공하는-기준)에 따라 IPC/CLI 양면 제공.

## 내부 동작

### 창 — 세 탭

- **Installed (list)**: 설치된 플러그인 목록. 각 항목:
  - **이름 줄과 설명**: 상세 이름은 UI 최대 글자 크기·주 글자색·보통 굵기(굵은 UI 글꼴을 쓰지 않는다)이고, 같은 줄에 버전 Tag(앱 공용 Tag 위젯, mono 작은 글자)와 기본 제공 플러그인이면 기본 Tag `built-in` 이 이어진다. agent 강조색은 built-in 표시에 쓰지 않는다. 설명은 body 크기·보조 글자색·줄 높이 `line-height-ui`(1.4)이며, 상세 열이 넓어도 `measure-lg`(460) 폭에서 줄바꿈한다.
  - **메타 줄**: 상세 이름 줄 아래 `작성자 · id · 홈페이지` 를 ` · ` 로 이은 mono caption·text-muted 한 줄. 작성자가 여럿이면 쉼표로 잇고, 작성자가 없으면 id 가 맨 앞이다. 폭이 모자라면 항목 단위로 다음 줄로 넘어가며, 구분점은 뒤 항목과 함께 넘어가 줄 끝에 점만 남지 않는다. 한 항목이 줄 폭보다 길면 끝을 말줄임한다. 홈페이지가 있으면 마지막 항목이다. scheme 이 `http`·`https`(대소문자 무시)인 주소만 scheme 을 뺀 accent-primary 밑줄 링크로 보이고 누르면 기본 브라우저로 연다. 다른 scheme(`file:`·`javascript:` 등)이나 scheme 없는 값은 다른 항목과 같은 평문으로 보이며 눌러도 열리지 않는다. 본문에 따로 `Homepage:` 줄은 없다. 아바타·이름 줄·메타 줄로 된 정체 블록은 Attention 상세와 같은 위젯(`plugin_detail_identity`)이다.
  - **액션 바**: 상세 아래, 본문 스크롤 밖에 늘 보이는 바. 상세 열 폭 전체를 쓰고 열 아래 끝에 붙으며, 위 1px 구분선이 열 양끝에 닿는다. 왼쪽에 enable/disable Switch 와 현재 상태 라벨(`Enabled`/`Disabled`, 스위치와 한 컨트롤이라 라벨을 눌러도 전환되고 키보드 초점은 한 칸), 오른쪽에 `Configure`(ghost, 톱니 아이콘, 설정 창 Plugins 탭으로 이동)와 `Uninstall`(secondary, 위험 색 글자). 키보드 Tab 순서는 본문 다음 스위치 → Configure → Uninstall 로 화면 순서와 같다.
  - **health error** 인디케이터 (enable 상태인데 오류인 플러그인).
  - **권한 read-only 표시** (창에서 권한을 토글하지 않는다). 상세 `Permissions` 절은 권한마다 버전 표시와 같은 공용 Tag(mono 작은 글자)로 보인다.
  - **절 배치**: 상세의 절 사이에는 구분선이 없고 `space-lg` 만큼 띄운다. 순서는 `PERMISSIONS` → `COMMANDS`(명령이 있을 때) → `SURFACE KINDS`(kind 가 있을 때) → `INSTALL PATH` 다. 머리글은 대문자 mono micro·text-muted 이며 본문과 `space-sm` 떨어진다. Surface kinds 는 Permissions 처럼 kind 마다 공용 Tag 로 보인다.
  - **설치 경로 절**(상세의 마지막 절): 머리글 줄에 대문자 라벨 글꼴 `INSTALL PATH` 와 오른쪽 끝 `Open folder`(secondary sm, 폴더 아이콘, OS 파일 관리자로 연다). 그 아래 설치 경로 한 줄과 `Log: <경로>` 한 줄은 mono caption·text-muted 이고, 공백이 없어도 아무 문자에서 줄바꿈하며 선택할 수 있다. 말줄임·툴팁은 없다. 버튼이 경로 줄에 없으므로 창 최소 폭 720 과 기본 폭 880 에서 긴 경로가 버튼을 밀어내지 않는다.
  - **Commands 절**: 명령마다 한 행. 왼쪽에 명령 제목(라벨 글꼴 term-sm·text-secondary, 길면 말줄임), 오른쪽에 단축키 키캡(할당이 없으면 없음), 행 아래 1px 구분선. 행 높이는 구분선을 포함해 32 이고 아래 여백을 더하지 않는다. 키캡은 매니페스트 조합을 `+` 로 나눠 앞뒤 공백을 빼고, 한 글자 키는 대문자, 나머지는 첫 글자만 대문자로 쓴다(`ctrl + shift + h` → `Ctrl` `Shift` `H`). Windows·Linux 는 이 낱말 키캡을 매니페스트 순서대로 보인다. macOS 는 키바인딩 매핑(`alt` → Command, `option` → Option, `ctrl` → Control)을 거친 뒤 수식키를 Apple 순서 Ctrl · Option · Shift · Command(⌃ ⌥ ⇧ ⌘)로 놓고 키를 맨 뒤에 둔다. 각 수식키는 설정 › 일반의 표시 스타일(`alt_display_style` · `option_display_style` · `shift_display_style`)을 설정 키바인딩 행·수식키 안내와 같이 따른다: `alt` 는 `Alt`/`Cmd`/⌘, `option` 은 `Option`/⌥, `shift` 는 `Shift`/⇧. 기호는 수식키 안내처럼 아이콘 키캡으로 그린다. Ctrl 은 표시 스타일과 글리프가 없어 늘 낱말 `Ctrl` 이다. 표시 스타일은 창을 열 때와 창 안의 플러그인 동작 뒤 목록을 다시 받을 때 읽는다(설정 창과 Plugins 창은 동시에 열리지 않는 modal 이다).
  - **uninstall**: 액션 바의 `Uninstall` 을 누르면 같은 바의 내용이 그 자리에서 확인으로 바뀐다(본문 끝 블록·popup 없음): alertTriangle(16, accent-attention-raised) · `Uninstall {이름}?`(body·text-primary) 아래 안내 한 줄(caption·text-muted) · `Cancel`(ghost) · `Uninstall`(danger). 안내는 기본 제공 플러그인이면 `Built-in — it won't be installed again on the next launch.`, 아니면 `Its files are removed. Settings stay until you delete them.` 다. 확인은 그 plugin id 에 묶이며, 다른 플러그인을 고르면 확인이 취소되어 원래 플러그인으로 돌아와도 평소 바가 보인다. 확인이 열리면 키보드 포커스가 `Cancel` 로 간다. 확인이 열린 동안에는 어느 탭에서든 Esc 가 확인을 취소한다(다른 탭에 가려진 확인도 닫혀, Installed 로 돌아와도 다시 보이지 않는다). 확인이 없을 때 Esc 는 이 창에서 아무 일도 하지 않는다. 짧은 문구는 평소 바와 같은 높이이고, 줄바꿈되는 긴 문구에서는 바가 글 높이만큼 커진다.
- **Attention (확인 필요)**: 등록 거부(서명/신뢰), 예약 id 폴더, 실행 실패(health error) plugin 을 사유·조치와 함께 보여준다. 탭 라벨에 개수를 danger 배지로 표시. 목록 행 끝의 severity 점은 액션 바의 점과 같은 `status-dot-size`(8) 지름이다. 상세 맨 위 정체 블록(아바타 · 이름 줄 · `작성자 · id · 홈페이지` 메타 줄)은 Installed 와 같은 위젯이다. 홈페이지는 `http`·`https` 주소일 때만 링크로 싣고, 다른 값은 메타 줄에 넣지 않는다. 그 아래 매니페스트 설명(Installed 와 같은 설명 위젯, 비면 생략)이 사유 배너 앞에 온다. 서명이 유효하지 않은 사유(`SignatureInvalid`)는 매니페스트가 서명한 내용 그대로라는 보장이 없어 설명과 홈페이지를 숨기고, 설명 자리에 `Description hidden — the signature is invalid.`(caption·text-muted, `measure-lg` 폭에서 줄바꿈)를 둔다. 신뢰하지 않은 키(`UnknownKey`)·권한 변경·실행 오류는 매니페스트 설명과 홈페이지를 그대로 보인다. 정체 블록 · 설명 · 사유 배너 · 사유 detail 은 `space-lg` 간격으로 쌓는다. 액션 바도 Installed 와 같은 바 틀(가로 14 · 세로 `space-md` 여백, 위 1px 구분선, 본문 스크롤 밖 열 바닥)이다. 바는 열 폭 전체를 써서 구분선이 열 양끝에 닿는다. 바 안에는 왼쪽에 status-dot 크기의 점과 `Not registered`/`Needs review`(12, severity 색), 오른쪽에 사유별 버튼 — 권한 변경은 `Re-approve`(primary), 실행 오류는 `Configure`(ghost, 톱니 아이콘), 서명 사유는 `Register anyway`(danger), 예약 id 는 `Remove`(secondary, danger 글자) — 이 있다. 서명 사유의 사유 배너는 Add 탭과 같은 미인증 신뢰 상자(거부 기록의 원인 이름으로 고른 원인 문장과 공통 위험 문단, fingerprint 줄 없음)다. `Register anyway`는 누를 때만 그 폴더에 `AllowUnverified` 선택을 기록하고 등록한다(`plugin_register_unverified`). 행을 고르는 것만으로는 포커스를 이 버튼에 두지 않는다. 사유별 절 머리글(Permission changes · Signature · Log · Folder)은 상세 절처럼 대문자 mono micro·text-muted 다. Signature invalid 항목은 `Signature` 머리글 아래에 고정 설명 `The signature does not match this plugin's files.`과 서명 검증이 실패한 원인(사이드카 파일 없음 · 읽기 오류 · 서명 길이 오류 등) 한 줄을 mono 글자로 보인다.
- **fingerprint 줄**: Attention 서명 절과 Add 신뢰 상자가 같은 줄을 쓴다. colon-hex 값이 16바이트를 넘으면 앞 8바이트와 뒤 8바이트를 ` … `로 이어 한 줄로 보이고, 툴팁과 복사 버튼은 전체 값을 쓴다.
- **Install (add)**: 디렉터리(`tasty-plugin.toml`)에서 설치. 제목 없이 한 화면에서 진행한다.
  - **경로 선택**: `Plugin folder` 머리글 아래 mono 경로 입력, `Find folder…`(폴더 선택 대화상자), `Verify` 버튼과 설명 문단을 둔다. 경로가 비면 `Verify`는 비활성이다. 경로를 고치면 확인한 매니페스트를 버린다. 확인하기 전에는 그 아래에 `Choose a folder and press Verify to read its manifest.` 안내 상자를 두고, 매니페스트를 읽지 못하면(파일 없음·TOML 파싱 실패) 그 자리에 "Can't read tasty-plugin.toml" 오류 상자를, 읽었지만 선언 검사(바이너리 경로·감지기 등)에 실패하면 같은 상자에 "tasty-plugin.toml is not valid" 제목을 둔다. 두 상자 모두 아래 줄에 원문 메시지를 번역하지 않고 보인다.
  - **미리보기**: 경로 선택 바로 아래에 매니페스트 카드(이름·버전, `id · 첫 작성자 +N`(작성자 전체는 툴팁), 설명, 권한·surface 종류 Tag, 원본 경로, 홈페이지 링크)와 그 아래 신뢰 판정 상자. 홈페이지 링크는 text-secondary 글자에 밑줄을 늘 긋고, 마우스를 올리면 text-primary, 키보드 포커스면 focus ring 을 두른다. 누르면 기본 브라우저로 연다. 링크는 `http`·`https` 주소만이고, 다른 값은 같은 자리에 밑줄 없는 text-secondary 평문으로 보이며 열리지 않는다. 권한이나 surface 종류가 없으면 caption 크기 `None`을 적는다.
  - **신뢰 판정 상자 여섯 가지**(`PluginTrustKind`): 신뢰한 게시자(success, 그대로 추가) · 권한 변경(warning, 추가하면 새 권한 묶음을 신뢰하며, 권한은 Tasty 요청 범위만 제한하고 플러그인 프로그램을 격리하지 않는다는 문장을 함께 보인다) · 미인증 네 가지(danger) — 모르는 키 · 공개 키 파일 없음 · 서명 없음 · 서명 검증 실패. 미인증 상자는 같은 제목(`Unverified plugin`, 제목 색 `plugin-trust-title-fg`)과 공통 위험 문단(계정 권한으로 실행되는 프로그램이며 권한 목록은 Tasty 요청 범위만 제한한다)에 원인 문장만 다르고, 모르는 키는 키도 신뢰한다는 문장을 덧붙인다. 글리프 색은 trusted를 포함해 `plugin_trust_glyph_*` 접근자이며, trusted 본문은 `plugin_trust_body_trusted_fg()`다. 두 trusted 전경은 `accent_success_ink()`를 읽고 채움·테두리는 기존 `accent_success()` tint를 유지한다. 키가 있는 판정(권한 변경·모르는 키·공개 키 파일 없음)에는 fingerprint 줄이 붙는다. 서명 검증 실패의 원인은 상자에 쓰지 않고 로그에 남긴다. Add 탭 판정은 설치·부팅과 같은 `trust_verdict::judge_signature` 라 debug 에서도 서명 없는 폴더는 미인증이다.
  - **액션 바**: 매니페스트를 확인하기 전에는 Cancel 만 둔다. 확인한 뒤에는 왼쪽에 부여할 권한 수(`No permissions` · `Grants 1 permission` · `Grants N permissions`, caption), 오른쪽에 Cancel과 `Add plugin`. Cancel 은 경로와 미리보기를 비운다. 권한 변경은 같은 Primary 버튼이 `Trust & add`이고 `TrustAndInstall`로 설치한다. 미인증 네 가지는 버튼이 danger `Add anyway`(모르는 키는 `Trust key & add anyway`로 `TrustAndInstall`, 나머지는 `InstallUnverified`)이고, 판정을 처음 보일 때 Cancel 이 포커스를 받아 Enter 는 취소다. Esc 도 취소다. 추가를 보낸 뒤 결과가 오기 전에는 두 버튼을 라벨과 폭 그대로 disabled로 두고 버튼 왼쪽에 Spinner 와 `Adding…`(caption `text-muted`)을 보이며 두 번째 요청을 보내지 않는다. 실패하면 판정을 남기고 성공하면 입력 화면으로 돌아간다. 추가할 수 없는 경우는 이미 설치됨뿐이며, 버튼을 disabled로 두고 왼쪽 문구를 그 이유로 바꾼다. 이미 설치된 플러그인도 신뢰 상자는 판정대로 그린다.

### CLI (`tasty plugin …`)

`list` / `show <id>` / `install <path>` / `remove <id>` / `enable <id>` / `disable <id>` 등. CLI install 은 사용자 의도적 명령이라 매니페스트 권한을 자동 grant.

설치(GUI Add 탭·`plugin.install`)는 폴더를 복사하기 전에 신뢰 판정을 한다. 판정은 debug·release 모두 같은 함수(`trust_verdict::judge`)이고, 키로 출처를 확인하지 못한 미인증 플러그인은 호출자가 명시적으로 고른 경우에만 설치한다. 판정에서 거부되면 플러그인 폴더·명령·번역·권한 grant·신뢰 DB를 하나도 쓰지 않고, 실패 응답에 실제 원인(서명 없음·모르는 키·공개 키 파일 없음·공개 키 불일치·검증 오류)을 담는다. 미인증이라 거부한 `plugin.install` 오류(`-32000`)는 `data`에 `{"unverified": "<원인>", "plugin_id": "<id>"}`를 싣는다. 원인 이름은 `unknown-key` · `missing-pubkey` · `unsigned` · `signature-error`다. 복사한 뒤 패키지 목록에 들어가지 않으면 복사한 폴더와 이번 시도가 바꾼 신뢰 DB 항목을 되돌리고, 그 거부 사유로 실패한다. 같은 ID에 이미 있던 신뢰 기록·grant·비활성 표시와 다른 플러그인의 기록은 그대로 남는다. 그래서 비활성 표시가 있어도 등록되지 않은 설치를 성공으로 알리지 않고, 신뢰 판정에서 거부된 뒤 고친 플러그인을 다시 설치하면 `already installed`로 막히지 않는다. 되돌림 범위는 신뢰 판정·등록 거부까지다. 등록한 뒤 활성화(`enable after install failed`)가 실패하면 설치 폴더와 grant는 남고 오류만 돌려준다. 공개 키를 신뢰하고 설치할 때는 그 키로 서명이 검증되는지 먼저 확인하고, 다르면 신뢰 DB에 쓰지 않고 실패한다. GUI 의 추가·신뢰·미인증 선택과 확인 필요 탭의 등록은 화면이 보인 매니페스트의 SHA-256 을 싣는다. 설치 시점의 매니페스트가 그 값과 다르면(예: Verify 뒤 같은 키로 다시 서명해 권한을 늘린 경우) 신뢰·grant·선택을 기록하지 않고 다시 확인하라는 오류로 끝난다. 판정한 뒤 복사하는 사이에 원본이 바뀌어도 복사본의 매니페스트를 판정한 값과 대조해 되돌린다. CLI 설치는 미리보기가 없어 명령 시점의 내용을 판정한다.

플러그인을 띄우지 못하거나 연결하지 못하면 알림 목록에 경고가 남고, 실패가 쌓여 자동으로 끄면 오류가 남는다. 사용자가 켜지 않았는데 이어지는 같은 플러그인의 실패는 10 초 창 안에서 한 건으로 묶는다. 사용자가 다시 켜면 새 묶음이 시작되므로, 실패한 enable 마다 그 결과가 알림으로 남는다. 에이전트는 `plugin.enable` 응답의 기동 세대로 `plugin.show`의 `start`를 읽어 그 요청의 연결 성공·실패·취소를 확인하고, `plugin.list`·`plugin.show`의 `auto_disabled`·`last_failure`로 상태를 본다. 계약은 [플러그인 개발](../../dev-guide/plugin-development.md)의 "기동과 연결"에 있다.

- Given 실행 비트가 없는 진입점, When `tasty plugin enable <id>`, Then 명령은 `spawn_failed` 또는 `connect_failed` 원인과 함께 실패로 끝나고 알림 목록에 경고 한 건이 생긴다.
- Given 기동 중인 플러그인, When 연결 전에 disable 하면, Then 그 기동 세대는 `cancelled`이고 실패 알림은 없다.
- Given 사용자가 다시 켜지 않은 채 10 초 안에 세 번 기동에 실패한 플러그인, When 상태를 보면, Then `auto_disabled`가 true 이고 알림은 경고 한 건과 자동 비활성화 오류 한 건이다.
- Given 실행할 수 없는 진입점을 가진 플러그인, When 10 초 안에 `tasty plugin enable` 을 세 번 하면, Then 앞의 두 번은 각각 경고를 남기고 세 번째는 자동 비활성화 오류 한 건을 남기며 `auto_disabled`가 true 다. 네 번째 enable 은 자동 비활성화를 풀고 다시 시작한다.

미인증 선택은 `known-plugins.toml`의 `[unverified."<id>"]` 표에 매니페스트 SHA-256, 원인 이름, 선택 시각으로 남는다. 매니페스트가 바뀌면 기록이 맞지 않아 다시 묻고, 플러그인을 제거하면 기록을 지운다. 선택한 뒤에는 목록의 미인증 표시 외에 실행 제한을 두지 않는다. 현재 서명 판정은 매니페스트 한 파일(`tasty-plugin.toml`)이 신뢰한 키로 서명됐는지만 확인한다. 바이너리·부속 파일의 내용이나 악성 여부를 확인하지 않고 실행 중 동작을 막지도 않는다([플러그인 패키징](../../dev-guide/plugin-packaging.md#서명)). 소스 검토와 검증 빌드 바이너리에 대한 인증은 [플러그인 원칙](../../principles/plugins.md)이 정한 향후 기능이며, 이 판정의 `Trusted`를 그 인증으로 읽지 않는다. 플러그인의 행위 책임은 플러그인과 그것을 고른 사용자에게 있다. 본체는 이 선택을 근거로 외부 권한이나 설정을 스스로 바꾸지 않는다. CLI `tasty plugin install`은 `--allow-unverified`(IPC `allow_unverified: true`)가 없으면 미인증 플러그인을 거부하고, 오류 `data.unverified`로 원인을 알아 현재 언어의 경고와 그 플래그를 쓰는 명령을 표준 오류에 보인다. release 부팅 탐색은 신뢰했거나 사용자가 고른 플러그인만 등록하고, debug 부팅 탐색은 작업 공간의 번들 plugin 이 서명 없이 빌드되기 때문에 서명을 건너뛴다.

#### 예약 id (`com.tasty.`)

`com.tasty.`로 시작하는 id는 공식 플러그인 전용이다. [공식 id 목록](../../dev-guide/plugin-packaging.md#공식-id-목록)에 없는 `com.tasty.*` 플러그인은 debug·release 모두, 서명 판정보다 먼저 거부한다. 이 판정은 id만 본다. 공식 id를 쓴 다른 바이너리는 위 서명 판정이 다룬다.

- 설치(GUI Add 탭·`plugin.install`·확인 필요 탭의 등록)는 공통 매니페스트 검증에서 실패하므로 폴더·신뢰 DB·권한을 쓰지 않는다. `--allow-unverified`로도 설치되지 않는다. Add 탭은 "tasty-plugin.toml is not valid" 상자 아래 줄에 원문 이유를 보이고 미리보기·추가 버튼을 두지 않는다. `plugin.install` 오류(`-32000`)의 `data`는 `{"reserved_id": "<id>", "plugin_id": "<id>"}`다.
- 플러그인 폴더에 직접 넣은 것은 부팅·재탐색 때 등록하지 않고 경고 로그를 남긴다. 그 폴더가 새로 보일 때 알림 목록에 경고 한 건(제목 `Plugin '<id>' was not registered`, 본문은 접두 사유와 Plugins › Attention 안내)을 남기고, 같은 폴더가 남아 있는 동안 재탐색마다 다시 알리지 않는다. 폴더를 자동으로 지우지 않는다.
- 그 폴더는 확인 필요 탭에 폴더마다 한 행으로 보이고 탭 배지와 사이드바 배지에 함께 센다. 행과 상세는 폴더 이름과 매니페스트 id만 보인다. 공식이 아닌 매니페스트는 자기를 Tasty처럼 적을 수 있어 표시 이름·작성자·설명·홈페이지·버전을 보이지 않는다. 같은 id의 폴더가 둘이면 행도 둘이고 각각 고를 수 있다. 조치는 `Remove`뿐이다. 행을 고른 것만으로 포커스가 `Remove`로 가지 않는다.
- `Remove`를 누르면 바가 그 자리에서 확인으로 바뀌고 포커스가 `Cancel`에 간다. Esc·`Cancel`·다른 행 선택이 취소한다. 확인하면 `Removing…` 상태로 두 버튼이 비활성이 되고, 끝날 때까지 Esc와 행 클릭을 무시한다. 좁으면 버튼 묶음이 아래 줄로 내려가고, 더 좁으면 글리프도 글 위로 올라간다.
- 제거는 파일 작업 줄의 작업자 스레드에서 그 폴더만 지운다. 차례가 오면 경로가 아직 예약 id 목록에 있는지 다시 찾고, 지우기 직전에 부모가 플러그인 루트와 정확히 같은지, 링크가 아닌 디렉터리인지, 매니페스트가 여전히 거부되는 id인지 확인한다. 조건에 맞지 않거나 매니페스트를 읽지 못하면 지우지 않고 실패한다. 매니페스트 파일이 없으면 앞선 제거가 멈춘 잔여로 보고 지운다. `known-plugins.toml`·`plugins.toml`은 바꾸지 않는다. 같은 id의 다른 폴더가 있을 수 있기 때문이다.
- 성공하면 행이 빠지고 두 배지가 줄며 창에 토스트 `Removed '<폴더>'.`가 뜬다. 폴더를 앱 밖에서 이미 지웠어도 목표 상태이므로 성공으로 처리한다. 실패하면 창에 토스트 `Remove failed: <이유>`가 뜨고 바가 평소로 돌아가며 행은 남아 다시 시도할 수 있다. 링크 폴더는 이 경우로 실패하고 대상을 건드리지 않는다. 행은 링크를 직접 지울 때까지 남는다. 창을 닫은 뒤 끝나면 실패만 메인 창 토스트로 알린다.
- 한계: `remove_dir_all`이 도중에 실패하고 매니페스트가 이미 지워졌다면, 다음 재탐색이 그 폴더를 건너뛰어 행 없이 파일이 남을 수 있다. 재탐색 전에는 행이 남아 다시 시도할 수 있다.
- 등록되지 않은 id라 `plugin.list`에 없다. `tasty plugin remove <id>`·`enable`·`disable`은 폴더를 지우지 않고 실패하며, 예약 id 목록에 있는 id면 오류 문구가 확인 필요 탭의 제거를 안내한다. 같은 id의 폴더가 여럿일 수 있어 id로는 지울 폴더를 정할 수 없다.
- 번들 설치·`upgrade-builtins`는 번들 폴더 매니페스트의 id가 폴더 이름의 공식 id와 다르면 복사·권한 기록 없이 실패로 남긴다. `upgrade-builtins` 보고에서는 `Failed`다.

- Given `id = "com.tasty.fake"` 매니페스트 폴더, When `tasty plugin install <폴더>`(서명 여부·`--allow-unverified` 무관), Then 명령은 실패하고 `data.reserved_id`가 그 id 이며, 플러그인 폴더·`known-plugins.toml`·`plugins.toml`이 바뀌지 않는다.
- Given 같은 폴더, When GUI Add 탭에서 Verify, Then "tasty-plugin.toml is not valid" 상자에 예약 id 이유가 보이고 추가 버튼이 없다.
- Given 플러그인 폴더에 직접 넣은 `com.tasty.fake`, When 인스턴스를 시작한다, Then `plugin.list`에 없고 경고 로그와 경고 알림 한 건이 남는다.
- Given 폴더 이름이 id와 다르고 같은 id의 다른 폴더가 있는 예약 id 항목, When 그 행에서 `Remove`를 확인한다, Then 그 폴더만 지워지고 다른 폴더는 남으며 탭·사이드바 배지가 하나 줄어든다.
- Given 예약 id 행의 확인 상태, When 그 폴더의 매니페스트가 허용되는 id로 바뀐 뒤 확인한다, Then 폴더는 남고 실패 토스트가 뜨며 바가 평소로 돌아간다.
- Given 예약 id 폴더, When `tasty plugin remove <id>`, Then 명령은 실패하고 오류가 확인 필요 탭을 안내하며 폴더는 남는다. 목록에 없는 id의 오류는 `not installed`다.
- Given 공식 목록의 id(`com.tasty.agent-stream`·`com.tasty.mesh-demo` 포함) 또는 `com.example.x` 같은 다른 접두, When 설치·탐색, Then 서명 판정만 적용된다.

`enable <id>` / `disable <id>` 는 발견된 설치 패키지가 있는 ID만 받는다. 미설치 ID는
GUI·headless 모두 실패하며, 설정 파일과 메모리의 활성/비활성 상태를 바꾸거나 성공 이벤트를
발행하지 않는다. 토글 실패 응답은 기존 `-32000`과 `enable failed:` / `disable failed:`
접두어를 유지하고 `plugin '<id>' not installed`를 이유로 돌려준다. 예약 id 목록에 있는 id는 그 이유
대신 확인 필요 탭의 제거를 안내한다. 설치·부팅은 패키지 목록을 채운 뒤 토글을 수행한다.

끄거나(`disable`) 지우면(`remove`) 그 플러그인이 등록한 surface kind 는 재부팅 없이 **철회**된다
([플러그인 제작 가이드](../../dev-guide/plugin-development.md)). 그 kind 로
새 surface 를 만들려는 요청은 "그 kind 를 제공하던 plugin 이 꺼졌거나, 다시 켠 뒤 아직 연결되지 않았다" 는
사유로 거절되고(IPC 오류 문장, 사용자 조작이면 경고 toast), `surface.kinds` · 서피스 변환 팝업 목록에서 빠진다. 이미 열린 surface 는
닫거나 바꾸지 않는다 — 다시 켜면 이어진다. 닫은 탭 복원 · 프리셋 적용처럼 복원하는 경로는 그 자리를
kind 대기 placeholder 로 두었다가 다시 켜면 채운다.

### 설정(configure)

플러그인별 설정은 이 창이 아니라 [설정 창](../settings/index.md) 의 Plugins 탭에서 편집한다.

## 인터페이스

- **사용자(GUI)**: 사이드바 플러그인 버튼 → 관리 창. 탭 전환, 토글/설치/제거.
- **AI Agent(CLI)**: `tasty plugin {list,show,install,remove,enable,disable}`.
- **연결**:
  - 플러그인 설정 → [`features/settings/`](../settings/index.md) (Plugins 탭)
  - 플러그인 제작/권한/민감데이터 → [plugin-development](../../dev-guide/plugin-development.md) · [plugin-permissions](../../dev-guide/plugin-permissions.md) · [plugin-development 민감 데이터](../../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)

## 비-목표

- 플러그인 *제작*(SDK·매니페스트·권한 모델·서명) — dev-guide.
- 마켓플레이스(registry/install-by-id) — 현재 미도입(보류, [플러그인 권한 모델](../../dev-guide/plugin-permissions.md)).
- 권한 *변경* UI — 창은 read-only. (권한은 설치 시 grant.)

## Acceptance Criteria

- 사이드바 플러그인 버튼 클릭 시 관리 창이 열린다 (Installed / Attention / Install 탭).
- Installed 에서 enable/disable 토글이 동작하고, 오류 플러그인에 health 인디케이터가 뜬다.
- Given 창 최소 폭 720 또는 기본 폭 880 과 열 폭보다 긴 설치 경로, When 그 플러그인을 고르면, Then `Open folder` 와 설치 경로·로그 경로 전체가 창 안에 보인다.
- Install 탭에서 디렉터리 설치 시 매니페스트·권한 미리보기와 신뢰 검증을 거친다.
- `tasty plugin list/install/remove/enable/disable` CLI 가 동일 동작을 수행한다.
- Given 신뢰 판정에서 거부되는 플러그인과 같은 ID의 비활성 표시·grant, When 설치하면, Then 실제 원인이 담긴 오류로 끝나고 플러그인 폴더·`plugins.toml`·신뢰 DB가 설치 전과 같다.
- Given 서명하지 않은 키의 공개 키 파일, When 그 키를 신뢰하고 설치하면, Then 공개 키 불일치 오류로 끝나고 신뢰 DB에 항목이 남지 않는다.
- 미설치 ID의 enable/disable은 GUI·headless 모두 실패하고, 기존 설정 파일은 바이트 단위로 유지되며 없던 설정 파일을 만들지 않는다.
- 설치된 plugin은 disable 후 enable로 다시 실행할 수 있다.
- Given surface kind 를 등록한 plugin 이 켜져 있고 When 그 plugin 을 disable 하거나 remove 하면 Then 그 kind 로 새 surface 를 만드는 요청은 "제공 plugin 이 꺼졌거나 아직 다시 연결되지 않았다" 는 사유로 거절되고 `surface.kinds` 에서 빠지며, 이미 열린 그 kind 의 surface 는 그대로 남는다.
- Given 위처럼 철회된 kind 가 있고 When 그 plugin 을 다시 enable 하면 Then 그 kind 로 다시 만들 수 있다.
- 플러그인 설정은 설정 창 Plugins 탭에 나타난다 (이 창 아님).

> GUI 는 스크린샷, 설치/관리 동작은 `tasty plugin` CLI 시나리오로 검증.

## 구현

- 창: `src/view/plugins.rs` (`PluginsView`), `src/view/plugins/ui/list.rs`(Installed) / `add.rs`(Install).
- CLI: `crates/tasty-cli/src/commands/plugin_cmd.rs` (`PluginCommands`).

## 화면

화면정의서 — **플러그인 관리 창 화면**.

- **트리거 위치**: [사이드바](../sidebar/index.md#화면) 하단 **플러그인 버튼**
- **시각 소스**: tasty-page `vendor/ui_kits/terminal/overlays/plugins_window.jsx`

### 트리거

사이드바 하단 **플러그인 버튼** 클릭 → 플러그인 관리 모달 창이 열린다.

### 레이아웃

```
┌──────────────────────────────────────────┐
│ [Installed]  [Attention]  [Install]       │  탭
├──────────────────────────────────────────┤
│ Installed:                                │
│  ▸ plugin A          [enable ▢]  [⌫]      │  목록 — 토글 / uninstall
│    권한: …(read-only)                      │
│  ▸ plugin B  ⚠health  [enable ▣]          │
│                                            │
│ Install:                                  │
│  경로: …/tasty-plugin.toml                 │
│  매니페스트 미리보기 + 권한 미리보기          │
│  [신뢰 검증] → [ Add ]                      │
└──────────────────────────────────────────┘
```

### UI 요소 인벤토리

- **탭**: Installed / Attention(확인 필요 — 등록 거부·예약 id 폴더·실행 실패 plugin, 개수 danger 배지, 예약 id 행의 Remove) / Install.
- **Installed 항목**:
  - 상세 아래 액션 바의 enable/disable Switch·Configure·Uninstall, health error 인디케이터(오류 플러그인).
  - 권한 표시 — **read-only** (창에서 권한 토글 없음).
  - 설치 경로 절 — `INSTALL PATH` 머리글 줄 오른쪽 `Open folder`, 줄바꿈·선택 가능한 설치 경로와 로그 경로. **uninstall**.
- **Install 폼**:
  - 디렉터리 경로(`tasty-plugin.toml`).
  - 매니페스트 + 권한 **미리보기**.
  - **신뢰/서명 상태** — Trusted 면 바로 Add, 권한 변경 시 재신뢰(TrustAndInstall) 후 Add.
- **설정(configure)** 은 여기 없음 → [설정 창](../settings/screens/settings.md) Plugins 탭.

### 상태별 시각

- **health error**: enable 상태인데 오류인 플러그인에 빨간 인디케이터/박스.
- **신뢰 상태**: Trusted / PermissionsChanged 등에 따라 Install 버튼·안내 문구가 달라진다.

### 시각 소스

tasty-page `vendor/ui_kits/terminal/overlays/plugins_window.jsx` — 창 치수·탭·목록·설치 폼 배치의 단일 출처. 스크린샷: tasty-page `vendor/screens/plugins_window-installed.png`, `plugins_window-install.png`, `plugins_window-modal.png`.

Find folder…는 같은 플러그인 View의 Tasty 폴더 피커를 연다. 선택/현재 폴더를 확정하면 경로 입력을 채우고 기존 매니페스트·서명 검증을 실행한다. 취소는 입력을 보존하며 피커의 Esc는 제거 확인으로 전달하지 않는다.

미인증 미리보기 위에서 폴더 피커를 열었을 때도 첫 Esc는 피커만 취소하고 입력 경로와 미리보기를 보존한다. 피커가 닫힌 뒤의 Esc는 기존 미인증 취소 동작을 유지한다.

# 플러그인 관리 (Plugins)

- **Status**: Implemented
- **주체**: 로컬 사용자(플러그인 창) · AI Agent(`tasty plugin` CLI). 원격 접속 사용자는 mirror 로 봄.
- **ADR**: 없음
- **코드**: `src/view/plugins.rs`, `src/view/plugins/ui/`, `crates/tasty-cli/src/commands/plugin_cmd.rs`
- **화면**: [screens/plugins-window.md](screens/plugins-window.md)

> 이 문서는 *플러그인을 설치·관리* 하는 사용자/에이전트 기능이다. *플러그인을 제작* 하는 법은 [plugin-development](../../dev-guide/plugin-development.md) · [plugin-permissions](../../dev-guide/plugin-permissions.md) · [plugin-sensitive-data](../../dev-guide/plugin-sensitive-data.md).

## 목적

플러그인을 설치·활성/비활성·제거하는 기능. [사이드바](../sidebar/index.md) 플러그인 버튼이 여는 **관리 창**(GUI)과 **`tasty plugin` CLI** 두 표면을 가진다 — tasty-특화 기능이라 [identity §2.2](../../identity.md) 에 따라 IPC/CLI 양면 제공.

## 내부 동작

### 창 — 두 탭

- **Installed (list)**: 설치된 플러그인 목록. 각 항목:
  - **enable/disable** 토글.
  - **health error** 인디케이터 (enable 상태인데 오류인 플러그인).
  - **권한 read-only 표시** (창에서 권한을 토글하지 않는다).
  - **install dir 열기**, **uninstall**.
- **Install (add)**: 디렉터리(`tasty-plugin.toml`)에서 설치.
  - 매니페스트 + 권한 **미리보기**.
  - **서명/신뢰 검증**: `Trusted` 이면 바로 설치, 서명/권한이 바뀐 경우(`PermissionsChanged`) 재신뢰(`TrustAndInstall`) 후 설치.

### CLI (`tasty plugin …`)

`list` / `show <id>` / `install <path>` / `remove <id>` / `enable <id>` / `disable <id>` 등. CLI install 은 사용자 의도적 명령이라 매니페스트 권한을 자동 grant.

`enable <id>` / `disable <id>` 는 발견된 설치 패키지가 있는 ID만 받는다. 미설치 ID는
GUI·headless 모두 실패하며, 설정 파일과 메모리의 활성/비활성 상태를 바꾸거나 성공 이벤트를
발행하지 않는다. 토글 실패 응답은 기존 `-32000`과 `enable failed:` / `disable failed:`
접두어를 유지하고 `plugin '<id>' not installed`를 이유로 돌려준다. 설치·부팅은 패키지
목록을 채운 뒤 토글을 수행한다.

끄거나(`disable`) 지우면(`remove`) 그 플러그인이 등록한 surface kind 는 재부팅 없이 **철회**된다
([ADR-0534](../../adr/0534-a-disabled-plugins-surface-kinds-are-withdrawn-not-erased.md)). 그 kind 로
새 surface 를 만들려는 요청은 "그 kind 를 제공하던 plugin 이 꺼졌거나, 다시 켠 뒤 아직 연결되지 않았다" 는
사유로 거절되고(IPC 오류 문장, 사용자 조작이면 경고 toast), `surface.kinds` · 서피스 변환 팝업 목록에서 빠진다. 이미 열린 surface 는
닫거나 바꾸지 않는다 — 다시 켜면 이어진다. 닫은 탭 복원 · 프리셋 적용처럼 복원하는 경로는 그 자리를
kind 대기 placeholder 로 두었다가 다시 켜면 채운다.

### 설정(configure)

플러그인별 설정은 이 창이 아니라 [설정 창](../settings/index.md) 의 Plugins 탭에서 (연결 개념).

## 인터페이스

- **사용자(GUI)**: 사이드바 플러그인 버튼 → 관리 창. 탭 전환, 토글/설치/제거.
- **AI Agent(CLI)**: `tasty plugin {list,show,install,remove,enable,disable}`.
- **연결**:
  - 플러그인 설정 → [`features/settings/`](../settings/index.md) (Plugins 탭)
  - 플러그인 제작/권한/민감데이터 → [plugin-development](../../dev-guide/plugin-development.md) · [plugin-permissions](../../dev-guide/plugin-permissions.md) · [plugin-sensitive-data](../../dev-guide/plugin-sensitive-data.md)

## 비-목표

- 플러그인 *제작*(SDK·매니페스트·권한 모델·서명) — dev-guide.
- 마켓플레이스(registry/install-by-id) — 현재 미도입(보류, [ADR-0010](../../adr/0010-plugin-marketplace-deferred.md)).
- 권한 *변경* UI — 창은 read-only. (권한은 설치 시 grant.)

## Acceptance Criteria

- 사이드바 플러그인 버튼 클릭 시 관리 창이 열린다 (Installed / Install 탭).
- Installed 에서 enable/disable 토글이 동작하고, 오류 플러그인에 health 인디케이터가 뜬다.
- Install 탭에서 디렉터리 설치 시 매니페스트·권한 미리보기와 신뢰 검증을 거친다.
- `tasty plugin list/install/remove/enable/disable` CLI 가 동일 동작을 수행한다.
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

- [screens/plugins-window.md](screens/plugins-window.md) — 관리 창 레이아웃(Installed/Install 탭)과 연결.

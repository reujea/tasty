# Remote connections 창 화면 (remote_tool)

- **부모 기획**: [../index.md](../index.md)
- **트리거 위치**: [도구 메뉴](../../tools-menu/index.md#화면) `Remote connections`
- **시각 소스**: `site/vendor/ui_kits/terminal/overlays/remote_tool.jsx` — claude design.
- **구조**: 공통 헤더 + 상단 3탭 `[원격 접속 프로필] [Attach] [Passkey]`, 각 탭이 List / Form / ConfirmDelete 라우팅. 520×460, headless.

## 트리거

도구 메뉴 `Remote connections` 클릭 → 원격 접속 popup.

## 레이아웃

```
┌──────────────────────────────────┐
│ Remote connections          [×]   │  공통 헤더
├──────────────────────────────────┤
│ [원격 접속 프로필] [Attach] [Passkey]│  상단 3탭
├──────────────────────────────────┤
│ 프로필 목록         [+ 추가][⤓필터]│  add-bar (필터는 프로필 탭 전용)
│ ▸ prod-box   user@host:22   [✎][⌫]│  목록 — 편집/삭제
│ ▸ staging    …                   │
│ ─── SSH CONFIG 에서 ~/.ssh/config  3│  읽기 전용 섹션 (프로필 탭 전용)
│   gx10                            │  2줄 행 — alias
│   deploy@10.0.0.5:2200 [+ 프로필 추가]│  우측 ghost 액션 하나
│   prod-web / deploy@10.0.4.12 (등록됨)│  이미 등록됐으면 Tag
├──────────────────────────────────┤
│ 폼 (추가/편집):                    │
│  name / host / user / port        │
│  label / shell ▾ / passkey ▾      │
│  [ 저장 ]  [ 취소 ]                │
└──────────────────────────────────┘
```

## UI 요소 인벤토리

### 목록 스크롤 (3탭 공통)

세 탭의 목록(원격 접속 프로필 · Attach · Passkey)은 **스크롤바를 그리지 않는다.** 대신
스크롤할 내용이 남아 있는 쪽 가장자리에 `bg-panel` → 투명 세로 그라디언트(높이 `space-xl`)를
덮어 "이 위/아래에 더 있다"를 알린다(띠 높이는 뷰포트 절반을 넘지 않게 클램프 — 좁은
뷰포트에서 위·아래 띠가 포개져 콘텐츠를 덮는 것을 막는다). 위·아래 각각 독립 판정이라 중간에서는 양쪽 다 보이고,
끝까지 스크롤하면 그쪽 페이드만 사라진다. 스크롤 자체(휠·드래그·키보드)는 그대로다.

egui 기본 스크롤바는 콘텐츠 위에 **오버레이**로 뜬다 — 레이아웃 폭을 미리 빼지 않으므로 행
우측 끝 아이콘(가져오기/편집/삭제/재감지)과 같은 자리를 차지하고, 커서를 아이콘에 올리는
순간 나타나 클릭을 먹는다. 스크롤바 폭만큼 콘텐츠를 비켜 그리는 방식은 "커서가 스크롤바 위 =
클릭 불가" 구조를 남겨 다른 폭·해상도에서 재발하므로 채택하지 않았다. 이 선택은 tasty 전체의
스크롤 어포던스 표준이다 — [ADR-0079](../../../adr/0079-scroll-affordance-standard.md).

### 원격 접속 프로필 탭

- **프로토콜 필터** (원격 접속 프로필 탭 전용): add-bar 우측의 `Filter` 버튼(funnel 아이콘). 현재 프로필에 존재하는 프로토콜(`kind`, tasty-attach 제외)이 2종 이상일 때만 표시. 클릭 시 체크박스 드롭다운(프로토콜 목록 + `모두 선택`/`모두 해제`/`초기화`/`적용`). Apply-on-confirm(적용 눌러야 반영), 선택된 프로토콜만 목록에 표시. 결과 0건이면 "선택한 프로토콜에 해당하는 프로필이 없습니다" 빈 상태. 필터 상태는 **세션 한정·비영속**(popup 재오픈에는 유지, tasty 재시작 시 전체 선택으로 리셋).
- **프로필 목록**: 각 행 = 이름 + 요약(user@host:port) + 편집/삭제. **tasty-attach kind 는 이 목록에 나오지 않는다**(Attach 탭 전담).
- **추가/편집 폼 (ssh)**: name · host · user · port · label 텍스트 입력 + **shell**(콤보박스, 선택 가능 셸 목록 + `auto`) + **passkey**(저장된 passkey 선택 드롭다운, `passkey_ref`) + 저장/취소 — **순수 연결정보만**(remote_tasty 는 Attach 탭으로 이관, ADR-0032). (라벨 키 `field_name`/`field_host`/`field_user`/`field_port`/`field_label`/`field_shell`/`field_passkey`.)
  - **인증 = passkey**: GUI 폼에 `identity_file` 입력은 없다 — passkey 를 고르면 실제 ssh `-i` 경로는 내부에서 `passkey_ref → Passkey.path` 로 resolve 한다(`crates/tasty-ssh/src/lib.rs`).
  - **`port_mode` 는 ssh 폼 입력이 아니다**: shell 선택에서 자동 도출되는 내부 필드(`shell_to_port_mode`). `auto` 면 저장 후 워커가 감지해 채운다. (attach 가 명시 override 하려면 Attach 폼의 Port mode.)
  - (`use_agent` / `extra_options` / `remote_command` 은 폼에 없음 — 파일 직접 편집.)
- **검증 에러**: 이름 빈 값/중복, host 빈 값, port 형식, 저장 실패 메시지.
- **로컬 SSH config 섹션** (원격 접속 프로필 탭 전용, 프로필 목록 **아래 같은 스크롤**, 한 tier 아래): `border-frame` 구분선 + 섹션 헤더(대문자 `ssh config 에서` 라벨 + config 경로 mono + 우측 호스트 수) + 행들. 섹션 본문은 프로필 행보다 `space-xs` 만큼 들여쓰고, 구분선은 들여쓰지 않는다 — 들여쓰기가 "프로필 목록 아래 한 tier" 라는 관계를 말하는 자리다.
  - 행은 **2줄**이다 — alias(`text-secondary`, body) 위에 `user@host:port`(mono, caption, `text-muted`). 둘 다 폭이 좁으면 말줄임한다. 우측 슬롯을 먼저 잡아 긴 alias 가 액션을 밀어내지 않는다.
  - **아이콘 버튼이 없다.** 행 액션은 ghost `프로필 추가` 버튼 하나(leading `+`)이고, 이미 등록된 호스트는 액션 대신 `등록됨` Tag 를 보인다 — 같은 호스트를 두 번 등록하는 사고를 비활성 버튼이 아니라 **상태 표시**로 막는다.
  - **읽기 전용**이다 — 여기 나열되는 것은 tasty 레코드가 아니라 사용자의 `~/.ssh/config` 라, tasty 가 편집·삭제하지 않는다. 그래서 행 액션이 가져오기 하나뿐이다.
  - **프로토콜 필터의 영향을 받지 않는다.** 필터는 프로필의 `kind` 집합으로 만들어지는데 ssh config 항목엔 kind 개념이 없다 — 필터로 프로필이 전부 가려져도 이 섹션은 남는다. 프로필이 0건일 때도 마찬가지(빈 상태 문구와 **함께** 보인다).
  - 둘째 줄의 `[User@]HostName[:Port]` 는 그 Host 블록에 직접 적힌 **표시 전용 hint** 다. `Host *`/`Match` 가 실제 접속 시 덮어쓸 수 있어 정확성이 보장되지 않으며 저장에는 쓰지 않는다. 세 성분은 **적혀 있는 것만** 넣는다 — 없는 값을 ssh 기본값(`22` 등)으로 채우면 파일에 없는 것을 파일이 말한 것처럼 보인다. `HostName` 이 없으면 ssh 가 alias 를 호스트로 쓰므로 alias 를, 셋 다 없으면 `—` 를 보인다.
  - **가져오기** = 프로필 폼을 프리필로 여는 것이다(별도 모달 없음): `kind=ssh` · `name=alias`(바꿀 수 있다) · `host=alias` · `shell=auto`, **user/port 는 공란**. 값을 펼쳐 담으면 ssh config 위임이 깨진다. 저장 경로·이름 중복 검증은 기존 폼 그대로 쓴다(폼 저장이므로 `shell=auto` 감지 프로브도 기존과 동일하게 돈다 — 목록에서 바로 저장되지 않고 사용자가 저장을 누른 뒤다).
  - 이미 가져온 alias(= `kind=ssh` 프로필의 `fields.host` 가 그 alias)는 `등록됨` Tag 를 달고 액션이 없다. **그 프로필의 이름은 이 행에 보이지 않는다** — 행이 2줄이고 그 둘이 alias 와 target 이라 이름을 놓을 자리가 없다. 어느 프로필인지는 같은 화면 위쪽 목록에서 보면 된다.
  - 빈 상태 문구는 원인 3갈래를 가른다 — 파일이 없으면 `ssh config 가 없습니다.`, 있는데 열리지 않으면(권한 등) `~/.ssh/config 를 읽을 수 없습니다 — 권한이 없습니다.`, 열리는데 alias 가 0건이면 `~/.ssh/config 에 호스트가 없습니다.` 셋은 사용자가 할 일이 각각 다르다(만들기 / 권한 고치기 / Host 적기). 어느 경우에도 섹션 자체를 숨기지 않는다(기능의 존재를 알린다). 세 문구 다 `text-muted` 한 줄이고 **경고 톤이 아니다** — 설정이 없는 것은 오류가 아니다.
  - **파일 읽기는 popup 을 열 때 1회**다. egui 는 매 프레임 목록을 다시 그리므로 캐시하지 않으면 프레임마다 config + Include 를 통째로 읽는다. 캐시는 `UI_MEMORY_ID` 에 있어 popup 이 닫히면 함께 사라진다. **다시 읽는 길은 popup 을 다시 여는 것 하나다** — 섹션 헤더에 새로고침 아이콘이 없다(확정 시안이 이 섹션에서 아이콘 버튼을 두지 않는다).

### Attach 탭 (가운데)

tasty-attach kind(같은 레지스트리, ADR-0032) 전담 탭. add-bar 는 `+ Attach 추가` 만(프로토콜 필터 없음).

- **목록 행 (AttachRow)**:
  - row1 = 이름 + (label) + mode 태그(`profile`=ssh_ref 참조 / `inline`) + **비활성** 경고 배지(참조 ssh 프로필 감지실패 또는 인라인 detect_failed — 이름도 disabled 색).
  - row2 = target 요약 mono — 참조면 `→ <ssh_ref>`, 인라인이면 `user@host[:port]`(port 22 생략). `ssh_ref` 가 dangling 이면 **프로필 없음** 경고 배지(hard-error 아님).
  - row3 = `tasty: <remote_tasty>` · `port: <port_mode>` 캡션.
  - 우측 액션 = 편집/삭제 (재감지 없음 — ssh 레이어 소관).
- **추가/편집 폼 (AttachForm)**: name · label + **Connection 세그먼트 토글** —
  - `SSH 프로필`(ref): ssh kind 프로필만 나열하는 드롭다운(`ssh_ref`, 빈 값이면 "(프로필 선택)").
  - `직접 입력(인라인)`: host/user/port/shell/passkey — ssh 프로필 폼과 동일 필드셋.
  - 공통 **Remote tasty 그룹**(mono caps 헤더): 실행 파일(`remote_tasty`, 기본 `tasty`) · 포트 모드(`port_mode`: auto/subcommand/file-unix/file-windows, `PORT_MODES` 단일 출처) · 포트 파일(`port_file`, 선택 — 포트 모드보다 우선) + PATH/우선순위 힌트 캡션.
  - 저장 시 기본값(`tasty`/`auto`)은 파일에 쓰지 않는다. 이름 중복은 **레지스트리 전역**(ssh 포함) 검증.
- **검증 에러**: 이름 빈 값/중복, (ref) ssh_ref 미선택, (인라인) host 빈 값/port 형식.

### Passkey 탭

기존과 동일 (name/kind 세그먼트/value + Reveal).

### 폼 레이아웃 (디자인 `ProfileForm`/`AttachForm`/`PasskeyForm` 구조 전사)

- **2컬럼 행** `[112px 1fr]`: 모든 행(Type 포함)이 고정폭 112 라벨 컬럼(우측정렬, `subtext0`, 13px) + columnGap `space-md`(12) + 입력(1fr). `egui::Grid` 의 컬럼 협상이 라벨 폭을 붕괴시켜 truncate 되던 문제 때문에 수동 `ui.horizontal` 2컬럼(`form_row`)으로 통일했다. 행 간 rowGap `space-sm`(8).
- **본문/footer 분리**: 본문은 `rtScrollPad`(flex:1) = `CentralPanel` + `ScrollArea`(가용 높이를 채움)로, footer 는 `rtFooter`(flex:none) = `TopBottomPanel::bottom` 으로 패널 하단에 고정. footer 위 separator 는 팝업 전체폭(`clip_rect`)에 그어지고 버튼만 패딩으로 들여쓴다.
- **footer**: 우측정렬 `[취소 ghost][저장 primary]`, padding `space-md`/`space-lg`. attach·passkey 폼도 동일.
- **패딩**: 폼 좌우 `space-lg`(16). (리스트 뷰는 14 — 폼 뷰일 때만 외곽 콘텐츠 margin 0 으로 두고 폼이 패딩을 소유한다.)
- generic(비-ssh) 폼의 커스텀 필드 행은 디자인 `[112 1fr control-height(28)]` grid 정렬(key 112 / value 1fr / 삭제 버튼 28).
- Attach 폼의 Connection 토글은 공용 `segmented` 위젯(활성 = accent-primary + text-on-accent).

## 상태별 시각

- **목록 / 추가 / 편집**: 폼은 추가·편집 시 표시(편집이면 기존 값 채움).
- **검증 에러**: 입력 오류 시 해당 메시지.
- **Attach 상태 배지**: 비활성(accent-warning) / 프로필 없음(accent-warning) — 목록 전용, 저장·로드에는 영향 없음.

## 시각 소스

`site/vendor/ui_kits/terminal/overlays/remote_tool.jsx` — 창·3탭·목록·폼 배치의 단일 출처. 갤러리 specimen: `crates/tasty-gallery` Overlays › Remote connections (`remote`/`remote-attach`/`remote-attach-form`).
specimen 은 행을 자연 높이로 나열하고 스크롤 영역을 두지 않아 가장자리 페이드가 그려질 여지가
없다 — 정합 대상이 아니다.

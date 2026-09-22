# ADR-0259: 헤드리스도 plugin surface kind 를 등록하고, 그 kind 를 지목한 생성 요청이 **소유자 하나**를 띄운다

- **Status**: Accepted
- **Date**: 2026-09-10
- **Tags**: headless, plugin, surface-kind, lazy-start, attach, markdown, trust-boundary, adr-0136, adr-0173, adr-0255
- **Group**: plugin-system

## Context

[ADR-0255](0255-markdown-attach-mirror-forwards-content-not-pixels.md) 로 markdown attach mirror 가
나르는 것이 픽셀이 아니라 **원문**이 됐다 — 그리는 것은 client 이고 서버가 하는 일은 파일 read 와
control 프레임 왕복뿐이다. 그 채널의 서버측 코드(`core/attach_runtime.rs`)에는 feature 게이트가
하나도 없다.

그런데 헤드리스 데몬에서는 그 채널에 **닿을 방법이 없었다.** `register_one_surface_kind`
(`boot/headless_plugins.rs`)가 `rendering` 으로 갈라 `webview`/`remote` 를 건너뛰었고, 그래서
`tab.create {type:"markdown"}` 이 `-32603 unknown surface kind: markdown` 으로 죽었다. 등록 한
줄이 없어서 채널 하나가 통째로 사라진 형태다(`docs/identity.md` 원칙 2).

**등록만 열어서는 안 된다는 것이 그 다음에 드러났다.** 등록은 hello 가 하고, 헤드리스에서 plugin 이
뜨는 자리는 attach 세션(`boot/headless_stream.rs`)과 plugin namespace forward 둘뿐이다 —
`tab.create` 는 그 어느 쪽도 아니다. 실측(2026-09-09): 등록만 열면
`tests/attach_markdown_content_loopback.rs` 의 다섯이 그대로 `unknown surface kind` 로 죽는다.
그래서 **기동 트리거가 하나 더 필요하다**는 물음이 함께 선다. 그 트리거를 어떤 범위로 열 것인가가
이 ADR 이 정하는 것이다.

## Decision

셋을 함께 정한다.

**① 헤드리스도 세 rendering 을 전부 등록한다.** `register_one_surface_kind` 는
`webview`/`remote`/`egui-mesh` 를 gui 와 같은 집합으로 등록한다. webview 의 overlay 플래그를 읽는
것은 gui 의 매 프레임 `sync_webviews` 뿐이라 헤드리스에서 세워 두어도 소비자가 없다 — 대신 등록
사실이 조합에 따라 갈리지 않는다.

**② plugin 이 선언한 kind 를 지목한 생성 요청이 세 번째 기동 트리거다.**
`tab.create`·`pane.split`·`workspace.create` 의 `type` 이 그 자리이고, 판정은 namespace forward 와
**같은 두 층**이다 — 소속은 디스크(매니페스트)가 답하고 기동은 소속이 맞은 뒤에만 한다
([ADR-0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md)). 없는 이름을
지목한 요청은 소속 판정에서 끝나 plugin 을 하나도 안 띄운다.

**③ 소속은 매니페스트 ∩ `plugins.toml` 이고, 띄우는 것은 소유자 하나뿐이다.**
- 사용자가 끈 plugin 은 **소유자가 아니다**(`enabled_owner_of_kind`). 선언만 보면 영영 안 뜰
  plugin 을 기다리게 되고 그 대기가 데몬 전체를 세운다 — 아래 대안 D 의 실측.
- 기동은 `PluginManager::start_one_enabled` 로 **지목된 하나만** 한다. `discover_and_start` 는
  설치된 것을 전부 띄운다 — 대안 E.
- **설치·권한 grant 는 이 경로에 없다.** 소속 판정이 이미 설치된 package 표를 보므로, 여기 닿았다는
  것 자체가 설치가 끝났다는 뜻이다. 설치는 부팅에 걸려 있다(`src/boot.rs`,
  `source_guards::jobs_anchored_at_boot` 가 그 자리를 못 박는다).
- 기다리는 것은 **우리가 방금 띄운 프로세스**뿐이다. 이미 떠 있는데 kind 가 아직 없으면 기다려도 원인이
  우리 손에 없으므로 그냥 돌아간다. 기동은 연결 전에 돌아오므로([ADR-0505](0505-a-plugin-start-waits-for-its-connection-off-the-main-thread.md))
  대기는 두 단계다 — 연결 결과를 연결 한도까지, 그리고 연결이 성사된 뒤부터 hello 를 `KIND_REGISTRATION_WAIT`
  동안.

### 신뢰 경계 — 이 트리거는 비-Local 에게도 열려 있다

**caller 종류로 가르지 않는다.** 판정 근거는 실측이다(2026-09-10, 격리 홈 헤드리스 데몬, 세션 토큰
발급 후 `TASTY_SESSION_TOKEN` 으로 호출).

| 경로 | 필요한 권한 | 비-Local 결과 | 그 뒤 뜨는 프로세스 |
|---|---|---|---|
| `plugin.enable` | — (`local_only`) | `-32001 permission_denied` | 0 |
| namespace forward · **표에 없는 이름** (`markdown.recent`) | **없음** | 정상 응답 | **9 (전부)** |
| namespace forward · **표에 있는 이름** (`markdown.navigate` · `image.list`) | 그 표가 적은 것 (`fs.read` · `surface.read`) | `-32001 permission_denied` | 0 |
| 이 트리거 (`workspace.create {type:"markdown"}`) | `surface.write` | 정상 응답 | **1 (소유자)** |

읽을 것 셋.

- **"namespace forward 는 비-Local 에게 막혀 있다" 도, "열려 있다" 도 전칭으로는 틀렸다.**
  가르는 것은 namespace 가 아니라 **그 이름이 `METHOD_TABLE` 에 등재돼 있는가**다.
  `method_meta()` 는 그 표 → `DEBUG_METHODS` → 정적 `PREFIX_RULES` → **런타임 등록 plugin
  prefix** 순으로 해소하는데, 앞 단계에서 걸린 이름은 그 자리가 적은 권한을 그대로 요구하고,
  마지막 갈래까지 내려온 이름만 `plugin_callable: true, required: []` 가 된다. 실측
  (2026-09-10, 같은 데몬·같은 권한 0 토큰): `markdown.recent` 는 정상 응답이고
  `markdown.navigate` 는 `-32001 … missing permission 'fs.read'` 다 — **같은 namespace 안에서
  갈린다.** `image.list` 도 같은 형태로 `surface.read` 를 요구해 막히고, 그때 뜨는 프로세스는
  0 이다.
- **경계는 값으로 셀 수 있다.** 번들 plugin 이 선언한 namespace 여섯 중 `METHOD_TABLE` 에
  이름이 있는 것은 둘뿐이다 — `image` 8 건 · `markdown` 1 건(`markdown.navigate`), 나머지 넷
  (`agent_stream`·`claude`·`codex`·`html`)은 0 건. 그래서 대부분의 이름이 마지막 갈래로
  내려오고, 그중 한 번이 `discover_and_start` 로 9 개를 띄운다. 이것은 `main` 부터 있던
  성질이고 이 결정이 만든 것이 아니다.
- **뜨는 것은 프로세스뿐이다 — 이 경로가 설치나 grant 를 하지는 않는다.** 실측(2026-09-10,
  정상 홈): 권한 0 `markdown.recent` 전후로 `plugins.toml` 의 md5 가 같고 `plugins/` 아래
  파일 45 개의 목록 md5 도 같다. 부팅이 `install_builtins_if_needed` 를 이미 끝냈으므로 그
  호출은 no-op 다. 반대로 설치가 **안 된** 홈에서는 prefix 가 등록돼 있지 않아 이 호출이
  forward 에 닿지도 못한다 — 권한 0 토큰에 `-32001 unknown ipc method`, 토큰 없는 Local 에
  `-32601`.

그러므로 이 트리거는 위 표의 **둘째 줄보다 좁다** — 권한을 더 요구하고(`surface.write`),
띄우는 수가 적다(9 → 1). 새 신뢰 경계를 여는 것이 아니다. 한때 여기 "설치·grant 를 안
한다" 도 근거로 적혀 있었는데 그것은 비교가 아니다 — 셋째 불릿대로 **둘째 줄도 설치·grant 를
안 한다.** 비교는 권한 축과 9-vs-1 축만으로 성립한다.

`local_only` 로 맞추는 길은 **일부러 안 골랐다.** gui 는 첫 창을 만들 때 plugin 을 전부 띄우므로
Agent caller 가 `tab.create {type:"markdown"}` 을 언제 불러도 kind 가 차 있다. 헤드리스에서만 그
호출을 caller 종류로 막으면 **같은 에이전트가 같은 명령을 조합에 따라 다르게 받는다** — 이 lane 이
없애려던 바로 그 형태다(`docs/identity.md` 원칙 2).

위 표의 둘째 줄(표에 없는 이름이 권한 0 에 9 개를 띄우는 것)은 **이 결정의 범위 밖**이며,
좁히려면 소유자를 알고 있는 `owns_namespace` 를 id 를 돌려주는 형태로 바꿔야 한다. 별건으로 남긴다 —
여기서 조용히 함께 고치면 그 변경의 근거가 이 문서에 묻힌다.

## Consequences

- **얻은 것**: 헤드리스 데몬이 markdown/html attach mirror 채널에 닿는다. `plugin.show` 의
  `declared_rendering` 과 `registered` 가 두 조합에서 같은 답을 낸다. `--type <kind>` 첫 호출이
  **0.14 s**(실측)이고 그 kind 의 소유자 하나만 뜬다.
- **잃은 것**: 요청 하나가 프로세스 하나를 띄운다 — 요청이 자기 관측 대상을 만드는 형태가 남는다
  ([ADR-0136](0136-a-query-does-not-create-what-it-observes.md) 이 조회에 대해 금지한 것을 **쓰기**
  요청에는 허용하는 것이다: `tab.create` 는 관측이 아니라 생성 명령이고, 그 명령이 성립하려면 그
  plugin 이 떠 있어야 한다).
- **한계 (정직하게)**:
  - 첫 호출은 spawn + 연결 + hello 만큼 느리다. 그 대기 동안 데몬 IPC 는 **전부 선다** — 헤드리스 메인
    루프가 단일 스레드이기 때문이다. 상한은 둘이다: 연결 결과까지 연결 한도(10 초 + 거둘 여유), 그리고
    연결 뒤 `KIND_REGISTRATION_WAIT`(5 초). 뒤쪽을 꽉 채우는 갈래는 "연결은 됐는데 hello 가 안 온다"
    하나로 좁혀져 있고, 끝내 연결 안 하는 owner 는 앞쪽에서 끝난다.
  - **kind 마다 한 번씩** 이 비용을 낸다. 트리거가 소유자만 띄우므로 여덟 kind 를 쓰면 여덟 번이다.
    전부 미리 띄우고 싶으면 그것은 사용자의 명령이어야 한다(`plugin enable`).
  - 소속 판정은 `packages()` 를 **선형 탐색**한다. 설치 수가 지금 9 라 문제가 없고, 커지면 kind →
    plugin id 표를 `refresh_packages` 에서 유도하는 쪽이 맞다.
  - 이 트리거는 `type` 키 하나만 읽는다. 다른 키로 kind 를 지목하는 handler 가 생기면 여기서 안
    보인다.

## Alternatives Considered

- **A: 부팅 시 전량 기동** — 기각. 헤드리스의 성질이 "안 쓰면 안 뜬다" 이고, 그것은 attach·CLI 용
  데몬을 오래 띄워 두는 사용 형태에서 나온다. 기동을 부팅으로 옮기면 아무 plugin 도 안 쓰는 데몬이
  프로세스 9 개를 상시로 든다. (설치는 반대 방향이라 실제로 부팅에 걸었다 — `jobs_anchored_at_boot`
  의 명부 기준: *필요성이 트리거와 무관한 일*만 부팅에 온다.)
- **B: 매니페스트 층에서 kind 를 등록** — 기각. 등록의 유일한 트리거를 hello 로 두는 것이 지금
  구조이고, 매니페스트에서도 등록하면 **같은 사실의 사본이 둘**이 된다. 사본은 갈라진다 — plugin 이
  선언과 다른 것을 hello 로 보내는 갈래(`effective_rendering`)가 실제로 있다.
- **C: 그 다섯 시험을 `--skip` 으로 덮는다** — 기각. `tests/attach_markdown_content_loopback.rs` 의
  **자동 채널은 헤드리스 조합 하나뿐**이다(`check-headless` 의 전체 스위트). 거기서 빼면 그 시험의
  채널이 **0** 이 된다 — 초록으로 보이는데 아무 데서도 안 도는 상태다.
- **D: 소속을 선언만으로 판정** — 기각. 사용자가 끈 plugin 도 소유자로 잡혀, 그 kind 를 한 번
  지목하는 것만으로 데몬이 시한을 꽉 채운다. 실측(2026-09-10, `plugin disable com.tasty.markdown`
  뒤 `--type markdown` 한 번): 그 요청이 **5.34 s**, 그동안 무관한 `list info` 가 **5.04 s**,
  그리고 나머지 **8 개가 기동**했다. 고친 뒤 같은 절차는 0.09 s · 0.09 s · running 0 이다.
- **E: `discover_and_start` 로 전량 기동** — 기각. 위 D 의 "8 개가 기동" 이 그 대안의 정상 동작이다.
  하나만 띄우는 길이 `plugin.enable` 뿐이라던 서술은 **틀렸다** — `plugins.toml` 을 쓰는 것은
  `PluginManager::enable` 이고, 그 아래의 `start_enabled_package` 는 설정을 안 건드린다. 그것을
  `start_one_enabled` 로 노출했다.
- **F: 이 트리거를 `local_only` 로 맞춘다** — 기각. 근거는 위 "신뢰 경계" 절 — 조합에 따라 같은
  에이전트의 같은 명령이 갈린다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `enabled_owner_of_kind` 의 알맹이(`owner_of_kind`)가 `is_disabled` 를 안 보게 되면
  `boot::headless_plugins::tests::a_disabled_plugin_does_not_own_its_kind` 가 운다 — 그 자리가
  대안 D 의 실측이 난 자리다.
- `PluginManager::start_enabled_package` 의 재기동 방지 검사가 사라지면 같은 plugin 이 두 번 뜨고
  앞의 핸들이 회수 주체를 잃는다. 실측(2026-09-10): 검사를 지우면 kind 트리거 뒤 namespace forward
  한 번에 `plugin started: com.tasty.markdown` 이 **2 회** 찍히고, 검사가 있으면 1 회다.
- `install_builtins_if_needed` 가 부팅 경로에서 빠지면
  `source_guards::jobs_anchored_at_boot` 가 운다 — 그러면 이 트리거의 "설치는 이미 끝났다" 전제가
  깨진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 첫 호출의 대기가 체감 임계를 넘는다는 보고. 재는 법: 격리 `TASTY_HOME` 으로 헤드리스 데몬을 띄우고
  `/usr/bin/time` 으로 `new workspace --type <kind>` 의 첫/두 번째 호출을 각각 잰다(2026-09-10 값은
  0.14 s / 0.09 s). 같은 순간 다른 셸에서 `list info` 를 걸어 데몬 전체가 서는 폭도 함께 잰다.
- 설치 plugin 수가 늘어 소속 판정의 선형 탐색이 눈에 띄는 비용이 되는 것. 재는 법: 같은 절차에서
  없는 kind(`--type nosuchkind`)의 소요를 잰다 — 그 호출은 탐색만 하고 끝난다(2026-09-10: 0.09 s).
- plugin namespace forward 가 권한 0 인 caller 에게 9 개를 띄우는 것을 좁히기로 하는 것. 재는 법:
  권한 없는 세션 토큰을 발급해 `markdown.recent` 를 부른 뒤 `plugin list` 의 `running` 을 센다
  (2026-09-10: 9). **이름을 아무거나 고르면 안 된다** — 위 "신뢰 경계" 의 첫 불릿대로 `METHOD_TABLE`
  에 등재된 이름(`image.list`·`markdown.navigate`)은 그 표의 권한에서 `-32001` 로 끝나 `running` 이
  0 이다. 재는 대상은 **표에 없는 이름**이고, 그 판정은 `crates/tasty-ipc/src/method_meta.rs` 의
  `METHOD_TABLE` 을 그 prefix 로 훑어 먼저 확인한다.

## References

- [ADR-0255](0255-markdown-attach-mirror-forwards-content-not-pixels.md) — 이 결정의 전제(mirror 가
  나르는 것은 원문이라 서버에 창이 필요 없다).
- [ADR-0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md) — 소속은
  매니페스트가 답하고 기동은 그 뒤라는 두 층. 이 트리거가 그 형태를 그대로 쓴다.
- [ADR-0136](0136-a-query-does-not-create-what-it-observes.md) — 조회는 자기 관측 대상을 안 띄운다.
  이 트리거가 조회가 아니라 생성 명령이라는 것이 갈리는 지점이다.
- 코드 근거(결정이 실현된 **현재** 위치): `boot/headless_plugins.rs` 의
  `ensure_plugin_for_surface_kind` · `enabled_owner_of_kind` · `register_one_surface_kind`,
  `boot/headless_dispatch.rs` 의 `2e` 갈래, `tasty-host-plugin` 의
  `PluginManager::start_one_enabled`.
- [headless-ipc-surface.md](../dev-guide/headless-ipc-surface.md) "등록된 kind 조회" ·
  [self-verification.md](../dev-guide/self-verification.md) "headless 빌드에서 무엇이 없는가" ·
  [plugin-permissions.md](../dev-guide/plugin-permissions.md) "비-Local caller 가 유발할 수 있는
  plugin 수명주기".
- 후속: [ADR-0271](0271-a-plugin-namespace-is-invoked-with-its-token-from-every-gated-caller.md) — 위
  "신뢰 경계" 표의 둘째 줄을 닫았다. 표에 없는 이름도 권한 셋을 가진 caller 에게
  `ipc.invoke:<prefix>` 를 요구해, 권한 0 토큰의 그 호출은 forward 앞에서 거부되고 plugin 을 띄우지
  않는다. 토큰을 가진 caller 의 forward 가 설치된 전부를 띄우는 기동 경로는 바꾸지 않았다.

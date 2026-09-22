# ADR-0164: hook 실패 기록의 로케일 무관성은 산문이 아니라 좌표 필드가 진다 — ADR-0075 의 언어 조항 개정

- **Status**: Accepted
- **Accepted**: 2026-09-05
- **Date**: 2026-09-05
- **Tags**: cli, agent-hooks, diagnostics, i18n, plugin, partial-amendment, adr-0075
- **Group**: agent-integration

## Context

[ADR-0075](0075-agent-hook-delivery-failure-record.md) 는 agent hook 전달 실패를
`<tasty_home>/hook-failures.log` 에 `key=value` 한 줄로 남기기로 했다. 그 결정에는
언어 조항이 하나 붙어 있다 — *`reason` 의 **값**은 사용자 로케일과 무관한 영어 고정이다.*
근거로 적힌 것은 "이 파일은 사후에 — 사람이 아니라 에이전트가 — 알려진 실패 패턴과
대조하며 읽는다" 였고, 집행 수단으로 `DiagnosticEnglish` 타입과
`crates/tasty-doc-guards/tests/hook_failure_reason_stays_english.rs` 소스 가드를 세웠다.

**그 조항은 쓰인 날 이미 거짓이었다.**

| 날짜 | 커밋 | 무슨 일 |
|------|------|---------|
| 2026-08-10 | `ecaea2cb` | claude plugin 의 IPC 에러 문구가 `Translator` 를 타기 시작 |
| 2026-08-20 | `db580653` | ADR-0075 최초 — **언어 조항이 없다**(실측: "영어 고정" 0 건) |
| 2026-09-04 | `faa4b583` | ADR-0075 에 언어 조항을 **끼워 넣고** 타입·가드를 세움 |

즉 회귀가 아니다. 조항이 들어오기 25 일 전부터, 가장 흔한 hook method 인 `claude.hook`
에 대해 이미 성립하지 않았다. "언제 깨졌나" 를 찾으면 아무것도 안 나온다.

성립하지 않는 이유는 구조적이다. `reason` 의 출처가 셋이고 성질이 다르다.

| 출처 | 문구를 만드는 곳 | 로케일 |
|------|------------------|--------|
| tasty 미실행(포트 파일 부재) | CLI — `PortFileError` 의 `Display` | 영어 보장 (타입 + en 파리티 테스트) |
| 연결 실패 | CLI — `ConnectError::diagnostic` | 영어 보장 (타입 + en 파리티 테스트) |
| **요청은 닿았는데 오류 응답** | **답한 쪽(호스트 또는 plugin)** | **보장 없음** |

앞의 둘은 CLI 가 영어 원본을 쥐고 있어 표시용 번역문과 진단용 영어를 실제로 갈라 놓을
수 있다. 셋째는 CLI 에 영어 원본이 없다 — 문구가 **다른 프로세스에서** 만들어져 온다.
호스트가 답하면 영어지만(호스트는 번역을 안 거친다), plugin 이 답하면 앱 언어를 탄다.
그리고 소스 가드는 CLI 소스를 스캔하므로 그 자리에는 볼 `t()` 가 없다 — 스캔 범위를
넓히는 문제가 아니라 문구의 생산지가 프로세스 밖이라는 문제다.

실측(2026-09-05, ko 로케일 격리 인스턴스, `tasty {codex,claude} hook nosuchevent`):

```
method=codex.hook  event=nosuchevent surface=- reason=Error (-32602): invalid params: 알 수 없는 hook 이벤트 …
method=claude.hook event=nosuchevent surface=- reason=Error (-32602): invalid params: 알 수 없는 claude hook …
```

조항을 문자 그대로 실현하려면 두 길뿐인데 둘 다 값이 안 맞는다.

- plugin 의 hook 오류 경로를 영어로 강제하면 **같은 문자열이 stderr 로도 나가므로**
  사용자 표면이 영어로 되돌아간다. 그건 전수 측정으로 답이 난 결정을 다시 여는 것이다
  (번들 plugin 9 개 중 IPC 에러를 내는 4 개에서 `t()` 38 : 영어 리터럴 2 — 번역이 관례다).
- plugin 이 안정 토큰을 따로 싣는 채널을 만들면 SDK·plugin·CLI 넷을 건드리는데,
  그 토큰을 소비하는 자리가 레포에 하나도 없다.

한편 **조항이 지키려던 것은 이미 다른 조각이 떠받치고 있다.** 그 줄에서 로케일을 안 타는
값은 `method` · `event` · `surface` 이고, 오류 응답 갈래에는 JSON-RPC `code` 도 있다.
지금까지 코드는 `reason` 산문 앞머리에 `Error (-32602):` 로 묻혀 있어, 쓰려면 읽는 쪽이
산문을 파싱해야 했다 — 이 파일이 피하려던 바로 그 형태다.

## Decision

**`hook-failures.log` 의 로케일 무관성은 좌표 필드가 지고, `reason` 산문은 그 문구를
만든 쪽의 언어를 따른다. 그 좌표에 JSON-RPC `code` 를 필드로 추가한다.**

- 레코드 형식이 `<UTC> method=… event=… surface=… code=… reason=…` 이 된다.
  `code` 는 JSON-RPC 오류 코드이고, 호스트에 닿지도 못한 실패는 `-` 다(`event` 와 같은
  부재 표기). **`reason` 은 계속 마지막이다** — 공백을 담을 수 있는 값이 그것뿐이라,
  읽는 쪽이 `reason=` 뒤를 줄 끝까지로 자르는 성질이 유지된다.
- 코드를 데이터로 꺼내기 위해 `tasty-ipc` 의 클라이언트가 JSON-RPC 오류를
  `JsonRpcCallError { code, message }` 로 돌려준다. `Display` 는 종전 문자열 그대로라
  기존 호출자의 출력은 바뀌지 않는다.
- `DiagnosticEnglish` 타입과 소스 가드는 **CLI 가 문구를 만드는 두 갈래**에 대해 그대로
  유지한다. 그 갈래에서는 갈라 놓을 영어 원본이 실제로 있고, 실제로 두 변경이 각각
  그 자리에 번역문을 흘려 넣은 전례가 있다. 타입의 문서와 가드의 문서에 **셋째 갈래는
  덮지 못한다**는 것을 명시한다.

**ADR-0075 의 언어 조항을 이 결정으로 개정한다.** ADR-0075 의 나머지는 그대로 유효하다 —
셸 가드(`if [ -n "$TASTY_SURFACE_ID" ]`)와 `|| true` 의 역할 분리, 세 실패 지점 모두를
기록한다는 것, 기록 대상을 마지막 세그먼트가 `hook`/`_hook` 인 method 로 좁히는 것,
기록 위치를 `tasty_home()` 으로 잡는 근거, best-effort 와 256 KiB 1 단 로테이션,
loopback connect 3 초 상한. 어느 것도 이 ADR 이 건드리지 않는다.

## Consequences

- **얻은 것**: 읽는 쪽이 산문을 파싱하지 않고 실패를 가를 수 있다. 종전에는 코드가 산문
  앞머리에 묻혀 있어 "우리가 만든 문장을 우리가 다시 파싱" 해야 했다. 그리고 사용자
  표면(stderr)과 진단 기록이 **같은 결정 아래** 놓인다 — plugin IPC 오류 문구는 번역하고,
  기계가 쓸 좌표는 필드로 뽑는다.
- **잃은 것**: `reason` 산문이 로케일을 탄다는 것이 이제 **명시된 사실**이다. 산문을
  문자열로 대조하던 소비자가 있다면 깨진다 — 다만 레포 안에 그런 소비자는 0 이고,
  대체 앵커(`code`)가 같은 변경에서 함께 들어간다.
- **레코드 형식이 바뀐다.** 필드가 하나 늘었다. `reason=` 뒤를 줄 끝까지로 자르던 읽기는
  그대로 동작하고, 열 번호로 읽던 쪽은 `reason` 이 5 번째에서 6 번째로 밀린다.
- **못 잰 것**(명시): 레포 밖 에이전트가 `reason` 산문을 알려진 패턴과 대조하는지는
  이 저장소에서 측정할 수단이 없다. 잰 것은 셋이고 전부 좌표 필드 쪽이다 — 레포 안에서
  이 파일을 파싱·grep 하는 코드·스크립트·워크플로 **0**, 대조 좌표가 이미 줄에 있다는 것,
  그리고 출하 사용자 가이드 4 곳이 **사용자에게** 이 파일을 보라고 안내한다는 것
  (ADR-0075 의 독자 규정은 "사람이 **아니라** 에이전트" 로 배타적인데 그 배타성이 이미
  거짓이다). "외부 소비자가 없다" 가 아니라 "레포 안 증거는 전부 이쪽이고 레포 밖은
  측정 수단이 없다" 로 적는다.

## Alternatives Considered

- **plugin 소스도 스캔해 hook 오류 경로를 영어로 강제** — 정적으로 알 수는 있다(hook
  핸들러의 오류 경로 폐쇄). 그러나 그 폐쇄를 영어로 강제하면 같은 문자열이 stderr 로도
  나가므로 사용자 표면이 영어로 되돌아간다. 분석 비용의 문제가 아니라, 전수 측정으로
  답이 난 결정(plugin IPC 오류 문구는 번역한다)을 다시 여는 것이다.
- **기록하는 자리에서 런타임 판정(비-ASCII 검출 등)** — 술어는 세울 수 있다. 그러나 hook 을
  실패시킬 수 없으므로 탐지 후 할 일이 없다. 대체값이 있어야 쓸모가 생기고, 그 대체값을
  정하는 것이 곧 이 ADR 이다.
- **plugin 이 `error.data` 에 안정 토큰을 싣는 채널** — 계약을 문자 그대로 지킬 수 있다.
  그러나 SDK·두 plugin·CLI 넷을 건드리고 plugin 마다 토큰 어휘를 유지해야 하는데, 그
  토큰을 소비하는 자리가 레포에 하나도 없다. 외부 소비자가 산문을 대조한다는 보고가
  오면 이쪽을 다시 본다(아래 트리거).
- **조항을 그대로 두고 위반을 감수** — 문서가 거짓을 말하는 상태가 유지된다. 가드가
  초록인 것이 규약이 지켜졌다는 뜻으로 계속 읽힌다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **외부 소비자가 `reason` 산문을 알려진 패턴과 대조한다는 보고**가 오면 — 그때는 위
  토큰 채널(`error.data`)을 다시 검토한다. 이 ADR 의 유일한 미측정 값이 그것이다.
- `code` 만으로 실패를 못 가르는 사례가 나오면 — 같은 코드 아래 성질이 다른 실패가
  섞여 산문 없이는 구분이 안 되는 경우다.
- 기록 대상이 `*.hook` 밖으로 넓어지면 — 산문의 생산지가 늘어 이 ADR 의 출처 3 분류가
  다시 세어져야 한다.
- 호스트가 IPC 오류 문구를 번역하기 시작하면 — 지금은 호스트 응답이 영어라 셋째 갈래도
  호스트가 답할 때는 사실상 안 흔들린다. 그 전제가 바뀐다.

## References

- 개정 대상: [ADR-0075](0075-agent-hook-delivery-failure-record.md) (`reason` 의 값을 로케일 무관 영어로 고정한 언어 조항).
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md) (ADR-0028 의 image Canvas-하이브리드 조항만 부분 개정하고 나머지는 유효로 남긴 방식).
- 코드 근거: `crates/tasty-cli/src/hook_failure.rs`(`format_line` · `DiagnosticEnglish` · `is_hook_method`), `crates/tasty-cli/src/run.rs`(`run_dynamic_client` 의 세 실패 지점), `crates/tasty-ipc/src/client.rs`(`JsonRpcCallError`).
- 가드: `crates/tasty-doc-guards/tests/hook_failure_reason_stays_english.rs`(CLI 가 만드는 두 갈래만 덮는다는 것을 그 파일 문서가 명시한다).
- 이력 커밋: `ecaea2cb`(claude IPC 오류가 `Translator` 를 타기 시작), `db580653`(ADR-0075 최초), `faa4b583`(언어 조항 삽입 + 타입·가드).
- 관련: [ADR-0103](0103-plugin-locale-via-host-process-env.md)(plugin 로케일은 host 프로세스 env 로 전달), [i18n](../dev-guide/i18n.md) "plugin 이 돌려주는 IPC 에러 문구", [crash-diagnostics](../dev-guide/crash-diagnostics.md).

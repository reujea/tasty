# CLI 크레이트 내부 구조 — 세 갈래 대칭

`crates/tasty-cli` 는 명령 하나를 **세 갈래**로 나눠 다룬다. 새 CLI 명령을 추가할 때
어디를 고칠지는 이 표가 정한다.

| 디렉토리 | 답하는 질문 | 들어가는 것 | 들어가면 안 되는 것 |
|----------|-------------|-------------|---------------------|
| `commands/` | **무엇을 받나** | clap `Subcommand`/`Args` 선언, 도움말 문구(영어) | 실행 코드 (`pub fn` 은 `ValueEnum` 의 `as_str` 같은 선언 보조만), SSH·스트림·원격 계층 참조, 한국어 `///` |
| `request/` | 단발 RPC 면 **어디로** | `Commands` → JSON-RPC method/params 변환 | 통신 자체 |
| `local/` | 클라이언트 주도면 **무엇을** | `ClientCommand` 구현 + 그 실행부 | clap 선언 |

갈래 판정은 `dispatch.rs` 하나가 한다.

## 갈래 판정 (`dispatch.rs`)

```rust
pub enum Dispatch<'a> {
    Rpc,                                    // 단발 JSON-RPC — 보내고 응답 출력하면 끝
    ClientDriven(Box<dyn ClientCommand + 'a>),  // client가 실행 순서를 관리한다
}
```

분류 축은 하나다: **`request/` 가 만든 단발 JSON-RPC 하나로 끝나는가.**
아니면 전부 `ClientDriven` 이다 — 로컬 파일/프로세스 조작(`tasty port`,
`tool passkey`), raw 스트림(`remote attach`), 폴링 루프(`plugin audit-follow`),
SSH 터널 경유 조회(`remote workspaces`)가 여기 속한다. 용어 정의는
[ubiquitous-language.md](../concepts/ubiquitous-language.md) 의 "CLI 명령 갈래" 절.

**"로컬(local)" 은 통신 유무가 아니라 주도권을 뜻한다** — 이 갈래의 절반은 IPC 를
(여러 번) 탄다. variant 를 `Local` 로 부르지 않는 이유이기도 하다.

`Dispatch` 는 **명령을 빌린다**(`Dispatch<'a>`). 소유 형태로 만들면 clap enum 들에
`Clone` 구현을 새로 공개해야 한다. 명령을 빌리면 다른 크레이트에 이 구현을 요구하지 않아도 된다.
`Rpc` 가 요청을 담지 않는 것도 같은 이유다 — 진입점이 원 명령을 계속 들고 있다.

## 새 명령 추가 절차

1. `commands/` 에 clap 선언을 추가한다.
2. 단발 RPC 면 `request/` 에 변환을 추가한다. 진입점(`run.rs`) 수정은 필요 없다.
3. 클라이언트 주도면 `local/` 에 실행 모듈 + `ClientCommand` 구현을 추가하고,
   `dispatch.rs` 의 `classify` 에 arm 하나를 더한다. 여기서도 `run.rs`는 수정하지 않는다.

인자 조합 검증(`--ssh` + `--profile` 상호배타 등)은 `classify` 에서 끝낸다 —
검증 실패는 통신을 시작하기 전에 나야 한다.

## 도움말 문구

`commands/` 의 `///` doc comment 는 코드 주석이 아니라 **사용자에게 표시되는 도움말**이다 —
clap 이 첫 문단을 짧은 help(`-h`), 전체를 긴 help(`--help`)로 그대로 노출한다.
따라서 clap 항목(variant · `#[arg]` 필드) 의 `///` 는 영어로만 쓴다
([i18n.md](i18n.md) "하드코딩 허용 예외"). 값 허용 범위·상호배타 같은 사용자에게
필요한 상세는 긴 help 에 남기고, 설계 근거(원칙 문서 이름, 내부 단계명 등)는
`//` 주석이나 `docs/` 로 내린다.

`clap_help_text_is_english_only`
(`crates/tasty-doc-guards/tests/no_hardcoded_ui_strings.rs`)는 `commands/`와
`crates/tasty-cli/src/lib.rs`의 `///`에 CJK 문자가 있는지 검사한다. clap 항목인지까지
구분하지 않으며, `#[cfg(test)]` 안의 코드와 그 앞 doc 주석만 제외한다.
모듈 설명인 `//!`는 대상이 아니다. 이 파일들의 clap과 무관한 설명은 필요할 때만
`//`로 짧게 남긴다.

실제 표시는 `help_i18n::command`로 얻은 번역 트리와 `help_frame` 템플릿을 사용한다.
`help_error`는 clap의 구조화된 파싱 오류를 표시하며, plugin 동적 파싱도 같은 경로로
들어간다. plugin 번역은 discovery에서 공용 카탈로그를 읽는다.
상세: [국제화](i18n.md#cli-도움말-clap-about--help), [국제화 (i18n)](i18n.md).

### 빈 설명 칸은 없다

모든 하위 명령에 about, 모든 인자에 help를 작성한다.
`crates/tasty-cli/tests/help_i18n_slots.rs`의 `every_subcommand_carries_an_about`와
`arguments_without_help_do_not_increase`가 clap 트리를 검사하므로 다른 크레이트의
명령(`tasty-tui-simulator`의 `debug sim` 등)도 포함한다.

현재 빈 설명의 허용 개수는 0이다. 검사는 허용 개수보다 많거나 적어도 실패하도록
작성돼 있어, 예외를 줄였을 때 허용값도 함께 줄여야 한다.

`///`는 바로 아래 항목에 붙으므로 선언을 끼워 넣을 때 설명의 대상이 바뀌지 않는지
확인한다. 검사는 설명이 비었는지만 확인하며 내용의 정확성은 판단하지 않는다.
기본값·누적/교체 방식·플랫폼 차이는 실행 코드를 읽고 설명하고, 같은 이름의 옵션도
각 명령에서 실제로 무엇을 뜻하는지 적는다.

### 도움말 진입점

`tasty help`, `tasty -h`, `tasty --help`는 같은 명령 집합을 표시한다. 정적 트리가
도움말 요청(`DisplayHelp`)이나 모르는 이름(`InvalidSubcommand`)으로 끝나면
`try_run_plugin_cli`가 설치된 plugin 명령을 포함한 트리(`build_augmented_cli`)로 같은 argv를
다시 해석한다. 그래서 `help <경로>`와 `<경로> --help`는 host·plugin·3단 host 하위 명령에서 같은
대상을 가리킨다. 유효한 경로는 종료 코드 0, 없는 경로는 2이며 가장 가까운 유효 부모의 하위 명령을
후보로 보여 준다.

루트 옵션 구간은 `help_route::scan_root`가 루트 인자 선언을 읽어 해석한다. 값을 받는 옵션의 값과
`--` 뒤의 토큰은 명령이나 도움말 제어로 보지 않는다. 따라서 `--port-file <경로> help <경로>`도 같은
도움말을 출력하며, 하위 명령 뒤의 `--all`은 데이터로 남는다. 전체 트리(`-a`/`--all`)는 루트 위치에
있을 때만 요청으로 본다. 루트의 인자 없는 `tasty help`는 `-h`와 같은 짧은 도움말로 보낸다.

도움말은 매니페스트만 읽는다. plugin 프로세스·GUI·IPC 연결·stdin 읽기는 일어나지 않는다.
읽지 못한 매니페스트는 기존 경고를 출력하고 건너뛰며 host 도움말은 계속 표시한다.
시험은 `tests/cli_help_entrypoints.rs`다.

### 도움말 표시 계약

명령 존재 집합은 진입점마다 같고 설명의 깊이만 다르다.

| 진입점 | 표시하는 명령 | 설명 깊이 |
|---|---|---|
| `tasty help`, `tasty -h` | 모든 최상위 명령을 목적별 그룹으로 | 시작 안내와 짧은 설명 |
| `tasty --help` | 위와 같은 집합·그룹 | 루트 옵션의 긴 설명 포함 |
| `tasty <경로> -h` | 해당 경로의 모든 하위 명령, 내부 연동은 마지막 그룹 | 짧은 설명 |
| `tasty help <경로>`, `tasty <경로> --help` | 위와 같은 집합·그룹 | 긴 설명(`long_about`·`long_help`) |
| `tasty -a` | 해당 빌드의 모든 깊이 canonical 명령 | 경로와 짧은 설명 |

그룹 표시는 `help_groups`가 맡는다. clap은 하위 명령 그룹 제목을 지원하지 않으므로 그룹 목록을
템플릿 해석을 거치지 않는 before-help 자리에 넣고, 사용법·인자·옵션 절은 clap이 그린다.
파싱·실행 경로와 IPC 메서드·권한은 바꾸지 않는다.

- 루트: 기본 조작 / 에이전트 협업 / 백그라운드·원격 / 자동화·관찰 / 파일·클립보드 / 설정·관리 /
  플러그인 명령 / 기타 명령. host 명령의 그룹은 `HOST_ROOT` 표로 정한다. 표에 없는 host 명령은 "기타 명령"으로
  표시되고 `every_host_command_has_a_root_group` 시험이 실패한다. debug 빌드에만 있는 `debug`는
  사용자 조작을 재현하는 명령이라 표에서 명시적으로 "기타 명령"에 둔다. 루트 첫 부분에는 시작 안내
  (`list surfaces` → `send text` → `read screen`, 개별 도움말과 전체 트리)를 둔다.
  시작 안내의 예시는 `start_examples_parse`가 실제 파서로 확인한다.
- `tasty agent`: 작업·DAG / 배리어·세마포어 / 임대(lease) / 속도 제한. 명령 경로는 그대로다.
- plugin 최상위 명령은 매니페스트의 `help_group`으로 그룹을 정한다. 생략하거나 모르는 값이면
  "플러그인 명령" 그룹이다. `internal = true`인 하위 명령은 그 plugin 도움말의 마지막
  "내부 연동" 그룹에 표시한다. 숨기거나 실행을 막지 않으며 `-a`와 직접 `--help`에도 나온다.
  설명 문구로 내부 여부를 추측하지 않는다.

그룹 제목(`cli.help_group.*`)과 시작 안내(`cli.help_start.*`)는 clap 슬롯이 아니므로
`help_groups::text_keys`가 키 목록을 제공하고 `group_headings_and_start_lines_exist_in_every_language`가
en/ko/ja 카탈로그를 대조한다. 그룹이 있는 명령은 en·ko·ja 모두 같은 템플릿을 쓴다.

짧은 도움말(`-h`)의 인자 설명은 doc comment 첫 문단이다. 긴 계약은 빈 줄 뒤 문단에 두어
`--help`에서만 펼친다. 결과 불명(-32061)이나 GUI 빌드의 헤드리스 제약처럼 오동작 판단에 필요한
사실은 첫 문단에 남긴다.

### 대상 옵션 이름

대상을 지정하는 옵션의 추천 표기와 호환 표기, 타입, IPC 키는 [API 규약](api-conventions.md#대상-옵션-표)의 표가 기준이다.
호환 표기는 clap 별칭이라 필드 이름과 요청 변환은 그대로다. 비영어 도움말에서는 `help_frame`이 clap의
영어 `[aliases: …]` 표기를 번역된 별칭 안내로 바꾸고 별칭은 숨김 별칭으로 남겨 계속 받는다.

### 전체 트리와 인자 사용법

`tasty -a`는 해당 빌드에 있는 host·plugin 명령을 모든 깊이까지 `경로 — 짧은 설명` 형태로 출력한다.
`help::tree_lines`가 clap 트리를 재귀로 순회하므로 수작업 명령 목록을 두지 않는다. clap이 만드는
help 하위 명령은 펼치지 않고, 별칭은 canonical 명령 줄에 함께 표시해 명령 수에 중복으로 세지 않는다.
debug 전용 명령은 release 빌드의 트리에 존재하지 않는다. 인자는 트리에 펼치지 않고 마지막 줄에서
개별 `--help`로 안내한다.

파싱 오류의 사용법 요약(`ArgInfo`)은 clap 인자 선언의 action과 값 개수를 읽는다. 값 없는 플래그는
`--strip-ansi`, 값 있는 옵션은 `--name <NAME>`, 여러 값은 `<COMMAND>...`, 선택 인자는 `[...]`로
표시한다. 인자 간 requires/conflicts와 허용값은 요약에 넣지 않으며 개별 `--help`가 안내한다.
`help.rs`의 `every_option_usage_is_accepted_by_the_parser`가 모든 옵션의 표기로 만든 argv를
실제 파서에 넣어 값 유무 표기가 맞는지 확인한다.

## 진입점 (`run.rs`)

`run_client` 는 갈래를 묻고 `ClientDriven` 이면 넘긴 뒤, 나머지 단발 RPC 경로
(요청 매핑 → 포트 파일 읽기 → 연결 → 계약 확인 → 전송 → 출력, `auto_wait` 폴링 포함)만 직접 수행한다.

**요청 매핑(`request/`)은 연결보다 먼저다.** 매핑은 서버 없이 끝나는 검증(깨진 JSON 인자 ·
없는 `--cwd` 등)이라, 실패하면 그 자리에서 원인을 stderr에 출력하고 종료한다 — 인스턴스가 없어도
사용자는 "실행 중인 인스턴스가 없다" 가 아니라 자기 인자의 잘못을 받고, 호스트에는 아무것도
안 보낸 빈 연결이 생기지 않는다. 서버의 값이 있어야 하는 확인(계약 확인 `contract::ensure`)만
연결 뒤에 남는다. plugin 동적 명령(`try_run_plugin_cli`)도 같은 순서다. 그래서 인스턴스가 없을
때의 종료 코드도 그 인자 오류의 값이다(근거·대안은
이 문서).
시험은 `tests/cli_maps_args_before_connecting.rs`.

인자 오류의 코드는 해당 검사에서 정한 1 또는 2이며 서버 부재 때문에 1로 덮지 않는다.
`preset save --file -`는 연결 전에 stdin을 읽고, memory TTL의 expires_at도 요청 매핑
시점에 계산한다. 매핑에서는 서버를 조회하지 않는다.

## stdout 출력 (`out.rs`)

일반 CLI 출력은 `outln!`·`out!`·`flush`·`from_io`를 사용하고 오류를 Result로 전달한다.
`println!`·`print!`는 쓰기 실패를 panic으로 바꾸므로 사용하지 않는다.
BrokenPipe는 `StdoutClosed`로 구분해 CLI 진입점의 `quiet_if_stdout_closed`가 종료 코드 0으로
처리한다. 다른 I/O 오류는 오류 보고와 종료 코드 1로 끝낸다. 이 처리는 호스트에 적용하지 않는다.

출력 도중 즉시 process::exit하지 않고 오류를 전파해야 SSH 터널 등 Drop 정리가 수행된다.
폴링 명령은 다음 실제 출력 때 닫힌 파이프를 감지한다. 출력할 데이터가 없으면 빈 flush만으로
닫힘을 알 수 없어 기다릴 수 있다. raw attach bridge는 best-effort로 화면을 미러하므로
stdout 닫힘을 세션 detach로 승격하지 않는 별도 경로다.

stderr에는 `errln!`을 사용한다. 쓰기 실패는 더 보고할 채널이 없으므로 무시하지만,
원래 명령의 실패 1·파싱 오류 2 등 종료 코드는 유지한다. 실제 버그의 panic hook은 유지한다.
관련 시험은 `tests/cli_stdout_broken_pipe.rs`다.

## 부모 ID 표 (`format/parent_table.rs`)

child 목록과 작업 DAG를 AI에게 주는 기본 텍스트 형식이다. 표는 CLI가 호스트의 JSON 응답으로 만든다. 호스트 IPC 응답은 표를 모른다.

```text
| id | parents | name | state |
|---|---|---|---|
| a | - | build | succeeded |
| c | a,b | merge | waiting |
| b | - | lint | ready |
```

- 값 없음은 `-`다. 값이 정확히 `-`인 경우와 구분하지 않으므로 필요하면 `--format json`을 쓴다.
- 부모가 여럿이면 공백 없이 쉼표로 잇고, 같은 id는 한 번만 적으며, id 순으로 정렬한다. 표에 없는 부모도 그대로 적는다.
- 행 순서는 뿌리를 id 순으로 두는 깊이 우선 전위 순회다. 형제도 id 순으로 방문하고, 부모가 여럿인 행은 처음 닿는 자리에 한 번만 둔다. 표 밖의 부모만 가진 행은 뿌리다. 순환이라 뿌리에서 닿지 않는 행은 가장 작은 id부터 이어서 걷는다.
- id 비교 기준은 호출자가 정한다. surface id는 숫자로, task id는 문자열로 비교한다.
- 값 안의 `\`는 `\\`, `|`는 `\|`, 줄바꿈은 `\n`, 캐리지 리턴은 `\r`로 적고, 그 밖의 제어 문자는 지운다. `parents` 셀의 id 안 쉼표는 `\,`로 적는다.
- 빈 표는 머리 두 줄이고 종료 코드는 0이다.

DAG의 열은 [에이전트 러너](agent-runner.md)의 DAG 조회 절에, child의 열은 [child 터미널](../features/child-terminal/index.md#children-출력)에 있다.

## 호스트 오류 출력 (`rpc_error.rs`)

단발 RPC·동적 plugin 명령·auto_wait·폴링·계약 확인 오류는 `rpc_error::exit_with`를 사용한다.
첫 줄은 `Error (<code>): <message>`, 값이 있는 error.data는 둘째 줄
`data: <한 줄 JSON>`으로 그대로 출력하고 종료 코드 1로 끝낸다.
data가 없거나 null이면 둘째 줄을 출력하지 않는다. `data: ` 접두사는 번역하지 않는다.

`events follow`와 `plugin audit-follow`는 오류를 main으로 전달하므로 첫 줄의
`Error: Error (…)` 접두사를 유지한다. 이 경로에는 `with_data_line`을 사용해 둘째 줄을 더한다.
`JsonRpcCallError::Display` 자체는 바꾸지 않아 다른 진단 문구의 형식을 유지한다.
새 오류 경로도 직접 출력하는지 main으로 올리는지에 맞춰 두 함수를 선택한다.

## CLI와 IPC의 접근성

`METHOD_TABLE`과 `DEBUG_METHODS`의 각 메서드는 CLI 진입점이 있거나, 없는 이유가 있어야 한다.
응답이 플러그인 자신의 신원·설정·이벤트 수신처를 요구하면 일반 셸 명령으로 재현할 수 없다.
그 밖에 ID로 대상을 지정하거나 전역 상태를 읽는 기능은 CLI를 제공한다.
사유 표는 [API 규약](api-conventions.md)에 한 번만 관리한다.

`tests/cli_method_table_parity.rs`는 요청 매핑의 메서드, 명시적 method 필드,
번들 plugin 매니페스트의 ipc_method를 읽어 표와 양방향 대조한다.
플래그 뒤의 호출이나 통신하지 않는 클라이언트 명령도 있으므로 이름이나 네트워크 요청
개수만으로 진입점 부재를 단정하지 않는다. 문자열 추출은 실제 실행의 완전한 증명이 아니며
의심되는 항목은 요청 매핑과 실행 경로를 확인한다. 대안으로 안내한 CLI 명령도 실제 있어야 한다.

## 호스트 로그와 CLI 진단

공유 tracing 파일은 호스트로 확정된 뒤 `enable_host_file_log`에서 연다.
GUI와 headless가 대상이며, CLI는 stderr만 사용해 호스트 파일을 건드리지 않는다.
panic hook과 stderr tracing은 main 시작에 초기화해 동적 CLI 라우팅 중 오류도 남긴다.

host 파일은 실행할 때마다 비우므로 재시작을 넘는 영구 기록이 아니다.
CLI가 성공한 IPC 앞뒤에서 남긴 일반 warning은 hook-failures.log의 대상이 아닐 수 있다.
예를 들어 stdin JSON 파싱 경고는 stderr에서 확인한다.
파일 위치와 빌드별 필터는 [크래시 진단](crash-diagnostics.md)을 따른다.

## 새 워크스페이스의 윈도우와 작업 경로

`tasty new workspace --surface <숫자 ID>`는 그 서피스의 윈도우를 선택한다.
없는 ID는 거절하고 다른 윈도우로 대체하지 않는다. nickname·this나 `--window`는 받지 않는다.
생략하면 `TASTY_SURFACE_ID`를 적용하지 않고 기존의 대상 없는 요청 규칙을 따른다.
여러 윈도우를 제어하는 에이전트는 명시적으로 서피스를 지정한다.

terminal 워크스페이스의 cwd는 명시 `--cwd`가 우선이다. 생략하고 inherit_cwd가 켜져 있으면
지정한 서피스의 로컬 cwd를 상속한다. 서피스까지 생략하면 선택된 윈도우의 포커스 서피스에서
상속하는 기존 동작이다. 상속을 끄거나 원본이 mirror여서 로컬 cwd가 없으면 홈을 사용한다.
IPC의 숫자가 아닌 surface_id는 invalid_params다. 윈도우 선택은 라우터가, cwd 선택은
workspace 핸들러의 `resolve_create_cwd`·`inherit_cwd_for_create`가 맡는다.

## 호출자 환경 (`caller_context.rs`)

dynamic plugin 명령(`contributes.cli`)의 요청에 호출자 환경을 고정된 형식으로 싣는다.
다른 프로그램의 도구 호출 안에서 실행한 CLI는 그 프로그램이 넣은 환경 변수로 호출한 대화를
알 수 있다. 같은 환경의 `TASTY_SURFACE_ID`는 그 프로그램을 시작한 다른 터미널의 값일 수 있다.
무엇을 읽고 어떤 형식을 요구할지는 플러그인이 매니페스트 `[[contributes.caller_context]]`로
선언한다([선언 형식](plugin-development.md#cli-호출자-문맥-contributescaller_context)).
CLI는 선언을 해석만 하며 제품 이름이나 변수 이름을 알지 않는다. 예: Codex 플러그인이
`CODEX_THREAD_ID`(UUID)·`CODEX_HOME`·`codex` 실행 파일을 선언한다.

CLI는 `~/.tasty/plugins/*`의 매니페스트를 플러그인 ID 순으로 읽어 선언을 모은다. 플러그인
프로세스와 무관하게 디스크의 매니페스트만 보므로 첫 호출부터 적용된다. 비활성화한 플러그인의
선언도 읽는다. 명령이 그대로 보이고, 문맥을 받은 다른 플러그인이 "provider가 꺼져 있다"는 오류를
내야 다른 surface로 대신하지 않기 때문이다. 제거한 플러그인의 선언은 없다.

선언 하나를 호출 환경에 적용한 결과는 셋이다.

- `id_env`가 없거나 빈 문자열이면 그 provider의 호출자가 없다.
- 값이 `id_format`(`text`: 비어 있지 않은 UTF-8, `uuid`: 하이픈 위치가 고정된 36자 16진수)에 맞고 `path_env` 값을 절대 경로로 만들 수 있으면 유효한 호출자다. `path_env`는 비어 있지 않을 때만 싣는다. `executable`을 선언했으면 PATH에서 찾아 실행 정보로 싣고, 찾지 못하면 싣지 않는다.
- 그 밖에는 쓸 수 없는 값이다.

어떤 명령에 적용하는가:

- 유효한 호출자의 문맥은 모든 dynamic plugin 명령에 싣는다. Codex 대화에서 부른 `tasty claude spawn`처럼 선언하지 않은 플러그인의 명령도 같은 문맥으로 Codex 부모를 찾는다. 여러 provider가 함께 유효하면 플러그인 ID 순, 같은 플러그인 안에서는 선언 순의 첫 번째를 싣는다.
- 쓸 수 없는 값은 엄격 대상 명령만 실행하지 않고 오류로 끝낸다. `TASTY_SURFACE_ID`로 대신하지 않는다. 엄격 대상은 그 provider를 선언한 플러그인의 명령과 `requires_caller_context`에 그 provider를 적은 플러그인의 명령이다(예: Claude 플러그인이 `["codex"]`). 다른 플러그인 명령은 값이 없을 때와 같이 동작한다.

plugin 인자 그룹이 `surface`·`caller_surface` u32 인자를 선언했고 명시값이 없으면
`TASTY_SURFACE_ID`로 채운다. `surface`를 채우거나 명시하면 `surface_id`도 같은 값으로 맞춘다.
싣는 호출자의 선언이 `suppress_env_surface = true`(기본값)이면 채우지 않는다.
명시한 `--surface`(작업 대상 child 등)는 그대로 보낸다.
CLI는 요청 params에 예약 키 `_tasty_context`(객체)를 붙인다.

| 필드 | 형식 | 언제 |
|---|---|---|
| `provider` | 선언의 `provider` | 유효한 호출자 |
| 선언의 `id_field` | 문자열(환경 값 그대로) | 유효한 호출자 |
| 선언의 `path_env[].field` | 절대 경로 문자열 | 유효한 호출자이고 해당 변수가 비어 있지 않을 때 |
| `execution` | `{executable: 절대 경로, search_path: PATH 문자열}` | 유효한 호출자이고 선언한 `executable`을 호출 환경에서 찾았을 때 |
| `implicit` | 문자열 배열(`surface`·`surface_id`·`caller_surface` 중) | CLI가 `TASTY_SURFACE_ID`로 채운 키가 있을 때 |

채울 것도 호출자도 없으면 키를 붙이지 않는다. 인자·stdin으로 같은 이름이 들어오면
예약 이름 오류로 거절한다. 이 값은 호출자가 가진 환경 변수일 뿐 인증된 신원이 아니다.
받는 쪽은 호출자의 결과를 구분하고 해당 프로그램 설치본에 연결할 때 쓴다.
실행 정보는 `tasty-utils::execution`으로 찾는다. 상대 PATH 항목은 호출 cwd 기준으로 절대화하며,
셸 alias·function은 실행하지 않는다. POSIX에서는 실행 권한을 확인하고 Windows에서는 각 PATH
디렉터리의 `.exe`·`.cmd`·`.bat`·`.com` 순으로 찾는다. 실행 파일 경로나 PATH가 UTF-8이 아니면
쓸 수 없는 값으로 본다. 선언한 변수·실행 파일과 PATH 외의 환경·인증 값은 복사하지 않는다.

호스트 명령(`request.rs`)은 이 규칙을 쓰지 않는다. 대상 선택(`resolve_surface_id`·`"this"`)과
close 계열의 호출자 자기 보호(`caller_surface_id`)는 호출자 문맥과 관계없이
`TASTY_SURFACE_ID`를 읽는다. 대상 선택과 자기 보호는 목적이 달라 한 규칙으로 묶지 않는다.

## `debug` 갈래

`commands/debug.rs`(선언)와 `local/debug.rs`(실행) 둘 다 모듈째
`#![cfg(debug_assertions)]`로 제한한다. 사용자 입력을 재현하는 명령은 release 빌드에 포함하지 않는다.
[debug-ipc.md](debug-ipc.md), [사용자 통제와 데이터 원칙](../principles/control-and-data.md).

## 관련

- [api-conventions](api-conventions.md) — CLI/IPC 명명 + 안정성 정책
- [build](build.md) — 크레이트 **경계**(이 문서는 크레이트 **내부**)
- [attach-behavior](attach-behavior.md) — `local/attach.rs` 의 attach 세션 머신

## 에이전트 훅 전달 실패 기록

CLI는 포트 파일 부재, 연결 실패, JSON-RPC 오류를 접속 대상의
`<tasty_home>/hook-failures.log`에 기록한다. `TASTY_PARENT_HOME`은 접속 대상 홈을
정하지 않으므로 이 기록 위치에 사용하지 않는다. 마지막 메서드 이름이 `hook` 또는
`_hook`으로 끝나는 요청만 기록한다. 일반 대화형 명령은 stderr로 오류를 알린다.

레코드는 `<UTC> method=… event=… surface=… code=… reason=…` 한 줄이다.
이벤트나 JSON-RPC 코드가 없으면 `-`를 쓰며, 공백을 담는 `reason`은 마지막에 둔다.
실패 분류에는 앞의 필드를 사용한다. `reason`은 응답을 만든 쪽의 언어를 따르므로
plugin이 번역한 메시지도 올 수 있다. `DiagnosticEnglish`와 관련 소스 검사는 CLI가
직접 만드는 포트·연결 오류의 영어 진단만 보호하며 응답 메시지 전체를 보장하지 않는다.

정상 호출은 기록하지 않는다. 파일이 256 KiB에 이르면 `.log.1` 한 개로 교체하며
기록·로테이션 실패가 훅 실행을 추가로 실패시키지는 않는다. 따라서 크기는 엄격한 상한이
아니며 파일 권한이나 디스크 문제로 기록이 누락될 수도 있다. loopback 연결에는 3초 상한을 둔다.

설치 래퍼는 `TASTY_SURFACE_ID`가 없으면 호출하지 않는다. POSIX 래퍼의 `|| true`는
전달 실패가 외부 에이전트 턴을 막지 않도록 한다. 실패 상태 자체는 로그로 확인한다.
설치 명령을 바꾸면 기존 사용자 설정에도 다시 설치해야 하며, 같은 matcher 안의 사용자
핸들러를 보존하면서 Tasty 항목만 갱신하는지 확인한다.

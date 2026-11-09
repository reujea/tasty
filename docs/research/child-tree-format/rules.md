# child 트리 형식 측정: 설계와 판정 규칙

AI 에이전트가 child 트리를 어떤 텍스트 형식으로 받을 때 정확하고 싸게 읽는지 재는 방법과, 결과에서 권고를 끌어내는 규칙을 정한다. 이 문서는 어떤 측정 결과보다 먼저 커밋한다. 목적과 인용은 [개요](index.md)에 있다.

## 고정 범위

- [판정 규칙](#판정-규칙)과 [규모 축소 규칙](#규모-축소-규칙)의 수치는 결과를 보기 전에 정한다. 판정 규칙 구간은 이 문서의 첫 커밋 뒤에 바꾸지 않는다. 결과가 규칙과 어울리지 않아 보여도 규칙을 고치지 않고 결과 문서에 따로 설명한다. 이 구간이 정한 정의는 다른 절을 고쳐도 바뀌지 않는다.
- 그 밖의 절은 호출 명령, 응답 필드, 파싱처럼 실행해야 알 수 있는 문제가 있을 때만 고친다. 고치면 [규칙 변경 기록](#규칙-변경-기록)에 내용, 이유, 결과를 본 뒤인지, 전후 커밋을 적는다.
- 소규모 시험의 정답률을 근거로 규칙, 질문, 프롬프트를 고치지 않는다. 시험 호출은 판정에 넣지 않는다.

## 데이터

### 노드

트리 하나는 노드 목록이다. 모든 형식을 같은 노드 목록에서 생성한다.

| 필드 | 값 | 값 없음 |
|---|---|---|
| `id` | `n` 뒤 세 자리 숫자. 시드로 섞어 부여한다 | 없음 |
| `parent` | 다른 노드의 `id` | 뿌리 |
| `nickname` | 영문 소문자, 숫자, 하이픈만 쓰는 고유한 값 | 있음 |
| `state` | `active`, `idle`, `needs_input`, `stale`, `exited` | 없음 |
| `cwd` | `/work/repo-a/wt-01` 형식의 가명 경로 | 없음 |
| `last_report_at` | 초 단위 UTC ISO 8601 | 있음 |

`nickname`과 `cwd`에는 공백, 큰따옴표, 세미콜론, 파이프를 넣지 않는다. 생성 단계에서 위반을 오류로 처리한다. 형식마다 값을 다르게 이스케이프해 정보량이 달라지는 일을 막기 위해서다.

깊이는 뿌리를 0으로 센다. 뿌리가 여럿인 숲은 그대로 두고 가상 뿌리를 만들지 않는다.

### 트리

| 이름 | 노드 수 | 만드는 방법 |
|---|---|---|
| `real` | 스냅샷의 노드 수 | 실제 child 트리 스냅샷을 익명화한다 |
| `synthetic_40` | 정확히 40 | `real`과 같은 모양으로 시드 고정 생성한다 |
| `synthetic_80` | 정확히 80 | `real`과 같은 모양으로 시드 고정 생성한다 |

**스냅샷**: 저장된 child 관계 파일의 부모 관계를 뿌리까지 걸어 숲 전체를 잡는다. 호출 시점의 `tasty claude children`과 `tasty codex children` 결과는 살아 있는 노드의 현재 값을 채우는 데 쓴다. 여기서 조회되지 않는 노드의 상태는 `terminal.state`의 판정을 따른다. 이 명령이 `no live surface` 오류를 내면 그 surface는 이미 없으므로 `exited`로 둔다. `last_report_at`은 이 CLI 출력에 없어서 저장 파일의 `last_state_report_at`을 읽기 전용으로 합친다. 값이 없는 노드는 값 없음으로 두고 그 수를 기록한다. 값을 지어내지 않는다. `cwd`는 값 없음을 두지 않으므로, 저장 파일에 항목이 없어 경로를 읽을 수 없는 뿌리는 어떤 실제 저장소와도 묶지 않는 별도 가명 저장소 아래의 서로 다른 가명으로 두고 그 수를 기록한다. 부모가 surface가 아닌 외부 키인 노드는 뿌리로 두고 그 수를 기록한다.

**같은 모양**: 깊이 분포, 부모별 자식 수 분포, 상태 비율, `nickname` 없음 비율, 보고 시각 간격을 스냅샷과 맞춘다. 스냅샷의 최대 깊이가 3보다 얕거나 어떤 상태가 빠져 있으면, 합성 트리는 최대 깊이 3 이상과 다섯 상태 각 1개 이상을 강제한다. 달라진 항목은 변경 기록과 한계에 적는다.

**시각**: 트리에서 가장 늦은 보고 시각을 `2026-01-05T12:00:00Z`로 옮기고 나머지는 간격을 유지한 채 함께 옮긴다. 기준 시각 T는 모든 트리에서 `2026-01-05T12:05:00Z`다. 합성 트리의 보고 시각은 서로 겹치지 않는다. 저장값은 밀리초이므로 초 단위로 내림한 뒤 옮긴다. 내림으로 실제 트리의 시각은 겹칠 수 있고, 이 경우 `q11`의 동률 처리와 `q12`의 후보 조건이 적용된다.

### 익명화

공개 저장소에 남기므로 스냅샷의 경로, 비공개 저장소 이름, 작업 내용을 남기지 않는다. 스냅샷 원본과 대응표는 커밋하지 않는다.

| 필드 | 변환 |
|---|---|
| `id` | 시드로 섞은 새 번호. 원래 surface ID를 남기지 않는다 |
| `nickname` | 원문 전체를 버리고 `lane-a` 같은 어휘로 바꾼다. 값 없음은 그대로 둔다 |
| `cwd` | 가명 경로로 바꾼다. 같은 원본은 같은 가명으로 바꾸고 저장소 이름을 쓰지 않는다 |
| `state` | 원래 값을 유지한다 |
| `last_report_at` | 위 시각 규칙을 따른다 |
| 그 밖의 필드 | 저장하지 않는다 |

합성 트리도 같은 어휘를 쓴다. 측정 자료는 `.txt`, `.json`, `.jsonl`로 두고 압축하지 않는다. Markdown으로 저장하면 문서 검사가 측정 입력에 걸리고, 압축하면 검사와 리뷰에서 내용을 볼 수 없다. 합계가 5 MB를 넘으면 커밋 전에 실행 담당에게 알린다.

## 형식

| 이름 | 프롬프트에 적는 이름 | 값 없음 표기 |
|---|---|---|
| `parent_table` | `parent id table` | `-` |
| `adjacency` | `adjacency list with attributes` | `-` |
| `mermaid` | `Mermaid flowchart` | `-` |
| `flat_json` | `flat JSON array` | `null` |
| `nested_json` | `nested JSON` | `null` |
| `indented` | `indented list` | `-` |

모든 형식의 노드 순서는 같다. 뿌리를 `id` 오름차순으로 두고 깊이 우선 전위 순회하며, 형제도 `id` 오름차순으로 방문한다. 줄 끝 공백과 빈 줄은 넣지 않는다. 필드 이름은 형식과 관계없이 같다.

아래 예시는 뿌리 `n104` 아래에 `n230`, `n587`이 있는 숲이다. 시각과 경로는 형식을 보이기 위한 값이다.

`parent_table`:

```text
| id | parent | nickname | state | cwd | last_report_at |
|---|---|---|---|---|---|
| n104 | - | lane-a | active | /work/repo-a/wt-01 | 2026-01-05T11:40:00Z |
| n230 | n104 | - | idle | /work/repo-a/wt-02 | 2026-01-05T11:58:12Z |
| n587 | n104 | lane-b | exited | /work/repo-b/wt-01 | - |
```

`adjacency`: 노드마다 한 줄이다. 자식이 없으면 `children=-`로 적는다.

```text
n104 nickname=lane-a state=active cwd=/work/repo-a/wt-01 last_report_at=2026-01-05T11:40:00Z children=n230,n587
n230 nickname=- state=idle cwd=/work/repo-a/wt-02 last_report_at=2026-01-05T11:58:12Z children=-
n587 nickname=lane-b state=exited cwd=/work/repo-b/wt-01 last_report_at=- children=-
```

`mermaid`: 노드 줄을 모두 적은 뒤 간선 줄을 자식의 순서대로 적는다.

```text
flowchart TD
  n104["nickname=lane-a; state=active; cwd=/work/repo-a/wt-01; last_report_at=2026-01-05T11:40:00Z"]
  n230["nickname=-; state=idle; cwd=/work/repo-a/wt-02; last_report_at=2026-01-05T11:58:12Z"]
  n587["nickname=lane-b; state=exited; cwd=/work/repo-b/wt-01; last_report_at=-"]
  n104 --> n230
  n104 --> n587
```

두 JSON 형식은 구분자 규칙이 같다. 쉼표와 콜론 뒤에 공백을 넣지 않는다(Python `json.dumps`의 `separators=(",", ":")`). 토큰 비교가 구분자 차이에 영향받지 않게 하기 위해서다.

`flat_json`: 배열 괄호를 각각 한 줄에 쓰고 노드마다 한 줄을 쓴다.

```text
[
{"id":"n104","parent":null,"nickname":"lane-a","state":"active","cwd":"/work/repo-a/wt-01","last_report_at":"2026-01-05T11:40:00Z"},
{"id":"n230","parent":"n104","nickname":null,"state":"idle","cwd":"/work/repo-a/wt-02","last_report_at":"2026-01-05T11:58:12Z"},
{"id":"n587","parent":"n104","nickname":"lane-b","state":"exited","cwd":"/work/repo-b/wt-01","last_report_at":null}
]
```

`nested_json`: 숲 전체를 한 줄로 쓴다. 뿌리의 배열이 바깥이고 `parent` 키는 없다. 자식이 없으면 `"children":[]`로 쓴다. 아래 예시만 읽기 쉽도록 노드 단위로 줄을 나눴다.

```text
[{"id":"n104","nickname":"lane-a","state":"active","cwd":"/work/repo-a/wt-01","last_report_at":"2026-01-05T11:40:00Z","children":[
{"id":"n230","nickname":null,"state":"idle","cwd":"/work/repo-a/wt-02","last_report_at":"2026-01-05T11:58:12Z","children":[]},
{"id":"n587","nickname":"lane-b","state":"exited","cwd":"/work/repo-b/wt-01","last_report_at":null,"children":[]}]}]
```

`indented`: 깊이마다 공백 2칸을 들여 쓴다.

```text
- n104 nickname=lane-a state=active cwd=/work/repo-a/wt-01 last_report_at=2026-01-05T11:40:00Z
  - n230 nickname=- state=idle cwd=/work/repo-a/wt-02 last_report_at=2026-01-05T11:58:12Z
  - n587 nickname=lane-b state=exited cwd=/work/repo-b/wt-01 last_report_at=-
```

생성 스크립트는 여섯 형식을 다시 읽어 노드 집합, 부모 관계, 모든 필드, 값 없음 표기를 원본과 필드 단위로 대조한다. 형식 사이 정보량이 다르면 비교가 무의미하기 때문이다.

## 질문

질문은 15개이고 종류는 7개다. 정답은 모델이 아니라 스크립트가 노드 목록에서 계산한다. 대상 노드는 트리마다 한 번 뽑아 모든 형식, 모델, 반복에 같게 쓴다.

| ID | 종류 | 답 형태 | 프롬프트 문구 |
|---|---|---|---|
| `q1` | 필터 | id 집합 | `List the ids of all silent nodes.` |
| `q2` | 필터 | id 집합 | `List the ids of all nodes with state needs_input.` |
| `q3` | 필터 | id 집합 | `List the ids of the descendants of {a} with state exited.` |
| `q4` | 조회 | 문자열 | `What is the cwd of the node with nickname {nick}?` |
| `q5` | 조회 | id | `What is the id of the parent of {b}?` |
| `q6` | 구조 | 정수 | `How many direct children does {a} have?` |
| `q7` | 집계 | 정수 | `How many descendants does {a} have?` |
| `q8` | 집계 | 정수 | `How many descendants of {a} have state active?` |
| `q9` | 경로 | id 순서열 | `Give the ids on the path from the root down to {c}, including both ends.` |
| `q10` | 경로 | id | `What is the id of the lowest common ancestor of {c} and {d}?` |
| `q11` | 시간 | id | `Among nodes whose state is not exited and that have a last_report_at, which id has the oldest last_report_at?` |
| `q12` | 시간 | id | `Among the siblings of {e} that have a last_report_at, which id has the most recent last_report_at?` |
| `q13` | 구조 | 정수 | `How many siblings does {e} have?` |
| `q14` | 함정 | null | `What is the id of the parent of {missing_id}?` |
| `q15` | 함정 | null | `What is the state of the node with nickname {missing_nick}?` |

중괄호는 질문마다 따로 뽑아 채운다. 대상을 고르는 조건은 다음과 같다.

| 자리 | 후보 |
|---|---|
| `q3`, `q6`, `q7`, `q8`의 `{a}` | 자손이 2개 이상인 노드. 없으면 1개 이상인 노드 |
| `q5`의 `{b}`, `q9`의 `{c}` | 깊이 2 이상인 노드. 없으면 깊이 1 이상인 노드 |
| `q4`의 `{nick}` | `nickname`이 있는 노드. 없으면 문구를 `the node {b}`로 바꾼다 |
| `q10`의 `{c}`, `{d}` | 공통 조상이 있고 어느 쪽도 다른 쪽의 조상이 아닌 쌍 |
| `q12`의 `{e}` | 보고 시각이 있는 형제가 2개 이상이고 그 최댓값이 겹치지 않는 노드 |
| `q13`의 `{e}` | 형제가 1개 이상인 노드 |
| `{missing_id}` | 트리에 없는 같은 형태의 `id` |
| `{missing_nick}` | 트리에 없는 같은 어휘의 `nickname` |

- `q1`의 N은 `state`가 `active` 또는 `stale`이고 `last_report_at`이 있는 노드에서 T − `last_report_at`의 중앙값을 구해 분 단위로 내림한 값이다. 최솟값은 1이다. 해당 노드가 없으면 N은 30이고 정답은 빈 목록이다.
- `q2`, `q3`의 대상 상태가 트리에 없으면 빈 목록이 정답이다. 이 경우와 `real`의 깊이가 2 미만인 경우의 경로·집계 질문은 변별력이 낮다고 한계에 적는다.
- `q11`의 최솟값이 겹치면 겹치지 않는 부분 트리 중 노드가 가장 많은 뿌리 아래로 범위를 좁히고, 문구 앞에 `Among the descendants of {root} only:`를 붙인다. 후보는 자손 중 조건에 맞는 노드의 최솟값이 겹치지 않는 숲의 뿌리이고, 자손이 가장 많은 뿌리를 고른다. 자손 수가 같으면 `id`가 작은 뿌리를 고른다. 후보가 없으면 그 트리에서 `q11`을 뺀다.
- 조건에 맞는 대상이 없는 질문은 그 트리에서 빼고 뺀 사실을 기록한다. 채점 수는 실제 물은 질문으로 센다.

## 프롬프트

프롬프트 언어는 영어다. 시스템 문장과 사용자 문장은 다음과 같다. Codex는 시스템 문장 옵션이 없어 같은 문장을 입력 맨 앞에 둔다.

시스템:

```text
You read a tree of agent processes given as text and answer questions about it. Use only the tree in the message. You have no tools. Reply with one JSON object and nothing else.
```

사용자:

```text
Each node has an id, a nickname, a state, a cwd and a last_report_at, and sits under at most one parent. A node without a parent is a root.
Tree format: {format_name}
{missing_note}
Reference time: {T}

<tree>
{rendered}
</tree>

Definitions:
- States: active is working, idle is waiting for a new task, needs_input is waiting for user input, stale has shown no recent activity, exited is gone.
- Descendants of X are all nodes below X at any depth, not X itself.
- Siblings of X are the other nodes with the same parent. A root has no siblings.
- A node is silent when its state is active or stale and its last_report_at is at least {N} minutes before the reference time. A node without a last_report_at is not silent.

Answer rules:
- Reply with one JSON object only, with no code fence and no explanation. Use each question id as a key.
- A list of ids is a JSON array of strings. Use [] when there are none.
- A path is a JSON array of strings in order.
- A count is a JSON integer.
- An id or a text value is a JSON string.
- Use JSON null when the node or the value does not exist.

Questions:
{questions}
```

`{missing_note}`는 값 없음 표기가 `-`인 형식에 `A missing value is written as -.`, `null`인 형식에 `A missing value is written as null.`이다. `{questions}`는 호출마다 섞은 순서로 `q7: How many ...` 형태의 줄을 나열한다.

## 채점

- 응답 전체를 다듬은 뒤 JSON 객체로 읽는다. 읽지 못하면 응답 전체가 코드 펜스 하나로 싸인 경우에만 펜스를 벗겨 다시 읽고, 펜스 응답 수를 따로 센다.
- 그래도 JSON 객체가 아니면 파싱 실패다. 그 호출의 모든 질문을 오답으로 채점하고 파싱 실패 수를 따로 센다.

| 답 형태 | 정답 조건 |
|---|---|
| id 집합 | 문자열 배열이고 앞뒤 공백을 뺀 집합이 정답과 같다. 중복은 하나로 친다 |
| id 순서열 | 문자열 배열이고 앞뒤 공백을 뺀 순서가 정답과 같다 |
| 정수 | JSON 정수이고 값이 같다. 문자열, 실수, 불리언은 오답이다 |
| id, 문자열 | JSON 문자열이고 앞뒤 공백을 뺀 값이 정답과 정확히 같다 |
| null | JSON `null`이다. 문자열 `"null"`, 빈 문자열, 빈 배열은 오답이다 |

정답 키가 없으면 그 질문은 오답이다. 정답에 없는 키는 무시한다. 대소문자를 바꾸어 맞추지 않는다.

## 모델과 호출

| 구분 | 모델 ID | 추론 수준 |
|---|---|---|
| 높은 쪽 Claude | `claude-opus-5-5` | `medium` |
| 낮은 쪽 Claude | `claude-haiku-5-5` | `medium`. 거부하면 기본값으로 두고 기록한다 |
| Codex | `gpt-6.1-sol` | `medium` |

호출마다 새 세션을 쓰고 도구를 끈다. 호출은 하나씩 순서대로 실행한다. 호출 하나에 질문을 모두 묶는다.

```text
claude -p --safe-mode --tools "" --strict-mcp-config --no-session-persistence --system-prompt <시스템 문장> --model <ID> --effort medium --output-format stream-json --verbose
codex exec --json --ephemeral --skip-git-repo-check --ignore-user-config -C <빈 폴더> -m gpt-6.1-sol -c model_reasoning_effort=medium -c features.shell_tool=false -c project_doc_max_bytes=0
```

프롬프트는 stdin으로 넣는다. 격리 조건은 다음과 같다.

- 작업 디렉터리는 호출마다 새로 만든 빈 임시 폴더다. 저장소 안에서 실행하면 저장소의 에이전트 지침이 입력에 섞인다.
- 환경에서 `TASTY_`로 시작하는 변수를 모두 지운다. 지우지 않으면 Claude Code 훅이 측정 호출을 실행 중인 child의 상태로 보고한다.
- Claude의 `--bare`는 구독 인증을 읽지 않으므로 쓰지 않는다.
- 러너는 셸 별칭이 아닌 실행 파일을 직접 호출하고 CLI 버전을 기록한다.
- 샌드박스가 동작하지 않는 컴퓨터에서 Codex를 실행하려고 샌드박스를 끄면 프롬프트 첫 줄에 읽기 전용 지시를 넣고 실행 전후 `git status --porcelain`을 비교한다. 쓴 옵션과 지시는 변경 기록에 적는다.
- 전역 사용자 지침이 Codex 입력에서 빠지는지는 입력 토큰으로 단정할 수 없다. 확인하지 못하면 미확인으로 쓴다. 이 입력은 Codex 칸 안에서 형식과 관계없이 같으므로 Codex 안의 형식 비교에만 쓰고 모델 사이 토큰 비교에는 쓰지 않는다.

### 측정값

| 값 | 정의 |
|---|---|
| Claude 입력 토큰 | `input_tokens` + `cache_creation_input_tokens` + `cache_read_input_tokens` |
| Codex 입력 토큰 | `input_tokens`. 캐시 포함이므로 `cached_input_tokens`를 따로 저장한다 |
| 출력 토큰 | `output_tokens`. 추론 토큰이 포함되는지는 소규모 시험에서 확인해 변경 기록에 적는다. 확인 결과가 판정 규칙 3의 계산을 바꾸지 않는다 |
| 지연 | 러너가 잰 벽시계 시간. Claude의 `duration_ms`, `duration_api_ms`도 함께 저장한다 |

호출별 원자료에는 허용한 필드만 저장한다: 호출 ID, 형식, 트리, 모델 요청값과 응답값, CLI 버전, 시작·종료 시각, 벽시계 지연, 시드, 프롬프트 해시, 응답 본문, 토큰 필드, `total_cost_usd`(CLI의 추정치), 오류 종류, 상태 코드, 도구 위반 수, `rules_commit`, `rules_blob`, 7일·5시간 창 상태, 시도 번호(`attempt`), 시도 종류(`attempt_kind`), 실행 구분(`phase`), 수준(`level`), 반복 번호(`repeat`). 수준 변경 기록은 호출별 줄과 섞지 않고 원자료 폴더 안의 별도 파일 `data/raw/level-changes.jsonl`에 둔다. 호출별 줄은 `data/raw/pilot.jsonl`, `data/raw/main.jsonl`이다. 수준을 바꾸면 바꾼 시각, 이전·새 수준, 창별 사용량 값을 이 파일에 남긴다. CLI 원본 이벤트에는 작업 경로와 세션 ID가 들어갈 수 있어 그대로 저장하지 않는다.

요약은 추적 자료(원자료, 질문, 정답, 스케줄)만 입력으로 받아 스크립트로 생성한다. 형식별 정답률, 토큰, 지연을 모델·트리·질문 종류로 나누어 내고 반복 간 불확실성을 함께 적는다. 수치를 손으로 옮기지 않는다.

### 시드

기준 시드는 `20261109`다. 용도별 난수기는 `sha256("<기준 시드>/<용도>/<키>")`의 앞 8바이트를 빅 엔디언 정수로 읽어 Python `random.Random`에 넣는다.

| 용도 | 키 | 정하는 것 |
|---|---|---|
| `tree` | 트리 이름 | 번호 부여, 합성 트리 |
| `targets` | 트리 이름, 질문 ID | 질문 대상 |
| `order` | 호출 ID | 호출별 질문 순서 |
| `schedule` | 수준 이름. 소규모 시험은 `pilot` | 전체 호출의 실행 순서 |

키가 둘이면 `/`로 잇는다. `targets`의 입력은 `20261109/targets/real/q3` 형태다.

호출 ID는 `<트리>-<형식>-<모델 구분>-r<반복 번호>`다. 시험 호출은 앞에 `pilot-`를 붙인다. 모델 구분은 `claude-high`, `claude-low`, `codex`다. 재시도는 호출 ID와 질문 순서, 프롬프트 바이트를 그대로 두고 원자료의 `attempt`(1부터)로 구분한다. 재시도마다 순서를 바꾸면 같은 반복의 시도끼리 입력이 달라지기 때문이다.

## 호출 규모

```text
본 실행 L0 = 형식 6 × 트리 3 × 모델 3 × 반복 3 = 162회
소규모 시험 = 모델 3 × 형식 2 × 트리 1 × 반복 1 = 6회
```

소규모 시험은 `synthetic_80`에서 렌더링 바이트 수가 가장 작은 형식과 가장 큰 형식으로 실행한다. 명령, 응답 필드, 도구 차단, 사용량 필드를 확인하는 용도다. 재시도 상한은 2회다.

### 규모 축소 규칙

| 수준 | 조합 | 호출 수 | 재시도 상한 |
|---|---|---|---|
| L0 | 모든 모델 반복 3 | 6×3×3×3 = 162 | 17 |
| L1 | 낮은 쪽 Claude 반복 3, 높은 쪽 Claude와 Codex 반복 2 | 6×3×(3+2+2) = 126 | 13 |
| L2 | L1에서 `synthetic_40` 제외 | 6×2×(3+2+2) = 84 | 9 |
| 중단 | 실행하지 않는다 | 0 | 0 |

판정 규칙 2가 낮은 쪽 모델을 쓰므로 그 모델을 마지막까지 3회 유지한다. L1과 L2로 줄이면 칸마다 반복 수가 달라지므로 한계에 적는다.

수준은 소규모 시험 뒤에 다음 값으로 고른다.

```text
창 = 같은 한도를 나누어 쓰는 묶음. Claude 7일 창, 모델별 창이 있으면 그 창, Codex 7일 창
P  = 그 창을 쓰는 모델들의 (시험 호출당 평균 input + cache_creation + output 토큰 × 그 수준의 계획 호출 수) 합
W  = 그 창의 길이만큼 최근 기간의 같은 지표 합계. 7일 창은 최근 7일, 5시간 창은 최근 5시간이다
u  = 그 창의 사용률을 0~100으로 환산한 값. 1 미만이면 1로 둔다
u' = u + u × P / W
```

| 그 수준의 호출 수로 계산한 모든 7일 창의 조건 | 수준 |
|---|---|
| u' ≤ 70 | L0 |
| u' ≤ 85 | L1 |
| u' ≤ 95 | L2 |
| L2에서도 u' > 95, 또는 상태가 `allowed_warning`·`rejected` | 중단 |
| u를 읽지 못함, 또는 W = 0 | P / W ≤ 2%이면 L0 제안, 넘거나 W = 0이면 L2 제안. 실행 담당이 정한다 |

- 상태를 먼저 확인한다. 그다음 L0부터 차례로 그 수준의 호출 수로 P를 다시 계산해 조건을 처음 만족하는 수준을 고른다.
- 캐시 읽기가 한도에 반영되는 비중을 알 수 없어 식에서 뺀다. 식의 한계로 적는다.
- 5시간 창은 7일 창으로 고른 수준의 호출 수로 u'를 계산해 같은 표의 그 수준 상한과 비교한다. 상한을 넘으면 수준을 낮추지 않고 그 창이 풀릴 때까지 기다린다. 5시간 창의 W가 0이면 u를 그대로 u'로 둔다.
- 계산한 수준은 제안이다. 실행 담당이 승인한 수준만 실행한다.

### 실패와 중단

| 사건 | 처리 |
|---|---|
| `rejected`, 429, 사용량 제한 메시지 | 즉시 중단한다. 재시도하지 않는다 |
| `allowed_warning` | 그 호출을 기록하고 중단한다 |
| 7일 창 u가 승인 때 계산한 u'보다 5포인트 이상 높음 | 중단한다 |
| 도구 사용 위반 | 그 호출을 위반으로 기록하고 채점에서 뺀다. 1회 재시도한다. 같은 모델에서 2회 나오면 중단한다 |
| 일반 API 실패, 타임아웃 | 채점에서 뺀다. 1회 재시도한다. 재시도도 실패하면 같은 반복을 스케줄 끝에서 한 번 더 실행한다. 그것도 실패하면 중단한다 |
| JSON 파싱 실패 | 재시도하지 않는다. 전부 오답으로 채점한다. 재시도하면 잘 나올 때까지 뽑는 결과가 되기 때문이다 |
| 재시도 상한 도달 | 중단한다 |

- 도구 위반은 응답 메시지와 추론이 아닌 항목이 하나라도 나온 호출이다. 항목 종류 목록은 소규모 시험의 실제 값으로 변경 기록에 고정한다.
- 첫 실행 뒤의 모든 실행은 재시도 상한에 센다. 도구 위반의 재시도가 API 실패로 끝나면 API 실패 처리를 따른다.
- 재시도를 포함해 모든 호출을 원자료에 남긴다. 실패 기록을 지우지 않는다. 칸의 한 반복은 먼저 끝난 유효 호출 하나로 센다.
- 중단하면 부분 자료를 커밋하고 어디까지 실행했는지 적는다.
- 수준 변경과 사용량 중단 뒤의 재개는 실행 담당이 정한다. 중단 여부와 관계없이 근거는 사용량 값으로 한정하고 부분 자료의 정답률을 근거로 쓰지 않는다. 본 실행의 마지막 호출 뒤에는 수준을 바꿔 이미 실행한 반복을 버릴 수 없다.
- 위 한정의 예외로, 중단한 실행은 오류가 일반 API 실패·타임아웃이고, 사용량 표가 그 수준을 허용하며, 재시도 상한이 남았으면 같은 수준에서 재개한다. 정답률을 근거로 삼지 않는다.
- 재개 뒤에도 재시도 상한과 도구 위반의 "같은 모델에서 2회"는 이어서 센다.
- 재개하면 재개 시각과 근거를 수준 변경 기록에 남긴다. 조건이 맞지 않아 재개하지 않으면 그 조건 값을 같은 기록에 남긴다. 사용량 중단 뒤의 재개에도 같은 기록을 적용한다.
- 도구 사용 위반으로 중단한 실행은 재개하지 않는다. 형식에 따라 위반이 달라지면 살아남은 호출만 고르는 결과가 되기 때문이다.

## 판정 규칙

아래 표식 사이가 판정 규칙 구간이다. 이 구간의 해시가 첫 커밋과 같은지 스크립트로 확인할 수 있도록 표식을 둔다.

<!-- decision-rules:begin -->

판정은 권고이며 확정은 사용자가 한다.

1. 정답률이 가장 높은 형식과의 차이가 반복 간 불확실성 안에 있는 형식들을 후보로 둔다.
2. 그중 낮은 쪽 모델에서 더 정확한 형식을 고른다.
3. 그래도 같으면 토큰이 적은 형식을 고른다.

계산은 다음과 같이 고정한다.

- 승인한 수준은 본 실행의 마지막 호출 전에 마지막으로 승인한 수준이다. 수준을 바꿨다면 바꾼 시각과 사용량 값을 원자료에 남긴다.
- 칸은 승인한 수준이 계획한 (형식, 모델, 트리)다. 계획에 없는 칸과 반복의 호출은 넣지 않는다. 호출 정답률은 맞은 질문 수를 채점한 질문 수로 나눈 값이다. 칸의 평균과 표본 분산 s²을 그 칸의 반복별 호출 정답률로 구한다.
- 유효 호출은 채점한 호출이다. 파싱 실패 호출은 유효 호출이고 호출 정답률은 0이다. 시험 호출, 도구 위반 호출, API 실패 호출, 재시도 전의 실패 호출은 유효 호출이 아니며 어떤 계산에도 넣지 않는다.
- 형식 점수 A_f는 칸 평균의 단순 평균이다. 모델과 트리에 같은 가중을 준다.
- 형식 불확실성 U_f는 sqrt(Σ(s²/r)) / 칸 수다. r은 그 칸의 반복 수다.
- 규칙 1: A_best − A_f ≤ max(2 × sqrt(U_best² + U_f²), 1/Q_f)이면 후보다. A_best는 A_f의 최댓값이다. Q_f는 그 형식에서 채점한 전체 질문 수이고, 1/Q_f는 분산이 0일 때 쓰는 최소 분해능이다. 부동소수 비교의 허용 오차는 1e-12이고 경계는 후보에 넣는다.
- 규칙 2: 후보 중 낮은 쪽 Claude(`claude-haiku-5-5`)에서 맞힌 질문 수를 채점한 질문 수로 나눈 값이 가장 큰 형식을 고른다. 정수 분수로 정확히 비교한다. 채점 수가 같으면 맞힌 수 비교와 같다. 불확실성 폭을 다시 적용하지 않는다. 정확도가 낮은 형식이 토큰으로 이기는 일을 막기 위해서다.
- 규칙 3: 규칙 2의 값이 정확히 같은 형식만 남았을 때 낮은 쪽 Claude의 호출당 평균 토큰이 가장 적은 형식을 고른다. 호출의 토큰은 그 호출 `usage`의 `input_tokens` + `cache_creation_input_tokens` + `cache_read_input_tokens` + `output_tokens`다. 평균은 그 형식에서 낮은 쪽 Claude 칸의 모든 유효 호출로 구한다. 파싱 실패 호출을 포함한다. 합계와 호출 수의 정수 분수로 정확히 비교한다. 네 필드 중 하나라도 없는 호출이 있으면 규칙 3을 적용하지 않고 동률로 보고한다.
- 그래도 같으면 동률로 보고하고 임의로 고르지 않는다.
- 계획한 칸이 한 칸이라도 차지 않으면 권고를 내지 않고 그 사실을 보고한다. 칸이 찼다는 것은 계획한 모든 반복에 유효 호출이 있다는 뜻이다. 이때 형식을 일부만 골라 판정하지 않는다.
- 이 기준은 사전에 고정한 설명용 기준이다. 반복이 2~3회이므로 통계적 동등성의 증거로 해석하지 않는다. 결과 문서에 그렇게 적는다. 규칙 1의 계수를 1과 3으로 바꾼 후보 집합을 참고로 함께 내되 판정에 쓰지 않는다.

<!-- decision-rules:end -->

## 순서 확인

규칙이 측정보다 먼저라는 사실은 git 이력으로 확인한다.

- 이 문서의 첫 커밋이 원자료 첫 커밋의 조상이고, 커밋 시각이 원자료의 가장 이른 시작 시각보다 앞선다.
- 첫 커밋 뒤 이 문서의 모든 판본에서 판정 규칙 구간의 해시가 같다. 끝점만 비교하지 않고 커밋마다 읽어 바꿨다가 되돌린 변경도 찾는다.
- 원자료의 각 줄은 그 호출 때 적용한 판본의 `rules_commit`과 `rules_blob`을 가진다. 그 커밋의 이 문서 blob이 `rules_blob`과 같고, 그 커밋이 `main`의 조상이어야 한다.
- 판정 구간을 코드로 옮긴 `scripts/child-tree-format/summarize.py`와 `scripts/child-tree-format/score.py`의 이력도 `git log -p`로 읽는다. 원자료 첫 커밋 뒤에 판정과 채점에 닿는 변경이 있으면 그 판단은 결과를 본 뒤의 변경이다.

## 규칙 변경 기록

첫 커밋 뒤의 변경을 이 절에 적는다. 내용, 이유, 결과를 본 뒤인지, 전후 커밋을 함께 적는다. 판정 규칙 구간을 바꾼 변경은 없다.

| 변경 | 이유 | 결과를 본 뒤인가 | 전 커밋 → 후 커밋 |
|---|---|---|---|
| `q11` 좁히기의 동률 뿌리와 후보 없음 처리, 밀리초 내림, 시드 키 연결, 재시도 구분 필드 `attempt`, 원자료 파일 이름을 적었다 | 첫 커밋에서 구현 단계로 넘긴 세부다. 값은 트리마다 고정되어 형식 비교에 영향을 주지 않는다 | 아니다. 측정 호출 0회 | `6b6eb298f` → 이 기록을 더한 커밋 |
| 원자료 허용 필드에 `attempt_kind`, `phase`, `level`, `repeat`를 더했다 | 재시도·재개와 수준별 칸을 원자료만으로 판별하려면 필요하다. 사용자 작업 내용이나 경로를 담지 않는다 | 아니다. 측정 호출 0회 | `6b6eb298f` → 이 기록을 더한 커밋 |
| `terminal.state`의 `no live surface` 오류를 `exited`로 읽고, 경로를 읽을 수 없는 뿌리에 별도 가명을 주기로 했다 | 스냅샷을 잡아 보니 뿌리 2개가 조회되지 않고 저장 파일에 `cwd`도 없었다. `cwd`는 값 없음을 허용하지 않는다 | 아니다. 측정 호출 0회 | `6b6eb298f` → 이 기록을 더한 커밋 |
| 합성 트리는 스냅샷에 없던 `needs_input`, `stale`을 강제로 1개씩 넣는다. 깊이 강제는 필요 없었다(스냅샷 최대 깊이 3) | 위 "같은 모양" 절이 정한 강제 조건의 실행 결과다. 규칙 자체는 바뀌지 않았다 | 아니다. 측정 호출 0회 | 해당 없음(규칙 문장 변경 없음) |
| 규칙 1의 `A_best`를 가진 형식이 여럿일 때 구현은 `U_best`로 그중 가장 큰 값을 쓴다. 판정 구간은 그대로 두고 요약의 `decision`에 `best_tied`와 쓴 `u_best`를 낸다. 순서 확인에 `summarize.py`와 `score.py`의 이력 읽기를 더했다 | 규칙 1은 동률일 때 어느 U_best를 쓰는지 정하지 않았고, 값에 따라 후보 집합이 달라진다. 어느 형식을 최고로 부르느냐에 따라 후보가 달라지지 않게 하고, 그 선택을 결과에서 볼 수 있게 했다 | 아니다. 측정 호출 0회 | `714b4d3c8` → 이 기록을 더한 커밋 |

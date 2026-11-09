# child 트리 형식 측정

AI 에이전트가 child 트리를 어떤 텍스트 형식으로 받을 때 정확하고 싸게 읽는지 측정한다. 트리 출력의 기본 형식을 고를 때 쓸 근거 자료다. 측정 결과는 권고이며 기본 형식을 정하지 않는다. 확정은 사용자가 한다. 이 측정은 제품 동작을 바꾸지 않는다.

- [설계와 판정 규칙](rules.md): 형식, 트리, 질문, 프롬프트, 채점, 호출 명령, 규모 축소, 판정식

## 설계상 한계

아래는 설계에서 이미 정해지는 한계다. 측정 수치의 한계와 구분한다.

- 호출 수를 줄이려고 한 호출에 질문을 묶는다. 앞 답이 뒤 답에 영향을 줄 수 있다. 질문 순서를 섞어도 없어지지 않는다.
- 답을 JSON으로 받으므로 JSON 입력이 유리할 수 있다.
- CLI에 temperature 옵션이 없다. 반복 간 차이는 그 조건에서의 값이다.
- 칸마다 반복이 2~3회여서 불확실성 추정이 거칠다. 판정 규칙의 후보 기준을 통계적 동등성의 증거로 쓰지 않는다.
- 실제 트리는 한 시점의 스냅샷이다. 익명화로 긴 작업 이름의 효과가 사라지고, 합성 트리의 어휘는 실제 작업 이름보다 단순하다.
- 지연은 구독 부하와 동시에 실행 중인 다른 작업의 영향을 받는다.
- CLI마다 시스템 프롬프트 구성이 달라 모델 사이 토큰 절댓값은 비교하지 않는다.

## 입력의 한계

생성한 입력을 기준으로 정해진 사실이다. 값은 `data/manifest.json`에 있고 측정 수치의 한계와 구분한다.

- `real`은 2026-11-09의 한 스냅샷이고 노드 16개다. 스냅샷을 잡은 작업 자신과 동시에 돌던 작업이 트리에 들어 있다.
- `real`에는 `needs_input`과 `stale`이 없다. 합성 트리는 같은 모양 조건을 지키되 이 두 상태를 1개씩 강제한다(`synthetic_forced`). 깊이는 강제하지 않았다.
- `real`의 `q1`, `q2`, `q3`는 변별력이 낮다(`low_discrimination`). `q1`은 후보가 모두 silent이고, `q2`는 그 상태가 없으며, `q3`는 답이 빈 목록이다. 형식 비교에서 이 세 질문의 차이를 크게 읽지 않는다.
- `q1`의 N은 트리마다 다르다. `real`은 5분이고 합성 트리는 보고 시각 간격이 커서 수천 분이다.
- 측정의 `id`는 `n` 뒤 세 자리이고 실제 surface 번호와 형태가 다르다. 실제 출력에서 번호 형태가 읽기에 주는 영향은 이 측정으로 알 수 없다.
- 합성 트리의 값은 `real`에서 뽑은 분포로 만들었으므로 스냅샷 하나의 특성을 물려받는다.

## 재현

스크립트는 [`scripts/child-tree-format/`](../../../scripts/child-tree-format)에 있고 저장소 루트에서 실행한다. 입력 파일은 `data/` 아래에 추적한다.

| 목적 | 명령 | 종료 코드 |
|---|---|---|
| 자체 시험. 모델을 부르지 않는다 | `python3 -B scripts/child-tree-format/selftest.py` | 0 통과, 1 실패, 2 시험 없음 |
| 입력 재생성 대조 | `python3 -B scripts/child-tree-format/generate_inputs.py --check` | 0 같음, 1 다름, 2 판정 불가 |
| 호출 수 출력 | `python3 -B scripts/child-tree-format/generate_inputs.py --counts` | 0 |
| 판정 구간 해시와 모든 판본 대조 | `python3 -B scripts/child-tree-format/check_rules.py` | 0 같음, 1 바뀜, 2 판정 불가 |
| 원자료의 판본과 순서 대조 | `python3 -B scripts/child-tree-format/check_rules.py --raw <원자료 파일>` | 0 문제 없음, 1 문제 있음, 2 판정 불가 |
| 공개 패턴 검사 | `python3 -B scripts/child-tree-format/check_anonymity.py --public-only` | 0 위반 없음, 1 위반, 2 판정 불가 |
| 요약 생성과 대조 | `python3 -B scripts/child-tree-format/summarize.py`, 대조는 `--check`를 더한다 | 0 같음, 1 다름, 2 판정 불가 |

스냅샷 원본, 대응표, 금지어 파일은 커밋하지 않으므로 `capture_snapshot.py`와 `anonymize.py`의 재실행은 원본을 가진 컴퓨터에서만 된다. `generate_inputs.py`는 추적한 `data/trees/real.json`을 입력으로 삼으므로 어디서든 같은 바이트를 만든다.

실행 순서는 다음과 같다. 호출은 구독 사용량을 쓰므로 실행 담당의 승인을 받은 뒤에만 한다. `--data`는 하위 명령 앞에 둔다.

1. 시험: `runner.py run --phase pilot --claude-utilization-unit <fraction|percent>`
2. 창 값 측정: `measure_w.py --claude-dir <Claude 세션 기록 폴더> --codex-dir <Codex 세션 기록 폴더> --u claude_7d=<u> --u claude_5h=<u> --u codex_7d=<u> --u codex_5h=<u> --status claude_7d=<상태> > <창 값 JSON>`. 모델별 7일 창이 있으면 `--model-window <창 이름>=<측정 모델>`과 `--u <창 이름>=<u>`를 더한다.
3. 수준 제안: `usage_level.py --pilot data/raw/pilot.jsonl --windows <창 값 JSON>`
4. 승인 기록: `runner.py approve --level <수준> --windows <창 값 JSON> --approved-u '{"claude": <u>, "codex": <u>}'`
5. 본 실행: `runner.py run --phase main --level <수준> --claude-utilization-unit <fraction|percent>`. 끊기면 같은 명령이 원자료에서 이어서 실행한다.
6. 중단 뒤: `runner.py resume --phase main --level <수준> --usage <제안 JSON> [--approved-by <이름>]`. 재개와 거절은 조건 값과 함께 `data/raw/level-changes.jsonl`에 남는다. 종류는 `level_change`, `abort`, `resume`, `resume_refused`, `run_options`다.
7. 요약: `summarize.py`, 대조는 `check_rules.py --raw`.

W는 입력 + 캐시 생성 + 출력 토큰이고 캐시 읽기를 넣지 않는다. 옵션으로 바꿀 수 없다. 소규모 시험의 호출당 토큰 P도 같은 식으로 재므로 P/W를 그대로 비교할 수 있다. Claude 기록은 메시지 ID로, Codex 기록은 누적 토큰 수와 시각으로 중복을 지운다. 기록 폴더는 컴퓨터마다 다르므로 인자로 받고 추적 파일에 적지 않는다. 표준 출력에는 숫자와 창 이름만 나온다.

기록을 읽지 못한 창은 `w`가 비고, 규칙 표의 `W=0`과 같게 다룬다. 그래서 Codex 창을 읽지 못하면 기본 제안은 L2다. `w`는 읽었지만 `u`가 없는 창만 P/W를 보고, 2% 이하이면 L0, 넘으면 L2를 제안한다. 읽은 창은 그대로 표로 수준을 구하고, 미확인 창의 제안은 그 수준을 낮추기만 한다. 미확인 창이 있으면 `needs_decision`이 참이고 재개에 `--approved-by`가 필요하다.

러너는 호출마다 CLI를 실행하기 전에 `data/raw/<phase>-starts.jsonl`에 시작 줄을 남긴다. 강제 종료나 전원 차단으로 끝 줄이 없는 호출은 다음 `run`이 원자료에 `error_kind: interrupted` 줄로 남기고, 일반 API 실패처럼 재시도 상한과 실패 횟수에 센다. 사용량을 쓰고 기록이 사라진 호출을 공짜 재시도로 만들지 않기 위해서다. 채점에는 들어가지 않는다.

한 데이터 폴더에는 러너 명령 하나만 돈다. `data/raw/.runner.lock`을 잠그며, 다른 명령이 돌고 있으면 종료 코드 2를 낸다. 이 파일은 실행 중인 호출의 pid와 실행 파일을 적어 둔다. 러너가 죽어 호출만 살아 있으면 모든 명령이 그 pid를 알리고 종료 코드 2로 멈춘다. 호출을 죽이지 않으므로 끝나기를 기다리거나 직접 종료한 뒤 다시 실행한다. 정상 종료하면 파일이 지워지므로 커밋 대상이 아니다. 잠금과 pid 확인은 Linux의 `flock`과 `/proc`을 쓴다. 다른 OS에서는 살아 있는 호출을 찾지 못한다.

러너의 호출 제한 시간 기본값은 600초이고 `--timeout`으로 바꾼다. 규칙에 정한 값이 없어 실행 조건으로 기록한다.

본 실행은 `--max-calls`로 나누어 돌린다. Codex는 실행 중 사용량을 러너에 주지 않으므로, 각 지점에서 사람이 Codex의 `used_percent`를 읽어 다음 명령으로 남긴다.

```text
runner.py run --phase main --level <수준> --max-calls <지점까지 호출 수> --claude-utilization-unit <단위>
runner.py note-usage --cli codex --used-percent <읽은 값>
```

`note-usage`는 값을 `level-changes.jsonl`에 `usage_note`로 남기고 승인 u와 비교한다. 승인 u보다 5포인트 이상 높으면 종료 코드 3을 내고 다음 실행이 호출 없이 중단한다. `--approved-u`의 `codex`가 `null`이면 비교할 값이 없어 기록만 하고 그렇게 출력한다. 호출을 나누는 간격은 규칙에 없으므로 실행 담당이 정한다.

Claude 호출은 이벤트에서 7일 값을 읽어 자동으로 비교한다. 모델별 7일 창은 `--approved-u`에 그 창 이름으로 값을 주면 그 값과, 주지 않으면 `claude` 값과 비교한다. 본 실행의 채점된 Claude 호출에 7일 값이 없으면 `usage_unread`로 중단하고, 읽은 값이 승인 u의 10분의 1 미만이면 단위를 잘못 지정한 것으로 보고 `usage_unit_suspect`로 중단한다. 둘 다 사용량 중단이라 재개에 사람의 이름이 필요하다. 7일 값이 없는 상태로 계속하려면 `approve`에 `--allow-unread-usage`를 더해 승인한다.

### 사용량 확인의 한계

- Codex의 상승 검사는 사람이 남긴 지점에서만 일어난다. 지점 사이의 상승은 다음 지점까지 모른다.
- 단위 검사는 승인 u가 5 이상일 때만 한다. 그보다 작으면 잘못된 단위를 찾지 못한다.
- 실행 중에 주간 창이 초기화되면 읽은 값이 정상적으로 떨어져 `usage_unit_suspect`로 잘못 중단할 수 있다. 값을 확인해 다시 승인하고 재개한다.
- 모델별 창 이름은 CLI 이벤트의 이름과 같아야 그 값과 비교한다. 다르면 `claude` 값과 비교하므로 더 빨리 중단하는 쪽으로 틀린다.
- 모델별 창의 `u'`는 사람이 준 값이다. W가 그 창의 모델만 세는 값이므로 `measure_w.py --model-window`로 잰다.

### 소규모 시험에서 확인할 것

아래는 실제 CLI 출력을 보지 못하고 가짜 CLI로만 시험한 항목이다. 소규모 시험의 실제 값으로 확인한다.

- Claude 사용량 이벤트의 `utilization` 단위. 러너는 단위를 추측하지 않고 인자로 받는다.
- 도구 위반으로 칠 응답 항목 종류.
- Codex 사용량 창을 읽는 경로.
- 사용량 한도 오류의 실제 문구. `cli_events.py`의 문구 목록은 실제 오류 응답으로 대조하지 않았다.
- 모델별 7일 창이 이벤트에 나타나는 이름과 형태.

규칙 1의 `A_best`를 가진 형식이 여럿이면 `U_best`로 그중 가장 큰 값을 쓴다. 어느 형식을 최고로 부르느냐에 따라 후보 집합이 달라지지 않게 하려는 구현 선택이고, 판정 규칙 구간을 고치지 않는다. 이 선택은 [규칙 변경 기록](rules.md#규칙-변경-기록)에 있고, 요약의 `decision`과 `tables.txt`가 `best_tied`와 쓴 `u_best`를 보여 준다.

순서를 확인할 때는 [순서 확인](rules.md#순서-확인) 절대로 `rules.md`와 함께 판정을 옮긴 코드의 이력을 읽는다: `git log -p --reverse <규칙 첫 커밋>..origin/main -- docs/research/child-tree-format/rules.md scripts/child-tree-format/summarize.py scripts/child-tree-format/score.py`.

## 인용

아래 세 논문은 2026-11-09에 arXiv PDF의 본문을 직접 읽었다. 이 측정의 결론을 뒷받침하는 자료가 아니라 설계 참고 자료다. 어느 논문도 이 측정의 여섯 형식 전체를 비교하지 않고, 이 측정에 쓰는 모델을 시험하지 않았다.

### Fatemi, Halcrow, Perozzi. Talk like a Graph: Encoding Graphs for Large Language Models

[arXiv 2310.04560v1](https://arxiv.org/abs/2310.04560).

- 3.1절 표 1: PaLM 62B의 zero-shot 연결 노드 과제에서 간선을 나열한 adjacency 인코딩은 19.8%, 노드마다 이웃을 적는 incident 인코딩은 53.8%다. 논문의 abstract는 인코더 선택에 따른 향상 폭을 과제에 따라 4.8%~61.8%로 적는다.
- 부록 A.3 표 5: 대부분의 프롬프트 방식에서 incident 인코딩의 평균 순위가 가장 높다. zero-shot은 예외다.
- 4.2절 표 4: 같은 과제의 정확도가 그래프 구조에 따라 크게 달라진다.
- 이 측정에 준 영향: 인코딩, 과제, 그래프 구조가 모두 정확도를 바꾼다는 결과에 맞춰 형식별 결과를 질문 종류와 트리로 나누어 낸다.
- 다른 조건: 속성이 없는 무방향 그래프이고, 2023년의 PaLM 계열 모델이며, 호출마다 질문이 하나다. JSON, Mermaid, 표 형식은 비교하지 않았다. 3.5절의 끊어진 노드 과제는 간선이 없다는 추론을 묻는 과제이므로 이 측정의 없는 노드 함정 질문을 직접 뒷받침하지 않는다.

### Zhu, Huang, Jin, Jiao, Zhong, Chang, Lin, Han. Investigating Instruction Tuning Large Language Models on Graphs

[arXiv 2408.05457v1](https://arxiv.org/abs/2408.05457).

- 4.2.1절 표 4: 자연어, JSON, DOT을 비교한다. 본문은 그래프 지시 튜닝을 한 Llama-2, Mistral, Gemma 7B 모델에서 JSON이 가장 좋다고 결론짓는다. 같은 표에서 Mistral의 Maple 평균은 자연어 75.69, JSON 75.64로 자연어가 조금 높다.
- 같은 표의 튜닝 전 모델에서는 JSON 우위가 일관되지 않다. Amazon 평균에서 Llama-2-chat은 자연어 12.91, JSON 11.48이고 Gemma-Inst는 DOT 28.72, JSON 24.74다. Mistral-Inst는 JSON이 가장 높다(Amazon 자연어 13.84, JSON 14.75, DOT 14.02. Maple 자연어 15.04, JSON 15.77, DOT 15.74).
- 이 측정에 준 영향: 이 측정의 모델은 그래프 지시 튜닝을 하지 않았으므로 JSON이 유리하다고 가정하지 않는다.
- 다른 조건: LoRA 미세 조정 모델을 주로 평가하고, 노드 유형과 간선 유형이 있는 이종 그래프를 쓰며, JSON은 간선 유형을 키로 쓰는 인접 형태다. 평평한 배열과 중첩 트리를 구분하지 않고 Mermaid와 표 형식은 없다.

### Yu, Bornea, Sil, Kirchhoff. Graph Serializations for Reasoning LLMs: A Study Across Knowledge Extraction and Structural Reasoning

[arXiv 2610.22734v1](https://arxiv.org/abs/2610.22734).

- 3절: 모델과 과제마다 지시문, 그래프 사실, 질문, 시드, 노드 식별자, 노드와 간선 순서, 문구를 같게 두고 직렬화만 바꾼다. 정답은 직렬화 전 원본 그래프에서 계산한다. 이 측정의 형식 비교 통제와 같은 방식이다.
- 4절 표 2와 5절: 15개 형식을 9개 모델 구성에서 비교하며, 구조 추론에서는 형식 선택이 정확도를 크게 바꾸고 모든 설정에 맞는 기본 형식은 없다고 결론짓는다. 최선의 형식은 모델, 질의, 과제에 따라 달라진다.
- 4절: 추론 수준이 낮아질수록 형식 민감도가 커진다고 보고한다.
- 부록 C: gpt-5.4와 gpt-5.4-mini를 temperature 0으로 세 번 반복했을 때 형식별 반복 간 표준편차 평균은 1.5퍼센트포인트이고, 형식 사이 정확도 폭은 평균 7.7배 크다.
- 이 측정에 준 영향: 높은 쪽과 낮은 쪽 모델을 함께 재고 결과를 모델과 질문 종류로 나누어 낸다. 낮은 쪽 모델을 우선하는 판정 규칙 2는 이 논문의 결론이 아니라 이 측정의 선택이다. 이 측정의 권고도 다른 모델과 과제로 일반화하지 않는다.
- 다른 조건: 일반 그래프와 지식 그래프 조각을 쓰고 호출마다 질문이 하나다. 부록 C의 반복 안정성은 temperature 0에서 잰 값이어서 temperature를 지정하지 못하는 이 측정의 불확실성으로 가져오지 않는다. Mermaid, adjacency, 표, JSON-LD는 있지만 이 측정의 평평한 JSON 배열과 중첩 JSON과 같은 형식은 아니다.

# ADR-0119: 세마포어 한도는 원자적으로 조정하고, 홀더 만료는 opt-in 으로 둔다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: agent-collaboration, semaphore, lease, concurrency, operability
- **Group**: agent-integration

## Context

`agent.*` 협업 primitive 의 세마포어(`crates/tasty-agent/src/semaphore.rs`)에는 두 가지 공백이 있었고, 둘 다 실제로 사람을 막았다.

**실측 사고.** 여러 lane 이 한 tasty 인스턴스를 공유하며 GUI 검증을 직렬화하려고 permits 1 짜리 세마포어를 쓰고 있었다. permit 을 잡은 에이전트가 모델 사용 한도에 걸려 응답 불능이 됐고 `release` 를 보내지 못했다. 그 결과:

1. **대기자 3명이 30분 넘게 막혔다.** `holders` 는 문자열 id 배열이라 누가 언제부터 잡고 있는지 알 수 없었다 — "이 홀더가 죽었는가" 를 판정할 근거가 세마포어 쪽에 전혀 없었다. 제3자가 `semaphore-release <holder>` 로 대신 회수할 수는 있지만, 그 판단을 내릴 정보가 없다.
2. **한도를 올려 우회할 수도 없었다.** 이미 있는 이름으로 `semaphore-create` 를 부르면 `already exists (permits_total=1)` 로 거절된다. 실제 복구는 `semaphore-delete` → `semaphore-create --permits 2` → 살아 있는 홀더를 대신해 `semaphore-acquire` 를 다시 부르는 3단계였다. **그 사이 세마포어가 존재하지 않는 순간이 있다** — 그 틈에 acquire 를 시도한 에이전트는 아무 제약 없이 임계구역에 들어간다. 동시성 상한을 지키려고 만든 장치가 상한을 지키지 못하는 창이 복구 절차 자체에 들어 있었다.

같은 계열인 lease 에는 만료(`expires_at`, lazy evict)가 있는데 세마포어에는 없다. 이 비대칭 자체가 설계 결함이다 — 이 primitive 의 용도가 "여러 AI 에이전트가 같은 인스턴스를 공유" 하는 것이고, 에이전트는 사람보다 조용히 멈추는 빈도가 높다(사용 한도, 컨텍스트 소진, 세션 종료). 홀더가 죽는 것은 예외가 아니라 정상 시나리오다.

기존에도 회수 장치가 하나 있긴 했다 — `purge_stale_{semaphore,lease}_holders` 는 호스트 재시작 시 `metadata.*.holder == task.id` 인 Running task 의 permit 을 회수한다. 그러나 이건 **runner 가 소유한 task 홀더**만, **재시작 시점에만** 다룬다. 위 사고의 홀더는 CLI 로 직접 acquire 한 외부 에이전트였고 호스트는 계속 살아 있었다.

## Decision

두 공백을 각각 고친다. 둘은 독립 문제이므로 하나만 고치면 사고가 절반만 해결된다 — 만료만 넣으면 한도 조정에 여전히 delete→create 틈이 남고, 리사이즈만 넣으면 죽은 홀더의 permit 이 영구히 묶인 채 총량만 계속 커진다.

**① `semaphore-set-permits` 를 1급 명령으로 신설한다** (IPC `agent.semaphore_set_permits` + CLI `tasty agent semaphore-set-permits`). 한도를 제자리에서 바꾸므로 delete→create 우회의 "세마포어가 없는 순간"이 사라진다. `semaphore-create` 는 그대로 엄격하게 둔다 — 재실행이 조용히 한도를 바꾸는 것보다 명령이 갈라지는 편이 낫다.

**축소는 drain 이다.** `permits_total` 을 홀더 수보다 낮춰도 기존 홀더를 강제 회수하지 않는다. 초과 상태(`holders.len() > permits_total`)를 그대로 두고 새 acquire 만 거절해, 홀더 수가 새 한도 아래로 내려갈 때 자연히 수렴한다. `permits_available` 은 음수로 내려가지 않고 0 이다(`is_over_subscribed()` 로 구분한다).

**② 홀더에 `acquired_at` 과 `expires_at` 을 싣고, 만료는 lease 와 같은 메커니즘을 쓴다.** `holders: Vec<String>` → `Vec<SemaphoreHolder { id, acquired_at, expires_at }>`. `acquire` 에 `ttl_ms` 를 주면 `expires_at = now + ttl_ms`, 만료된 홀더는 다음 `acquire` 또는 `list(Some(now))` 에서 lazy evict, 같은 holder 의 재acquire 가 갱신(heartbeat). 필드명·의미·evict 시점을 lease 와 일부러 일치시켰다 — 두 primitive 가 각자 다른 만료 개념을 갖는 것이 다음 결함이 된다.

**만료의 기본값은 "만료 없음"이다** (`ttl_ms: None`). 자동 회수는 호출자가 명시적으로 켜는 opt-in 이다.

`acquired_at` 이 `Option` 인 이유는 구 형식으로 이미 영속된 홀더의 획득 시각이 **정말로 미상**이기 때문이다. 이를 0 으로 적으면 "56년째 점유 중" 이라는 거짓말이 되어, 회수 판단을 도우려고 넣은 필드가 오판을 만든다. 모르는 것은 모른다고 적는다.

## Consequences

- **얻은 것**: 운용 중 한도 조정에 틈이 없다. `semaphore-list` 만으로 "누가 언제부터 잡고 있는지" 를 판단할 수 있다(사고 당시 이 판단이 불가능했다). 자동 회수가 필요한 용도는 `ttl_ms` 로 얻는다. 세마포어와 lease 의 만료 어휘가 하나다.
- **잃은 것**: `holders` 의 JSON 형태가 문자열 배열에서 객체 배열로 바뀐다 — 응답을 파싱하던 외부 소비자에게는 breaking change 다. 구 형식으로 **영속된** 레코드는 계속 읽는다(`SemaphoreHolder` 의 커스텀 `Deserialize` 가 문자열도 받는다) — 그게 없으면 실행 중 인스턴스의 세마포어가 다음 부팅에 통째로 사라진다.
- **운영 비용 / 유지 부담**: `permits_available` 이 파생값이 되어 읽기·쓰기마다 재계산된다(홀더 목록과 어긋날 수 없다). 만료를 안 쓰는 호출자에게는 동작 변화가 없으므로, 교착 복구는 여전히 사람이 `semaphore-release` 또는 `set-permits` 로 개입해야 한다 — 그 개입에 필요한 정보를 준 것이 이 결정의 몫이다.

## Alternatives Considered

- **A: `acquired_at` 만 싣고 회수는 호출자 정책에 위임 (만료 없음)** — advisory 철학에는 가장 충실하지만, 회수가 여전히 100% 수동이다. 조용히 죽는 홀더가 정상 시나리오인 이상 "필요하면 켤 수 있는" 자동 회수는 있어야 한다. 기각.
- **B: 만료를 기본으로 켠다 (전역 기본 TTL)** — **명시적으로 기각한다.** 이 질문은 반드시 다시 나온다(이 사고 자체가 그 동기다). 기본을 만료로 두면 교착은 사라지지만 더 나쁜 문제가 들어온다: 오래 걸리는 **정당한** 작업(release 빌드 + 캡처는 20분도 걸린다)의 permit 이 도중에 만료되고, 그 사이 다른 홀더가 들어와 두 프로세스가 같은 자원(GPU 등)을 동시에 잡는다. 홀더는 자기 permit 이 회수됐다는 사실을 알 방법이 없으므로 이 상태는 정상 동작과 구별되지 않는다. **조용한 데이터 손상이 교착보다 나쁘다** — 교착은 눈에 보이고 사람이 개입할 수 있지만, 이중 점유는 결과가 틀어진 뒤에야 드러난다. 그래서 회수를 원하는 쪽이 명시적으로 켠다.
- **C: 축소 시 초과 홀더를 강제 회수** — 한도를 줄이는 행위가 **이미 임계구역에 들어가 있는 작업을 중단시키는** 결과가 된다. 홀더에게 통지 채널이 없어 (B) 와 같은 형태의 이중 점유가 되므로 기각. drain 을 택했다.
- **D: 축소를 홀더 수 아래로는 거부** — 안전하지만 "점점 줄여서 1로 수렴시킨다" 는 정당한 운용 의도를 표현할 수 없다. 거부하면 운영자는 다시 delete→create 우회로 돌아간다. 기각.
- **F: runner 가 실행 중인 task 의 permit 을 poll 마다 renew** — task DAG runner 는 dispatch 시점에만 permit 을 잡고 `Running` task 를 재dispatch 하지 않아, `metadata.semaphore.ttl_ms` 를 실제 소요보다 짧게 걸면 permit 이 실행 도중 만료돼 대기 task 가 같은 슬롯을 취득한다(이중 dispatch). renew 를 넣으면 그 창은 닫히지만 **정반대 위험과 맞바꾸는 것**이다: 응답 없이 멈춘 task(hung)의 permit 이 poll 마다 갱신되며 슬롯을 영원히 점유한다 — TTL 이 애초에 막으려던 바로 그 상태다. 어느 쪽이 나은지 지금 근거가 없어(둘 다 실물 사례가 없다) 위험을 옮기지 않고 **계약을 날카롭게 적는 쪽**을 택했다: `ttl_ms` 는 그 task 의 최대 소요보다 길어야 한다([`docs/dev-guide/agent-runner.md`](../dev-guide/agent-runner.md) "dispatch 게이트"). 기각.
- **E: 현상 유지 + "홀더 생사는 호출자가 관리한다" 를 문서화** — 이미 물린 사례가 있고, 문서만으로는 판단에 필요한 정보(획득 시각)가 생기지 않는다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `ttl_ms` 없이 잡는 용도가 다수가 되고 교착 복구를 위한 수동 개입이 반복된다. 이때도 **먼저 검토할 것은 용도별 기본값**(자원 직렬화 계열은 무한, 그 외는 만료)이지 전역 기본 전환이 아니다 — 전역 전환은 위 대안 B 의 이중 점유를 그대로 들여온다.
- 축소 drain 상태(`is_over_subscribed`)가 장기간 해소되지 않는 사례가 나온다 — 그때는 "초과 홀더에게 반납을 요청하는 통지 채널" 이 강제 회수보다 먼저 검토 대상이다.
- 세마포어에 blocking/queue/fairness 가 도입된다. 대기 큐가 생기면 만료·drain 이 대기자 순서와 어떻게 상호작용하는지 다시 정의해야 한다.
- lease 의 만료 모델이 바뀐다. 두 primitive 의 만료 어휘를 일치시킨 것이 이 결정의 전제다.
- task-level `ttl_ms` 를 소요보다 짧게 걸어 생긴 이중 dispatch 가 **실제로** 발생한다 — 그때 대안 F(runner renew)를 다시 본다. 다만 F 는 hung task 의 영구 점유와 맞바꾸는 것이라, 재검토의 첫 질문은 "renew 를 켤 것인가" 가 아니라 **"둘 중 어느 쪽이 이 워크로드에서 더 자주 나는가"** 다. 한쪽 실물 사례만으로 반대쪽을 들이면 위험을 옮기기만 한다.

## References

- `docs/dev-guide/agent-runner.md` — primitive 6종, dispatch 게이트, 재시작 정화
- `docs/features/agent-collaboration/index.md` — 기획·인터페이스
- `crates/tasty-agent/src/lease.rs` — 재사용한 만료 모델의 원형
- `crates/tasty-agent/src/semaphore.rs` — 구현과 회귀 테스트(사고 재현 포함)

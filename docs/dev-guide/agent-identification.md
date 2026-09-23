# Agent 식별 (`AgentId`)

`AgentId`(`crates/tasty-telemetry/src/agent_id.rs`)는 메트릭/cap/anomaly/rate-limit 이 *agent 차원* 으로 집계되기 위한 식별자다. session token 을 동반한 호출은 호스트가 발급 때 기록한 `agent_id` 로 검증되고, 토큰 없는 Local 호출의 env 값은 위조 가능하다(아래 한계).

## 도출 규칙

| Caller | agent_id |
|--------|----------|
| Agent (session token 동반) | 호스트가 `session.issue` 때 기록한 `agent_id`(토큰 검증 통과) |
| Plugin process | 매니페스트 `plugin_id`(매니페스트 등록으로 인증됨) |
| Local CLI/사용자 | env `TASTY_AGENT_ID`, 없으면 sentinel `_host` |

```rust
caller.agent_id()                  // CallerContext (crates/tasty-ipc/src/caller.rs)
tasty_telemetry::AgentId::from_env()  // 라이브러리 크레이트가 자기 caller 식별
```

- `AgentId::HOST` = `"_host"` (빈 문자열도 HOST 로 대체), env key = `AgentId::ENV_KEY` (`"TASTY_AGENT_ID"`).

## child agent env 주입

`claude.spawn`/`launch` 로 띄운 child 는 PTY shell 위에서 inline env prefix 로 실행된다:

```
$ TASTY_SURFACE_ID=<surface_id> TASTY_AGENT_ID=claude_s<surface_id> TASTY_SESSION_TOKEN=<hex> claude
```

- `claude_s<surface_id>` 는 surface 와 1:1 — 호스트가 surface_id 로 역추적 가능.
- inline prefix 라 export 와 달리 history/profile 오염 없음(cd echo 와 같은 정책).

## 호환 표

| 위치 | sentinel / 형식 |
|------|-----------------|
| `AgentId::HOST` | `"_host"` |
| `tasty_memory::HOST_OWNER` | `"_host"` (memory.db `owner` 컬럼) |
| `CallerContext::owner()` | Local → `_host`, Plugin → `plugin_id`, Agent → `agent_id` |
| `CallerContext::agent_id()` | Local → env 또는 `_host`, Plugin → `plugin_id`, Agent → `agent_id` |

`agent_id()` 는 Local 분기에서 env 를 본다는 점이 `owner()` 와 다르다 — memory `owner` 는 plugin 간 데이터 격리용이라 `_host` 로 일괄 묶고, telemetry `agent_id` 는 child agent 까지 분리해야 해 env 를 추가로 본다.

## 보안 한계

토큰 없는 Local 호출의 env `TASTY_AGENT_ID` 는 **위조 가능**하다 — 적대적 agent 가 다른 id 를 사칭해 cap/budget 우회 가능. 이 모델은 **악의보다 버그** 영역으로 처리한다 — *정직한 agent 의 폭주를 막는 안전망*. 적대적 agent 방어는 OS 권한·plugin 매니페스트가 우선.

검증 가능한 신원: claude plugin 은 child 기동 때 `issue_session_token`(`crates/tasty-plugin-claude/src/handlers.rs`)으로 `session.issue` 토큰을 받아 `TASTY_SESSION_TOKEN` 으로 싣고, CLI 가 그 env 를 envelope 에 실으면 `resolve_caller_from_envelope`(`crates/tasty-ipc/src/caller.rs`)가 `CallerContext::Agent` 로 해석한다. 토큰이 잘못됐거나 만료면 Local 로 떨어지지 않고 거부된다.

## 테스트

`tasty_telemetry::agent_id::tests` 의 `from_env_all_cases` 하나가 세 경우(미설정 → host / 값 있음 → 그 값 / 빈 값 → host)를 함께 단정한다.

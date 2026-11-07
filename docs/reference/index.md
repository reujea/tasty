# 레퍼런스 (Reference)

IPC·이벤트·출력 파서의 이름과 데이터 형식을 찾는 참고 자료다. 기능의 목적과 동작은 [features/](../features/index.md)에서 설명한다.
CLI를 처음 쓰는 순서와 명령별 사용법은 공개 사용자 가이드의 [CLI 입문](https://tasty.zilhak.org/guide/agents/cli/)과 [CLI 명령 참조](https://tasty.zilhak.org/guide/agents/cli-reference/)가 다루며, 이 폴더는 그 내용을 복제하지 않는다.

| 문서 | 내용 | 구현 기준 |
|------|------|----------|
| [api.md](api.md) | 주요 IPC/CLI — 네임스페이스별 메서드·권한·시스템/윈도우 관측 범위·journal 캐시 진단·GUI/headless 설정·GUI 최근 목록의 미저장 변경 나이 | `crates/tasty-ipc/src/method_meta.rs` |
| [event-catalog.md](event-catalog.md) | Event Bus 1.0 wire 계약 (plugin 공개 API) | `tasty_plugin_protocol::events` |
| [output-parsers.md](output-parsers.md) | 터미널 출력 파서 카탈로그 | `tasty-output` |
| [environments.md](environments.md) | OS별 경로·에이전트 실행 준비 | — |
| [plan.schema.json](plan.schema.json) | 공유 컨텍스트 Plan 의 JSON Schema (memory 키 `tasty.plan.<id>`) | `crates/tasty-memory/src/plan.rs` |

> 메서드 시그니처·권한의 *정답*은 항상 코드(`method_meta.rs` / 각 핸들러)다. 이 문서들은 사람이 읽기 위한 요약이며, 충돌 시 코드를 따른다.

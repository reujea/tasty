# IPC 서버

IPC 서버가 요청을 **받아들이고 처리하는 쪽**의 규칙을 모으는 문서다 — 입장 상한, dispatch 회차 예산, 기한, wake, 요청 압력 게이지. 요청이 모듈 경계를 어떻게 흐르는지는 [data-flows](data-flows.md) §3, 요청·응답의 계약(오류 의미 · 멱등 · 협상)은 [api-conventions](../dev-guide/api-conventions.md), caller 별 관측은 [telemetry](../features/telemetry/index.md) 가 다룬다. 압력 게이지는 caller 별 관측과 다른 축이라 여기 둔다.

## 입장 상한

## dispatch 회차 예산

## 기한

## wake

## 요청 압력 게이지

## 관련

- [data-flows](data-flows.md) — IPC 요청 → 처리 → 응답의 흐름
- [api-conventions](../dev-guide/api-conventions.md) — IPC 계약
- [telemetry](../features/telemetry/index.md) — caller 별 관측

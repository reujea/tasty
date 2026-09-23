# Plugin 런타임 계약

호스트가 plugin 프로세스에게 주는 **런타임 계약**을 모으는 문서다 — 수명주기(기동 · 연결 대기 · 종료 · 재시작), namespace(해소 · 예약 · 충돌 · 만료), 채널(세 방향의 포화 · 바이트 상한 · 버린 수 통지 · 요청 번호). 호스트↔plugin 경계의 다른 축은 각자 문서가 있다: 권한은 [plugin-permissions](plugin-permissions.md), 렌더는 [egui-mesh-channel](egui-mesh-channel.md) · [webview](../design/systems/webview.md), 번들·배포·버전은 [plugin-packaging](plugin-packaging.md). plugin 을 **만드는 쪽**의 안내는 [plugin-development](plugin-development.md) 이고, 개별 번들 plugin 이 제공하는 동작은 [`docs/plugins/`](../plugins/index.md) 다.

## 수명주기

### 기동과 연결 대기

### 종료

### 재시작

## namespace

### 해소

### 예약

### 충돌

### 만료

## 채널

### 포화

### 바이트 상한

### 버린 수 통지

### 요청 번호

## 관련

- [plugin-development](plugin-development.md) — plugin 작성자 가이드
- [plugin-permissions](plugin-permissions.md) — 권한
- [plugin-packaging](plugin-packaging.md) — 번들 · 배포 · 버전

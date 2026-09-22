# ADR-0021: 자체 업데이트 확인 기능(update-check) 전면 제거

- **Status**: Accepted
- **Date**: 2026-06-25
- **Tags**: update, auto-update, scope, distribution, removal, maintenance, cli, plugin
- **Group**: build-release

## Context

tasty 는 초기에 자체 업데이트 확인 기능을 갖고 있었다. 구성 요소:

- `tasty-update` 크레이트 — 릴리스 메타 조회 / 다운로드 / 설치 (`download.rs` 497줄, `install.rs` 173줄, `lib.rs` 342줄).
- host 측 — update-check poller, "새 버전" 알림(notification), update popup GUI, Settings 의 Updates 탭, `update_check` state.
- CLI — `tasty update` 서브커맨드(199줄).
- 갤러리 — update popup specimen, tools_menu "Check for updates" 항목, command_palette update 명령.
- i18n — `[update]` / `[update.network]` / `[update.notify]` / `settings.tab.updates` / `[settings.updates]` 키 (3개 lang).
- docs — `features/auto-update/`.

이 기능은 사실상 가치가 낮았다. ① tasty 의 실제 배포는 OS 패키지 채널(DMG / MSIX / AppImage)과 CI 릴리스 파이프라인을 통하며, 인앱 다운로드/설치 경로는 각 플랫폼의 패키지·서명 정책과 충돌하거나 우회한다. ② update-check poller·알림·popup 은 "사용자 행동 ↔ 에이전트 행동 분리" 원칙상 host 상태를 건드리는 부수 표면을 늘린다. ③ 한 기능이 크레이트·host·CLI·갤러리·i18n·docs 6개 층에 흩어져 유지 비용이 컸다. 기능 가치가 그 유지 비용·표면적을 정당화하지 못했다.

## Decision

자체 업데이트 확인 기능을 **전면 제거**한다. `tasty-update` 크레이트, host 측 poller/알림/popup/Settings 탭/state, `tasty update` CLI 서브커맨드, 갤러리 specimen·메뉴 항목, 관련 i18n 키, `features/auto-update/` 문서를 모두 삭제한다. 업데이트 배포는 OS 패키지 채널과 CI 릴리스 파이프라인에 일임한다 — tasty 본체는 자기 버전을 확인·다운로드·설치하지 않는다.

## Consequences

- **얻은 것**: 크레이트 1개(28→27) 제거, 코드 약 -1,519줄(host -739, crate+CLI -1,351 중 코드분, i18n/docs 정리 별도). update poller·알림이 host 상태에 닿던 부수 표면 제거 → 사용자/에이전트 분리 원칙에 더 부합. 유지 대상 6개 층 소멸.
- **잃은 것**: 인앱 "새 버전 알림" UX. 사용자는 OS 패키지 매니저 / 릴리스 페이지를 통해 업데이트를 인지·적용한다.
- **운영 비용 / 유지 부담**: 배포 책임이 CI 릴리스 파이프라인 + OS 패키지 채널로 단일화 → tasty 본체는 버전 비교 로직을 갖지 않는다. 재도입 시 아래 트리거 참조.

## Alternatives Considered

- **A. 기능 유지·개선**: poller 주기·알림 UX 를 다듬어 존속 — 6개 층 유지 비용과 OS 패키지 정책 충돌이 그대로 남는다. 가치 대비 부담이 커서 기각.
- **B. plugin 으로 외부화**: update-check 를 선택적 plugin 으로 분리 — 현재 plugin 생태계(로컬 path install, ADR-0010 marketplace 보류)에서 배포 채널이 없어 실효가 없다. 코어 표면만 줄이고 죽은 plugin 을 남기게 되어 기각.
- **C. CLI 만 남기고 GUI 제거**: `tasty update` CLI 만 존속 — 인앱 GUI 부수 표면은 줄지만 크레이트·다운로드·설치 로직(가장 큰 유지 부담)이 그대로다. 부분 제거의 이득이 작아 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- OS 패키지 채널 밖에서 배포되는 경로(예: portable 단독 바이너리 직접 배포)가 1급 지원 대상이 되어, 인앱 업데이트가 유일한 갱신 수단이 될 때.
- plugin 생태계에 배포 채널(marketplace / 자동 upgrade, ADR-0010)이 생겨 update-check 를 코어 부담 없이 plugin 으로 제공할 수 있을 때.
- 사용자 다수가 OS 패키지 매니저를 거치지 않아 구버전 잔존이 실제 운영 문제로 측정될 때.

## References

- 제거 커밋: `1aea06e8`(host GUI/poller/notifications/state) · `89b440f7`+`4d7d4c7d`+`7471f6be`(갤러리) · `122aa26d`(CLI+크레이트) · `76ceab06`(i18n+docs).
- [ADR-0010](0010-plugin-marketplace-deferred.md) — plugin marketplace 보류 (대안 B 의 전제).
- [dev-guide/release.md](../dev-guide/release.md) — 릴리스 워크플로(배포 책임 일임 대상).

# ADR-0245: image 의 편집은 임시다 — 미저장 편집은 복원하지도, 알리지도 않는다

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: image, plugin, persistence, restore, snapshot, identity, non-goal, adr-0030
- **Group**: bundled-plugins

## Context

image surface 의 **파일 경로 round-trip 은 닫혀 있다.** preset(TOML)·layout(JSON) 양쪽이
surface snapshot 에 `file` 을 보존하고 restore 가 그 경로를 다시 로드한다.

그러나 **저장하지 않은 편집은 그 경로에 실리지 않는다.** `ImageDoc::paste_image` 는
메모리 버퍼만 바꾸고, 디스크로 가는 것은 명시적으로 부른 `ImageDoc::save_png` 뿐이다.
그리고 `ImageDoc` 에는 **document-level 의 "미저장 편집이 있다" 상태가 없다** —
`ImageDoc::draw_texture_dirty` 는 GPU 텍스처를 다시 올릴지 정하는 렌더 트리거지 문서
상태가 아니다. 그래서 복원은 `ImageDoc::reload_from_disk` 로 원본을 다시 읽고, 붙여넣거나
그린 것은 사라진다.

경로 층이 닫히기 **전에는** 복원된 image surface 가 빈 채로 와서 손실이 눈에 띄었다.
경로 층이 닫힌 **뒤에는** 화면이 맞아 보이는 채로 미저장분만 없다 — 손실이 조용해졌다.
그 변화가 "이것을 결함으로 볼 것인가" 라는 물음을 열었고, 이 ADR 이 그 물음을 닫는다.

## Decision

**image surface 의 정체성은 그림을 열어 보고 그 위에 임시로 그리는 것이다.** 변경을
파일로 저장하는 것은 그 위에 얹은 부가 기능이지 이 surface 가 존재하는 이유가 아니다.

그러므로 **저장하지 않은 편집은 복원 대상이 아니고, 복원 시점에 그 사실을 사용자에게
알리지도 않는다.** 이를 위한 document-level dirty 상태를 `ImageDoc` 에 신설하지 않는다.

범위는 **image surface 에 한정한다.** 다른 surface 종류의 미저장 상태 정책은 이 ADR 이
정하지 않는다 — 그것들은 정체성이 다르다.

## Consequences

- **얻은 것**: `ImageDoc` 에 문서 상태가 하나도 안 늘어난다. snapshot 이 경로 하나로
  유지되어 preset/layout 이 비대해지지 않는다. 임시파일 수명 관리가 생기지 않는다.
  그리고 **이 물음이 다시 열리지 않을 좌표가 생긴다** — 이 결정은 이전부터 서 있었으나
  어디에도 적혀 있지 않아, 재측정 비용을 한 번 치르고서야 여기 도달했다.
- **잃은 것**: 붙여넣거나 그린 뒤 저장하지 않고 복원하면 그 편집은 **조용히** 사라진다.
  화면이 맞아 보이므로 사용자가 손실을 알아챌 단서가 없다. 알면서 받아들인다 — 임시로
  그리는 자리에서 저장하지 않은 것은 임시다.
- **운영 비용 / 유지 부담**: 없다. 코드 변경이 없는 결정이다.

## Alternatives Considered

- **A: dirty 플래그를 싣고 복원 후 "미저장 편집이 있었다" 고 알린다** — 복원은 안 하면서
  문서 상태만 하나 늘린다. 그리고 알림 자체가 "이 자리의 편집은 임시다" 라는 정체성과
  **반대 신호**를 준다 — 알릴 만한 것이면 지킬 만한 것이라고 읽힌다.
- **B: 미저장분을 임시파일로 저장하고 그 경로를 snapshot 에 싣는다** — 실제로 복원되지만
  임시파일의 수명·정리·충돌 관리가 새로 생기고, 임시 그림판이 사실상 자동저장 에디터가
  된다. 정체성을 바꾸는 선택이다.
- **C: 픽셀을 snapshot 에 직접 싣는다** — preset/layout 파일이 이미지 크기만큼 비대해진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `ImageDoc` 에 document-level dirty 상태가 생긴다. 이 결정은 "그런 상태가 없다" 를
  근거의 일부로 썼고, 생기는 순간 알림 축이 추가 비용 없이 가능해진다.
- `docs/plugins/image/index.md` 의 목적 절이 image surface 를 **파일을 편집하는 주 도구**로
  선언한다. 그러면 "임시" 라는 전제가 문서 안에서 깨진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 사용자가 미저장 편집 손실을 실제 사고로 겪었다고 보고한다. 재는 법: 그 보고를 티켓으로
  열고, 잃은 것이 임시 낙서인지 오래 쌓은 작업인지를 그 자리에서 사람이 가른다 — 후자가
  나오면 이 ADR 이 전제한 "임시" 가 실제 사용과 다르다는 뜻이다.

## References

- [ADR-0030](0030-image-egui-mesh-bitmap-texture.md) — image surface 의 렌더 채널 결정
- [docs/plugins/image/index.md](../plugins/image/index.md) — 이 결정의 현재 운영 상태
- 코드 근거 (**결정이 실현된 현재 위치**): `crates/tasty-plugin-image/src/doc.rs` 의
  `ImageDoc::paste_image` · `ImageDoc::save_png` · `ImageDoc::reload_from_disk` ·
  `ImageDoc::draw_texture_dirty`

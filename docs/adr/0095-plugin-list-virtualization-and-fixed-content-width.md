# ADR-0095: plugin 리스트는 `show_rows` 로 virtualize 하고, 가로 폭은 한 번 재서 고정한다

- **Status**: Accepted
- **Date**: 2026-09-02
- **Tags**: plugin, egui-mesh, git-viewer, scroll, virtualization, performance, layout
- **Group**: plugin-render

## Context

egui 의 `ScrollArea::show()` 는 자식 클로저를 **전부** 실행한다 — 화면 밖 행을 건너뛰는 것은
호출자 책임이다. tessellator 가 clip 밖 glyph 의 vertex 생성을 건너뛰므로 mesh 바이트는 보이는
만큼이지만, 그 앞단의 galley 조회 · Shape 생성 · Vec 할당 · 레이아웃 계산은 전 행에서 발생한다.

egui-mesh plugin popup 은 host 가 `popup.set_context` 를 보낼 때마다 egui pass 를 통째로 다시
돈다. 스크롤 중에는 휠 이벤트마다, 그리고 egui 스크롤 스무딩의 self-repaint 마다 set_context 가
오므로 이 비용이 "한 번 무거운" 것이 아니라 **스크롤이 진행되는 동안 매 프레임 무겁다**.

git-viewer popup 의 리스트 4개(worktree rail / Changes / Commits / Diff)가 모두 `show()` 였다.
커밋 목록은 조회 상한이 200 행 고정이고 diff 는 파일 전체 라인이라, 뷰포트에 20~30 행만 보이는
상황에서도 매 프레임 전량이 레이아웃됐다.

Diff pane 만 추가 제약이 있다. `ScrollArea::both` 의 **가로** 스크롤 폭이 "모든 라인을
`layout_no_wrap` 해서 얻은 최장 폭" 으로 결정되고 있었다. 세로 virtualization 을 넣으면 보이지
않는 라인의 폭을 알 수 없어, 스크롤 위치에 따라 콘텐츠 폭이 출렁이고 가로 스크롤이 최장 라인
끝에 도달하지 못한다.

## Decision

**plugin 이 그리는 행 리스트는 `ScrollArea::show_rows` 로 virtualize 한다.** 네 리스트 모두 행
높이가 한 프레임 안에서 균일하므로(상수이거나 theme 파생이지만 행마다 같음) `show_rows` 의 전제를
만족한다. `show_rows` 가 주는 `row_range` 는 **전체 목록 기준 인덱스**라, 선택 핸들러
(`select_worktree(idx)` / `load_diff(idx)`)가 받는 값의 의미는 바뀌지 않는다.

행마다 `Ui::push_id(row_index, …)` 로 **행 인덱스에서 파생된 안정적 id** 를 부여한다.
`show_rows` 는 `skip_ahead_auto_ids(min_row)` 로 id 를 보정하지만 이는 "행 하나가 auto id 하나를
쓴다" 를 가정한다. 행이 소비하는 id 수가 행마다 다르면(예: 커밋 행의 refs pill 개수) 스크롤
위치에 따라 같은 행의 id 가 바뀌고, egui 가 id 기준으로 유지하는 hover/press 상태가 엉뚱한 행에
붙는다. `push_id` 는 그 가정에 의존하지 않는다.

**가로로도 스크롤하는 리스트(diff)는 콘텐츠 폭을 한 번만 재서 캐시하고, 모든 행이 그 폭을
할당한다.** 캐시는 `(폭을 잰 폰트 크기, 폭)` 이며 plugin 상태에 산다. 무효화는 diff 를 바꾸는
유일한 경로인 setter 하나가 맡고, 폰트 크기가 달라지면 키 불일치로 자동 재측정된다.

## Consequences

- **얻은 것**: 한 프레임에 레이아웃되는 행 수가 목록 길이가 아니라 뷰포트 높이에 비례한다. 커밋
  200 행 · diff 수천 라인에서도 프레임당 비용이 상수에 가깝다. diff 가로 스크롤 폭이 스크롤
  위치와 무관하게 고정된다.
- **잃은 것**: 리스트 코드가 "행을 순회한다" 에서 "행 인덱스를 해석한다" 로 바뀌어, 인덱스 매핑이
  새로운 회귀 표면이 됐다(특히 diff 의 hunk 헤더/라인 평탄화). 행 높이 계산이 행 함수 안에서
  호출자와 공유하는 헬퍼로 빠져 나와, 행 함수와 헬퍼가 어긋나면 행이 겹치거나 벌어진다.
- **운영 비용 / 유지 부담**: 행 높이를 바꿀 때 행 함수와 높이 헬퍼를 함께 고쳐야 한다. diff 폭
  캐시는 setter 를 우회해 diff 를 갈아끼우면 낡은 값이 남는다 — 그래서 필드 대입이 아니라 setter
  하나로 좁혀 두었다.

## Alternatives Considered

- **`show_viewport` + 직접 계산**: 행 높이가 불균일할 때 필요한 API 다. 네 리스트가 모두 균일
  높이라 `show_rows` 로 충분했고, 뷰포트 산술을 직접 들고 있을 이유가 없었다.
- **행 함수에서 `ui.is_rect_visible(rect)` 로 조기 반환**: `allocate_exact_size` 는 여전히 전
  행에서 돌고, 커밋 행의 oid 폭 측정처럼 allocate 이전에 하는 레이아웃은 그대로 남는다. 리스트가
  길수록 남는 비용이 선형으로 늘어 근본 해결이 아니다.
- **diff 가로 폭을 매 프레임 전 라인에서 재측정**: 정확하지만 virtualization 이 없애려던 O(라인
  수) 레이아웃을 그대로 되살린다.
- **diff 가로 폭을 문자 수 × monospace 글리프 폭으로 추정**: 폰트 없이 계산할 수 있어 캐시가
  필요 없지만, CJK·이모지 등 폭이 다른 글리프에서 과소 추정돼 최장 라인 끝에 도달하지 못한다.
- **diff 폭 캐시를 diff 내용의 구조 지문(파일 경로 · hunk 수 · 라인 수)으로 무효화**: 상태 필드와
  setter 를 안 건드려도 되지만, 지문이 같고 내용만 다른 diff 에서 낡은 폭이 남는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 어느 리스트든 행 높이가 행마다 달라진다(예: 커밋 summary 줄바꿈, diff 라인 wrap) — `show_rows`
  전제가 깨지므로 `show_viewport` 기반으로 갈아타야 한다.
- egui 가 불균일 높이 virtualization 또는 콘텐츠 폭 선언 API 를 제공한다 — 폭 캐시를 걷어낼 수
  있다.
- 리스트가 읽기 전용을 벗어나 드래그·범위 선택처럼 프레임을 가로지르는 상호작용을 갖는다 — 행 id
  안정성 요구가 지금보다 강해진다.

## References

- [`dev-guide/egui-mesh-channel.md`](../dev-guide/egui-mesh-channel.md) — plugin 콘텐츠 렌더 채널과
  `set_context` 마다 도는 egui pass
- [`plugins/git-viewer/screens/git-viewer.md`](../plugins/git-viewer/screens/git-viewer.md) — 이
  결정이 적용된 리스트 4개의 현재 동작
- [ADR-0028](0028-plugin-egui-mesh-render-channel.md) — plugin egui-mesh 자가 렌더 채널

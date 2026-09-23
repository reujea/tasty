# Split 명령

IPC/CLI 모두 **단일 `split` 명령**으로 상위(Pane)/하위(Surface) 레이아웃 분할을 통합한다([work-area](../../features/work-area/index.md)의 두 레벨 레이아웃).

```bash
tasty split --level surface --target-surface this --direction vertical --meta '{"nickname":"logs"}'
tasty split --level pane --target-pane 2 --direction horizontal
tasty split --level pane --target-surface this --type markdown --file /path/doc.md
```

## 파라미터

| 파라미터 | 필수 | 설명 |
|----------|------|------|
| `level` | yes | `pane` \| `surface` |
| `target_surface` | * | surface ID / `"this"` / nickname |
| `target_pane` | * | pane ID (pane level 만) |
| `direction` | no | `vertical`(기본) \| `horizontal` |
| `type` | no | `terminal`(기본) \| `markdown` \| `explorer` \| `html` + plugin kind |
| `file`/`path`/`url` | type별 | markdown=file 필수, explorer=path, html=url 필수 |
| `cwd` | no | 터미널 작업 디렉토리 |
| `meta` | no | 새 surface 에 설정할 메타데이터(JSON) |

`target_surface` 와 `target_pane` 중 정확히 하나(둘 다 지정 시 에러). ID 는 전역 고유 → target 주어지면 **전 workspace 검색**.

### target 해석

- `target_surface`: 숫자=ID 직접 / `"this"`=`TASTY_SURFACE_ID` 환경변수(자기 surface) / 문자열=surface_meta `nickname` 검색.
- level별: pane+target_surface = surface 가 속한 pane 옆 분할 / pane+target_pane = 그 pane 옆 / surface+target_surface = 그 surface 내부 분할 / **surface+target_pane = 에러**.

## cwd 결정 (우선순위)

1. 호출자가 명시한 `cwd`(IPC/CLI).
2. `inherit_cwd` 켜져 있으면 source surface 의 `Surface::source_cwd()`.
3. 그 외 None → 셸 home.

source 별 `source_cwd()` 는 [cwd 정책](../policies/cwd.md). 이 정책은 새 탭·새 워크스페이스·pane/surface 분할·타입 변환 등 **모든 생성 경로**에 동일 적용(carry invariant: [surface-cwd](../policies/cwd.md#surface-cwd-invariant)).

## 포커스 정책

**split 은 포커스를 이동하지 않는다.** workspace.create/tab.create 도 IPC/CLI 시 포커스 유지:

| 동작 | UI(키보드/클릭) | IPC/CLI |
|------|-----------------|---------|
| split / workspace 생성 / tab 생성 | 새 영역으로 포커스 | **포커스 유지** |

포커스 이동은 CLI/IPC 로 불가, 단축키/마우스로만([focus 독립성](../policies/focus.md)).

## meta

새 surface 에 key-value 설정(각각 `surface.meta.set`). 주 용도: `nickname`(이름 참조), 커스텀 태그(에이전트가 surface 분류/추적). 응답: `{new_pane_id?, new_surface_id}`.

## 관련

- [work-area](../../features/work-area/index.md) — 두 레벨 레이아웃 · [cwd 정책](../policies/cwd.md) · [surface-cwd invariant](../policies/cwd.md#surface-cwd-invariant) · [reference/api](../../reference/api.md)

# 디자인 변경 워크플로 — 요청문서 → 시안 → 정합 루프

디자인에 **없는** UI 요소를 새로 만들거나, 디자인과 **다른** 형태로 바꿔야 할 때 따르는 절차다.
"소스부터 고치지 않는다" — 먼저 디자인을 *확보*하고, 받은 디자인을 갤러리·본체로 내린다.

> 디자인에 *이미 있는데* 소스만 못 따라간 경우(구현 누락/불일치)는 이 워크플로가 필요 없다 —
> 디자인 변경이 아니므로 바로 [gallery-first](gallery-first.md) 의 구조 전사(1단계)로 간다.

이 워크플로의 도구 분담(Claude design 디자인 / claude code 구현)·정합 순서와 그 근거·재검토 조건은
[ADR-0510](../adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) 에 있다. 본 문서는 그 결정의 *현재 운영 절차* 만 기술한다.

## 요청문서 전달 경로 (직접 접근 우선 / 로컬 fallback)

요청문서를 designer(Claude design)에게 넘기는 방법은 **claude design 프로젝트 직접 접근 수단(예: DesignSync MCP)의 유무**에 따라 갈린다. 어느 경로든 요청문서의 내용·구성(§0~§8)·changelog 는 동일하다.

| | **A. 직접 접근 가능 (우선)** | **B. fallback (직접 접근 없음/실패/미인증)** |
|---|---|---|
| 요청문서 위치 | 원격 프로젝트의 **전용 요청 인박스** `design-request/<slug>.md` 에 직접 write | 로컬 작업 폴더의 `design-request/MMDDhhmm-design-request-<slug>.md`(gitignored — 폴더 위치는 로컬 전용 지침이 정한다) |
| 넣는 주체 | claude code 가 write (요청 md 한정 — 실제 디자인 파일 산출은 아님) | claude code 가 로컬 파일로 작성 |
| designer 에게 전달 | 사용자가 Claude design 을 열어 직접 지시 | 사용자가 요청문서를 Claude design 에 직접 제출 |
| 시안 수령 | 갱신 디자인/`.DONE` 을 직접 접근 수단으로 읽어 정합(폴더 경로 수령 불필요) | 사용자가 받아온 디자인 폴더 경로를 넘겨받아 정합 |

- **직접 접근 수단으로 write 하는 대상은 요청 인박스의 요청 md 뿐**이다. 토큰/컴포넌트/UI kit 등 실제 디자인 산출물은 여전히 designer(Claude design)만 만든다 — claude code 가 직접 수정하지 않는다는 원칙은 불변.
- **A 경로 지시는 항상 사용자 몫**이다 — claude code 는 원격 인박스에 요청문서를 write 할 뿐, designer 에게 실제로 알리는 것(지시)은 사용자가 Claude design 을 열어 직접 한다.
- 이 저장소의 직접 접근 설정(접근 수단·projectId 등)은 커밋되지 않는 로컬 전용 지침(Claude Code 가 세션 시작 시 함께 로드하는 프로젝트 로컬 CLAUDE.md)에 기록한다(세션 고유값이라 커밋 문서에 박지 않는다).
- 요청 인박스는 원격 프로젝트에 **전용 폴더 `design-request/`** 를 둔다(파일명 `<slug>.md`). claude design 의 `uploads/` 는 아무 파일이나 업로드되면 쌓이는 **범용 싱크**라 요청문서가 잡동사니와 섞이므로 인박스로 쓰지 않는다. `design-request/` 이름은 로컬 B경로 폴더명과 맞춰 A/B 대칭을 이룬다. B(로컬)의 파일명은 tasty 컨벤션 `MMDDhhmm-design-request-<slug>.md`.
  - designer(Claude design)는 인박스 폴더를 스스로 스캔하지 않고 요청 경로를 사용자가 직접 열어 보여주므로, 이 폴더명은 순수 claude code 측 컨벤션이다(designer 규율은 인박스 폴더명을 소유하지 않는다).

아래 다이어그램·표·라이프사이클은 **B(fallback)** 경로를 기준으로 그린 것이다. A 경로에서는 `[2] 사용자 제출`이 "claude code 가 원격 요청 인박스에 write → 사용자가 Claude design 을 열어 직접 지시"로 대체되고, 시안 수령이 직접 읽기가 된다.

## 뱅글뱅글 도는 루프

```
   ┌──────────────────────── 재요청 (부족·불일치 발견 시) ────────────────────────┐
   │                                                                              │
   ▼                                                                              │
[1] planner ──작성──▶  디자인 요청문서  ──[2] 사용자 제출──▶  [3] Claude design       │
 (요청 산출물 정의)     (로컬 작업 폴더/            (사용자가 직접 제출)    (고충실 HTML/CSS 시안)  │
                        design-request/*.md)                                  │            │
                                                                              │ 시안 수령   │
                                                                              ▼            │
                              [4] 정합: 갤러리 specimen → 본체 구현 → 사이트 사본 ─────────┘
                                  (구조 전사 + 토큰 정합)     (site/vendor 재-vendoring)
```

| 단계 | 행위자 | 하는 일 | 산출물 |
|------|--------|---------|--------|
| 1 | **planner** | 무엇을·어떻게 보이게 할지 정의한 **디자인 요청문서** 작성 | 로컬 작업 폴더의 `design-request/MMDDhhmm-design-request-<slug>.md` |
| 2 | **사용자** | 요청문서를 **Claude design 에 직접 제출** | (제출) |
| 3 | **designer** (Claude design) | 색·간격·인터랙션 살아있는 **고충실 시안** 생성 | HTML/CSS 시안 (원격 프로젝트의 파일) |
| 4 | **구현** (claude code) | 갤러리 specimen → 본체 반영 → **사이트 사본 재-vendoring**(`site/vendor/`) | 코드 + vendor 사본 |
| → 재요청 | planner | 4 에서 부족·불일치가 드러나면 **추가 요청문서**로 다시 2 로 | 새/갱신 요청문서 |

루프인 이유: 한 번에 끝나지 않는다. 시안이 열린 결정(아래 §6)을 확정하거나
요청문서에 없던 구조를 드러내면, 부족분은 다음 요청문서로 다시 돈다.

## 디자인 요청문서란

**planner 가 작성하는 입력 산출물.** "이번에 무엇을, 어떤 화면·컴포넌트·상태·인터랙션으로 보이게 할지"를
디자이너(Claude design)가 고충실 시안으로 옮길 수 있도록 정의한 문서다. *왜/구현 배선*이 아니라 **무엇을 어떻게 보이게 할지**만 담는다.

- **위치**: 전달 경로에 따라 다르다(위 "요청문서 전달 경로" 참조) — A(직접 접근)면 원격 프로젝트의 전용 요청 인박스 `design-request/`(범용 `uploads/` 싱크가 아님), B(fallback)면 로컬 작업 폴더의 `design-request/`(gitignored, 커밋 대상 아님 — 폴더 위치는 로컬 전용 지침이 정한다).
- **파일명 규칙**: `MMDDhhmm-design-request-<slug>.md` — 맨 앞에 **월일시분(MMDDhhmm)**, 이어서 **`design-request`**, 마지막에 내용 slug 를 붙인다(예: `07101430-design-request-explorer-file-manager.md`). 날짜·시간을 선두에 둬 파일이 시간순으로 정렬되게 하고, `design-request` 접두로 문서 종류를 명시한다.
- **기준 구성**: 아래 §0~§8 구성을 그대로 따른다.

### 요청문서 구성 (§0~§8)

| 섹션 | 내용 |
|------|------|
| 헤더 메타 | 작성일 · 요청자(planner) · 수행자(designer) · **상태** · 연계 계획 |
| §0 한 줄 요약 | 이 요청이 만들려는 것 한 줄 |
| §1 맥락 / 제약 | 토큰=코드 SoT(Catppuccin Mocha, raw hex 금지) · 4px 그리드/14px 폰트 상한/1px 보더 · gallery-first 부품 재사용 · i18n 가변폭 · 기존 구현과의 관계 |
| §2 인벤토리 | 디자이너가 만들 화면/컴포넌트/팝업/상태 목록 표 (신규/기존 구분) |
| §3 화면별 요구사항 | 화면 단위 레이아웃·영역 구성 |
| §4 컴포넌트별 요구사항 | 컴포넌트 단위 상태·변형 |
| §5 인터랙션 / 동작 | 시안에 주석으로 달 조작·상태전이 규칙 |
| §6 열린 결정 | 디자이너/사용자가 확정해야 할 미결 사항 |
| §7 산출물 형식 | 디자이너에게: 고충실 HTML/CSS, 상태 변형 포함, 토큰 안에서만, 구현 친화 박스 경계 |
| §8 참고 | 계획 문서, 토큰/팝업 정책 링크 |

### 요청문서가 강제하는 디자인 규칙 (시안 단계부터)

요청문서 §1·§7 은 시안이 본체 토큰·정책과 어긋나지 않도록 다음을 **시안 단계에서부터** 못박는다:

- **토큰 SoT = 코드(`Theme`)** — 색·간격·치수·보더는 모두 Catppuccin Mocha 토큰 안에서만. raw hex 하드코딩 금지(시안에서도 토큰 의미 이름으로 표기). ([theme UI 규칙](../design/systems/theme.md#ui-디자인-규칙-필수))
- **레이아웃 규칙** — 4px 그리드 · 폰트 14px 상한 · 보더 1px · 호버(흰색 +8%)/액티브(+12%) 오버레이는 직접 값 금지(자동 도출).
- **gallery-first 부품 재사용** — 보편 부품(버튼/입력/표/스크롤바/컨텍스트 메뉴/팝업)은 [공용 위젯](../design/policies/shared-widgets.md)·기존 카탈로그와 시각 일관. ([gallery-first](gallery-first.md))
- **i18n 가변폭** — 모든 문자열은 [`t()`](i18n.md) 번역 키로 노출될 예정. 시안 텍스트는 영어 기준이되 독/일/한 가변 길이를 감안해 여유 폭.

## 정합 대상 — 사이트 사본 (필수)

레포에는 같은 원격 canonical 의 vendored 사본이 **둘** 있고, 디자인 결정은 **둘 다** 따라와야 한다.

| 사본 | 무엇이 받는가 | 갱신 절차 | 안 따라오면 |
|------|--------------|-----------|-------------|
| 앱 — `crates/tasty-design-tokens/dtcg/tasty.tokens.json` | 토큰(DTCG) | [`crates/tasty-design-tokens/README.md`](../../crates/tasty-design-tokens/README.md) "vendor 갱신 절차" | `crates/tasty-design-tokens/tests/freshness.rs` 의 census 스냅샷이 빨개진다 |
| 사이트 — `site/vendor/` | 킷 전부(토큰 · 컴포넌트 · UI kit · 갤러리 · 가이드라인) | [`site/vendor/README.md`](../../site/vendor/README.md) "vendor 갱신 절차" | **공개 사이트가 결정 이전 UI 를 현재형으로 전시한다** |

뒤엣것이 조용한 이유는 사이트가 그 사본을 **정상적으로** 렌더하기 때문이다 — 빌드도 시험도 CI 도 전부 초록이고, 낡았다는 사실이 값으로 안 남는다. 실제로 제거하기로 결정된 컨트롤 한 줄이 그렇게 공개 페이지에 남았다.

부분적인 자동 채널이 **둘** 있다. `crates/tasty-doc-guards/tests/site_vendor_tokens_track_the_app_export.rs` 가 두 사본의 **토큰 이름 집합**을, `crates/tasty-doc-guards/tests/site_vendor_icons_match_the_app_transcription.rs` 가 **아이콘 기하·채움과 그 그릇**(`viewBox` · 선 굵기 · cap/join)을 대조한다 — 앱이 받은 것을 사이트가 못 받으면 그 차이가 명부와 어긋나 빨개진다. **그 둘이 덮는 범위와 이 절차가 덮는 범위는 다르다** — 판정기는 토큰 이름과 아이콘(기하·채움·그릇) 두 층만 보고, 그 둘을 안 건드리는 결정(문구 변경 · 구성 변경 · 컨트롤 삭제)은 양쪽 좌변을 똑같이 남겨두므로 **여전히 안 잡힌다.** 그 층은 이 단계(사람이 도는 재-vendoring)만 닫는다. 판정기가 초록인 것을 "사이트 사본이 최신" 으로 읽지 마라.

이 정합 대상을 세운 근거·대안·재검토 조건은 [ADR-0510](../adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md).

재-vendoring 은 원격 프로젝트 접근(DesignSync 세션)을 쓰므로, 그 권한이 없는 세션에서는 이 단계를 **미완으로 남기고 그 사실을 적는다** — 조용히 건너뛰지 않는다.

## 상태 라이프사이클

요청문서 헤더 메타의 `상태:` 필드로 추적한다.

```
requested ──(사용자 제출·시안 수령)──▶ received ──(갤러리/본체/사이트 정합)──▶ reconciled
    ▲                                                                          │
    └────────────────── re-requested (부족·불일치 발견 시 새 요청으로) ─────────────┘
```

| 상태 | 의미 |
|------|------|
| `requested` | planner 가 요청문서를 작성·확정. 아직 시안 없음 (제출 대기). |
| `received` | 사용자가 Claude design 시안을 받아옴. 정합 작업 대기/진행. |
| `reconciled` | 시안을 갤러리 specimen + 본체 + **사이트 사본(`site/vendor/`)** 에 반영 완료 — 사이트 사본이 빠지면 코드가 따라와도 **공개 사이트는 결정 이전 UI 를 계속 전시한다**(위 [정합 대상 — 사이트 사본](#정합-대상--사이트-사본-필수)). 이 시점에 **요청문서를 삭제한다**(아래 [정합 완료 후 요청문서 삭제](#정합-완료-후-요청문서-삭제-필수)). |
| `re-requested` | 정합 중 부족·불일치가 드러나 추가/갱신 요청문서로 루프 재진입. |

## 정합 완료 후 요청문서 삭제 (필수)

요청문서는 designer 에게 "무엇을 만들지"를 넘기기 위한 **입력 산출물**일 뿐, 정합이 끝나면 더 이상 쓸모가 없다. 그래서 `reconciled` 에 도달하면(디자인 작업이 수락·완료되고 시안이 갤러리/본체/사이트 사본에 반영 완료) **그 작업을 유발한 요청문서를 삭제한다.** 이력이 필요한 결정은 이미 ADR·docs 에 남으므로 요청문서를 붙잡아 둘 이유가 없다.

**전제 (삭제 조건).** 아래를 모두 만족할 때만 삭제한다:

1. 상태가 확정적으로 `reconciled` 다 — 아직 `re-requested` 로 루프가 도는 중이면 삭제하지 않는다(다음 turn 에 다시 쓰인다).
2. 그 작업을 유발한 요청문서가 로컬 또는 원격에 **실제로 존재함을 확인**했다(A=원격 `list_files`, B=로컬 파일 존재 확인).
3. claude code 가 그것을 **직접 삭제하거나 삭제를 요청할 수단이 있다**(아래 경로별). 수단이 없거나 실패하면 강제하지 않고 상태만 `reconciled` 로 두고 넘어간다(능력 없음 ≠ 미완).

**경로별 삭제 방법.**

| 경로 | 요청문서 위치 | 삭제 방법 |
|------|--------------|-----------|
| **B. 로컬 (fallback)** | 로컬 작업 폴더의 `design-request/MMDDhhmm-design-request-<slug>.md` | 로컬 파일을 **직접 삭제**한다(gitignored 라 이력 남길 필요 없음). |
| **A. 원격 (직접 접근)** | 원격 전용 인박스 `design-request/<slug>.md` | **DesignSync `delete_files`** 로 삭제한다 — `list_files`(존재 확인) → `finalize_plan` 의 `deletes`(+ `writes` 는 빈 배열) 에 그 경로를 넣어 `planId` 획득(권한 프롬프트) → `delete_files`(`planId`) 순. 원격 파일 삭제는 이 메서드로 가능하다. |

**삭제 대상 = 요청문서(md)뿐.** 확정 시안(원격 프로젝트의 시안 파일 · `docs/design/` 에 보존한 HTML)은 **삭제하지 않는다** — 그것은 아래 "누가 무엇을 하나"·ADR-0510 의 **보존** 대상이다. 삭제하는 것은 입력 요청문서 한 건이지 산출물이 아니다.

## 누가 무엇을 하나 (역할 분리)

| 역할 | 누가 | 책임 |
|------|------|------|
| **planner** | claude code(todo-maker 겸) 또는 사용자 | 요청문서 작성 — 인벤토리·제약·열린 결정 정의. |
| **사용자** | 사람 | 요청문서를 **Claude design 에 직접 제출**하고 시안 결과를 받아온다. (이 제출은 사용자 행위 — 에이전트가 대행하지 않는다.) |
| **designer** | Claude design (claude.ai Artifacts) | 고충실 HTML/CSS 시안 생성. 색·치수는 요청문서가 준 토큰 팔레트 안에서만. |
| **구현** | claude code | 확정 시안을 [gallery-first](gallery-first.md) 순서로 — 갤러리 specimen(구조 전사+토큰 정합) → 본체 반영 → 사이트 사본 재-vendoring. |

> 확정 시안은 원격 Claude design 프로젝트에 파일로 남는다(정합 뒤 지우는 것은 요청문서뿐이다). 레포 안에
> 근거로 남겨야 하는 시안은 HTML 을 `docs/design/` 에 보존한다(ADR-0510).

## 관련

- [ADR-0510](../adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) — 도구 분담·정합 순서·갤러리 완전성·사이트 사본 결정 근거와 재검토 조건.
- [gallery-first](gallery-first.md) — 확보한 디자인을 갤러리→본체로 내리는 순서(0단계 "디자인 확보"가 이 워크플로).
- [design/policies/gallery-completeness](../design/policies/gallery-completeness.md) — cut 금지 시 디자인 보강(=이 워크플로로 재요청).
- [design/systems/design-parity-notes](../design/systems/design-parity-notes.md) · [design-gallery-mapping](../design/systems/design-gallery-mapping.md) — 시안→소스 구조 전사 원칙·매핑.
- [theme.md "UI 디자인 규칙"](../design/systems/theme.md#ui-디자인-규칙-필수) — 토큰 정합 규칙(시안부터 적용).

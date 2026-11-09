# 커밋 컨벤션

tasty 는 [Conventional Commits](https://www.conventionalcommits.org/) 를 따른다. 강제 정책은 [`../../CLAUDE.md`](../../CLAUDE.md) "커밋 정책" — 이 문서는 형식·type·단위 기준의 상세다.

## 형식

```
<type>(<scope>): <description>

[optional body]
```

- `<description>` — 영어로 작성, 명령형 현재 시제("add X"). "fix bug" 처럼 무엇을 고쳤는지 알 수 없는 제목 금지: 무엇을·어디서·왜가 한 줄에 드러나게.
- `(scope)` — 영향 범위(선택). 예: `feat(themes)`, `fix(ipc)`, `refactor(state)`.
- `body` — 필요할 때만. 동기·트레이드오프·거부한 대안.

## 언어와 평문

커밋 메시지는 Markdown 문서가 아니다. 제목과 본문에 각각 다음 규칙을 적용한다.

- 제목 전체(type·scope·description)는 영어 평문으로 작성한다.
- 본문도 영어 평문으로 작성한다. 제목만 영어로 쓰고 본문을 한국어로 쓰지 않는다.
- 제목과 본문 모두 Markdown 서식을 금지한다. 별표 강조, 백틱 인라인 코드, 제목 표기, 글머리·번호 목록, 인용문, Markdown 링크·이미지, 코드 블록, 표를 사용하지 않는다.
- 본문은 문장과 빈 줄로 나눈 문단으로 구성한다. 파일 경로·명령·식별자는 백틱이나 강조 없이 그대로 적는다. URL이 필요하면 Markdown 링크 대신 주소를 그대로 적는다.
- Conventional Commits의 type(scope): description 구조와 Git trailer의 Key: Value 구조는 평문 메타데이터이므로 유지한다. 경로나 식별자에 포함된 기호 자체를 Markdown 서식으로 취급하지 않는다.

이 문서의 코드 블록과 Markdown은 규칙을 설명하기 위한 문서 서식이며, 커밋 메시지에 복사할 서식이 아니다.

## Type

| 타입 | 용도 |
|------|------|
| `feat` | 새 기능 |
| `fix` | 버그 수정 |
| `docs` | 문서만 변경 |
| `refactor` | 동작 변화 없는 구조 개선 |
| `test` | 테스트 추가/수정 |
| `perf` | 동작은 같고 성능만 개선 |
| `style` | 동작·구조 변화 없는 표기 변경(포맷, 정렬 등) |
| `i18n` | 번역 변경 — 하드코딩 문자열의 번역 키 전환, `lang/{en,ko,ja,zh-Hans}.toml` 키 추가/수정 |
| `chore` | 빌드 설정, 의존성, CI, 버전 bump 등 |

`i18n` 을 `refactor` 로 흡수하지 않는 이유: 리뷰어가 "어떤 문자열이 번역 대상이 됐고 어떤 lang 파일이 같이 바뀌었나" 로 히스토리를 걸러야 하는데, 그 기준이 구조 개선과 섞이면 잡히지 않는다. 키 정합(3 파일 동일 키 · placeholder 개수)이 깨지는 회귀도 이 type 안에서 추적된다. 번역 키를 건드리지 않는 문자열 리팩토링은 그대로 `refactor`.

## 단위 — 한 커밋 = 한 변경

**기능 하나를 수정/추가할 때마다 즉시 커밋한다. 여러 기능을 한 커밋에 묶지 않는다.**

판단 기준: 이 커밋이 revert 되면 "딱 그 한 가지" 가 사라지는가? 둘 이상이 한꺼번에 사라지면 쪼갰어야 한다.

- 한 줄 변경이라도 의미가 독립적이면 별도 커밋.
- 같은 의미의 후속(예: rename 한 김에 호출처 갱신)은 한 커밋에 넣어도 됨.
- **리팩토링 + 기능 추가는 분리** — 리팩토링 먼저, 기능 다음 커밋.

## body 작성

다음 중 하나면 body 를 적는다: 표제만으로 *왜* 가 부족할 때 · 거부한 대안이 있을 때 · 다른 코드의 가정/호환성을 깰 때 · issue/PR 인용이 필요할 때. body 가 길어지면 별도 design 문서/ADR 로 빼고 커밋은 그것을 참조한다.

## ADR 커밋

ADR([`../adr/index.md`](../adr/index.md)) 관련은 `docs(adr)` scope 권장.

- 신규: `docs(adr): add ADR-XXXX <slug>`
- Supersede: `docs(adr): supersede ADR-XXXX by ADR-YYYY`
- Deprecate: `docs(adr): deprecate ADR-XXXX`

## 버전 bump 커밋

릴리스 버전 bump 의 형식·절차는 [`release.md`](release.md). 본체/plugin 패치 자동 +1 규칙은 [`../../CLAUDE.md`](../../CLAUDE.md) "버전 정책".

## 예시

```
feat(memory): add namespaced secret storage
fix(focus): preserve focus when closing non-active tab
refactor(intent): split surface intent module by action type
docs(dev-guide): add i18n policy
i18n(cli): route passkey CLI strings through translation keys
chore(deps): bump wgpu to 22.1
```


이 규칙은 새로 작성하는 커밋 메시지에 적용한다. 기존 커밋 메시지는 소급하여 변경하지 않는다.

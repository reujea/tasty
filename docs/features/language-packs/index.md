# 언어팩 (Language packs)

- **Status**: Implemented
- **주체**: 로컬 사용자
- **ADR**: [ADR-0114](../../adr/0114-language-pack-directory-shape-and-english-fallback.md)
- **코드**: `crates/tasty-i18n/src/lib.rs`(로더 · 스캐너 · `LoadReport`), `src/boot/locale.rs`(부팅 적용), `src/boot/locale_font.rs`(`[font]` resolve), `crates/tasty-egui-theme/src/lib.rs`(`install_locale_font_fallback`), `src/app/boot_machine.rs`(폴백 토스트), `crates/tasty-ui-widgets/src/language_select.rs` + `src/view/settings/ui/tabs/general.rs`(콤보)
- **화면**: [설정 창](../settings/screens/settings.md) General › Language 콤보 · 부팅 직후 경고 토스트

## 목적

내장 3 개 언어(`en`/`ko`/`ja`) 밖의 언어를 사용자가 **파일을 놓는 것만으로** 추가하고, 설정 창에서 다른 언어와 같은 콤보로 고를 수 있게 한다. 팩이 문자열만이 아니라 글리프(폰트)까지 책임진다는 계약을 형상에 박아, 문자열은 뜨는데 글자가 □ 로 깨지는 상태를 "팩의 결함" 으로 드러낸다.

## 내부 동작 (headless-valid)

### 형상

- **언어팩** = `~/.tasty/lang/<code>/pack.toml` (+ 동봉 폰트 파일). 내장이 아닌 새 코드용.
  - `[meta] name` — 표시 이름(선택, 없으면 코드가 라벨).
  - `[font]` — **필수**. `builtin = true` / `file = "<팩 기준 상대경로>"` / `family = "<패밀리명>"` / `candidates = [...]` 중 하나(우선순위 그 순서). 섹션이 없거나 넷 다 없으면 형상 위반.
  - 나머지 문자열 키 — 내장 lang 파일과 같은 트리. 영어 베이스 위에 overlay.
  - **크기 상한 2 MiB** — 넘으면 파싱 전에 거부하고 warn 후 목록에서 뺀다. 가장 큰 내장 파일(`lang/ja.toml`, 약 305 KiB / 약 2,850 키)의 약 6.7 배다. 상한이 있는 이유는 팩을 읽는 비용이 **사용자가 놓은 파일 크기에 비례**하고 그 비용을 설정 창 첫 오픈 때 렌더 스레드가 물기 때문이다.
  - **빈 값은 "번역 없음"** — 값이 비었거나 공백뿐인 키는 overlay 에서 빠져 아래 층이 그대로 보인다(라벨 없는 버튼이 생기지 않는다). 로드 시 몇 개가 빠졌는지 `tracing::warn!` 한 줄. **세 로드 경로에 같은 규칙**이 걸리고([ADR-0124](../../adr/0124-blank-value-rule-is-load-path-independent.md)), 드러나는 것만 다르다 — 팩은 영어 베이스 위에 얹히므로 영어가, 오버라이드는 그 내장 언어 위에 얹히므로 **그 언어의 원래 문구**가, plugin 언어파일은 그 plugin 의 `en.toml` 위에 얹히므로 **그 plugin 의 영어**가 보인다. 일부러 비운 텍스트가 필요하면 **폭 없는 문자(U+200B)** 를 쓴다 — NBSP(U+00A0)는 안 된다(`str::trim` 이 유니코드 `White_Space` 를 전부 먹는다).
- **오버라이드** = `~/.tasty/lang/<builtin>.toml` 단일 파일. 내장 코드 전용, `[font]` 불필요. 내장이 아닌 코드의 단일 파일은 팩이 아니다(경고 후 무시). 크기 상한과 **빈 값 규칙**은 팩과 같다 — 빠진 키는 내장 `<code>.toml` 의 문구로 되돌아간다.
- 내장 `lang/{en,ko,ja}.toml` 도 `[meta] name` 을 갖는다(`English` / `한국어` / `日本語`).

### plugin 문자열 덮어쓰기

사용자 파일 하나를 host의 plugin 라벨과 SDK UI가 함께 읽는다. 내장 코드면
`~/.tasty/lang/plugins/<plugin-id>/<code>.toml`, 새 언어팩이면
`~/.tasty/lang/<code>/plugins/<plugin-id>.toml`이다. plugin 설치본 영어 → 선택 언어 →
사용자 파일 순서이며, 빈 값·누락 키는 앞 층의 문구를 유지한다. 잘못된 TOML이나 2 MiB를
넘는 사용자 파일은 경고 후 무시한다. 변경 후 재시작한다. plugin 전용 키를 본체 팩에
중복해서 넣지 않는다(본체 카탈로그 우선 조회는 유지된다).

SDK는 host가 알려 준 `TASTY_PARENT_HOME` 아래에서 읽으므로 설치 자산을 고치거나
별도 파일 권한을 요청할 필요가 없다. plugin ID·언어 코드가 경로를 선택하며 다른
plugin의 파일은 합치지 않는다. 구체 규약은 [i18n](../../dev-guide/i18n.md#사용자-plugin-번역).

### 발견 (`available_languages`)

내장 3 개(고정 순서) + `~/.tasty/lang/` 의 유효한 팩(코드순). 항목마다 코드 · 표시 이름 · 출처(`Builtin` / `BuiltinOverridden` / `Pack`) · `[font]` 선언 종류 · 경로. 파싱 실패, `[font]` 부재/무효, 크기 상한 초과, 내장 코드 이름의 디렉토리, 새 코드의 단일 파일은 전부 `tracing::warn!` + 제외. 규칙 표는 [dev-guide/i18n](../../dev-guide/i18n.md) "언어팩".

발견은 목록에 필요한 `[meta] name` 과 `[font]` **둘만** 읽는다 — 문자열 키를 flatten 하지 않는다. 이 스캔은 설정 창 첫 오픈 때 렌더 스레드에서 동기로 돌기 때문이다. 수락/거절 판정은 실제 로드와 동일해 "목록에 오르면 로드된다" 가 성립한다.

### 부팅 적용과 폴백

1. `general.language` 를 정규화(공백 → `en`)해 `tasty_i18n::init` 에 넘긴다.
2. 내장 코드면 내장 + 오버라이드. 아니면 `<code>/pack.toml` 을 읽는다.
3. 팩이 **없거나 거부되면(크기 상한 초과 포함) 영어로 폴백** — 테이블은 en(+ en 오버라이드), `current_language()` 와 자식 env `TASTY_LOCALE` 은 `en`. `LoadReport { requested, effective, outcome }` 가 이유(`PackMissing { expected }` / `PackInvalid { path, error }`)를 담는다.
4. 알림: 로드 시점 `tracing::warn!` 한 줄(요청 코드 + 기대 경로, **경로는 온전한 값**) — headless/CLI 는 이것이 전부. GUI 는 부팅 후 **경고 토스트 1회**(`i18n.warn.pack_missing` / `pack_invalid`, 창 스코프). 토스트는 본문 200자 캡이 있어, 메시지가 캡을 넘기면 **경로만 가운데를 생략**해(`…`) 캡 안에 맞춘다 — 머리(어느 루트)와 꼬리(`<code>/pack.toml`)는 남는다. 잘려서 안내가 사라지는 것보다 경로를 줄이는 쪽이 낫다.
5. **설정값은 어느 경로에서도 고쳐 쓰지 않는다.** 팩을 넣고 재시작하면 그 언어가 뜬다.

`[font]` 선언은 부팅 1회 실제 폰트 파일로 resolve 해 **UI 폴백 체인 맨 뒤**에 붙인다 — 라틴은 기본 폰트, 팩 스크립트만 팩 폰트로 렌더(혼합). `file` 은 팩 상대경로(팩 밖 이탈 거부), `family`/`candidates` 는 시스템 폰트 DB 에서 파일 경로를 찾고, `candidates` 는 "경로처럼 보이면 file, 아니면 family" 로 순서대로 첫 성공. 붙이기 전 `ab_glyph` 로 파싱 검증(egui 는 깨진 폰트에 panic). resolve 한 절대경로는 `TASTY_LOCALE_FONT` env 로 자식 plugin 에 전달하고 셸 UI 두 경로 + egui UI plugin 넷이 **같은 헬퍼**로 소비한다(검증이 곧 판정이라 사본을 두지 않는다). **resolve/검증 실패는 폴백 없이 경고만** — 문자열은 그대로 뜨고(그 스크립트는 □) UI 폰트만 안 붙는다(GUI 토스트 1회 / headless warn). 규칙 표는 [dev-guide/i18n](../../dev-guide/i18n.md) "`[font]` resolve 와 UI 폴백 체인". RTL(아랍·히브리) 어순은 egui 한계로 범위 밖이다.

### 설정 콤보

목록은 설정 창을 열 때 1회 스캔. 라벨은 `[meta] name` 없으면 코드. 현재 값이 목록에 없으면 `"<code> (not found)"` 행을 끝에 붙여 선택 상태로 보여 주고, 그 행을 다시 골라도 값은 바뀌지 않는다. 다른 행을 고르면 그 코드가 draft 에 들어가고 Save 시 `general.language` 로 영속 — 적용은 재시작 후.

## 인터페이스

- **사용자 트리거**: `~/.tasty/lang/<code>/pack.toml` 작성 → 설정 창 General › Language 에서 선택 → Save → 재시작. 또는 `config.toml` 의 `general.language` 직접 편집.
- **AI Agent (IPC/CLI)**: 전용 메서드 없음 — `tasty debug settings apply --json '{"general":{"language":"<code>"}}'`(debug 빌드)로 설정값을 바꾸고 재시작. CLI(`tasty <subcommand>`)도 같은 부팅 경로를 타므로 팩의 `cli.*` 키가 CLI 출력에 반영된다. **`--help` 도 포함된다** — 도움말 트리는 파싱 직전에 번역되므로 팩이 `cli.help.*` 키를 담으면 그대로 보인다. 번역이 없는 자리는 영어 원문이 남는다. 도움말 heading·기본값 안내·파싱 오류는 로컬 언어로 표시하고, 명령/옵션 토큰·하위 파서 원문·wire 오류는 유지한다. plugin 도움말은 같은 사용자 plugin 카탈로그를 읽는다([i18n.md](../../dev-guide/i18n.md) "CLI 도움말").
- **원격 / 점유**: 해당 없음(로컬 인스턴스의 표시 언어).

## 비-목표 (Out of scope)

- **RTL 어순**(아랍·히브리) — egui 가 양방향 텍스트를 지원하지 않아 글리프가 붙어도 어순이 깨진다. `[font]` 는 글리프 유무만 해결한다([ADR-0193](../../adr/0193-font-resolution-covers-glyph-coverage-not-rtl-ordering.md)).
- **"전부 팩 폰트"** — 팩 폰트는 마지막 폴백이라 라틴은 기본 폰트로 남는다(혼합 렌더). 팩 폰트를 최우선으로 두는 `priority` 옵션은 후일.
- 재시작 없는 언어 전환.
- 팩 설치/배포 도구(다운로드 · 서명 · 버전).
- plugin 설치본 `lang/` 자체의 팩화 — 설치 자산은 기존 코드별 파일을 유지하며 사용자 번역만 위 별도 경로에서 덮는다.

## Acceptance Criteria

- Given `~/.tasty/lang/` 에 `xx/pack.toml`(`[font] builtin = true`) · `yy/pack.toml`(`[font]` 없음) · `ko.toml` · `zz.toml` · `bad/pack.toml`(깨진 TOML) When `available_languages()` Then 목록은 `en, ko(BuiltinOverridden), ja, xx(Pack)` 이고 `yy`/`zz`/`bad` 는 없다(각각 warn).
- Given `general.language = "xx"` When 부팅 Then `xx` 의 문자열이 뜨고 `current_language() == "xx"`, `TASTY_LOCALE=xx`.
- Given `ar/pack.toml` 이 `[font] file = "fonts/NotoSansArabic-Regular.ttf"`(유효 폰트) 를 선언 When `ar` 로 부팅 Then resolve 절대경로가 `TASTY_LOCALE_FONT` 에 실리고 셸 UI·plugin UI 가 아랍 글리프를 □ 없이 렌더(어순은 범위 밖).
- Given `[font] file` 경로가 틀림(없는 파일) 또는 파일이 깨진 폰트 When 부팅 Then panic 없이 `TASTY_LOCALE_FONT` 는 unset, GUI 경고 토스트 1회 / headless warn, 문자열은 그대로 로드(그 스크립트는 □).
- Given `[font] builtin = true` When 부팅 Then 폰트를 붙이지 않고 기본 스택만(라틴 팩용), `TASTY_LOCALE_FONT` unset.
- Given `[font] file` 이 `..` 나 절대경로로 팩 밖을 가리킴 When 부팅 Then resolve 거부(경고) — 팩 디렉토리 밖 파일은 읽지 않는다.
- Given `general.language = "zz"` 이고 팩 없음 When 부팅 Then 영어 UI + 경고 토스트 1회(코드 · 기대 경로), headless/CLI 는 warn 1줄, `config.toml` 의 값은 `zz` 그대로.
- Given 콤보의 현재 값이 목록에 없음 When 콤보를 열고 닫음 Then 값이 바뀌지 않는다.
- Given 팩이 어떤 키의 값을 `""` 로 두었음 When 그 팩으로 부팅 Then 그 키는 영어로 보이고 화면에 빈 라벨이 없다.
- Given `ko.toml` 오버라이드가 같은 키를 `""` 로 두었음 When `ko` 로 부팅 Then 그 키는 **내장 한국어 문구**로 보이고 화면에 빈 라벨이 없다(팩과 같은 규칙, 드러나는 층만 다르다).
- Given `~/.tasty/lang/big/pack.toml` 이 2 MiB 를 넘음 When `available_languages()` Then `big` 은 목록에 없고 warn 이 남는다. And `general.language = "big"` 으로 부팅하면 영어로 폴백하고 `config.toml` 의 값은 `big` 그대로다.
- Given 데이터 루트가 길어 경고 메시지가 200자를 넘김 When 폴백 토스트 Then 경로 가운데가 `…` 로 줄고 `(200자 제한)` 접미가 붙지 않는다.

## 화면

- [설정 창](../settings/screens/settings.md) — General › Language 콤보(갤러리 Settings specimen 의 `language_select` 행과 같은 위젯).

CLI 도움말의 표시/프로토콜 경계는 [ADR-0280](../../adr/0280-cli-help-localizes-presentation-not-protocol.md)을 따른다.

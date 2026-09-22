# ADR-0103: 활성 로케일은 host 프로세스 env 로 plugin 에 전달한다 — 부팅 단일 스레드 구간에서 한 번 set 한다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: i18n, locale, plugin, boot, env, unsafe, language-pack
- **Group**: i18n

## Context

plugin 프로세스는 host 의 i18n 카탈로그에 접근할 수 없다. plugin 이 자기 프로세스에서 직접 그리는 문자열은 SDK `tasty_plugin_sdk::i18n::Translator` 가 plugin 의 `lang/{en,<locale>}.toml` 을 직접 로드해 만들고, 활성 언어는 **`TASTY_LOCALE` 환경변수 하나로만** 받는다(미주입 시 `en` 폴백 — `crates/tasty-plugin-sdk/src/env.rs`).

host 쪽 spawn 코드(`crates/tasty-host-plugin/src/process.rs`)는 설계상 `tasty-i18n` 에 의존하지 않는다 — host 본 바이너리가 부팅 시 자기 env 에 set 해 둔 `TASTY_LOCALE` 을 자식에 그대로 propagate 한다고 주석에 적혀 있었다. 그런데 host 본 바이너리 어디에도 그 env 를 set 하는 코드가 없었다. `src/boot/locale.rs` 는 `crate::i18n::init(&settings.general.language)` 한 줄뿐이었고, `tasty_i18n::current_language()` 는 만들어졌지만 소비처가 0 건이었다. 결과적으로 `general.language = "ko"` 로 실행해도 모든 UI plugin(클립보드 뷰어 · git 뷰어 · 이미지 뷰어 등)이 영어로 떴고, 사용자가 셸에서 직접 `export TASTY_LOCALE=ko` 한 경우에만 우연히 동작했다.

제약이 둘 있다.

1. edition 2024 에서 `std::env::set_var` / `remove_var` 는 `unsafe` 다 — 다른 스레드가 동시에 env 를 읽는 중이면 data race 가 된다. 호출 위치가 곧 안전 근거다.
2. 언어팩은 언어 코드 외에 **폰트 파일**도 제공할 수 있다. 그 경로도 plugin 까지 같은 경로로 도달해야 한다 — plugin 이 CJK 글리프를 자기 폰트 스택에서 못 찾을 때 쓸 폴백이 된다.

## Decision

활성 로케일의 **단일 출처는 `general.language`** 이고, 부팅 시 `boot::locale::init()` 이 이를 `ResolvedLocale { code, font_file }` 로 확정해 두 곳에 함께 반영한다.

1. host 자신의 i18n 테이블 초기화(`crate::i18n::init(&code)`).
2. host **자기 프로세스 env** 에 export — `TASTY_LOCALE=<code>` 는 항상 set, `TASTY_LOCALE_FONT=<절대경로>` 는 언어팩 폰트가 resolve 됐을 때만 set 하고 아니면 **unset** 한다(셸에서 상속된 stale 값이 자식으로 흘러가지 않도록 두 경우를 모두 명시한다). 셸의 export 값은 설정을 이기지 못한다 — host 가 부팅 시 덮어쓴다.

export 는 **프로세스가 아직 단일 스레드인 부팅 구간**에서만 일어난다 — `boot::run_gui` / `run_headless` / `run_subcommand` 의 첫 단계로, 이벤트 루프 · IPC accept 스레드 · plugin spawner 스레드 · PTY reader 가 하나도 생기기 전이다. 이 위치 제약이 `unsafe` 의 안전 근거이며, 부팅 이후에는 env 를 다시 쓰지 않는다. 따라서 언어 변경은 재시작 후에 plugin 에 반영된다(host 자신의 i18n 도 이미 같은 정책이다).

`tasty-host-plugin` 은 계속 `tasty-i18n` 에 의존하지 않는다. spawn 시 host env 의 두 값을 명시적으로 자식에 propagate 한다 — `TASTY_LOCALE` 은 host env 에 없으면 `en` 폴백(이 크레이트가 host 본 바이너리 밖 — 테스트 · 다른 호스트 — 에서 쓰일 때의 계약), `TASTY_LOCALE_FONT` 는 비어 있지 않을 때만 전달하고 아니면 자식 env 에서 제거한다.

## Consequences

- **얻은 것**: plugin UI 가 설정 언어를 따른다. host 가 spawn 하는 모든 자식(plugin 뿐 아니라 PTY 셸도 `Command` env 상속)이 같은 값을 보므로 set 지점이 한 곳이다. host-plugin 의 크레이트 경계(i18n 비의존)가 유지된다. 언어팩 폰트 폴백은 `ResolvedLocale.font_file` 만 채우면 plugin 까지 그대로 전달된다 — 전달 경로를 다시 뚫을 필요가 없다.
- **잃은 것**: 런타임 언어 변경이 plugin 에 즉시 반영되지 않는다 — 이미 host 도 재시작이 필요한 정책이라 새로 잃는 것은 없지만, `language.changed` 이벤트를 받은 plugin 이 자기 env 값과 다른 코드를 보게 되는 창은 존재한다. 사용자 셸 env 에 `TASTY_LOCALE` 이 정보성으로 노출된다.
- **운영 비용 / 유지 부담**: 부팅 시퀀스에서 `locale::init()` 보다 앞에 스레드를 만드는 변경은 이 안전 조건을 깬다 — boot 변경 시 확인해야 할 불변식이 하나 생긴다(`export_to_process_env` 의 doc 이 그 조건을 명시한다).

## Alternatives Considered

- **A. `tasty-host-plugin` 이 `tasty_i18n::current_language()` 를 직접 호출** — leaf 크레이트 결합. host-plugin 은 i18n init 없이도(단위 테스트 · 다른 호스트 바이너리) 동작해야 하고, 그 경우 `current_language()` 의 `en` 폴백은 env 폴백과 같은 값이라 결합 대가 대비 이득이 없다.
- **B. `PluginManager` 생성 인자/설정으로 언어를 넘겨 spawn 시 `.env()` 로 명시** — manager → process 로 plumbing 이 하나 늘고, 언어팩 폰트까지 오면 필드가 더 는다. 설정이 부팅 시점 고정이라 인자로 옮겨도 런타임 갱신 이점이 없다. env 상속이면 plugin 외의 자식도 공짜로 받는다. 다만 런타임 언어 전환이 설계되면 이 안이 1 순위 후보다(아래 재검토 트리거).
- **C. plugin 이 `config.toml` 을 직접 읽음** — plugin 이 host 설정 스키마와 데이터 루트 판정(`TASTY_HOME` / debug-release 분기)을 알아야 한다. 격리가 깨지고, 언어팩 폰트 resolve 로직까지 plugin 마다 복제된다.
- **D. 문제를 알고도 방치(셸 export 에 의존)** — 문서화된 설정(`general.language`)이 plugin 에 닿지 않는 상태를 사용자가 알 방법이 없다. 채택 불가.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 재시작 없는 런타임 언어 전환(plugin 언어 즉시 갱신)이 요구될 때 — env 는 spawn 시점 고정이라 B(명시 인자 + 재spawn 또는 SDK reload) 로 대체해야 한다.
- 부팅 시퀀스에서 `locale::init()` 이전에 스레드를 만들어야 하는 변경이 필요해질 때 — export 지점을 그 앞으로 옮기거나 B 로 전환한다.
- 언어 외의 부팅 고정 값을 plugin 에 더 넘겨야 해서 env 이름이 계속 늘어날 때 — 구조화된 값 하나로 묶는 것을 검토한다.

## References

- [`dev-guide/i18n.md`](../dev-guide/i18n.md) — "Plugin 네임스페이스"
- [`dev-guide/plugin-development.md`](../dev-guide/plugin-development.md) §7 "spawn 시 주입 환경변수"
- `src/boot/locale.rs` — `ResolvedLocale` · `export_to_process_env`
- `crates/tasty-host-plugin/src/process.rs` — `inject_locale_env`
- `crates/tasty-plugin-sdk/src/env.rs` — `PluginEnv::locale` / `locale_font`

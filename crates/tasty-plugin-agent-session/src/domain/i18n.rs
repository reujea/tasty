//! 이 plugin 의 문구 — **한 곳에서 해석한다**.
//!
//! # 왜 전역인가
//!
//! `client.rs` 만 해도 자유 함수가 17 개고 공유 상태가 없다. 거기에
//! `&Translator` 를 인자로 흘리면 호출 사슬 전체가 바뀌고, 그 변경은
//! 번역과 무관한 자리까지 건드린다.
//!
//! 대신 **기동 시 1회** 채우고 이후에는 읽기만 한다(`OnceLock`).
//! 못 채웠으면(비정상 기동) `t()` 가 **키를 그대로** 낸다 — 화면이
//! 비는 것보다 키라도 보이는 편이 낫다(SDK `Translator` 와 같은 규약).

use std::sync::OnceLock;

use tasty_plugin_sdk::i18n::Translator;

static TR: OnceLock<Translator> = OnceLock::new();

/// 지금 쓸 번역기.
///
/// 시험에서는 **실물 `lang/` 을 읽어** 채운다 — 시험이 화면 문구를
/// 한국어로 단언하는데 번역기가 비면 키가 나와 전부 빨간불이 된다.
/// 가짜 표를 넣으면 lang 파일이 깨져도 통과하므로 실물을 쓴다.
fn tr() -> Option<&'static Translator> {
    #[cfg(test)]
    {
        use std::io::Write;
        // 반환값 불요 — 채우는 것이 목적이고, 이미 있으면 그대로 둔다.
        let _ = TR.get_or_init(|| {
            // **PID 로 가른다** — 시험이 병렬 프로세스로 돌면 고정 경로는
            // 서로 덮어쓴다.
            //
            // 사유: 이 자리는 **일부러 프로세스당 하나**다. `OnceLock::
            // get_or_init` 안이라 한 프로세스에서 **한 번만** 돈다 —
            // 재호출로 서로를 지울 수 있는 자리가 아니다. 카운터를 더하면
            // 매번 새 폴더를 만드는 것처럼 읽혀 뜻이 흐려진다.
            let dir = std::env::temp_dir().join(format!("tasty-sup-i18n-{}", std::process::id()));
            // 이미 있으면 성공이다. 못 만들면 아래 `create` 가 실패하고
            // `Translator::load` 가 빈 표를 내 키가 그대로 나온다 — 시험이 잡는다.
            let _ = std::fs::create_dir_all(&dir);
            for (name, body) in [
                ("en.toml", include_str!("../../lang/en.toml")),
                ("ko.toml", include_str!("../../lang/ko.toml")),
            ] {
                if let Ok(mut f) = std::fs::File::create(dir.join(name)) {
                    // 같은 축 — 실패하면 빈 표가 되고 시험이 빨간불이 된다.
                    let _ = f.write_all(body.as_bytes());
                }
            }
            Translator::load(&dir, "ko")
        });
    }
    TR.get()
}

/// 기동 시 1회. 두 번째부터는 조용히 무시된다(`OnceLock` 계약).
pub fn init(tr: Translator) {
    // 두 번째 호출은 조용히 무시한다(`OnceLock` 계약) — 기동 시 1회가
    // 규약이고, 중복 호출을 오류로 올릴 이유가 없다.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "A second init keeps the first translator by design."
    )]
    let _ = TR.set(tr); // 두 번째 초기화는 첫 번역기를 둔다(위 주석).
}

/// 키를 문구로. **미설정·키 미스는 키 자체**를 낸다.
pub fn t(key: &'static str) -> String {
    match tr() {
        Some(tr) => tr.t(key).to_string(),
        None => key.to_string(),
    }
}

/// `{}` 하나를 채운다.
///
/// **미설정이어도 인자는 살린다** — `키: 인자` 로 낸다. 오류 문면의
/// 값(세션 id·사유)이 사라지면 *"어느 세션인지 빠졌다"* 가 되고,
/// 그 손실은 번역이 없을 때만 나서 눈에 안 띈다.
pub fn t1(key: &'static str, a: &str) -> String {
    match tr() {
        Some(tr) => tr.t_fmt(key, a),
        None => format!("{key}: {a}"),
    }
}

/// `{}` 둘 이상 — 순서대로 채운다.
///
/// SDK `t_fmt` 는 인자가 하나라 여기서 직접 돈다. 자리표시자가 모자라면
/// 남은 인자는 버려지고, 남으면 그대로 남는다 — **문구가 깨져도 죽지
/// 않는다**(오류 경로에서 쓰이는 문구가 많다).
pub fn tn(key: &'static str, args: &[&str]) -> String {
    let mut s = t(key);
    // 미설정이면 `t()` 가 키를 그대로 낸다 — 거기엔 `{}` 가 없으므로
    // 아래 루프가 한 번도 안 돌고 **인자가 통째로 사라진다**. 그래서
    // 그때는 키 뒤에 붙여 낸다(`t1` 과 같은 규약).
    if tr().is_none() {
        return format!("{key}: {}", args.join(" · "));
    }
    for a in args {
        match s.find("{}") {
            Some(i) => s.replace_range(i..i + 2, a),
            None => break,
        }
    }
    s
}

#[cfg(test)]
mod tests {
    /// **화면에 나가는 문구에 한국어가 박혀 있나**(2026-09-30).
    ///
    /// # 왜 이 가드가 필요했나
    ///
    /// 0.1.15 가 한국어 literal 53건을 옮겼는데 **시작 폼 절이 통째로
    /// 빠져 있었다**. 시험은 전부 `ko` 로케일로 돌아 번역된 것과 박힌
    /// 것이 **같은 문자열**이라 차이가 0 이었다 — 실물을 영어 로케일로
    /// 띄우고서야 드러났다(화면 절반이 영어, 절반이 한국어).
    ///
    /// **로케일을 바꿔 보는 시험으로는 못 잡는다.** 박힌 문구는 어느
    /// 로케일에서도 그대로 나오므로, *"ko 에서 한국어가 나온다"* 는
    /// 단정이 늘 참이다. 소스에서 **박혀 있는지**를 직접 봐야 한다.
    #[test]
    fn no_korean_is_baked_into_what_the_screen_shows() {
        // 시작 폼은 하위 모듈로 나뉘었다 — 그 파일을 빼면 이 가드가 생긴 바로 그 자리를 안 본다.
        // 각 파일의 시험 모듈 앞까지만 — 시험 본문의 한국어는 단언 문면이다.
        let prod: String = [
            include_str!("../application/session_surface.rs"),
            include_str!("../adapters/driving/view.rs"),
            include_str!("convo.rs"),
            include_str!("../adapters/driving/url.rs"),
            include_str!("../adapters/driving/html.rs"),
            include_str!("../adapters/driving/view/clear_button.rs"),
            include_str!("../adapters/driving/view/start_form.rs"),
            include_str!("../adapters/driving/view/start_style.rs"),
            include_str!("../adapters/driving/view/table.rs"),
            include_str!("../adapters/driving/manage/view.rs"),
        ]
        .iter()
        .map(|src| src.split("#[cfg(test)]").next().unwrap_or(src))
        .collect::<Vec<_>>()
        .join("\n");

        let mut baked = Vec::new();
        for (n, line) in prod.lines().enumerate() {
            let t = line.trim_start();
            // 주석은 뺀다 — 한국어로 적는 것이 이 저장소 규약이다.
            if t.starts_with("//") {
                continue;
            }
            // 로그도 뺀다 — 개발자가 읽는 것이지 화면에 안 나간다.
            //
            // ⚠️ **`client.rs` 의 교훈을 여기 적어 둔다**: 0.1.15 에서
            // 그쪽 21건을 *"로그용"* 으로 볼 뻔했는데 `to_ipc_error` 가
            // IPC 응답으로 옮겨 **전부 화면에 떴다**. 이 파일은 HTML 을
            // 만드는 곳이라 `tracing!` 이 화면으로 새는 길이 없지만,
            // 다른 파일에 이 가드를 넓힐 때는 그 축을 먼저 본다.
            if t.starts_with("tracing::") {
                continue;
            }
            // 문자열 리터럴 안의 한글만 본다.
            let mut in_str = false;
            let mut has_hangul = false;
            for c in line.chars() {
                if c == '"' {
                    in_str = !in_str;
                }
                if in_str && ('\u{ac00}'..='\u{d7a3}').contains(&c) {
                    has_hangul = true;
                }
            }
            if has_hangul {
                baked.push(format!("  {}: {}", n + 1, line.trim()));
            }
        }

        assert!(
            baked.is_empty(),
            "화면에 나가는 한국어가 소스에 박혀 있다({}건) — \
             영어·일본어 로케일에서 그대로 한국어가 나온다:\n{}",
            baked.len(),
            baked.join("\n")
        );
    }

    /// 키를 뽑는 좌변 — **이 plugin 의 소스 전부**다. 손으로 고른 일부만 읽었을 때 새 모듈의 키가
    /// 조용히 검사 밖에 있었다(실측 2026-10-21: 27 파일 중 9 개만 읽었다). 그래서 목록을 손으로 두지 않고
    /// `src/` 를 훑는다 — 파일을 새로 만들거나 옮겨도 좌변에서 빠지지 않는다.
    fn sources() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            let mut entries: Vec<_> = std::fs::read_dir(dir)
                .expect("src 를 못 읽었다")
                .map(|e| e.expect("src 항목").path())
                .collect();
            entries.sort();
            for p in entries {
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let body = std::fs::read_to_string(&p).expect("소스를 못 읽었다");
                    out.push((p.display().to_string(), body));
                }
            }
        }
        let mut out = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut out,
        );
        out
    }

    /// 좌변이 실제로 소스를 읽었나 — 0 건이면 아래 키 대조가 공허하다. 이 파일 자신이 들어 있어야 한다.
    #[test]
    fn the_key_guard_reads_every_module() {
        let got = sources();
        assert!(got.len() >= 60, "소스를 {}개만 읽었다", got.len());
        assert!(
            got.iter()
                .any(|(p, body)| p.ends_with("i18n.rs") && body.contains("fn sources()")),
            "좌변에 i18n.rs 자신이 없다"
        );
    }

    /// **코드가 부르는 키가 lang 파일 셋에 다 있나**(2026-09-28).
    ///
    /// 없으면 `Translator` 가 **키를 그대로** 낸다 — 화면에
    /// `sup.err.need_text` 같은 것이 찍히고, 그것은 컴파일도 시험도
    /// 통과한다. 이 가드만이 그 자리를 본다.
    ///
    /// 세 파일을 다 보는 이유: 하나만 채우면 그 locale 밖 사용자에게만
    /// 키가 노출되어 **국내에서는 안 보이는 결함**이 된다.
    #[test]
    fn every_key_the_code_calls_exists_in_all_locales() {
        // 주석 줄은 뺀다 — 이 파일의 설명문이 `i18n::t("키")` 를 예로 든다.
        let all = sources();
        let src: String = all
            .iter()
            .flat_map(|(_, body)| body.lines())
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        // `i18n::t("키")` · `t1(...)` · `tn(...)` 에서 첫 인자를 뽑는다. `use crate::domain::i18n::t` 로 들여와
        // `t("sup.…")` 로 부르는 자리(관리 탭 등)도 같은 키라 함께 뽑는다.
        let mut keys: Vec<&str> = Vec::new();
        for (pat, _) in [
            ("i18n::t(\"", 0),
            ("i18n::t1(\"", 0),
            ("i18n::tn(\"", 0),
            ("t(\"sup.", 0),
            ("t1(\"sup.", 0),
        ] {
            let mut rest = src.as_str();
            while let Some(i) = rest.find(pat) {
                rest = &rest[i + pat.len()..];
                if let Some(j) = rest.find('"') {
                    keys.push(&rest[..j]);
                }
            }
        }
        keys.sort_unstable();
        keys.dedup();

        // 분모를 먼저 본다 — 추출이 0건이면 아래 루프가 공허하다.
        assert!(
            keys.len() >= 40,
            "키를 {}건만 뽑았다 — 추출식이 깨졌나: {keys:?}",
            keys.len()
        );

        for (locale, body) in [
            ("ko", include_str!("../../lang/ko.toml")),
            ("en", include_str!("../../lang/en.toml")),
            ("ja", include_str!("../../lang/ja.toml")),
        ] {
            for k in &keys {
                // `sup.err.need_text` → 섹션 `[sup.err]` 의 `need_text`.
                let leaf = k.rsplit('.').next().unwrap_or(k);
                assert!(
                    body.contains(&format!("\n{leaf} =")),
                    "{locale}.toml 에 `{k}` 가 없다 — 그 locale 사용자에게 키가 그대로 보인다"
                );
            }
        }
    }

    /// compact 문구가 **요약 언어를 못박는가**. 한국어 문구로 compact 해도
    /// 요약이 영어로 나온 사례가 있다 — 문구가 언어를 말하지 않으면 모델은
    /// 대화 일부(도구 출력·코드)의 언어를 따라간다.
    #[test]
    fn the_compact_prompt_names_the_summary_language() {
        for (locale, body, phrase) in [
            (
                "ko",
                include_str!("../../lang/ko.toml"),
                "같은 한국어로 쓴다",
            ),
            ("en", include_str!("../../lang/en.toml"), "in English"),
            ("ja", include_str!("../../lang/ja.toml"), "同じ日本語で書く"),
        ] {
            // `[sup.default]` 절만 본다 — 설정 창 라벨(`[sup.settings]`)에도 같은 이름의 키가 있다.
            let line = body
                .split("[sup.default]")
                .nth(1)
                .and_then(|s| s.split("\n[").next())
                .unwrap_or_else(|| panic!("{locale}.toml 에 [sup.default] 가 없다"))
                .lines()
                .find(|l| l.starts_with("compact_prompt ="))
                .unwrap_or_else(|| panic!("{locale}.toml 에 compact_prompt 가 없다"));
            assert!(
                line.contains(phrase),
                "{locale}.toml compact_prompt 에 요약 언어 지시({phrase})가 없다: {line}"
            );
        }
    }
}

//! 공식 플러그인 잠금 파일(`official-plugins.lock.toml`)의 가드.
//!
//! 1. 완비: 형식을 읽을 수 있고, 모든 플러그인(`bundle = false` 포함)에 5플랫폼 산출물이 있으며
//!    sha256·파일 이름·URL 형태가 맞다.
//! 2. 계약 호환: 잠금 파일이 가리키는 릴리스가 빌드에 쓴 계약 값이 현재 tasty와 같다.
//!    (a) host_api_version, (b) protocol 크레이트 버전, (c) manifest_version,
//!    (d) Cargo.lock의 epaint 버전, (e) `HEAD:vendor/epaint` 트리 id.
//! 3. 집합: 호스트 `BUILTINS`의 id 집합(두 cfg 판 모두)과 `OFFICIAL_PLUGIN_IDS`가 잠금 파일 전체 id에
//!    tasty에 남는 mesh-demo를 더한 집합과 같다.
//! 4. 배포 집합: 배포 패키지 스크립트(`scripts/lib/bundled-plugins.sh`, `scripts/build-windows.ps1`)는
//!    `fetch-official-plugins.sh`를 `--all` 없이 불러 잠금 파일의 `bundle = true` 항목만 받고, 개발 번들
//!    (`scripts/stage-dev-plugins.sh`의 기본 경로)은 `--all`로 모두 받는다. 잠금 항목의 `bundle`과 매니페스트의
//!    `bundle`이 같은지는 `official_plugin_fixtures.rs`가 픽스처 매니페스트로 본다.
//!
//! 한계: 계약 대조는 버전 값만 본다. 버전을 올리지 않은 protocol schema 변경은 잡지 못한다.
//! tasty 릴리스 사이의 develop에서는 protocol 변경이 `CHANGELOG.md`의 `[Unreleased]`에만 쌓이고
//! 버전은 그대로이므로 이 가드는 그 변경을 원리적으로 보지 못한다. 실제로 잠금 릴리스의 tasty_rev
//! 뒤 편집 결과 회신(Hello `capabilities`, `EditOutcome`)이 버전 0.2.16 그대로 들어왔다. 추가 변형이라
//! 옛 플러그인과 호환되지만, 호환이 아닌 변경도 같은 방식으로 지나갈 수 있다. 트리 id 비교(엄격안)를
//! 쓰지 않은 이유와 대신 지키는 규칙은 docs/dev-guide/plugin-packaging.md의 "계약 호환 가드" 절이다.

use std::collections::BTreeSet;

use tasty_doc_guards::official_plugins::{
    LOCK_FILE, Lock, REFRESH_HINT, check, contract_mismatches, parse_lock, read_host_contract,
};
use tasty_doc_guards::repo_root;

/// 공식 플러그인 레포로 옮기지 않고 tasty에 남는 시범 플러그인.
const STAYS_IN_TASTY: &str = "com.tasty.mesh-demo";

fn lock() -> Lock {
    let path = repo_root().join(LOCK_FILE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}를 읽지 못했다: {e}", path.display()));
    parse_lock(&text)
        .unwrap_or_else(|errors| panic!("{LOCK_FILE}를 읽지 못했다:\n  {}", errors.join("\n  ")))
}

fn locked_ids(lock: &Lock) -> BTreeSet<String> {
    lock.plugins.iter().map(|p| p.id.clone()).collect()
}

/// `const <name>` 정의마다 그 블록(다음 `];`까지)의 `"com.tasty.…"` 문자열 집합을 돌려준다.
/// 줄 주석은 지운다.
fn id_sets_in_const(src: &str, name: &str) -> Vec<BTreeSet<String>> {
    let needle = format!("const {name}:");
    let mut sets = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find(&needle) {
        let block = &rest[at..];
        let end = block
            .find("\n];")
            .unwrap_or_else(|| panic!("{name} 블록의 끝을 못 찾았다"));
        let mut ids = BTreeSet::new();
        for line in block[..end].lines() {
            let code = line.split("//").next().unwrap_or("");
            let mut parts = code.split('"');
            parts.next();
            while let Some(lit) = parts.next() {
                if lit.starts_with("com.tasty.") {
                    ids.insert(lit.to_string());
                }
                parts.next();
            }
        }
        sets.push(ids);
        rest = &block[end..];
    }
    sets
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel}를 읽지 못했다: {e}"))
}

/// 스크립트 원문에서 `fetch-official-plugins.sh`를 실행하는 명령(단어 목록). 주석과 따옴표 안 안내 문자열
/// (`Write-Error "... fetch-official-plugins.sh."`)은 실행이 아니라 빠진다.
/// PowerShell 원문은 Bash 따옴표 규칙이 다르므로 물리적 줄마다 따로 읽는다(줄 연속을 쓰지 않는다).
fn fetch_calls(rel: &str) -> Vec<Vec<String>> {
    let text = read(rel);
    if rel.ends_with(".ps1") {
        text.lines()
            .flat_map(|l| fetch_calls_in_dialect(l, Dialect::PowerShell))
            .collect()
    } else {
        fetch_calls_in(&text)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Dialect {
    Bash,
    /// 한 줄씩 넘긴다. 홀로 쓴 `&`는 호출 연산자(`& $Bash`)라 단어로 남긴다.
    PowerShell,
}

/// 원문을 Bash처럼 논리적 명령으로 나누고 각 명령을 단어로 나눈다.
/// - 따옴표 밖 줄바꿈·`;`·`|`·`||`·`&&`·`&`에서 명령이 끝난다(PowerShell의 홀로 쓴 `&`는 단어).
///   따옴표 밖과 큰따옴표 안의 `\` 바로 뒤 줄바꿈은 줄 연속이라 둘 다 지운다.
/// - 큰·작은따옴표 안의 공백·줄바꿈은 단어를 나누지 않고 따옴표는 벗긴다. 따옴표 밖 `\`는 다음 글자를
///   그대로 둔다. 큰따옴표 안 `\`는 `$`·`` ` ``·`"`·`\` 앞에서만 이스케이프이고 그 밖에서는 글자로 남는다
///   (`"\--all"`은 `\--all`).
/// - 단어 첫머리의 따옴표 밖 `#`부터 줄 끝까지는 주석이다.
/// - 변수 확장은 하지 않는다(`"$stage"`는 글자 그대로 `$stage`).
fn commands_in(text: &str, dialect: Dialect) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    let end_word = |words: &mut Vec<String>, word: &mut String, in_word: &mut bool| {
        if *in_word {
            words.push(std::mem::take(word));
            *in_word = false;
        }
    };
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' => match chars.peek() {
                Some(&n @ ('$' | '`' | '"' | '\\')) => {
                    word.push(n);
                    chars.next();
                }
                Some('\n') => {
                    chars.next();
                }
                _ => word.push(c),
            },
            Some(_) => word.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_word = true;
            }
            None if c == '\\' => match chars.next() {
                Some('\n') | None => {}
                Some(n) => {
                    word.push(n);
                    in_word = true;
                }
            },
            None if c == '#' && !in_word => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
                end_word(&mut words, &mut word, &mut in_word);
                commands.push(std::mem::take(&mut words));
            }
            None if c == '\n'
                || c == ';'
                || c == '|'
                || (c == '&' && (dialect == Dialect::Bash || chars.peek() == Some(&'&'))) =>
            {
                if (c == '&' && chars.peek() == Some(&'&'))
                    || (c == '|' && chars.peek() == Some(&'|'))
                {
                    chars.next();
                }
                end_word(&mut words, &mut word, &mut in_word);
                commands.push(std::mem::take(&mut words));
            }
            None if c.is_whitespace() => end_word(&mut words, &mut word, &mut in_word),
            None => {
                word.push(c);
                in_word = true;
            }
        }
    }
    end_word(&mut words, &mut word, &mut in_word);
    commands.push(words);
    commands.retain(|c| !c.is_empty());
    commands
}

/// 한 줄 호출의 첫 명령 단어. 회귀 입력을 짧게 쓰기 위한 것이다.
fn call_args(call: &str) -> Vec<String> {
    commands_in(call, Dialect::Bash)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// 배포 스크립트의 실제 호출 꼴. Bash는 `if ! ./scripts/fetch-official-plugins.sh ...`,
/// PowerShell은 `& $Bash ./scripts/fetch-official-plugins.sh ...`다.
fn is_package_fetch_call(rel: &str, args: &[String]) -> bool {
    const SCRIPT: &str = "./scripts/fetch-official-plugins.sh";
    match rel {
        "scripts/lib/bundled-plugins.sh" => {
            matches!(args, [a, b, script, ..] if a == "if" && b == "!" && script == SCRIPT)
        }
        "scripts/build-windows.ps1" => {
            matches!(args, [a, b, script, ..] if a == "&" && b == "$Bash" && script == SCRIPT)
        }
        _ => false,
    }
}

/// 개발 스테이징이 쓰는 실제 호출 꼴: `bash "$root/scripts/fetch-official-plugins.sh" ...`.
fn is_staging_fetch_call(args: &[String]) -> bool {
    matches!(args, [bash, script, ..] if bash == "bash" && script.ends_with("/scripts/fetch-official-plugins.sh"))
}

/// fetch 스크립트 뒤의 독립 옵션. `scripts/fetch-official-plugins.sh`의 파서처럼 값을 받는 옵션은
/// 다음 단어를 값으로 소비하므로 `--cache --all`의 `--all`은 옵션이 아니다.
fn fetch_flags(args: &[String]) -> Vec<&str> {
    const TAKES_VALUE: [&str; 4] = ["--platform", "--out", "--lock", "--cache"];
    let Some(script) = args
        .iter()
        .position(|a| a.ends_with("fetch-official-plugins.sh"))
    else {
        return Vec::new();
    };
    let mut flags = Vec::new();
    let mut rest = args[script + 1..].iter();
    while let Some(a) = rest.next() {
        if TAKES_VALUE.contains(&a.as_str()) {
            rest.next();
        } else {
            flags.push(a.as_str());
        }
    }
    flags
}

fn has_word(args: &[String], want: &str) -> bool {
    args.iter().any(|a| a == want)
}

/// `scripts/stage-dev-plugins.sh`의 기본 분기(`--from-checkout`이 아닐 때) 본문. 맨 앞 열의
/// `if [[ "$from_checkout" -eq 1 ]]; then` 뒤 맨 앞 열 `else`부터 맨 앞 열 `fi`까지다.
fn default_staging_branch(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "if [[ \"$from_checkout\" -eq 1 ]]; then")?;
    let open = start + lines[start..].iter().position(|l| *l == "else")?;
    let close = open + lines[open..].iter().position(|l| *l == "fi")?;
    Some(lines[open + 1..close].join("\n"))
}

fn fetch_calls_in(text: &str) -> Vec<Vec<String>> {
    fetch_calls_in_dialect(text, Dialect::Bash)
}

/// 스크립트 이름으로 끝나는 단어가 있는 명령. 안내만 내는 명령(`echo`·`printf`·`Write-Error`·`Write-Host`)의
/// 인자는 실행이 아니라 뺀다. 그 밖의 명령은 실행 접두(`if !`·`bash`·`env` 등)와 관계없이 후보로 남겨
/// 호출 꼴 검사가 판정한다.
fn fetch_calls_in_dialect(text: &str, dialect: Dialect) -> Vec<Vec<String>> {
    const MESSAGE: [&str; 4] = ["echo", "printf", "Write-Error", "Write-Host"];
    commands_in(text, dialect)
        .into_iter()
        .filter(|c| !MESSAGE.contains(&c[0].as_str()))
        .filter(|c| c.iter().any(|w| w.ends_with("fetch-official-plugins.sh")))
        .collect()
}

/// fetch 호출이 실제로 받는지: 도움말(`-h`·`--help`)로 끝나지 않고 `--platform`·`--out`에 값이 있다.
fn fetch_really_runs(args: &[String]) -> bool {
    let flags = fetch_flags(args);
    if flags.iter().any(|f| matches!(*f, "-h" | "--help")) {
        return false;
    }
    let Some(script) = args
        .iter()
        .position(|a| a.ends_with("fetch-official-plugins.sh"))
    else {
        return false;
    };
    let rest = &args[script + 1..];
    ["--platform", "--out"].iter().all(|opt| {
        rest.windows(2)
            .any(|w| w[0] == *opt && !w[1].is_empty() && !w[1].starts_with("--"))
    })
}

#[test]
fn the_lock_file_is_complete() {
    let lock = lock();
    let problems = check(&lock);
    assert!(
        problems.is_empty(),
        "{LOCK_FILE}가 완비되지 않았다. 손으로 고치지 말고 official-plugins generate로 다시 만든다:\n  {}",
        problems.join("\n  ")
    );
    assert!(!lock.plugins.is_empty(), "{LOCK_FILE}에 플러그인이 없다");
}

#[test]
fn the_lock_matches_the_host_contract() {
    let lock = lock();
    let host = read_host_contract(&repo_root()).unwrap_or_else(|e| panic!("{e}"));
    let mismatches = contract_mismatches(&lock.release, &host);
    assert!(
        mismatches.is_empty(),
        "잠금 파일의 릴리스 {} {}(tasty_rev {})는 현재 tasty와 계약이 다르다:\n  {}\n{REFRESH_HINT}",
        lock.release.repository,
        lock.release.tag,
        lock.release.tasty_rev,
        mismatches.join("\n  ")
    );
}

#[test]
fn builtins_are_the_locked_plugins_plus_mesh_demo() {
    let lock = lock();
    let mut want = locked_ids(&lock);
    want.insert(STAYS_IN_TASTY.to_string());

    let builtin = read("crates/tasty-host-plugin/src/builtin.rs");
    let sets = id_sets_in_const(&builtin, "BUILTINS");
    assert_eq!(
        sets.len(),
        2,
        "BUILTINS 정의가 두 cfg 판(windows, not windows)이 아니다. 판독 전제를 확인한다."
    );
    for (i, set) in sets.iter().enumerate() {
        assert_eq!(
            set,
            &want,
            "BUILTINS {}번째 정의의 id 집합이 잠금 파일 id + {STAYS_IN_TASTY}와 다르다",
            i + 1
        );
    }

    let official = read("crates/tasty-host-plugin/src/official.rs");
    let sets = id_sets_in_const(&official, "OFFICIAL_PLUGIN_IDS");
    assert_eq!(sets.len(), 1, "OFFICIAL_PLUGIN_IDS 정의가 하나가 아니다");
    assert_eq!(
        sets[0], want,
        "OFFICIAL_PLUGIN_IDS가 잠금 파일 id + {STAYS_IN_TASTY}와 다르다"
    );
}

/// 배포 패키지는 잠금 파일의 `bundle = true` 항목만 싣고 개발 번들은 모두 싣는다. 받을 집합은
/// `fetch-official-plugins.sh`가 잠금 파일에서 고르므로 호출 옵션 `--all` 유무가 배포 집합을 정한다.
#[test]
fn the_package_scripts_fetch_only_bundled_entries_and_dev_staging_fetches_all() {
    for rel in [
        "scripts/lib/bundled-plugins.sh",
        "scripts/build-windows.ps1",
    ] {
        let calls = fetch_calls(rel);
        assert!(!calls.is_empty(), "{rel}에서 fetch 호출을 찾지 못했다");
        for call in &calls {
            assert!(
                is_package_fetch_call(rel, call) && fetch_really_runs(call),
                "{rel}의 fetch 명령이 --platform·--out 으로 받는 호출 꼴이 아니다: {call:?}"
            );
            assert!(
                !fetch_flags(call).contains(&"--all"),
                "{rel}의 배포 fetch가 --all 로 bundle = false 항목까지 싣는다: {call:?}"
            );
        }
    }
    let branch = default_staging_branch(&read("scripts/stage-dev-plugins.sh")).expect(
        "scripts/stage-dev-plugins.sh 에서 --from-checkout 이 아닌 기본 분기를 찾지 못했다",
    );
    let staging = fetch_calls_in(&branch);
    assert_eq!(
        staging.len(),
        1,
        "기본 분기의 fetch 호출이 하나가 아니다: {staging:?}"
    );
    let args = &staging[0];
    assert!(
        is_staging_fetch_call(args) && fetch_really_runs(args),
        "scripts/stage-dev-plugins.sh 기본 분기의 fetch 명령이 bash 로 스크립트를 부르는 호출이 아니다: {args:?}"
    );
    assert!(
        fetch_flags(args).contains(&"--all"),
        "scripts/stage-dev-plugins.sh 기본 분기가 --all 로 받지 않는다: {args:?}"
    );
}

/// 주석에만 남은 `--all`은 인자가 아니고, 기본 분기 밖의 호출은 기본 분기로 세지 않는다.
#[test]
fn the_staging_check_reads_only_real_arguments_of_the_default_branch() {
    assert!(!has_word(
        &call_args("bash fetch-official-plugins.sh --out x # --all"),
        "--all"
    ));
    assert!(has_word(
        &call_args("bash fetch-official-plugins.sh --out x --all # 모두"),
        "--all"
    ));
    assert!(
        !has_word(
            &call_args("bash fetch-official-plugins.sh --out \"$stage\"\t# --all # 설명"),
            "--all"
        ),
        "탭 뒤 주석의 --all 을 인자로 읽었다"
    );
    let real = "bash \"$root/scripts/fetch-official-plugins.sh\" --platform \"$platform\" --out \"$stage\" --all";
    assert!(is_staging_fetch_call(&call_args(real)));
    assert!(fetch_flags(&call_args(real)).contains(&"--all"));
    for (what, tail) in [
        ("값 자리", "--cache --all"),
        ("따옴표 안 공백이 든 값", "--cache \"$stage --all cache\""),
    ] {
        assert!(
            !fetch_flags(&call_args(&real.replacen("--all", tail, 1))).contains(&"--all"),
            "{what}의 --all 을 옵션으로 읽었다"
        );
    }
    assert!(
        fetch_flags(&call_args(&real.replacen("--all", "\"--all\"", 1))).contains(&"--all"),
        "따옴표로 쓴 실제 옵션 --all 을 놓쳤다"
    );
    assert!(
        !fetch_flags(&call_args(&real.replacen("--all", "\"\\--all\"", 1))).contains(&"--all"),
        "큰따옴표 안 \\--all 을 --all 로 읽었다(Bash는 백슬래시를 남긴다)"
    );
    let package = "if ! ./scripts/fetch-official-plugins.sh --platform \"$platform\" --out \"$BUNDLED_PLUGINS_DIR\"; then";
    let rel = "scripts/lib/bundled-plugins.sh";
    assert!(is_package_fetch_call(rel, &call_args(package)));
    assert!(
        !is_package_fetch_call(rel, &call_args(&package.replacen("if !", "if ! :", 1))),
        "무동작 명령을 배포 fetch 호출로 보았다"
    );
    assert!(
        !fetch_flags(&call_args(&package.replacen(
            "--out",
            "--cache \"--all\" --out",
            1
        )))
        .contains(&"--all"),
        "값 자리의 \"--all\" 을 배포 옵션으로 읽었다"
    );
    assert!(
        fetch_flags(&call_args(&package.replacen("; then", " --all; then", 1))).contains(&"--all"),
        "배포 호출의 실제 --all 을 놓쳤다"
    );
    let ps = "& $Bash ./scripts/fetch-official-plugins.sh --platform $PluginPlatform --out target/bundled-plugins";
    assert!(is_package_fetch_call(
        "scripts/build-windows.ps1",
        &fetch_calls_in_dialect(ps, Dialect::PowerShell)[0]
    ));
    assert!(
        !is_staging_fetch_call(&call_args(&real.replacen("bash", ":", 1))),
        "무동작 명령을 fetch 호출로 보았다"
    );
    let text = "if [[ \"$from_checkout\" -eq 1 ]]; then\n    bash scripts/fetch-official-plugins.sh --all\nelse\n    bash scripts/fetch-official-plugins.sh --out y\nfi\n";
    let branch = default_staging_branch(text).expect("기본 분기");
    assert_eq!(
        fetch_calls_in(&branch),
        [call_args("bash scripts/fetch-official-plugins.sh --out y")],
        "다른 분기의 호출을 기본 분기로 읽었다"
    );
    // 논리적 명령 경계: 같은 줄의 두 번째 fetch, 뒤따르는 다른 명령의 --all, 줄 연속으로 넘긴 --all.
    let second = package.replacen(
        "; then",
        "; ./scripts/fetch-official-plugins.sh --platform x --out y --all; then",
        1,
    );
    let calls = fetch_calls_in(&second);
    assert_eq!(
        calls.len(),
        2,
        "같은 줄의 두 번째 fetch 를 세지 않았다: {calls:?}"
    );
    assert!(
        fetch_flags(&calls[1]).contains(&"--all"),
        "두 번째 fetch 의 --all 을 놓쳤다"
    );
    assert!(
        !fetch_flags(&fetch_calls_in(&real.replacen(" --all", " && : --all", 1))[0])
            .contains(&"--all"),
        "다른 명령의 --all 을 fetch 옵션으로 읽었다"
    );
    assert!(
        !fetch_flags(&fetch_calls_in(&real.replacen(" --all", " & : --all", 1))[0])
            .contains(&"--all"),
        "Bash 의 & 뒤 명령의 --all 을 fetch 옵션으로 읽었다"
    );
    let ps = "& $Bash ./scripts/fetch-official-plugins.sh --platform $P --out target/x";
    let calls = fetch_calls_in_dialect(ps, Dialect::PowerShell);
    assert_eq!(
        calls.len(),
        1,
        "PowerShell 호출 연산자에서 명령을 끊었다: {calls:?}"
    );
    assert!(is_package_fetch_call(
        "scripts/build-windows.ps1",
        &calls[0]
    ));
    let quoted = real.replacen(" --all", " \"--a\\\nll\"", 1);
    assert!(
        fetch_flags(&fetch_calls_in(&quoted)[0]).contains(&"--all"),
        "큰따옴표 안 줄 연속으로 이은 --all 을 놓쳤다"
    );
    for help in ["--help --all", "-h"] {
        let call = &fetch_calls_in(&real.replacen("--all", help, 1))[0];
        assert!(
            !fetch_really_runs(call),
            "도움말로 끝나는 호출을 받기로 보았다: {help}"
        );
    }
    assert!(fetch_really_runs(&call_args(real)));
    assert!(
        !fetch_really_runs(&call_args(
            "if ! ./scripts/fetch-official-plugins.sh --help; then"
        )),
        "인자를 --help 로 바꾼 배포 호출을 받기로 보았다"
    );
    let windows = "Write-Error \"Install Git for Windows to run scripts/fetch-official-plugins.sh\"\n& $Bash ./scripts/fetch-official-plugins.sh --platform $P --out target/x\n";
    let calls: Vec<_> = windows
        .lines()
        .flat_map(|l| fetch_calls_in_dialect(l, Dialect::PowerShell))
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "마침표 없는 안내문을 fetch 호출로 셌다: {calls:?}"
    );
    let continued = real.replacen(" --all", " \\\n    --all", 1);
    let calls = fetch_calls_in(&continued);
    assert_eq!(calls.len(), 1, "줄 연속을 명령 둘로 나눴다: {calls:?}");
    assert!(
        fetch_flags(&calls[0]).contains(&"--all"),
        "줄 연속으로 넘긴 --all 을 놓쳤다"
    );
}

#[test]
fn the_fetch_call_reader_skips_comments_and_messages() {
    let text = "# fetch-official-plugins.sh --all 은 설명이다\n\
                Write-Error \"Install Git to run scripts/fetch-official-plugins.sh.\"\n\
                    bash scripts/fetch-official-plugins.sh --platform x --out y --all\n\
                echo done\n";
    assert_eq!(
        fetch_calls_in(text),
        [call_args(
            "bash scripts/fetch-official-plugins.sh --platform x --out y --all"
        )]
    );
}

#[test]
fn the_const_reader_sees_every_literal_and_skips_comments() {
    let src = "const X: &[S] = &[\n    S { id: \"com.tasty.a\" },\n    // S { id: \"com.tasty.gone\" },\n    S { id: \"com.tasty.b\", other: \"x\" },\n];\nconst X: &[S] = &[\n    S { id: \"com.tasty.c\" },\n];\n";
    let sets = id_sets_in_const(src, "X");
    assert_eq!(sets.len(), 2);
    assert_eq!(
        sets[0],
        ["com.tasty.a", "com.tasty.b"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(sets[1].len(), 1);
}

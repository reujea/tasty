//! `scripts/build-windows.ps1`의 `Get-NoticeSetFiles`가 셸 `notice_set_files`와 같은 규칙을
//! 지키는지 소스에서 확인한다. Linux CI에는 PowerShell이 없어 함수를 실행하지 못하므로,
//! 규칙을 이루는 세 요소가 함수 블록의 코드(주석 제외)에 있는지 본다.
//! - `Get-ChildItem` 명령에 `-Force`가 있다. 없으면 Hidden 속성 폴더·링크가 열거에서 빠져 거부를 피한다.
//! - 이름이 점으로 시작하는지 보는 `if` 블록 안에 `exit 1`이 있다.
//! - 폴더(`PSIsContainer`)·링크(`ReparsePoint`)를 보는 `if` 블록 안에 `exit 1`이 있다.
//!
//! PowerShell 파서가 아니다. `#` 줄 주석과 `<# #>` 블록 주석을 빼고, 따옴표 문자열 안의 괄호와
//! `exit 1`은 코드로 세지 않으며, 대소문자를 가리지 않는다. here-string과 `$(...)` 안의 따옴표는
//! 다루지 않는다.
//!
//! 이 가드는 요소가 있는지와 `exit 1`이 그 조건의 블록에 속하는지만 본다. 보지 않는 것:
//! - 중첩되거나 도달할 수 없는 `exit 1`(블록 안 다른 `if`나 `return` 뒤의 `exit 1`도 인정한다)
//! - `-Force:$false`처럼 값으로 끈 스위치, 다른 명령의 인자 안에 놓인 `-Force`
//! - `Get-ChildItem` 앞뒤의 `Where-Object` 같은 필터가 숨김 항목을 걸러 내는 경우
//! - 조건 연산자 변경(예: `-or`를 `-and`로, `StartsWith`를 부정)
//!
//! 목적은 편집 중 실수로 생긴 회귀를 잡는 것이지 의도적인 우회를 막는 것이 아니다.
//! 실행 결과(실제로 Hidden 항목을 거부하는지)는 보장하지 않으며 그 동작은 Windows에서
//! 함수를 실행해 확인한다.

const SCRIPT: &str = "scripts/build-windows.ps1";
const NO_FUNCTION: &str = "Get-NoticeSetFiles 함수를 찾지 못했다";
const NO_FORCE: &str = "LICENSES 를 읽는 Get-ChildItem 에 -Force 가 없다";
const NO_DOT_EXIT: &str = "점으로 시작하는 이름을 exit 1 로 거부하지 않는다";
const NO_CONTAINER_EXIT: &str = "폴더·링크를 exit 1 로 거부하지 않는다";
const FUNCTION: &str = "get-noticesetfiles";

/// 같은 길이의 두 사본과 표시. `code`는 주석을 공백으로 지우고 소문자로 바꾼 것,
/// `shape`는 거기에 더해 문자열 내용까지 공백으로 지운 것이다. 줄바꿈은 그대로 둔다.
/// `quoted[i]`는 i가 따옴표 문자열의 내용(따옴표 자체 제외)인지다.
struct Views {
    code: Vec<char>,
    shape: Vec<char>,
    quoted: Vec<bool>,
}

fn views(src: &str) -> Views {
    let s: Vec<char> = src.chars().map(|c| c.to_ascii_lowercase()).collect();
    let mut code = s.clone();
    let mut shape = s.clone();
    let mut quoted = vec![false; s.len()];
    let blank = |v: &mut Vec<char>, i: usize| {
        if v[i] != '\n' {
            v[i] = ' ';
        }
    };
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            '<' if s.get(i + 1) == Some(&'#') => {
                let start = i;
                i += 2;
                while i < s.len() && !(s[i] == '#' && s.get(i + 1) == Some(&'>')) {
                    i += 1;
                }
                let end = (i + 2).min(s.len());
                for k in start..end {
                    blank(&mut code, k);
                    blank(&mut shape, k);
                }
                i = end;
            }
            '#' => {
                while i < s.len() && s[i] != '\n' {
                    blank(&mut code, i);
                    blank(&mut shape, i);
                    i += 1;
                }
            }
            q @ ('\'' | '"') => {
                i += 1;
                while i < s.len() {
                    if q == '"' && s[i] == '`' {
                        blank(&mut shape, i);
                        quoted[i] = true;
                        i += 1;
                    } else if s[i] == q {
                        // 같은 따옴표 두 개는 문자열 안의 따옴표 하나다.
                        if s.get(i + 1) == Some(&q) {
                            blank(&mut shape, i);
                            blank(&mut shape, i + 1);
                            quoted[i] = true;
                            quoted[i + 1] = true;
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    if i < s.len() {
                        blank(&mut shape, i);
                        quoted[i] = true;
                    }
                    i += 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    Views {
        code,
        shape,
        quoted,
    }
}

/// `shape[open]`의 여는 괄호와 짝인 닫는 괄호 위치.
fn matching(shape: &[char], open: usize) -> Option<usize> {
    let (o, c) = match shape[open] {
        '{' => ('{', '}'),
        '(' => ('(', ')'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (k, &ch) in shape.iter().enumerate().skip(open) {
        if ch == o {
            depth += 1;
        } else if ch == c {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// `word`가 낱말로 나타나는 위치들(앞뒤가 낱말 문자가 아니다).
fn word_positions(shape: &[char], range: std::ops::Range<usize>, word: &str) -> Vec<usize> {
    let w: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    let mut i = range.start;
    while i + w.len() <= range.end {
        if shape[i..i + w.len()] == w[..]
            && (i == 0 || !is_word(shape[i - 1]))
            && shape.get(i + w.len()).is_none_or(|&c| !is_word(c))
        {
            out.push(i);
        }
        i += 1;
    }
    out
}

fn skip_ws(shape: &[char], mut i: usize) -> usize {
    while i < shape.len() && shape[i].is_whitespace() {
        i += 1;
    }
    i
}

/// `function Get-NoticeSetFiles` 블록의 안쪽 범위(여는 `{` 다음부터 닫는 `}` 전까지).
fn function_block(v: &Views) -> Option<std::ops::Range<usize>> {
    for at in word_positions(&v.shape, 0..v.shape.len(), "function") {
        let name = skip_ws(&v.shape, at + "function".len());
        let end = name + FUNCTION.len();
        if end > v.shape.len() || v.shape[name..end].iter().collect::<String>() != FUNCTION {
            continue;
        }
        if v.shape.get(end).is_some_and(|&c| is_word(c)) {
            continue;
        }
        let open = skip_ws(&v.shape, end);
        if v.shape.get(open) != Some(&'{') {
            continue;
        }
        let close = matching(&v.shape, open)?;
        return Some(open + 1..close);
    }
    None
}

/// 범위 안의 `if (조건) { 블록 }`을 (조건 코드, 블록 코드)로 모은다. 공백은 뺀다.
fn if_blocks(v: &Views, range: std::ops::Range<usize>) -> Vec<(String, String)> {
    // 조건은 문자열 밖 공백만 지운다. 문자열 인자 값(예: ". ")은 그대로 남긴다.
    let squash = |r: std::ops::Range<usize>| -> String {
        r.filter(|&k| v.quoted[k] || !v.code[k].is_whitespace())
            .map(|k| v.code[k])
            .collect()
    };
    let mut out = Vec::new();
    for at in word_positions(&v.shape, range.clone(), "if") {
        let open = skip_ws(&v.shape, at + 2);
        if v.shape.get(open) != Some(&'(') {
            continue;
        }
        let Some(close) = matching(&v.shape, open) else {
            continue;
        };
        let brace = skip_ws(&v.shape, close + 1);
        if v.shape.get(brace) != Some(&'{') || brace >= range.end {
            continue;
        }
        let Some(end) = matching(&v.shape, brace) else {
            continue;
        };
        // 블록은 문자열을 가린 shape에서 자른다. 문자열 안의 "exit 1"은 문장이 아니다.
        // exit 1 판정은 원래 공백이 필요하므로 공백을 하나로 줄여 둔다.
        let block: String = v.shape[brace + 1..end].iter().collect();
        let block = block.split_whitespace().collect::<Vec<_>>().join(" ");
        out.push((squash(open..close + 1), block));
    }
    out
}

/// 블록에 `exit 1` 문장이 있는가(`exit`와 `1` 사이 공백 여러 개, 뒤의 `;`·`}`·끝 허용).
fn has_exit_one(block: &str) -> bool {
    let b: Vec<char> = block.chars().collect();
    word_positions(&b, 0..b.len(), "exit")
        .into_iter()
        .any(|at| {
            let n = skip_ws(&b, at + 4);
            n > at + 4
                && b.get(n) == Some(&'1')
                && b.get(n + 1).is_none_or(|&c| !c.is_ascii_digit())
        })
}

/// 규칙 위반 목록. 비어 있으면 세 요소가 모두 있다.
fn violations(src: &str) -> Vec<&'static str> {
    let v = views(src);
    let Some(body) = function_block(&v) else {
        return vec![NO_FUNCTION];
    };
    let mut out = Vec::new();
    // 명령은 줄 끝·파이프·닫는 괄호에서 끝난다고 본다.
    let forced = word_positions(&v.shape, body.clone(), "get-childitem")
        .into_iter()
        .any(|at| {
            let end = (at..body.end)
                .find(|&k| matches!(v.shape[k], '\n' | '|' | ')' | ';'))
                .unwrap_or(body.end);
            let cmd: String = v.code[at..end].iter().collect();
            let cmd_shape: Vec<char> = v.shape[at..end].to_vec();
            !word_positions(&cmd_shape, 0..cmd_shape.len(), "-force").is_empty()
                && cmd.contains("licenses")
        });
    if !forced {
        out.push(NO_FORCE);
    }
    let ifs = if_blocks(&v, body);
    let dot = ifs.iter().any(|(cond, block)| {
        (cond.contains(".startswith(\".\")") || cond.contains(".startswith('.')"))
            && has_exit_one(block)
    });
    if !dot {
        out.push(NO_DOT_EXIT);
    }
    let container = ifs.iter().any(|(cond, block)| {
        cond.contains("psiscontainer") && cond.contains("reparsepoint") && has_exit_one(block)
    });
    if !container {
        out.push(NO_CONTAINER_EXIT);
    }
    out
}

#[test]
fn get_notice_set_files_keeps_the_shell_rule() {
    let root = tasty_doc_guards::repo_root();
    let src = std::fs::read_to_string(root.join(SCRIPT))
        .unwrap_or_else(|e| panic!("{SCRIPT} 을 읽지 못했다: {e}"));
    let v = violations(&src);
    assert!(
        v.is_empty(),
        "{SCRIPT} 의 Get-NoticeSetFiles 가 scripts/lib/notice-set.sh 와 같은 규칙을 지키지 않는다: {v:?}"
    );
}

const GOOD: &str = r#"function Get-NoticeSetFiles {
    $files = @("LICENSE", "THIRD_PARTY_LICENSES.md")
    $entries = @(Get-ChildItem -Force -Path "LICENSES" | Sort-Object Name)
    foreach ($e in $entries) {
        if ($e.Name.StartsWith(".")) {
            Write-Error "LICENSES\$($e.Name) is a hidden entry; LICENSES\ holds licence texts only"
            exit 1
        }
        if ($e.PSIsContainer -or ($e.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            Write-Error "LICENSES\$($e.Name) is not a regular file"
            exit 1
        }
    }
    return $files
}

function Stage-Notice([string]$Dest) {
    foreach ($f in Get-NoticeSetFiles) { Copy-Item $f -Destination (Join-Path $Dest $f) }
}
"#;

const DOT_IF: &str = r#"        if ($e.Name.StartsWith(".")) {
            Write-Error "LICENSES\$($e.Name) is a hidden entry; LICENSES\ holds licence texts only"
            exit 1
        }
"#;

fn swap(from: &str, to: &str) -> String {
    assert!(GOOD.contains(from), "본보기에 없는 문자열: {from}");
    GOOD.replacen(from, to, 1)
}

#[test]
fn the_sample_passes() {
    assert!(violations(GOOD).is_empty(), "{:?}", violations(GOOD));
}

#[test]
fn regressions_are_reported() {
    let cases: Vec<(&str, String, Vec<&str>)> = vec![
        // m4: -Force 제거
        (
            "no force",
            swap("Get-ChildItem -Force -Path", "Get-ChildItem -Path"),
            vec![NO_FORCE],
        ),
        // p1: 꼬리 주석에만 -Force
        (
            "trailing comment",
            swap(
                "@(Get-ChildItem -Force -Path \"LICENSES\" | Sort-Object Name)",
                "@(Get-ChildItem -Path \"LICENSES\" | Sort-Object Name)  # was Get-ChildItem -Force LICENSES",
            ),
            vec![NO_FORCE],
        ),
        // p2: 블록 주석 안에만 -Force
        (
            "block comment",
            swap(
                "    $entries = @(Get-ChildItem -Force -Path",
                "    <#\n    $old = @(Get-ChildItem -Force -Path \"LICENSES\")\n    #>\n    $entries = @(Get-ChildItem -Path",
            ),
            vec![NO_FORCE],
        ),
        // 문자열 안의 -Force 는 명령의 매개변수가 아니다.
        (
            "force inside a string",
            swap(
                "Get-ChildItem -Force -Path \"LICENSES\"",
                "Get-ChildItem -Path \"LICENSES -Force\"",
            ),
            vec![NO_FORCE],
        ),
        // 함수 끝 } 앞 공백 + 다음 함수에만 -Force
        (
            "next function holds -Force",
            swap(
                "Get-ChildItem -Force -Path \"LICENSES\" | Sort-Object Name)",
                "Get-ChildItem -Path \"LICENSES\" | Sort-Object Name)",
            )
            .replacen("    return $files\n}\n", "    return $files\n  }\n", 1)
            .replacen(
                "    foreach ($f in Get-NoticeSetFiles)",
                "    $x = @(Get-ChildItem -Force -Path \"LICENSES\")\n    foreach ($f in Get-NoticeSetFiles)",
                1,
            ),
            vec![NO_FORCE],
        ),
        // p3: 점 항목을 조용히 건너뛰는 한 줄 블록
        (
            "one-line continue",
            swap(DOT_IF, "        if ($e.Name.StartsWith(\".\")) { continue }\n"),
            vec![NO_DOT_EXIT],
        ),
        // 문자열 안에만 exit 1 이 있는 한 줄 블록
        (
            "exit 1 inside a string",
            swap(
                DOT_IF,
                "        if ($e.Name.StartsWith(\".\")) { Write-Warning \"skip, would exit 1\"; continue }\n",
            ),
            vec![NO_DOT_EXIT],
        ),
        // 점 뒤에 공백이 붙은 인자는 다른 조건이다.
        (
            "dot and space in double quotes",
            swap("StartsWith(\".\")", "StartsWith(\". \")"),
            vec![NO_DOT_EXIT],
        ),
        (
            "dot and space in single quotes",
            swap("StartsWith(\".\")", "StartsWith('. ')"),
            vec![NO_DOT_EXIT],
        ),
        // 여러 줄 continue (점 블록)
        (
            "multi-line continue",
            swap(
                "            exit 1\n        }\n        if ($e.PSIsContainer",
                "            continue\n        }\n        if ($e.PSIsContainer",
            ),
            vec![NO_DOT_EXIT],
        ),
        // 점 거부 블록 제거
        ("no dot refusal", swap(DOT_IF, ""), vec![NO_DOT_EXIT]),
        // 폴더·링크 블록의 exit 1 을 continue 로
        (
            "container continue",
            swap(
                "is not a regular file\"\n            exit 1",
                "is not a regular file\"\n            continue",
            ),
            vec![NO_CONTAINER_EXIT],
        ),
        (
            "missing function",
            "function Other {\n}\n".to_string(),
            vec![NO_FUNCTION],
        ),
    ];
    for (label, src, want) in cases {
        assert_eq!(violations(&src), want, "{label}:\n{src}");
    }
}

#[test]
fn equivalent_spellings_pass() {
    let cases = [
        // f1: 소문자 매개변수·명령
        (
            "lower case",
            swap("Get-ChildItem -Force", "get-childitem -force"),
        ),
        // f2: 작은따옴표
        (
            "single quotes",
            swap("StartsWith(\".\")", "StartsWith('.')"),
        ),
        // f3: 한 줄 블록
        (
            "one-line refusal",
            swap(
                DOT_IF,
                "        if ($e.Name.StartsWith(\".\")) { Write-Error \"hidden\"; exit 1 }\n",
            ),
        ),
        // f4: 소문자 함수 이름
        (
            "lower case function",
            swap(
                "function Get-NoticeSetFiles {",
                "function get-noticesetfiles {",
            ),
        ),
        // f5: 여는 중괄호가 다음 줄
        (
            "brace on the next line",
            swap(
                "function Get-NoticeSetFiles {",
                "function Get-NoticeSetFiles\n{",
            ),
        ),
        // f6: CRLF·탭 들여쓰기
        (
            "crlf and tabs",
            GOOD.replace('\n', "\r\n").replace("    ", "\t"),
        ),
        // exit 와 1 사이 공백 여러 개, 뒤 ;
        (
            "spaced exit",
            swap(
                "            exit 1\n        }\n        if ($e.PSIsContainer",
                "            exit   1;\n        }\n        if ($e.PSIsContainer",
            ),
        ),
        // 함수 끝 } 앞 공백
        (
            "indented close",
            GOOD.replacen("    return $files\n}\n", "    return $files\n  }\n", 1),
        ),
        // 문자열 안 중괄호는 블록 경계가 아니다.
        (
            "brace in a string",
            swap(
                "Write-Error \"LICENSES\\$($e.Name) is not a regular file\"",
                "Write-Error \"not a file }\"",
            ),
        ),
    ];
    for (label, src) in cases {
        assert!(
            violations(&src).is_empty(),
            "{label}: {:?}\n{src}",
            violations(&src)
        );
    }
}

//! `scripts/build-windows.ps1`의 `Get-NoticeSetFiles`가 셸 `notice_set_files`와 같은 규칙을
//! 지키는지 소스에서 확인한다. Linux CI에는 PowerShell이 없어 함수를 실행하지 못하므로,
//! 규칙을 이루는 세 요소가 함수 본문의 주석 아닌 줄에 있는지만 본다.
//! - `Get-ChildItem`에 `-Force`가 있다. 없으면 Hidden 속성 폴더·링크가 열거에서 빠져 거부를 피한다.
//! - 이름이 점으로 시작하면 `exit 1`로 멈춘다.
//! - 폴더(`PSIsContainer`)와 링크(`ReparsePoint`)면 `exit 1`로 멈춘다.
//!
//! 실행 결과(예: 실제로 Hidden 항목을 거부하는지)는 보장하지 않는다. 그 동작은 Windows에서
//! 함수를 실행해 확인한다.

const SCRIPT: &str = "scripts/build-windows.ps1";
const HEADER: &str = "function Get-NoticeSetFiles {";

/// `HEADER` 다음 줄부터 0열의 `}` 전까지, `#` 주석 줄을 뺀 본문 줄.
fn function_body(src: &str) -> Option<Vec<&str>> {
    let mut lines = src.lines();
    lines.by_ref().find(|l| l.trim_end() == HEADER)?;
    let mut body = Vec::new();
    for line in lines {
        if line.trim_end() == "}" {
            return Some(body);
        }
        if !line.trim_start().starts_with('#') {
            body.push(line);
        }
    }
    None
}

/// `cond`를 담은 줄 뒤 몇 줄 안에 `exit 1`이 있으면 그 조건이 실패로 이어진다고 본다.
fn guarded_by_exit(body: &[&str], cond: impl Fn(&str) -> bool) -> bool {
    body.iter()
        .enumerate()
        .any(|(i, l)| cond(l) && body[i + 1..].iter().take(4).any(|n| n.trim() == "exit 1"))
}

/// 규칙 위반 목록. 비어 있으면 세 요소가 모두 있다.
fn violations(src: &str) -> Vec<&'static str> {
    let Some(body) = function_body(src) else {
        return vec!["Get-NoticeSetFiles 함수를 찾지 못했다"];
    };
    let mut out = Vec::new();
    if !body
        .iter()
        .any(|l| l.contains("Get-ChildItem") && l.contains("-Force") && l.contains("LICENSES"))
    {
        out.push("LICENSES 를 읽는 Get-ChildItem 에 -Force 가 없다");
    }
    if !guarded_by_exit(&body, |l| l.contains(".StartsWith(\".\")")) {
        out.push("점으로 시작하는 이름을 exit 1 로 거부하지 않는다");
    }
    if !guarded_by_exit(&body, |l| {
        l.contains("PSIsContainer") && l.contains("ReparsePoint")
    }) {
        out.push("폴더·링크를 exit 1 로 거부하지 않는다");
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

#[test]
fn each_missing_element_is_reported() {
    let good = "function Get-NoticeSetFiles {\n\
                \x20   $entries = @(Get-ChildItem -Force -Path \"LICENSES\" | Sort-Object Name)\n\
                \x20   foreach ($e in $entries) {\n\
                \x20       if ($e.Name.StartsWith(\".\")) {\n\
                \x20           Write-Error \"hidden\"\n\
                \x20           exit 1\n\
                \x20       }\n\
                \x20       if ($e.PSIsContainer -or ($e.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {\n\
                \x20           Write-Error \"not a file\"\n\
                \x20           exit 1\n\
                \x20       }\n\
                \x20   }\n\
                }\n";
    assert!(violations(good).is_empty(), "{:?}", violations(good));

    let no_force = good.replace("Get-ChildItem -Force -Path", "Get-ChildItem -Path");
    assert_eq!(violations(&no_force).len(), 1);

    // 주석으로 남은 -Force 는 세지 않는다.
    let commented = no_force.replace(
        "    $entries",
        "    # Get-ChildItem -Force -Path \"LICENSES\"\n    $entries",
    );
    assert_eq!(violations(&commented).len(), 1);

    let no_dot = good.replace("$e.Name.StartsWith(\".\")", "$false");
    assert_eq!(violations(&no_dot).len(), 1);

    let dot_without_exit = good.replacen("exit 1", "continue", 1);
    assert_eq!(violations(&dot_without_exit).len(), 1);

    assert_eq!(violations("function Other {\n}\n").len(), 1);
}

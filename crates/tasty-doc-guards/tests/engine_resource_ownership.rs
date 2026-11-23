//! Checks named ownership fields and direct references, not aliases or transitive dependencies.
use tasty_doc_guards::{repo_root, source_text::mask_non_code};

fn fields(source: &str, name: &str) -> String {
    let code = mask_non_code(source);
    let marker = format!("struct {name}");
    let start = code
        .match_indices(&marker)
        .map(|(start, _)| start)
        .find(|&start| {
            code.as_bytes()
                .get(start + marker.len())
                .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'{' | b'<'))
        })
        .expect("exact owner exists");
    let open = start + code[start..].find('{').unwrap();
    let mut depth = 1;
    for (offset, ch) in code[open + 1..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return code[open + 1..open + 1 + offset].to_owned();
                }
            }
            _ => {}
        }
    }
    panic!("complete owner body");
}

fn has_execution_owner(fields: &str) -> bool {
    fields
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|word| {
            matches!(
                word,
                "EngineRuntime"
                    | "TerminalStore"
                    | "Pty"
                    | "PtyState"
                    | "Terminal"
                    | "PtyRegistry"
                    | "ChildTerminalRegistry"
                    | "HookRuntimeState"
                    | "TaskScope"
                    | "ObserverRouter"
                    | "RemoteState"
                    | "LiveDomainState"
                    | "readonly_views"
            )
        })
}

#[test]
fn core_state_does_not_regain_the_moved_resource_families() {
    let source =
        std::fs::read_to_string(repo_root().join("crates/tasty-core/src/state.rs")).unwrap();
    assert!(!has_execution_owner(&fields(&source, "CoreState")));
    // Other unnamed service handles and Surface instances remain outside this guard's claim.
}

#[test]
fn session_is_the_owner_and_execution_contexts_only_borrow() {
    let root = repo_root();
    let session = std::fs::read_to_string(root.join("src/runtime/engine_session.rs")).unwrap();
    let body = fields(&session, "EngineSession");
    for ty in [
        "EngineRuntime",
        "HookRuntimeState",
        "TaskScope",
        "ObserverRouter",
        "CoreState",
        "RemoteState",
        "LiveDomainState",
    ] {
        assert!(body.contains(ty), "Session lost {ty}");
    }
    let access = std::fs::read_to_string(root.join("src/runtime/engine_access.rs")).unwrap();
    for ty in ["EngineRef", "EngineMut"] {
        let body = fields(&access, ty);
        let types: Vec<_> = body
            .split(',')
            .filter_map(|field| field.split_once(':').map(|(_, value)| value.trim()))
            .collect();
        assert_eq!(types.len(), 9, "review the actual borrow fields of {ty}");
        for field in types {
            assert!(
                field.starts_with('&') || field.starts_with("Option<&"),
                "{ty} owns {field}"
            );
        }
    }
    let runtime = std::fs::read_to_string(root.join("src/runtime/engine_runtime.rs")).unwrap();
    assert!(fields(&runtime, "EngineRuntime").contains("readonly_views"));
}

#[test]
fn resource_guard_detects_direct_and_nested_reintroduction() {
    for value in [
        "runtime: EngineRuntime",
        "tasks: Option<TaskScope>",
        "observer: Box<ObserverRouter>",
        "displays: std::collections::HashMap<u32, Terminal>",
    ] {
        assert!(has_execution_owner(&fields(
            &format!("struct CoreState {{ {value} }}"),
            "CoreState"
        )));
    }
    assert!(!has_execution_owner(&fields(
        "struct CoreState { workspaces: Vec<Workspace>, /* TaskScope */ revision: u64 }",
        "CoreState"
    )));
}

#[test]
fn session_has_no_implicit_or_test_only_deref_to_a_domain_state() {
    let root = repo_root();
    let paths = [
        "src/runtime/engine_session.rs",
        "src/runtime/engine_session/bootstrap.rs",
        "src/runtime/engine_session/tests.rs",
    ];
    for path in paths {
        let source = std::fs::read_to_string(root.join(path)).unwrap();
        assert!(
            !has_session_deref(&source),
            "Session type differs in {path}"
        );
    }
    assert!(has_session_deref(
        "#[cfg(test)] impl std::ops::Deref for EngineSession { type Target = CoreState; }"
    ));
}

fn has_session_deref(source: &str) -> bool {
    mask_non_code(source).split('{').any(|header| {
        let words: Vec<_> = header
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|word| !word.is_empty())
            .collect();
        words.contains(&"impl")
            && words
                .iter()
                .any(|word| matches!(*word, "Deref" | "DerefMut"))
            && words
                .windows(2)
                .any(|pair| pair == ["for", "EngineSession"])
    })
}

fn terminal_owns_os_resource(source: &str) -> bool {
    ["Terminal", "TerminalState"]
        .iter()
        .map(|name| fields(source, name))
        .any(|body| {
            body.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                .any(|word| {
                    matches!(
                        word,
                        "Pty" | "PtyState" | "PtyRegistry" | "Child" | "MasterPty"
                    )
                })
        })
}

#[test]
fn content_and_physical_resources_are_owned_separately_without_an_external_watcher() {
    let root = repo_root();
    let terminal = std::fs::read_to_string(root.join("crates/tasty-terminal/src/lib.rs")).unwrap();
    assert!(!terminal_owns_os_resource(&terminal));
    let pty = std::fs::read_to_string(root.join("crates/tasty-terminal/src/pty.rs")).unwrap();
    let physical = fields(&pty, "Pty");
    assert!(physical.contains("Child") && physical.contains("MasterPty"));
    assert!(!mask_non_code(&pty).contains("fn take_child"));
    assert!(!root.join("src/core/pty_registry.rs").exists());
    let store = std::fs::read_to_string(root.join("src/runtime/terminal_store.rs")).unwrap();
    assert!(fields(&store, "TerminalStore").contains("HashMap<SurfaceId, TerminalEntry>"));
    let entry = fields(&store, "TerminalEntry");
    assert!(entry.contains("terminal: Terminal,") && entry.contains("pty: Option<Pty>,"));
}

#[test]
fn terminal_owner_guard_checks_the_exact_struct_and_nested_resources() {
    let good = "struct TerminalState { parser: Parser } struct Terminal { state: Arc<Mutex<TerminalState>> }";
    assert!(!terminal_owns_os_resource(good));
    for field in [
        "pty: Option<Pty>",
        "child: Box<dyn portable_pty::Child>",
        "master: Box<dyn MasterPty>",
    ] {
        assert!(terminal_owns_os_resource(&format!(
            "struct TerminalConfig {{ cols: usize }} struct TerminalState {{ parser: Parser }} struct Terminal {{ {field} }}"
        )));
        assert!(terminal_owns_os_resource(&format!(
            "struct Terminal {{ state: Arc<Mutex<TerminalState>> }} struct TerminalState {{ {field} }}"
        )));
    }
}

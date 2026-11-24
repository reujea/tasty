use super::*;

/// 잠금 파일이 고른 egui-winit 버전.
fn locked_egui_winit_version() -> String {
    const LOCK: &str = include_str!("../../../../Cargo.lock");
    let at = LOCK
        .find("name = \"egui-winit\"\nversion = \"")
        .expect("Cargo.lock 에 egui-winit 이 있다");
    let rest = &LOCK[at + "name = \"egui-winit\"\nversion = \"".len()..];
    rest[..rest.find('"').expect("버전 끝")].to_string()
}

/// cargo 레지스트리에 풀린 잠금 버전 egui-winit 의 `src/lib.rs`.
fn egui_winit_source() -> String {
    let home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cargo")))
        .expect("CARGO_HOME 이나 HOME");
    let dir = format!("egui-winit-{}", locked_egui_winit_version());
    let src = home.join("registry").join("src");
    let found = std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("{}: {e}", src.display()))
        .filter_map(Result::ok)
        .map(|index| index.path().join(&dir).join("src").join("lib.rs"))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("{} 아래 {dir} 이 없다", src.display()));
    std::fs::read_to_string(found).expect("읽기")
}

/// `fn name(` 본문의 `=>` 갈래(주석·`_` 갈래 제외)를 공백 없이 늘어놓는다.
fn arms(source: &str, name: &str) -> Vec<String> {
    let at = source
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("{name} 없음"));
    let body = &source[at..];
    let body = &body[..body.find("\n}\n").expect("함수 끝")];
    let mut out: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("=>") && !l.starts_with("//") && !l.starts_with("_ =>"))
        .map(|l| l.split_whitespace().collect::<String>())
        .collect();
    out.sort();
    out
}

/// 함수 본문(서명 다음 줄부터 끝까지)을 공백 없이.
fn body(source: &str, name: &str) -> String {
    let at = source
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("{name} 없음"));
    let body = &source[at..];
    let body = &body[body.find('{').expect("본문 시작")..body.find("\n}\n").expect("함수 끝")];
    body.split_whitespace().collect()
}

/// T-12b-2: egui 키 판정의 Named 표와 KeyCode 표가 잠금 버전 egui-winit 의 표와 갈래 하나하나 같다.
#[test]
fn the_egui_key_tables_match_the_locked_egui_winit() {
    let theirs = egui_winit_source();
    let ours = include_str!("key_class.rs");
    for name in ["key_from_named_key", "key_from_key_code"] {
        let (a, b) = (arms(ours, name), arms(&theirs, name));
        assert!(a.len() > 50, "{name}: {}갈래만 읽었다", a.len());
        assert_eq!(a, b, "{name}");
    }
}

/// T-12b-2: 클립보드 조합 판정 셋이 egui-winit 의 `is_cut_command`·`is_copy_command`·`is_paste_command` 와 같다.
#[test]
fn the_clipboard_combos_match_the_locked_egui_winit() {
    let theirs = egui_winit_source();
    let ours = include_str!("key_class.rs");
    for name in ["is_cut_command", "is_copy_command", "is_paste_command"] {
        assert_eq!(body(ours, name), body(&theirs, name), "{name}");
    }
    // egui-winit 이 수식키 사건에서 정하는 값과 같은 식이다.
    let modifiers = body(&theirs, "on_window_event");
    for line in [
        "self.egui_input.modifiers.mac_cmd=cfg!(target_os=\"macos\")&&super_;",
        "self.egui_input.modifiers.command=ifcfg!(target_os=\"macos\"){super_}else{ctrl};",
    ] {
        assert!(modifiers.contains(line), "{line}");
    }
}

fn value(logical: &Key, code: KeyCode, mods: ModifiersState) -> KeyEventValue<'_> {
    KeyEventValue {
        logical,
        physical: PhysicalKey::Code(code),
        pressed: true,
        mods,
        egui_mods: egui_modifiers(mods),
    }
}

fn class(logical: Key, code: KeyCode, mods: ModifiersState) -> KeyClass {
    classify_key(&value(&logical, code, mods), false)
}

/// 경로 키와 지금 경로로 남는 사건의 경계.
#[test]
fn only_pressed_egui_keys_outside_the_current_path_are_path_keys() {
    let down = Key::Named(NamedKey::ArrowDown);
    assert!(matches!(
        class(down.clone(), KeyCode::ArrowDown, ModifiersState::empty()),
        KeyClass::Path(RouteKey {
            key: egui::Key::ArrowDown,
            ..
        })
    ));
    let mut released = value(&down, KeyCode::ArrowDown, ModifiersState::empty());
    released.pressed = false;
    assert_eq!(
        classify_key(&released, false),
        KeyClass::Current(CurrentReason::Release)
    );
    assert_eq!(
        class(
            Key::Named(NamedKey::Escape),
            KeyCode::Escape,
            ModifiersState::empty()
        ),
        KeyClass::Path(RouteKey {
            key: egui::Key::Escape,
            modifiers: egui::Modifiers::NONE,
            super_held: false,
        })
    );
    // 수식키 Esc 는 경로 키다.
    assert!(matches!(
        class(
            Key::Named(NamedKey::Escape),
            KeyCode::Escape,
            ModifiersState::SHIFT
        ),
        KeyClass::Path(_)
    ));
    // 수식키 단독 누름과 egui 키 없이 글자만 내는 키.
    assert_eq!(
        class(
            Key::Named(NamedKey::AltGraph),
            KeyCode::AltRight,
            ModifiersState::empty()
        ),
        KeyClass::Current(CurrentReason::NoEguiKey)
    );
    assert_eq!(
        class(
            Key::Named(NamedKey::Shift),
            KeyCode::ShiftLeft,
            ModifiersState::SHIFT
        ),
        KeyClass::Current(CurrentReason::NoEguiKey)
    );
    assert_eq!(
        class(
            Key::Character("§".into()),
            KeyCode::IntlBackslash,
            ModifiersState::empty()
        ),
        KeyClass::Current(CurrentReason::NoEguiKey)
    );
    assert_eq!(
        classify_key(
            &value(
                &Key::Character("v".into()),
                KeyCode::KeyV,
                ModifiersState::ALT
            ),
            true
        ),
        KeyClass::Current(CurrentReason::EditBinding)
    );
}

/// 클립보드 조합은 egui `command`(Linux·Windows Ctrl, macOS Cmd)+C·X·V 다. Shift 는 보지 않는다.
#[test]
fn command_c_x_v_are_clipboard_combos() {
    let command = if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        ModifiersState::CONTROL
    };
    for (text, code) in [
        ("c", KeyCode::KeyC),
        ("x", KeyCode::KeyX),
        ("v", KeyCode::KeyV),
    ] {
        for mods in [command, command | ModifiersState::SHIFT] {
            assert_eq!(
                class(Key::Character(text.into()), code, mods),
                KeyClass::Current(CurrentReason::Clipboard),
                "{text} {mods:?}"
            );
        }
    }
    // 논리 키가 egui 키가 아니면 물리 키로 판정한다(라틴 문자가 없는 배열).
    assert_eq!(
        class(Key::Character("ㅊ".into()), KeyCode::KeyC, command),
        KeyClass::Current(CurrentReason::Clipboard)
    );
    if !cfg!(target_os = "macos") {
        assert!(matches!(
            class(
                Key::Character("v".into()),
                KeyCode::KeyV,
                ModifiersState::ALT
            ),
            KeyClass::Path(_)
        ));
    }
}

/// 클립보드 조합은 egui-winit 이 그 창에 가진 수식키로 가른다. 창 값(`mods`)이 비었어도 egui-winit 에 command 가 남아
/// 있으면 egui-winit 은 그 누름을 `Copy` 로 바꾸므로 지금 경로다. 반대로 egui-winit 에 수식키가 없으면 경로 키다.
#[test]
fn the_clipboard_combo_reads_the_egui_winit_modifiers() {
    let c = Key::Character("c".into());
    let command = if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        ModifiersState::CONTROL
    };
    let stale = KeyEventValue {
        egui_mods: egui_modifiers(command),
        ..value(&c, KeyCode::KeyC, ModifiersState::empty())
    };
    assert_eq!(
        classify_key(&stale, false),
        KeyClass::Current(CurrentReason::Clipboard)
    );
    let cleared = KeyEventValue {
        egui_mods: egui::Modifiers::NONE,
        ..value(&c, KeyCode::KeyC, command)
    };
    assert!(matches!(classify_key(&cleared, false), KeyClass::Path(_)));
}

/// M-E0b-9 F: 수식키 없는 Esc 가 사용자 편집 바인딩과 겹쳐도 편집 바인딩 판정보다 앞에서 Esc 경로 키다. 물리 키로만
/// Escape 인 누름(논리 키를 모르는 배열)도 같은 판정이다(M-E0b-8 의 분류).
#[test]
fn a_bare_escape_bound_as_an_edit_key_takes_the_escape_route() {
    let bare = KeyClass::Path(RouteKey {
        key: egui::Key::Escape,
        modifiers: egui::Modifiers::NONE,
        super_held: false,
    });
    let escape = Key::Named(NamedKey::Escape);
    assert_eq!(
        classify_key(
            &value(&escape, KeyCode::Escape, ModifiersState::empty()),
            true
        ),
        bare
    );
    let unknown = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
    assert_eq!(
        classify_key(
            &value(&unknown, KeyCode::Escape, ModifiersState::empty()),
            false
        ),
        bare
    );
}

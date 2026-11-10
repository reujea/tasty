//! 디버그 CLI가 만드는 요청의 메서드와 파라미터를 확인한다.
//! 디버그 전용 항목은 `mod` 선언에 cfg가 붙은 파일에 모아야 하므로
//! release에서도 실행하는 `cli_entry_tests`와 분리한다.

use tasty_cli::request::command_to_request;
use tasty_cli::{Commands, DebugCommands, ModalDebugCommands, SettingsDebugCommands};

/// App 상태가 필요한 핸들러는 여기서 호출하지 않고 요청 형식을 확인한다.
/// 활성 모달을 닫는 명령이므로 대상 파라미터를 받지 않는다.
#[test]
fn modal_close_request_cli_entry_point_matches_the_registered_method() {
    let req = command_to_request(&Commands::Debug {
        command: DebugCommands::Modal(ModalDebugCommands::CloseRequest),
    });
    assert_eq!(req.method, "debug.modal.close_request");
    assert!(
        req.params.as_object().is_some_and(|o| o.is_empty()),
        "대상을 받지 않는 명령이다 — params 는 비어야 한다. 지금: {}",
        req.params
    );
}

/// 설정 창 닫기는 대상 인자가 없고 등록된 debug 메서드로 간다.
#[test]
fn settings_close_cli_entry_point_matches_the_registered_method() {
    let req = command_to_request(&Commands::Debug {
        command: DebugCommands::Settings(SettingsDebugCommands::Close),
    });
    assert_eq!(req.method, "debug.settings.close");
    assert!(req.params.as_object().is_some_and(|o| o.is_empty()));
    assert!(tasty_ipc::method_meta::method_meta("debug.settings.close").is_some());
}

/// 창 범위가 기본값이고, 생략한 대상 인자는 null로 보내 핸들러가 단일 메인 창을 고르게 한다.
#[test]
fn toast_cli_entry_point_defaults_to_the_window_scope() {
    let req = command_to_request(&Commands::Debug {
        command: DebugCommands::Toast {
            message: "Saved".to_string(),
            scope: "window".to_string(),
            target_id: None,
            window_id: None,
            kind: "info".to_string(),
            hint: Vec::new(),
        },
    });
    assert_eq!(req.method, "debug.toast");
    assert_eq!(req.params["scope"], "window");
    assert_eq!(req.params["message"], "Saved");
    assert!(req.params["window_id"].is_null());
    assert!(req.params["target_id"].is_null());
}

/// 보조 창까지 나열하는 debug 목록이다. 대상 인자가 없다.
#[test]
fn windows_cli_entry_point_matches_the_registered_method() {
    let req = command_to_request(&Commands::Debug {
        command: DebugCommands::Windows,
    });
    assert_eq!(req.method, "debug.window.list");
    assert!(req.params.as_object().is_some_and(|o| o.is_empty()));
    assert!(tasty_ipc::method_meta::method_meta("debug.window.list").is_some());
}

/// 다운로드 시작은 mirror 의 로컬 workspace id 와 원격 경로를 보낸다. 확인·취소는 시작이 준 id 를 보낸다.
#[test]
fn remote_download_cli_entry_points_match_the_registered_methods() {
    use tasty_cli::RemoteDownloadDebugCommands;
    let req = command_to_request(&Commands::Debug {
        command: DebugCommands::RemoteDownload(RemoteDownloadDebugCommands::Start {
            workspace: 4,
            path: "/srv/a.bin".to_string(),
        }),
    });
    assert_eq!(req.method, "debug.remote_download.start");
    assert_eq!(req.params["workspace_id"], 4);
    assert_eq!(req.params["path"], "/srv/a.bin");
    for (command, method) in [
        (
            RemoteDownloadDebugCommands::Confirm { id: 7 },
            "debug.remote_download.confirm",
        ),
        (
            RemoteDownloadDebugCommands::Cancel { id: 7 },
            "debug.remote_download.cancel",
        ),
    ] {
        let req = command_to_request(&Commands::Debug {
            command: DebugCommands::RemoteDownload(command),
        });
        assert_eq!(req.method, method);
        assert_eq!(req.params["download_id"], 7);
        assert!(tasty_ipc::method_meta::method_meta(method).is_some());
    }
    assert!(tasty_ipc::method_meta::method_meta("debug.remote_download.start").is_some());
}

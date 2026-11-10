//! MSI 제거가 `~/.tasty`를 지우는 경로가 제거 화면의 체크박스 하나뿐인지 `wix/main.wxs`에서 확인한다.
//! 삭제가 실행 시퀀스로 돌아가면 화면 없는 제거와 업그레이드가 사용자 데이터를 지운다.
//! MSI를 만들어 실행하지는 않는다. 실제 동작은 설치본에서 따로 확인한다.

use std::path::Path;

const ACTION: &str = "RemoveTastyUserData";
const CHECKBOX_PROPERTY: &str = "TastyRemoveUserData";
const COMMAND_PROPERTY: &str = "TastyRemoveUserDataCommand";

fn main_wxs() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wix/main.wxs");
    std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
}

/// `open`과 `close` 사이의 조각을 모두 모은다.
fn blocks<'a>(text: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    text.split(open)
        .skip(1)
        .map(|rest| rest.split(close).next().unwrap_or(rest))
        .collect()
}

#[test]
fn user_data_is_deleted_by_one_action() {
    let wxs = main_wxs();
    assert_eq!(wxs.matches("rmdir").count(), 1, "삭제 명령은 하나여야 한다");
    let command = blocks(&wxs, "<Property ", "/>")
        .into_iter()
        .find(|property| property.contains("rmdir"))
        .expect("삭제 명령을 가진 Property가 있어야 한다");
    assert!(
        command.contains(&format!("Id='{COMMAND_PROPERTY}'")),
        "{command}"
    );
    let action = blocks(&wxs, "<CustomAction", "/>")
        .into_iter()
        .find(|action| action.contains(&format!("Id='{ACTION}'")))
        .expect("삭제 CustomAction이 있어야 한다");
    assert!(
        action.contains(&format!("ExeCommand='[{COMMAND_PROPERTY}]'")),
        "{action}"
    );
    assert!(action.contains("Execute='immediate'"), "{action}");
}

#[test]
fn deletion_runs_only_from_the_ui_sequence() {
    let wxs = main_wxs();
    for block in blocks(
        &wxs,
        "<InstallExecuteSequence>",
        "</InstallExecuteSequence>",
    ) {
        assert!(
            !block.contains(ACTION),
            "실행 시퀀스에 넣으면 화면 없는 제거도 삭제한다: {block}"
        );
    }
    let scheduled: Vec<&str> = blocks(&wxs, "<InstallUISequence>", "</InstallUISequence>")
        .into_iter()
        .filter(|block| block.contains(ACTION))
        .collect();
    assert_eq!(scheduled.len(), 1, "UI 시퀀스에 한 번만 있어야 한다");
    let row = scheduled[0];
    assert!(row.contains("After='ExecuteAction'"), "{row}");
    for needed in [
        format!("{CHECKBOX_PROPERTY}=\"1\""),
        "WixUI_InstallMode=\"Remove\"".to_string(),
        "NOT UPGRADINGPRODUCTCODE".to_string(),
    ] {
        assert!(row.contains(&needed), "조건에 {needed}가 없다: {row}");
    }
}

#[test]
fn checkbox_starts_off_and_cannot_be_set_from_the_command_line() {
    let wxs = main_wxs();
    let checkbox = blocks(&wxs, "<Control ", "/>")
        .into_iter()
        .find(|control| control.contains("Type='CheckBox'"))
        .expect("제거 화면에 체크박스가 있어야 한다");
    assert!(
        checkbox.contains(&format!("Property='{CHECKBOX_PROPERTY}'")),
        "{checkbox}"
    );
    assert!(
        !wxs.contains(&format!("<Property Id='{CHECKBOX_PROPERTY}'")),
        "기본값을 주면 체크박스가 켜진 채 시작한다"
    );
    assert!(
        !wxs.contains(&format!("Id='{CHECKBOX_PROPERTY}'"))
            && CHECKBOX_PROPERTY.chars().any(char::is_lowercase),
        "공개 프로퍼티나 SetProperty로 값을 줄 수 있으면 안 된다"
    );
}

#[test]
fn windows_uninstall_and_modify_stay_native() {
    let wxs = main_wxs();
    for hidden in [
        "ARPSYSTEMCOMPONENT",
        "ARPNOMODIFY",
        "ARPNOREPAIR",
        "ARPNOREMOVE",
        "Name='NoModify'",
        "Name='NoRepair'",
        "Name='UninstallString'",
        "CurrentVersion\\Uninstall\\",
    ] {
        assert!(
            !wxs.contains(hidden),
            "설정 앱과 제어판의 기본 제거와 수정을 숨기거나 우회하면 안 된다: {hidden}"
        );
    }
    assert!(
        wxs.contains(
            "<Publish Dialog='MaintenanceTypeDlg' Control='RemoveButton' Event='NewDialog' Value='TastyRemoveDataDlg'"
        ),
        "수정 화면의 Remove가 체크박스 화면으로 가야 한다"
    );
    assert!(
        wxs.contains("<Show Dialog='TastyRemoveDataDlg'"),
        "/x 전체 UI 경로에도 체크박스 화면이 있어야 한다"
    );
    assert!(
        wxs.contains("(Preselected AND NOT REMOVE=\"ALL\")"),
        "ResumeDlg가 체크박스 화면을 가로막으면 안 된다"
    );
}

//! Fixed workspace and tab metadata inputs admitted by the application journal.
/// 대상은 ID로 지정한다. 팝업이 열린 동안 순서가 바뀌어도 같은 대상을 바꾼다.
#[cfg_attr(
    all(not(feature = "gui"), not(test)),
    expect(dead_code, reason = "only the gui rename popup raises a direct rename")
)]
#[derive(Debug, Clone)]
pub enum DirectRename {
    WorkspaceName {
        workspace_id: u32,
        name: String,
    },
    #[cfg_attr(
        all(not(feature = "gui"), test),
        expect(
            dead_code,
            reason = "subtitle rename is produced by the GUI popup; headless tests exercise name and tab values only"
        )
    )]
    WorkspaceSubtitle {
        workspace_id: u32,
        subtitle: String,
    },
    /// None이면 사용자 이름을 지운다.
    TabName {
        tab_id: u32,
        name: Option<String>,
    },
}

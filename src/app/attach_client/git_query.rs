//! 원격 git 조회 결과를 요청한 plugin에 전달한다. host는 결과를 받을 plugin을 이름으로 정하지 않는다.

/// `requester`에 결과 이벤트를 보내고, 다시 그려야 할 그 plugin의 popup instance를 돌려준다.
/// 결과 이벤트만으로는 렌더 입력이 바뀌지 않아 popup repaint도 함께 요청해야 한다.
pub(super) fn deliver_git_query_result(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    requester: &str,
    payload: &serde_json::Value,
) -> Vec<u64> {
    let Some(manager) = plugin_manager.as_mut() else {
        return Vec::new();
    };
    manager.emit_host_event_to_plugin(
        requester,
        "git_viewer.query_result",
        payload,
        tasty_plugin_protocol::EventScope::System,
    );
    manager
        .popup_instances()
        .filter(|(_, popup)| popup.plugin_id == requester)
        .map(|(id, _)| id)
        .collect()
}

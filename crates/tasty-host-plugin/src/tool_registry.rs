//! 사이드바 도구 항목을 정렬해 제공한다. 렌더링과 ToolAction 실행은 호스트가 맡는다.

use tasty_plugin_manifest::ToolAction;

/// 한 도구 메뉴 항목.
#[derive(Debug, Clone)]
pub struct ToolItem {
    pub source: ToolSource,
    /// 전역 식별자. 플러그인 항목은 <plugin_id>/<tool_id>를 쓴다.
    pub key: String,
    /// `t()`에 전달할 i18n 키. 키가 없으면 원본 문자열 fallback.
    pub label_i18n_key: String,
    pub icon: Option<String>,
    pub action: ToolAction,
    /// 작을수록 위. 호스트 빌트인은 0..=99, plugin 항목은 100 이상 권장.
    pub order_hint: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
    /// 플러그인이 contributes.tool로 선언한 항목.
    Plugin {
        plugin_id: String,
        /// plugin 내부에서의 tool id (매니페스트의 `[[contributes.tool]].id`).
        tool_id: String,
    },
}

/// Plugin 출처 항목을 관리하는 레지스트리.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    plugin: Vec<ToolItem>,
    /// surface 범위로 선언한 플러그인 팝업(`<plugin_id>/<popup_id>`). 대상 surface 가 없으면 열 수 없다.
    surface_popups: std::collections::HashSet<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// surface 범위로 선언한 팝업 목록을 통째로 교체한다.
    pub fn set_surface_popups(&mut self, ids: std::collections::HashSet<String>) {
        self.surface_popups = ids;
    }

    /// 이 동작이 대상 surface 를 필요로 하는지. surface 선언 팝업을 여는 동작만 해당한다.
    pub fn needs_surface(&self, action: &ToolAction) -> bool {
        matches!(action, ToolAction::OpenPopup { popup_id } if self.surface_popups.contains(popup_id))
    }

    /// plugin 출처 항목을 통째로 교체한다.
    pub fn set_plugin_items(&mut self, items: Vec<ToolItem>) {
        self.plugin = items;
    }

    /// 정렬된(order_hint asc, 동률 시 key asc) 전체 항목.
    pub fn visible_items(&self) -> Vec<ToolItem> {
        let mut all = self.plugin.clone();
        all.sort_by(|a, b| {
            a.order_hint
                .cmp(&b.order_hint)
                .then_with(|| a.key.cmp(&b.key))
        });
        all
    }

    /// 특정 key에 매칭되는 항목을 1개 반환한다. tools_menu UI 외에 IPC/CLI에서
    /// invoke할 때 식별용.
    pub fn find(&self, key: &str) -> Option<ToolItem> {
        self.plugin.iter().find(|i| i.key == key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plugin_item(plugin_id: &str, tool_id: &str, order: i32) -> ToolItem {
        ToolItem {
            source: ToolSource::Plugin {
                plugin_id: plugin_id.into(),
                tool_id: tool_id.into(),
            },
            key: format!("{plugin_id}/{tool_id}"),
            label_i18n_key: format!("{plugin_id}.{tool_id}"),
            icon: None,
            action: ToolAction::Event {
                event_key: format!("{plugin_id}.test"),
            },
            order_hint: order,
        }
    }

    #[test]
    fn empty_registry_has_no_items() {
        let reg = ToolRegistry::new();
        assert!(reg.visible_items().is_empty());
    }

    #[test]
    fn set_plugin_items_replaces_all_entries() {
        let mut reg = ToolRegistry::new();
        reg.set_plugin_items(vec![make_plugin_item("com.example.a", "x", 100)]);
        assert_eq!(reg.visible_items().len(), 1);
        reg.set_plugin_items(vec![]);
        assert!(reg.visible_items().is_empty());
    }

    #[test]
    fn visible_items_sorted_by_order_hint_then_key() {
        let mut reg = ToolRegistry::new();
        reg.set_plugin_items(vec![
            make_plugin_item("com.example.a", "b", 100),
            make_plugin_item("com.example.a", "a", 100),
            make_plugin_item("com.example.b", "c", 50),
        ]);
        let items = reg.visible_items();
        let keys: Vec<_> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["com.example.b/c", "com.example.a/a", "com.example.a/b",]
        );
    }

    #[test]
    fn only_a_surface_popup_action_needs_a_surface() {
        let mut reg = ToolRegistry::new();
        reg.set_surface_popups(["com.example.a/p".to_string()].into_iter().collect());
        let open = |id: &str| ToolAction::OpenPopup {
            popup_id: id.into(),
        };
        assert!(reg.needs_surface(&open("com.example.a/p")));
        assert!(!reg.needs_surface(&open("com.example.a/w")));
        assert!(!reg.needs_surface(&ToolAction::Event {
            event_key: "com.example.a/p".into()
        }));
    }

    #[test]
    fn find_returns_item_by_key() {
        let mut reg = ToolRegistry::new();
        reg.set_plugin_items(vec![make_plugin_item("com.example.a", "x", 100)]);
        assert!(reg.find("com.example.a/x").is_some());
        assert!(reg.find("nope").is_none());
    }
}

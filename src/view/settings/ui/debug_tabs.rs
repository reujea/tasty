//! debug 전용 — `debug.settings.open` 의 탭 지정을 설정 UI 상태에 적용한다.
//! 새로 연 창과 이미 열린 창이 같은 순서(상위 탭 다음 하위 탭)로 적용한다.

use super::SettingsUiState;

/// 탭 지정 결과. 지정하지 않은 쪽은 `None`, 알 수 없는 키는 `Some(false)` 다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugTabsApplied {
    pub tab: Option<bool>,
    pub subtab: Option<bool>,
}

impl SettingsUiState {
    /// 상위 탭을 먼저 고른 뒤 하위 탭을 고른다. 하위 탭 키는 그 시점의 상위 탭에 속한다.
    /// 알 수 없는 키는 그 단계만 건너뛰고 현재 선택을 유지한다.
    pub fn apply_debug_tabs(
        &mut self,
        tab: Option<&str>,
        subtab: Option<&str>,
    ) -> DebugTabsApplied {
        let tab = tab.map(|key| self.select_tab_by_key(key));
        let subtab = subtab.map(|key| self.select_section_by_key(key));
        DebugTabsApplied { tab, subtab }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AppearanceSubTab, SettingsTab, TerminalSubTab};
    use super::*;

    /// 이미 다른 탭을 보고 있는 상태에서도 지정한 상위·하위 탭으로 바뀐다.
    #[test]
    fn an_open_state_switches_to_the_requested_tab_and_section() {
        let mut st = SettingsUiState::new();
        assert!(st.select_tab_by_key("appearance"));
        assert!(st.select_section_by_key("colors"));

        let applied = st.apply_debug_tabs(Some("terminal"), Some("tui"));
        assert_eq!(
            applied,
            DebugTabsApplied {
                tab: Some(true),
                subtab: Some(true)
            }
        );
        assert_eq!(st.active_tab, SettingsTab::Terminal);
        assert_eq!(st.terminal_sub_tab, TerminalSubTab::Tui);
    }

    /// 하위 탭만 주면 지금 보고 있는 상위 탭 안에서 바꾼다.
    #[test]
    fn a_section_alone_applies_within_the_current_tab() {
        let mut st = SettingsUiState::new();
        assert!(st.select_tab_by_key("appearance"));

        let applied = st.apply_debug_tabs(None, Some("display"));
        assert_eq!(applied.tab, None);
        assert_eq!(applied.subtab, Some(true));
        assert_eq!(st.active_tab, SettingsTab::Appearance);
        assert_eq!(st.appearance_sub_tab, AppearanceSubTab::Display);
    }

    /// 알 수 없는 상위 탭은 현재 탭을 유지하고 결과로 알린다.
    #[test]
    fn an_unknown_tab_keeps_the_current_tab() {
        let mut st = SettingsUiState::new();
        assert!(st.select_tab_by_key("appearance"));

        let applied = st.apply_debug_tabs(Some("nonexistent"), None);
        assert_eq!(
            applied,
            DebugTabsApplied {
                tab: Some(false),
                subtab: None
            }
        );
        assert_eq!(st.active_tab, SettingsTab::Appearance);
    }
    #[test]
    fn both_anomaly_aliases_select_the_new_misc_subtab() {
        for alias in ["anomaly_detection", "anomaly-detection"] {
            let mut state = SettingsUiState::new();
            let applied = state.apply_debug_tabs(Some("misc"), Some(alias));
            assert_eq!(applied.subtab, Some(true));
            assert_eq!(
                state.misc_sub_tab,
                super::super::MiscSubTab::AnomalyDetection
            );
        }
    }

    #[test]
    fn both_frame_rate_aliases_select_the_terminal_subtab() {
        for alias in ["frame_rate", "frame-rate"] {
            let mut state = SettingsUiState::new();
            let applied = state.apply_debug_tabs(Some("terminal"), Some(alias));
            assert_eq!(applied.subtab, Some(true));
            assert_eq!(state.terminal_sub_tab, TerminalSubTab::FrameRate);
        }
    }
}

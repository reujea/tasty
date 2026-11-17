//! 명령 팔레트의 입력 상태와 검색. 선택한 명령은 pending_run에 넣어 후속 프레임에서 처리한다.
//! 호스트 단축키 목록과 플러그인의 Global 명령 사본을 합친다.
//! Surface 명령은 팔레트 실행 시 대상 포커스를 보장할 수 없어 제외한다.

use tasty_settings::KeybindingSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteCommand {
    /// 호스트 내장 명령 (keybinding field).
    Host {
        /// keybinding `field_id` (예: `"new_workspace"`)
        id: &'static str,
        /// i18n 키 (예: `"settings.keybindings.new_workspace_label"`)
        label_key: &'static str,
    },
    /// Plugin이 `[[contributes.commands]]`로 선언한 전역(scope=Global) command.
    Plugin {
        plugin_id: String,
        command_id: String,
        /// plugin 자신의 lang 네임스페이스에서 해석해야 하는 i18n 키.
        title_i18n_key: String,
    },
}

// 팔레트 자체 열기와, 팔레트와 동시에 사용할 수 없는 무대 종료 명령은 제외한다.
// copy_link의 대상은 마우스 포인터 아래 링크라서 팔레트가 열린 동안에는 대상이 없다.
const PALETTE_EXCLUDED: &[&str] = &[
    "toggle_command_palette",
    "fullscreen_stage_exit",
    "copy_link",
];

// 탐색기 목록 키 가운데 선택 항목에 하는 동작만 팔레트에 둔다. 이동 키는 팔레트에서 쓸 일이 없다.
const PALETTE_EXPLORER_ITEM: &[&str] = &["explorer_rename", "explorer_trash"];

/// 호스트 단축키와 활성 플러그인의 전역 명령을 합치고 제외 목록을 적용한다.
pub fn all_commands(
    plugin_commands: &[crate::plugin::command_registry::PluginCommandEntry],
) -> Vec<PaletteCommand> {
    let mut out: Vec<PaletteCommand> = KeybindingSettings::GENERAL_BINDING_FIELDS
        .iter()
        .filter(|(id, _)| !PALETTE_EXCLUDED.contains(id))
        .chain(
            KeybindingSettings::EXPLORER_LIST_BINDING_FIELDS
                .iter()
                .filter(|(id, _)| PALETTE_EXPLORER_ITEM.contains(id)),
        )
        .map(|(id, label_key)| PaletteCommand::Host { id, label_key })
        .collect();
    out.extend(plugin_commands.iter().map(|e| PaletteCommand::Plugin {
        plugin_id: e.plugin_id.clone(),
        command_id: e.command_id.clone(),
        title_i18n_key: e.title_i18n_key.clone(),
    }));
    out
}

#[cfg(feature = "gui")]
#[derive(Debug, Default)]
pub struct CommandPaletteState {
    pub query: String,
    /// 필터링된 결과에서 선택한 인덱스.
    pub selected: usize,
    /// 실행할 명령과 선택한 시점의 대상 surface.
    pub pending_run: Option<(
        PaletteCommand,
        Option<super::attach_pending::deferred::CommandSurfaceTarget>,
    )>,
}

#[cfg(feature = "gui")]
impl CommandPaletteState {
    pub fn reset(&mut self) {
        self.query.clear();
        self.selected = 0;
    }
}

/// 대소문자를 구분하지 않고 부분 문자열·부분 수열을 검색해 점수순으로 반환한다.
pub fn search<'a>(
    query: &str,
    commands: &'a [PaletteCommand],
    labels: &[String],
) -> Vec<(i32, &'a PaletteCommand)> {
    let q = query.trim();
    if q.is_empty() {
        return commands.iter().map(|c| (0, c)).collect();
    }
    let q_lower = q.to_lowercase();

    let mut scored: Vec<(i32, &PaletteCommand)> = Vec::new();
    for (cmd, label) in commands.iter().zip(labels.iter()) {
        let label_lower = label.to_lowercase();
        if let Some(score) = match_score(&q_lower, &label_lower) {
            scored.push((score, cmd));
        }
    }
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored
}

/// `query`(이미 lowercase)가 `text`(이미 lowercase)의 부분 시퀀스인지 확인하고,
/// 매칭 시 점수를 반환. 없으면 None.
///
/// 점수 룰:
/// - 정확 substring 매칭: 1000 + (단어 시작 위치면 +500) - 시작 인덱스
/// - 부분 시퀀스 매칭: 100 - 매칭 간격 합
fn match_score(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    if let Some(idx) = text.find(query) {
        let mut score = 1000 - (idx as i32);
        let is_word_start = idx == 0
            || text
                .as_bytes()
                .get(idx.wrapping_sub(1))
                .map(|b| !b.is_ascii_alphanumeric())
                .unwrap_or(true);
        if is_word_start {
            score += 500;
        }
        return Some(score);
    }
    let mut chars = query.chars();
    let mut current = chars.next()?;
    let mut last_idx: Option<usize> = None;
    let mut gap_sum: i32 = 0;
    for (i, ch) in text.char_indices() {
        if ch == current {
            if let Some(prev) = last_idx {
                gap_sum += (i - prev) as i32;
            }
            last_idx = Some(i);
            match chars.next() {
                Some(c) => current = c,
                None => return Some(100 - gap_sum),
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(id: &'static str) -> PaletteCommand {
        PaletteCommand::Host { id, label_key: "" }
    }

    #[test]
    fn empty_query_returns_all() {
        let cmds = all_commands(&[]);
        let labels: Vec<String> = cmds
            .iter()
            .map(|c| match c {
                PaletteCommand::Host { id, .. } => id.to_string(),
                PaletteCommand::Plugin { command_id, .. } => command_id.clone(),
            })
            .collect();
        let results = search("", &cmds, &labels);
        assert_eq!(results.len(), cmds.len());
    }

    #[test]
    fn excluded_ids_are_absent_from_the_palette() {
        let cmds = all_commands(&[]);
        for excluded in PALETTE_EXCLUDED {
            assert!(
                !cmds
                    .iter()
                    .any(|c| matches!(c, PaletteCommand::Host { id, .. } if id == excluded)),
                "'{excluded}' 가 팔레트 목록에 노출됐다"
            );
        }
        // 일반 명령까지 모두 제외하는 구현은 통과하지 않아야 한다.
        assert!(
            cmds.iter()
                .any(|c| matches!(c, PaletteCommand::Host { id, .. } if *id == "new_tab")),
            "일반 명령까지 제외됐다"
        );
    }

    #[test]
    fn plugin_commands_are_appended() {
        let plugin_entry = crate::plugin::command_registry::PluginCommandEntry {
            plugin_id: "com.example.a".to_string(),
            command_id: "a.open".to_string(),
            title_i18n_key: "a.open.title".to_string(),
            manifest_default: None,
            binding_mode: tasty_plugin_manifest::BindingMode::Independent,
            scope: tasty_plugin_manifest::CommandScope::Global,
            action: None,
        };
        let cmds = all_commands(std::slice::from_ref(&plugin_entry));
        assert!(cmds.iter().any(|c| matches!(
            c,
            PaletteCommand::Plugin { plugin_id, command_id, .. }
                if plugin_id == "com.example.a" && command_id == "a.open"
        )));
    }

    #[test]
    fn substring_outranks_subsequence() {
        let cmds = vec![host("a"), host("b")];
        // "open" appears verbatim in first label, scattered in second.
        let labels = vec!["Open file".to_string(), "Other punks even".to_string()];
        let results = search("open", &cmds, &labels);
        assert_eq!(results.first().unwrap().1, &cmds[0]);
    }

    #[test]
    fn word_start_gets_bonus() {
        let cmds = vec![host("early"), host("late")];
        let labels = vec!["new tab".to_string(), "renew tab".to_string()];
        let results = search("new", &cmds, &labels);
        assert_eq!(results.first().unwrap().1, &cmds[0]);
    }

    #[test]
    fn no_match_returns_empty() {
        let cmds = vec![host("x")];
        let labels = vec!["foo bar".to_string()];
        let results = search("xyz", &cmds, &labels);
        assert!(results.is_empty());
    }
}

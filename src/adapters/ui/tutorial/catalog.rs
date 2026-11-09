//! Stable tutorial content identifiers; display order is independent of saved progress.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerTarget {
    ContentArea,
    TabHeader,
    Pane,
    Surface,
    Summary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requirement {
    Read,
    Prepare,
    SplitSurface,
    NewTab,
    SwitchTab,
    SplitPane,
    OpenPalette,
}

pub struct Step {
    pub id: &'static str,
    pub target: MarkerTarget,
    pub title_key: &'static str,
    pub body_key: &'static str,
    pub requirement: Requirement,
    pub action: Option<&'static str>,
}

pub struct Topic {
    pub id: &'static str,
    pub revision: u32,
    pub title_key: &'static str,
    pub desc_key: &'static str,
    pub steps: &'static [Step],
}

macro_rules! step {
    ($id:literal, $target:ident, $req:ident, $action:expr) => {
        Step {
            id: $id,
            target: MarkerTarget::$target,
            title_key: concat!("tutorial.step_", $id, "_title"),
            body_key: concat!("tutorial.step_", $id, "_body"),
            requirement: Requirement::$req,
            action: $action,
        }
    };
}

pub fn all_topics() -> &'static [Topic] {
    &TOPICS
}

static TOPICS: [Topic; 3] = [
    Topic {
        id: "basics",
        revision: 1,
        title_key: "tutorial.topic_basics_title",
        desc_key: "tutorial.topic_basics_desc",
        steps: &[
            step!("workspace", ContentArea, Read, None),
            step!("pane", Pane, Read, None),
            step!("tab", TabHeader, Read, None),
            step!("surface", Surface, Read, None),
            step!("summary", Summary, Read, None),
        ],
    },
    Topic {
        id: "layout-practice",
        revision: 1,
        title_key: "tutorial.topic_practice_title",
        desc_key: "tutorial.topic_practice_desc",
        steps: &[
            step!("prepare", Summary, Prepare, None),
            step!(
                "split_surface",
                Surface,
                SplitSurface,
                Some("split_surface_vertical")
            ),
            step!("new_tab", TabHeader, NewTab, Some("new_tab")),
            step!("switch_tab", TabHeader, SwitchTab, None),
            step!("split_pane", Pane, SplitPane, Some("split_pane_vertical")),
            step!("practice_done", Summary, Read, None),
        ],
    },
    Topic {
        id: "commands",
        revision: 1,
        title_key: "tutorial.topic_commands_title",
        desc_key: "tutorial.topic_commands_desc",
        steps: &[
            step!(
                "palette",
                Summary,
                OpenPalette,
                Some("toggle_command_palette")
            ),
            step!("bindings", Summary, Read, Some("open_tutorial")),
            step!("commands_done", Summary, Read, None),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_ids_and_translation_keys_are_valid() {
        let mut ids = std::collections::HashSet::new();
        for topic in all_topics() {
            assert!(ids.insert(topic.id));
            let mut steps = std::collections::HashSet::new();
            assert!(!topic.steps.is_empty());
            for step in topic.steps {
                assert!(steps.insert(step.id));
                for code in tasty_i18n::BUILTIN_CODES {
                    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(format!("lang/{code}.toml"));
                    let source = std::fs::read_to_string(&path).unwrap();
                    let doc: toml::Value = toml::from_str(&source).unwrap();
                    for key in [
                        topic.title_key,
                        topic.desc_key,
                        step.title_key,
                        step.body_key,
                    ] {
                        assert!(
                            doc["tutorial"]
                                .get(key.strip_prefix("tutorial.").unwrap())
                                .is_some(),
                            "{key}"
                        );
                    }
                }
            }
        }
    }
}

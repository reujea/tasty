//! 설정 페이지의 묶음(`kind = "group"`) 검사.
//!
//! 항목의 `group` 은 같은 페이지의 묶음 머리를 가리켜야 한다. 스위치 없는 머리는 항목이 하나 이상
//! 있어야 한다 — 없으면 빈 머리만 남는다. 머리 스위치의 저장 키는 같은 페이지의 다른 항목과 겹치면
//! 안 된다 — 한 칸을 두 컨트롤이 쓰게 된다. 묶음 id 의 형식 · 중복은 다른 항목과 같은 검사를 거친다.

use super::types::{SettingsItemDecl, SettingsPageContribute};

pub(crate) fn validate(page: &SettingsPageContribute) -> anyhow::Result<()> {
    for item in &page.items {
        let Some(group) = item.group() else {
            continue;
        };
        let found = page
            .items
            .iter()
            .any(|g| matches!(g, SettingsItemDecl::Group { id, .. } if id == group));
        if !found {
            anyhow::bail!(
                "contributes.settings_pages '{}' item '{}': group '{}' is not a group item on this page",
                page.id,
                item.common().0,
                group
            );
        }
    }
    for item in &page.items {
        let SettingsItemDecl::Group {
            id, storage_key, ..
        } = item
        else {
            continue;
        };
        match storage_key {
            None if !page.items.iter().any(|m| m.group() == Some(id.as_str())) => {
                anyhow::bail!(
                    "contributes.settings_pages '{}' group '{}': a group without a switch (storage_key) needs at least one item",
                    page.id,
                    id
                )
            }
            Some(key) => {
                let shared = page.items.iter().any(|other| {
                    !std::ptr::eq(other, item)
                        && !matches!(other, SettingsItemDecl::FontOverride { .. })
                        && other.common().2 == Some(key.as_str())
                });
                if shared {
                    anyhow::bail!(
                        "contributes.settings_pages '{}' group '{}': storage_key '{}' is already used by another item",
                        page.id,
                        id,
                        key
                    );
                }
            }
            None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Manifest, SettingsEntry, SettingsItemDecl};

    fn page(items: &str) -> anyhow::Result<Manifest> {
        let src = format!(
            r#"
            manifest_version = 1
            id = "com.tasty.x"
            name = "X"
            version = "0.1.0"
            api_version = "1"
            permissions = ["ui.settings_page"]
            [entry]
            type = "process"
            command = "x"

            [[contributes.settings_pages]]
            id = "x"
            title_key = "settings.subtab.x"
            category = "plugin"
{items}
            "#
        );
        let m: Manifest = toml::from_str(&src)?;
        m.validate()?;
        Ok(m)
    }

    const SWITCHED: &str = r#"
            [[contributes.settings_pages.items]]
            kind = "number"
            id = "loose"
            label_key = "x.loose"
            storage_key = "loose"

            [[contributes.settings_pages.items]]
            kind = "group"
            id = "sync"
            label_key = "x.sync"
            storage_key = "sync_on"

            [[contributes.settings_pages.items]]
            kind = "group"
            id = "keep"
            label_key = "x.keep"

            [[contributes.settings_pages.items]]
            kind = "toggle"
            id = "swap"
            label_key = "x.swap"
            storage_key = "swap"
            group = "keep"

            [[contributes.settings_pages.items]]
            kind = "number"
            id = "tokens"
            label_key = "x.tokens"
            storage_key = "tokens"
            group = "sync"
    "#;

    /// 머리 스위치는 기본 켬이고 저장 키가 있을 때만 있다. 그리는 순서는 머리 선언 순서이고,
    /// 항목은 선언 위치와 상관없이 제 머리 뒤에 모인다.
    #[test]
    fn groups_parse_and_order_members_under_their_header() {
        let m = page(SWITCHED).expect("valid");
        let p = &m.contributes.settings_pages[0];
        match &p.items[1] {
            SettingsItemDecl::Group {
                storage_key,
                default,
                ..
            } => {
                assert_eq!(storage_key.as_deref(), Some("sync_on"));
                assert!(*default);
            }
            other => panic!("expected Group, got {other:?}"),
        }
        let shape: Vec<(String, Vec<String>)> = p
            .entries()
            .iter()
            .map(|e| match e {
                SettingsEntry::Item(i) => (i.common().0.to_string(), Vec::new()),
                SettingsEntry::Group { header, members } => (
                    header.common().0.to_string(),
                    members.iter().map(|m| m.common().0.to_string()).collect(),
                ),
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                ("loose".to_string(), vec![]),
                ("sync".to_string(), vec!["tokens".to_string()]),
                ("keep".to_string(), vec!["swap".to_string()]),
            ]
        );
    }

    #[test]
    fn a_member_must_point_at_a_group_on_the_page() {
        let missing = SWITCHED.replace("group = \"sync\"", "group = \"nope\"");
        let e = page(&missing).unwrap_err().to_string();
        assert!(e.contains("group 'nope' is not a group item"), "{e}");
        // 묶음 머리가 아닌 항목을 가리켜도 안 된다.
        let not_group = SWITCHED.replace("group = \"sync\"", "group = \"loose\"");
        let e = page(&not_group).unwrap_err().to_string();
        assert!(e.contains("group 'loose' is not a group item"), "{e}");
    }

    #[test]
    fn a_plain_group_needs_a_member_but_a_switched_one_does_not() {
        let empty_plain = SWITCHED.replace("group = \"keep\"", "");
        let e = page(&empty_plain).unwrap_err().to_string();
        assert!(e.contains("group 'keep': a group without a switch"), "{e}");
        let empty_switched = SWITCHED.replace("group = \"sync\"", "");
        page(&empty_switched).expect("a switched group may stand alone");
    }

    #[test]
    fn a_group_switch_cannot_share_a_storage_key() {
        let shared = SWITCHED.replace("storage_key = \"sync_on\"", "storage_key = \"tokens\"");
        let e = page(&shared).unwrap_err().to_string();
        assert!(e.contains("storage_key 'tokens' is already used"), "{e}");
    }

    #[test]
    fn group_ids_share_the_item_id_rules() {
        let dup = SWITCHED.replace("id = \"keep\"", "id = \"sync\"");
        let e = page(&dup).unwrap_err().to_string();
        assert!(e.contains("item id 'sync' declared twice"), "{e}");
        let bad = SWITCHED.replace("storage_key = \"sync_on\"", "storage_key = \"Sync On\"");
        let e = page(&bad).unwrap_err().to_string();
        assert!(e.contains("storage_key 'Sync On'"), "{e}");
    }

    /// 묶음이 없는 페이지는 이전과 같은 순서로 그린다.
    #[test]
    fn a_page_without_groups_keeps_its_order() {
        let m = page(
            r#"
            [[contributes.settings_pages.items]]
            kind = "toggle"
            id = "a"
            label_key = "x.a"
            storage_key = "a"

            [[contributes.settings_pages.items]]
            kind = "number"
            id = "b"
            label_key = "x.b"
            storage_key = "b"
            "#,
        )
        .expect("valid");
        let e = m.contributes.settings_pages[0].entries();
        assert!(matches!(
            e.as_slice(),
            [SettingsEntry::Item(_), SettingsEntry::Item(_)]
        ));
    }
}

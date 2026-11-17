//! 대상 plugin을 확장하는 plugin의 활성 상태를 관리한다.
//! 비활성 설정, 대상의 존재·버전을 확인한 뒤 같은 대상을 확장하는 후보 간 충돌을 정한다.

use std::collections::HashMap;

use tasty_plugin_manifest::{ExtendsDecl, Manifest};

/// 한 extension plugin의 현재 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionState {
    /// 활성. target도 존재하고 호환되며 사용자도 enable해 둠.
    Active {
        target_id: String,
        target_version: String,
    },
    /// target 부재 또는 호환성 깨짐. hook 등록되지 않음.
    Pending(PendingReason),
    /// 사용자가 명시적으로 비활성화. recompute에서 Active로 자동 승격되지 않는다.
    Disabled,
    /// 다른 extension이 동일 target을 이미 점유. 우선순위 규칙은
    /// `recompute`의 정의 참조.
    Conflict { other_extension_id: String },
}

/// `Pending` 상태의 세부 사유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingReason {
    /// target plugin이 설치 자체가 안 됨.
    TargetMissing,
    /// target은 설치돼 있으나 사용자가 disable.
    TargetDisabled,
    /// target.version이 extension의 `version_req`를 만족하지 않음.
    VersionMismatch {
        target_version: String,
        required: String,
    },
    /// target.version이 semver 형식이 아님 (target plugin의 매니페스트 오류).
    InvalidTargetVersion { target_version: String },
}

/// extension 등록 상태 머신. PluginManager가 소유.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ExtensionRegistry {
    /// extension plugin id → 현재 상태. extends 블록이 없는 plugin은 등록되지 않음.
    states: HashMap<String, ExtensionState>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// extension plugin의 현재 상태를 조회. 등록되지 않은 plugin은 `None`.
    pub fn state(&self, extension_id: &str) -> Option<&ExtensionState> {
        self.states.get(extension_id)
    }

    /// 대상별 활성 확장은 최대 하나이며 hook 실행에 사용한다.
    pub fn active_extension_for_target(&self, target_id: &str) -> Option<&str> {
        self.states.iter().find_map(|(ext_id, state)| match state {
            ExtensionState::Active { target_id: t, .. } if t == target_id => Some(ext_id.as_str()),
            _ => None,
        })
    }

    /// 모든 extension 상태를 (id, state) iterator로 반환. UI/디버그용.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ExtensionState)> {
        self.states.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// 전체 상태를 재계산. plugin 디스커버리/enable/disable/install/remove 후
    /// 매번 호출한다.
    ///
    /// 입력:
    /// - `packages`: 현재 설치된 모든 plugin의 매니페스트
    /// - `is_disabled`: plugin id → 사용자가 disable했는가
    ///
    /// 같은 대상의 활성 후보가 여럿이면 ID 사전순으로 첫 항목을 선택한다.
    /// 다른 후보를 쓰려면 앞선 후보를 비활성화한다.
    pub fn recompute(&mut self, manifests: &[&Manifest], is_disabled: &dyn Fn(&str) -> bool) {
        // 1) 빈 상태로 시작 — extends 블록이 있는 plugin만 새로 채워넣는다.
        let mut next: HashMap<String, ExtensionState> = HashMap::new();

        // 2) 모든 extension 후보를 모은다. (extension_id, &decl)
        let extensions: Vec<(&str, &ExtendsDecl)> = manifests
            .iter()
            .filter_map(|m| m.extends.as_ref().map(|d| (m.id.as_str(), d)))
            .collect();

        // 3) 각 extension별 1차 평가 — Disabled / Pending / Active-후보로 분류.
        //    Conflict는 이 단계에서 결정할 수 없다 (다른 후보와 비교해야).
        //    target lookup은 manifests에서 plugin_id 매칭.
        let target_for = |target_id: &str| -> Option<&&Manifest> {
            manifests.iter().find(|m| m.id == target_id)
        };

        // (ext_id, decl, classification) — Active-후보면 classification은 None.
        //                                   확정된 state면 Some(state).
        type Classification = Option<ExtensionState>;
        let mut classified: Vec<(&str, &ExtendsDecl, Classification)> = Vec::new();

        for (ext_id, decl) in &extensions {
            if is_disabled(ext_id) {
                classified.push((ext_id, decl, Some(ExtensionState::Disabled)));
                continue;
            }
            let Some(target) = target_for(&decl.plugin_id) else {
                classified.push((
                    ext_id,
                    decl,
                    Some(ExtensionState::Pending(PendingReason::TargetMissing)),
                ));
                continue;
            };
            if is_disabled(&target.id) {
                classified.push((
                    ext_id,
                    decl,
                    Some(ExtensionState::Pending(PendingReason::TargetDisabled)),
                ));
                continue;
            }
            // version 매칭. target.version, decl.version_req 모두 검증.
            let target_ver = match semver::Version::parse(&target.version) {
                Ok(v) => v,
                Err(_) => {
                    classified.push((
                        ext_id,
                        decl,
                        Some(ExtensionState::Pending(
                            PendingReason::InvalidTargetVersion {
                                target_version: target.version.clone(),
                            },
                        )),
                    ));
                    continue;
                }
            };
            let req = match semver::VersionReq::parse(&decl.version_req) {
                Ok(r) => r,
                Err(_) => {
                    // 매니페스트 검증에서 이미 걸렀어야 하지만 안전망.
                    classified.push((
                        ext_id,
                        decl,
                        Some(ExtensionState::Pending(PendingReason::VersionMismatch {
                            target_version: target.version.clone(),
                            required: decl.version_req.clone(),
                        })),
                    ));
                    continue;
                }
            };
            if !req.matches(&target_ver) {
                classified.push((
                    ext_id,
                    decl,
                    Some(ExtensionState::Pending(PendingReason::VersionMismatch {
                        target_version: target.version.clone(),
                        required: decl.version_req.clone(),
                    })),
                ));
                continue;
            }
            // Active 후보.
            classified.push((ext_id, decl, None));
        }

        // 4) 충돌 해결. target_id → 그 target에 대한 Active 후보 ext_id 일람.
        let mut candidates_by_target: HashMap<&str, Vec<&str>> = HashMap::new();
        for (ext_id, decl, cls) in &classified {
            if cls.is_none() {
                candidates_by_target
                    .entry(decl.plugin_id.as_str())
                    .or_default()
                    .push(ext_id);
            }
        }
        // 각 target에 대해 사전식 최솟값을 winner로 선택.
        let mut winners: HashMap<&str, &str> = HashMap::new();
        for (target, mut cands) in candidates_by_target {
            cands.sort_unstable();
            if let Some(winner) = cands.first() {
                winners.insert(target, *winner);
            }
        }

        // 5) 최종 상태 확정.
        for (ext_id, decl, cls) in classified {
            let state = match cls {
                Some(s) => s,
                None => {
                    let target =
                        target_for(&decl.plugin_id).expect("target present in active candidate");
                    let winner = winners.get(decl.plugin_id.as_str()).copied();
                    if winner == Some(ext_id) {
                        ExtensionState::Active {
                            target_id: decl.plugin_id.clone(),
                            target_version: target.version.clone(),
                        }
                    } else {
                        ExtensionState::Conflict {
                            other_extension_id: winner.unwrap_or("").to_string(),
                        }
                    }
                }
            };
            next.insert(ext_id.to_string(), state);
        }

        self.states = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_plugin_manifest::{
        Contributes, Entry, EventHookDecl, ExtendsDecl, HookMode, IpcHookDecl,
    };

    fn mk_manifest(id: &str, version: &str, extends: Option<ExtendsDecl>) -> Manifest {
        Manifest {
            manifest_version: 1,
            id: id.to_string(),
            name: id.to_string(),
            version: version.to_string(),
            authors: vec![],
            description: String::new(),
            homepage: String::new(),
            api_version: "1".to_string(),
            entry: Entry::Process {
                command: "x".to_string(),
                args: vec![],
            },
            surface_kinds: vec![],
            event_subscribe: vec![],
            event_publish: vec![],
            events_emitted: vec![],
            contributes: Contributes::default(),
            extends,
            lang_dir: "lang".to_string(),
            bundle: true,
        }
    }

    fn mk_extends(target: &str, req: &str) -> ExtendsDecl {
        ExtendsDecl {
            plugin_id: target.to_string(),
            version_req: req.to_string(),
            api_version: "1".to_string(),
            pre_event: vec![],
            post_event: vec![],
            pre_ipc: vec![IpcHookDecl {
                method: "foo.bar".to_string(),
                modifies: vec!["entry".into()],
                mode: HookMode::Transform,
                timeout_ms: 100,
            }],
            post_ipc: vec![],
        }
    }

    fn never_disabled(_: &str) -> bool {
        false
    }

    #[test]
    fn empty_manifests_yield_empty_registry() {
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[], &never_disabled);
        assert_eq!(reg.iter().count(), 0);
    }

    #[test]
    fn non_extension_plugin_is_not_tracked() {
        let m = mk_manifest("com.a.target", "1.0.0", None);
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&m], &never_disabled);
        assert!(reg.state("com.a.target").is_none());
    }

    #[test]
    fn active_when_target_compatible_and_enabled() {
        let target = mk_manifest("com.a.target", "1.2.0", None);
        let ext = mk_manifest(
            "com.b.ext",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0, <2.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &ext], &never_disabled);
        match reg.state("com.b.ext") {
            Some(ExtensionState::Active {
                target_id,
                target_version,
            }) => {
                assert_eq!(target_id, "com.a.target");
                assert_eq!(target_version, "1.2.0");
            }
            other => panic!("expected Active, got {other:?}"),
        }
        assert_eq!(
            reg.active_extension_for_target("com.a.target"),
            Some("com.b.ext")
        );
    }

    #[test]
    fn pending_when_target_missing() {
        let ext = mk_manifest(
            "com.b.ext",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&ext], &never_disabled);
        assert_eq!(
            reg.state("com.b.ext"),
            Some(&ExtensionState::Pending(PendingReason::TargetMissing))
        );
    }

    #[test]
    fn pending_when_target_disabled() {
        let target = mk_manifest("com.a.target", "1.0.0", None);
        let ext = mk_manifest(
            "com.b.ext",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &ext], &|id| id == "com.a.target");
        assert_eq!(
            reg.state("com.b.ext"),
            Some(&ExtensionState::Pending(PendingReason::TargetDisabled))
        );
    }

    #[test]
    fn pending_when_version_mismatch() {
        let target = mk_manifest("com.a.target", "0.5.0", None);
        let ext = mk_manifest(
            "com.b.ext",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &ext], &never_disabled);
        match reg.state("com.b.ext") {
            Some(ExtensionState::Pending(PendingReason::VersionMismatch {
                target_version,
                required,
            })) => {
                assert_eq!(target_version, "0.5.0");
                assert_eq!(required, ">=1.0.0");
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn pending_when_target_version_not_semver() {
        let target = mk_manifest("com.a.target", "not-a-version", None);
        let ext = mk_manifest(
            "com.b.ext",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &ext], &never_disabled);
        match reg.state("com.b.ext") {
            Some(ExtensionState::Pending(PendingReason::InvalidTargetVersion {
                target_version,
            })) => assert_eq!(target_version, "not-a-version"),
            other => panic!("expected InvalidTargetVersion, got {other:?}"),
        }
    }

    #[test]
    fn disabled_when_extension_user_disabled() {
        let target = mk_manifest("com.a.target", "1.0.0", None);
        let ext = mk_manifest(
            "com.b.ext",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &ext], &|id| id == "com.b.ext");
        assert_eq!(reg.state("com.b.ext"), Some(&ExtensionState::Disabled));
        assert!(reg.active_extension_for_target("com.a.target").is_none());
    }

    #[test]
    fn conflict_uses_lexicographic_winner() {
        let target = mk_manifest("com.a.target", "1.0.0", None);
        let b1 = mk_manifest(
            "com.b.alpha",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let b2 = mk_manifest(
            "com.b.beta",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &b1, &b2], &never_disabled);
        // alpha < beta alphabetically — alpha wins.
        assert!(matches!(
            reg.state("com.b.alpha"),
            Some(ExtensionState::Active { .. })
        ));
        match reg.state("com.b.beta") {
            Some(ExtensionState::Conflict { other_extension_id }) => {
                assert_eq!(other_extension_id, "com.b.alpha");
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
        assert_eq!(
            reg.active_extension_for_target("com.a.target"),
            Some("com.b.alpha")
        );
    }

    #[test]
    fn disabling_target_demotes_active_to_pending() {
        let target = mk_manifest("com.a.target", "1.0.0", None);
        let ext = mk_manifest(
            "com.b.ext",
            "0.1.0",
            Some(mk_extends("com.a.target", ">=1.0.0")),
        );
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &ext], &never_disabled);
        assert!(matches!(
            reg.state("com.b.ext"),
            Some(ExtensionState::Active { .. })
        ));
        // user disables target
        reg.recompute(&[&target, &ext], &|id| id == "com.a.target");
        assert_eq!(
            reg.state("com.b.ext"),
            Some(&ExtensionState::Pending(PendingReason::TargetDisabled))
        );
        // re-enable target
        reg.recompute(&[&target, &ext], &never_disabled);
        assert!(matches!(
            reg.state("com.b.ext"),
            Some(ExtensionState::Active { .. })
        ));
    }

    #[test]
    fn event_hook_extension_also_supported() {
        let target = mk_manifest("com.a.target", "1.0.0", None);
        let mut decl = mk_extends("com.a.target", ">=1.0.0");
        decl.pre_ipc.clear();
        decl.pre_event.push(EventHookDecl {
            event: "com.a.target.something".into(),
            modifies: vec!["payload".into()],
            mode: HookMode::Transform,
            timeout_ms: 100,
        });
        let ext = mk_manifest("com.b.ext", "0.1.0", Some(decl));
        let mut reg = ExtensionRegistry::new();
        reg.recompute(&[&target, &ext], &never_disabled);
        assert!(matches!(
            reg.state("com.b.ext"),
            Some(ExtensionState::Active { .. })
        ));
    }
}

//! `FileFormatRegistry` — 조회용 스냅샷 도메인.
//!
//! finalize 된 detector 하나와 그것을 만든 출처별 contribution 을 함께 돌려준다. 병합
//! 결과만 주면 "왜 이 값이 이겼는가" 를 밖에서 재현할 수 없다.
//!
//! 락은 한 구간이 아니다. `detector_snapshots` 는 먼저 `ensure_finalized` 를 부르고 —
//! 그것이 read 락으로 `dirty` 를 보고, 필요하면 놓은 뒤 write 락을 잡아 finalize 한다 —
//! 그 락을 놓은 **다음에** read 락을 새로 잡아 `finalized` 와 `contributions` 를 읽는다.
//! 두 칸은 마지막 read 락 하나 안에서 읽히지만, finalize 와 그 read 락 사이에 `dirty` 를
//! 세우는 쓰기(reload · plugin enable/disable · Settings 파일 탭 편집 등)가 끼면 `dirty`
//! 가 선 채로 새 `contributions` 와 **그 전 시점의** `finalized` 가 한 응답에 실린다. 그 경합이 없을 때만 두 칸이 같은
//! 시점이다. 기존 `detector()` · `list_detectors()` 도 같은 두 단계다.

use super::FileFormatRegistry;
use super::helpers::rule_kind_to_toml;
use crate::types::{DetectorRuleKind, FileFormatDetector, RuleOrigin};

/// 한 출처가 detector 하나에 기여한 내용 — 병합 전 원본.
#[derive(Debug, Clone)]
pub struct ContributionSnapshot {
    pub origin: RuleOrigin,
    pub display_name_i18n_key: Option<String>,
    pub icon: Option<String>,
    /// 그 출처가 적은 켜기/끄기 patch. `None` 은 "적지 않음" 이라 병합에서 무시된다.
    pub disabled: Option<bool>,
    pub rules: Vec<DetectorRuleKind>,
}

/// finalize 된 detector 와 그 contribution 들.
#[derive(Debug, Clone)]
pub struct DetectorSnapshot {
    pub detector: FileFormatDetector,
    /// 저장된 순서(설치 순서) 그대로다 — 병합 순서가 아니다. 병합이 이 목록을 어떤 순서로
    /// 읽는지는 finalize 의 몫이고, 그 결과가 `detector` 다.
    pub contributions: Vec<ContributionSnapshot>,
}

impl FileFormatRegistry {
    /// finalize 된 전 detector 를 id 순으로, 각자의 contribution 과 함께 돌려준다.
    pub fn detector_snapshots(&self) -> Vec<DetectorSnapshot> {
        self.ensure_finalized();
        let inner = self.lock_read();
        inner
            .finalized
            .iter()
            .map(|(id, det)| DetectorSnapshot {
                detector: det.clone(),
                contributions: inner
                    .contributions
                    .get(id)
                    .map(|cs| {
                        cs.iter()
                            .map(|c| ContributionSnapshot {
                                origin: c.origin.clone(),
                                display_name_i18n_key: c.display_name_i18n_key.clone(),
                                icon: c.icon.clone(),
                                disabled: c.disabled_override,
                                rules: c.rules.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect()
    }
}

impl DetectorRuleKind {
    /// 이 rule 을 user 설정 파일(`[[detector.rule]]`)과 같은 키로 적은 표.
    ///
    /// 저장(`export_user_config`)이 쓰는 변환과 같은 함수다 — 조회 출력과 저장 파일이
    /// 같은 rule 을 다른 모양으로 적지 않게 한다.
    pub fn to_config_table(&self) -> toml::value::Table {
        rule_kind_to_toml(self)
    }
}

impl RuleOrigin {
    /// 표시·조회용 짧은 이름 — `host` · `plugin:<id>` · `user`. Settings 의 출처 칸과 같은 표기다.
    pub fn label(&self) -> String {
        match self {
            RuleOrigin::HostDefault => "host".to_string(),
            RuleOrigin::Plugin(id) => format!("plugin:{id}"),
            RuleOrigin::User => "user".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::helpers::parse_detector_section;
    use super::*;
    use crate::types::DetectorId;

    const HOST: &str = r#"
[[detector]]
id = "fmt"
display_name_i18n_key = "host.key"

[[detector.rule]]
kind = "extension"
values = ["aa"]
"#;

    const PLUGIN: &str = r#"
[[detector]]
id = "fmt"
display_name_i18n_key = "plugin.key"
icon = "puzzle"
disabled = true

[[detector.rule]]
kind = "extension"
values = ["bb"]
"#;

    const USER: &str = r#"
[[detector]]
id = "fmt"
display_name_i18n_key = "user.key"

[[detector.rule]]
kind = "extension"
values = ["aa"]
"#;

    /// 부팅 순서(host → user → plugin)로 설치한 detector 의 스냅샷이 출처별 원본을 설치
    /// 순서대로 싣고, 병합 결과는 같은 시점의 `detector()` 와 같다.
    #[test]
    fn a_snapshot_carries_each_source_as_installed_and_the_merged_result() {
        let reg = FileFormatRegistry::new();
        reg.install_host_defaults(HOST);
        let dir = tempfile::tempdir().expect("tempdir");
        let user = dir.path().join("file-handlers.toml");
        std::fs::write(&user, USER).expect("user config");
        reg.install_user_config(&user);
        let decls = parse_detector_section(PLUGIN).expect("plugin decls");
        reg.install_plugin_detectors("p1", &decls);

        let snaps = reg.detector_snapshots();
        let snap = snaps
            .iter()
            .find(|s| s.detector.id == DetectorId("fmt".into()))
            .expect("fmt 스냅샷");

        let origins: Vec<String> = snap
            .contributions
            .iter()
            .map(|c| c.origin.label())
            .collect();
        assert_eq!(origins, ["host", "user", "plugin:p1"]);
        let plugin = &snap.contributions[2];
        assert_eq!(plugin.display_name_i18n_key.as_deref(), Some("plugin.key"));
        assert_eq!(plugin.icon.as_deref(), Some("puzzle"));
        assert_eq!(plugin.disabled, Some(true));
        assert_eq!(
            snap.contributions[0].disabled, None,
            "host 는 켜기/끄기를 안 적었다"
        );
        assert_eq!(
            snap.contributions[1].display_name_i18n_key.as_deref(),
            Some("user.key")
        );

        let merged = reg
            .detector(&DetectorId("fmt".into()))
            .expect("finalize 된 fmt");
        assert_eq!(
            snap.detector.display_name_i18n_key,
            merged.display_name_i18n_key
        );
        assert_eq!(snap.detector.icon, merged.icon);
        assert_eq!(snap.detector.disabled, merged.disabled);
        assert_eq!(snap.detector.rules.len(), merged.rules.len());
        assert_eq!(snap.detector.rules.len(), 2, "aa 는 한 번만 남는다");
    }

    #[test]
    fn a_rule_is_written_with_the_user_config_keys() {
        let t = DetectorRuleKind::Extension {
            values: vec!["md".into()],
        }
        .to_config_table();
        assert_eq!(t.get("kind").and_then(|v| v.as_str()), Some("extension"));
        assert_eq!(
            t.get("values").and_then(|v| v.as_array()).map(|a| a.len()),
            Some(1)
        );
    }
}

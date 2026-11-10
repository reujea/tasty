//! 원격 attach 연결 설정. 무응답 끊김 시간과 저장된 원격 행의 재연결 시간이다.
//!
//! 기본값은 `tasty_ipc::stream::HEARTBEAT_TIMEOUT` 과 같아야 하고 하한은 heartbeat 주기의 2배 이상이어야
//! 한다. 이 크레이트는 IPC 크레이트에 의존하지 않으므로 값을 따로 두고, 루트 패키지의 시험이 두 쪽을 대조한다.

use serde::{Deserialize, Serialize};

/// 무응답 끊김 시간의 기본값(초).
pub const DEFAULT_HEARTBEAT_TIMEOUT_SECS: u64 = 30;
/// 무응답 끊김 시간으로 받는 범위(초).
pub const HEARTBEAT_TIMEOUT_SECS_RANGE: (u64, u64) = (10, 600);
/// 재연결 시간의 기본값(초).
pub const DEFAULT_RECONNECT_TIMEOUT_SECS: u64 = 300;
/// 재연결 시간으로 받는 범위(초). 0이면 재연결하지 않는다.
pub const RECONNECT_TIMEOUT_SECS_RANGE: (u64, u64) = (0, 3600);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteAttachSettings {
    /// 이 프로세스가 상대의 프레임을 이 시간 동안 받지 못하면 연결을 끊는다. 상대와 협상하지 않으며
    /// 새 연결부터 적용한다.
    pub heartbeat_timeout_secs: u64,
    /// 저장된 원격 행의 연결이 끊긴 뒤 자동 재연결을 시도하는 시간. 끊김을 판정한 때부터 세며,
    /// 지나면 형상을 남긴 끊긴 행이 된다. 0이면 바로 끊긴 행이 된다.
    pub reconnect_timeout_secs: u64,
}

impl Default for RemoteAttachSettings {
    fn default() -> Self {
        Self {
            heartbeat_timeout_secs: DEFAULT_HEARTBEAT_TIMEOUT_SECS,
            reconnect_timeout_secs: DEFAULT_RECONNECT_TIMEOUT_SECS,
        }
    }
}

impl RemoteAttachSettings {
    /// 범위를 확인한다. 적용과 파일 읽기가 같은 검사를 쓴다.
    pub fn validate(&self) -> Result<(), String> {
        for (key, value, (lo, hi)) in [
            (
                "heartbeat_timeout_secs",
                self.heartbeat_timeout_secs,
                HEARTBEAT_TIMEOUT_SECS_RANGE,
            ),
            (
                "reconnect_timeout_secs",
                self.reconnect_timeout_secs,
                RECONNECT_TIMEOUT_SECS_RANGE,
            ),
        ] {
            if !(lo..=hi).contains(&value) {
                return Err(format!(
                    "remote_attach.{key} {value} is outside {lo}..={hi}"
                ));
            }
        }
        Ok(())
    }

    /// 설정 파일의 값이 검사를 통과하지 못하면 기본값으로 되돌린다.
    pub(crate) fn normalize(&mut self, changed: &mut bool) {
        if let Err(e) = self.validate() {
            tracing::warn!("{e} → remote_attach reset to defaults");
            *self = Self::default();
            *changed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Settings;

    use super::*;

    #[test]
    fn a_missing_key_or_empty_section_uses_the_default() {
        for text in ["", "[remote_attach]"] {
            let parsed: Settings = toml::from_str(text).unwrap();
            assert_eq!(parsed.remote_attach.heartbeat_timeout_secs, 30, "{text:?}");
        }
    }

    #[test]
    fn the_value_survives_a_roundtrip() {
        let mut settings = Settings::default();
        settings.remote_attach.heartbeat_timeout_secs = 42;
        let text = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&text).unwrap();
        assert_eq!(parsed.remote_attach.heartbeat_timeout_secs, 42);
    }

    #[test]
    fn the_range_bounds_are_inclusive() {
        for (secs, ok) in [
            (9, false),
            (10, true),
            (600, true),
            (601, false),
            (0, false),
        ] {
            let settings = RemoteAttachSettings {
                heartbeat_timeout_secs: secs,
                ..Default::default()
            };
            assert_eq!(settings.validate().is_ok(), ok, "{secs}");
        }
    }

    #[test]
    fn normalize_reports_and_restores_an_out_of_range_value() {
        for secs in [9, 601] {
            let mut settings: Settings = toml::from_str(&format!(
                "[remote_attach]\nheartbeat_timeout_secs = {secs}\n"
            ))
            .unwrap();
            assert!(settings.validate_runtime().is_err());
            assert!(settings.normalize().changed, "{secs}");
            assert_eq!(settings.remote_attach.heartbeat_timeout_secs, 30);
        }
    }

    #[test]
    fn normalize_keeps_a_value_in_range() {
        let mut settings = Settings::default();
        settings.remote_attach.heartbeat_timeout_secs = 12;
        assert!(!settings.normalize().changed);
        assert_eq!(settings.remote_attach.heartbeat_timeout_secs, 12);
    }

    #[test]
    fn the_reconnect_window_defaults_to_five_minutes_and_accepts_zero_to_an_hour() {
        let parsed: Settings = toml::from_str("[remote_attach]").unwrap();
        assert_eq!(parsed.remote_attach.reconnect_timeout_secs, 300);
        for (secs, ok) in [(0, true), (3600, true), (3601, false)] {
            let settings = RemoteAttachSettings {
                reconnect_timeout_secs: secs,
                ..Default::default()
            };
            assert_eq!(settings.validate().is_ok(), ok, "{secs}");
        }
        let mut settings: Settings =
            toml::from_str("[remote_attach]\nreconnect_timeout_secs = 4000\n").unwrap();
        assert!(settings.normalize().changed);
        assert_eq!(settings.remote_attach.reconnect_timeout_secs, 300);
    }
}

//! 설정의 attach 무응답 끊김 시간을 새 연결이 읽는 공유 값으로 옮긴다.

use std::time::Duration;

use tasty_settings::RemoteAttachSettings;

/// 검사를 통과하지 못한 값(직접 고친 설정 파일 등)은 기본값으로 바꾼다.
pub(crate) fn timeout(settings: &RemoteAttachSettings) -> Duration {
    if let Err(e) = settings.validate() {
        tracing::warn!("{e}; attach connections use the default heartbeat timeout");
        return tasty_ipc::stream::HEARTBEAT_TIMEOUT;
    }
    Duration::from_secs(settings.heartbeat_timeout_secs)
}

/// 이후에 맺는 연결의 제한 시간과 재연결 시간을 바꾼다. 이미 열린 소켓의 timeout은 그대로 둔다.
/// 재연결 시간은 진행 중인 재연결에도 다음 판정부터 적용한다.
pub(crate) fn apply(core: &crate::app::services::AppServices, settings: &tasty_settings::Settings) {
    core.attach_heartbeat.set(timeout(&settings.remote_attach));
    let window = if settings.remote_attach.validate().is_ok() {
        settings.remote_attach.reconnect_timeout_secs
    } else {
        tasty_settings::DEFAULT_RECONNECT_TIMEOUT_SECS
    };
    core.reconnect_window_secs
        .store(window, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use tasty_ipc::stream::{HEARTBEAT_INTERVAL, HEARTBEAT_TIMEOUT};

    use super::*;

    /// 설정 크레이트는 IPC 크레이트에 의존하지 않아 같은 숫자를 따로 둔다.
    #[test]
    fn the_settings_numbers_match_the_stream_contract() {
        assert_eq!(
            Duration::from_secs(tasty_settings::DEFAULT_HEARTBEAT_TIMEOUT_SECS),
            HEARTBEAT_TIMEOUT
        );
        assert_eq!(timeout(&RemoteAttachSettings::default()), HEARTBEAT_TIMEOUT);
        assert!(
            Duration::from_secs(tasty_settings::HEARTBEAT_TIMEOUT_SECS_RANGE.0)
                >= HEARTBEAT_INTERVAL * 2
        );
    }

    #[test]
    fn an_out_of_range_value_falls_back_to_the_default() {
        let bad = RemoteAttachSettings {
            heartbeat_timeout_secs: 5,
            ..Default::default()
        };
        assert_eq!(timeout(&bad), HEARTBEAT_TIMEOUT);
        let good = RemoteAttachSettings {
            heartbeat_timeout_secs: 12,
            ..Default::default()
        };
        assert_eq!(timeout(&good), Duration::from_secs(12));
    }
}

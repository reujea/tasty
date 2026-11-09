//! 호스트가 전달한 플러그인 환경변수를 읽는다.
//!
//! 호스트는 spawn 시 다음 환경변수를 주입한다 (`docs/dev-guide/plugin-development.md`
//! §7 "spawn 시 주입 환경변수" 참조):
//! - `TASTY_PLUGIN_ID`
//! - `TASTY_PLUGIN_DIR`
//! - `TASTY_PLUGIN_DATA_DIR`
//! - `TASTY_PLUGIN_CONFIG_PATH`
//! - `TASTY_PLUGIN_LOG_PATH`
//! - `TASTY_HOST_IPC_PORT`
//! - `TASTY_PLUGIN_TOKEN`
//! - `TASTY_HOST_API_VERSION`
//! - `TASTY_LOCALE` (선택) — 활성 언어 코드 (예: `en`, `ko`, `ja`, `zh-Hans`). 미주입 시 `en` fallback.
//!   출처는 host 설정 `general.language` — host 본 바이너리가 부팅 시 자기 프로세스 env 에
//!   set 하고(`src/boot/locale.rs`) host-plugin 이 spawn 시 그대로 propagate 한다
//!   (`crates/tasty-host-plugin/src/process.rs`). spawn 시점 고정이라 언어 변경은 host
//!   재시작 후 반영된다.
//! - `TASTY_LOCALE_FONT` (선택) — 언어팩이 제공하는 폰트 파일의 절대경로. 언어팩 폰트가
//!   resolve 됐을 때만 주입되며, 없으면 변수 자체가 없다(빈 값 없음). 출처는 `TASTY_LOCALE`
//!   과 같다.
//! - `TASTY_LOCALE_FONT_FACE` (선택) — `TASTY_LOCALE_FONT` 파일 안에서 읽을 글꼴 번호.
//!   `TASTY_LOCALE_FONT` 와 함께만 주입된다.
//! - `TASTY_PLUGIN_HANDLE_ENDPOINT` (선택) — Unix는 socket path, Windows는 Named Pipe 이름.
//!   호스트가 보조 핸들 채널을 활성화한 경우에만 주입된다. plugin은 이 값이 있으면
//!   메인 채널 인증 후 보조 채널에도 connect를 시도한다.

use std::path::PathBuf;

use crate::error::{PluginError, Result};

#[derive(Debug, Clone)]
pub struct PluginEnv {
    pub plugin_id: String,
    pub host_port: u16,
    pub token: String,
    pub host_api_version: String,
    pub plugin_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    /// 활성 언어 코드 (예: `en`, `ko`, `ja`). 호스트가 부팅 시 `general.language` 에서
    /// 확정해 `TASTY_LOCALE` 로 주입(모듈 doc 참조). 미주입 시 `"en"`. plugin 이 자체
    /// lang 파일을 로드할 때 사용.
    pub locale: String,
    /// 언어팩이 제공하는 폰트 파일의 절대경로. 호스트가 언어팩 폰트를 resolve 했을 때만
    /// `TASTY_LOCALE_FONT` 로 주입 — 없으면 `None`(내장 폰트 · 언어팩 미사용).
    pub locale_font: Option<PathBuf>,
    /// `locale_font` 파일 안에서 읽을 글꼴 번호. `TASTY_LOCALE_FONT_FACE` 로 주입되며 없으면 0.
    pub locale_font_face: u32,
    /// 보조 핸들 채널 endpoint. 호스트가 활성화한 경우에만 `Some`.
    /// Unix는 socket path (`/tmp/.../tasty-handle.sock`), Windows는 Named Pipe 이름
    /// (`\\.\pipe\tasty-handle-<random>`).
    pub handle_endpoint: Option<String>,
}

impl PluginEnv {
    /// 환경변수에서 plugin 부트스트랩 정보를 읽는다. 필수 변수가 없으면 에러.
    pub fn load() -> Result<Self> {
        let plugin_id = std::env::var("TASTY_PLUGIN_ID")
            .map_err(|_| PluginError::EnvMissing("TASTY_PLUGIN_ID"))?;
        let port_raw = std::env::var("TASTY_HOST_IPC_PORT")
            .map_err(|_| PluginError::EnvMissing("TASTY_HOST_IPC_PORT"))?;
        let host_port = port_raw.parse::<u16>().map_err(|e| PluginError::EnvParse {
            var: "TASTY_HOST_IPC_PORT",
            message: e.to_string(),
        })?;
        let token = std::env::var("TASTY_PLUGIN_TOKEN")
            .map_err(|_| PluginError::EnvMissing("TASTY_PLUGIN_TOKEN"))?;
        let host_api_version =
            std::env::var("TASTY_HOST_API_VERSION").unwrap_or_else(|_| "1".to_string());
        Ok(Self {
            plugin_id,
            host_port,
            token,
            host_api_version,
            plugin_dir: std::env::var_os("TASTY_PLUGIN_DIR").map(PathBuf::from),
            data_dir: std::env::var_os("TASTY_PLUGIN_DATA_DIR").map(PathBuf::from),
            config_path: std::env::var_os("TASTY_PLUGIN_CONFIG_PATH").map(PathBuf::from),
            log_path: std::env::var_os("TASTY_PLUGIN_LOG_PATH").map(PathBuf::from),
            locale: std::env::var("TASTY_LOCALE").unwrap_or_else(|_| "en".to_string()),
            locale_font: std::env::var_os("TASTY_LOCALE_FONT")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            locale_font_face: std::env::var("TASTY_LOCALE_FONT_FACE")
                .ok()
                .and_then(|face| face.parse().ok())
                .unwrap_or(0),
            handle_endpoint: std::env::var("TASTY_PLUGIN_HANDLE_ENDPOINT")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }
}

//! `[[surface_kinds]]`의 host 기능 능력 선언. host는 kind 이름이나 plugin id가 아니라 이 선언으로
//! 기능을 켠다. 공식 plugin도 커스텀 plugin과 같은 키로 선언한다.

use serde::Deserialize;

/// 각 키는 쓰지 않으면 `None`이다. host는 선언하지 않은 키를 자기 규칙으로 해석한다.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub struct SurfaceKindCapabilityDecl {
    /// 탭 위의 주소 띠, 주소창 포커스 단축키, 생성 인자 `url`의 문서 출처 기록.
    #[serde(default)]
    pub address_band: Option<bool>,
    /// 주소 띠 아래에 쌓는 크기·원격·스크립트 안내 띠. 그 높이만큼 페이지를 내린다.
    #[serde(default)]
    pub inset_notices: Option<bool>,
    /// 문서 단위 스크립트 감지·허용 게이트와 그 IPC.
    #[serde(default)]
    pub script_gate: Option<bool>,
    /// mirror workspace의 새 탭 메뉴에서 원격 파일을 골라 이 kind로 연다.
    #[serde(default)]
    pub opens_url: Option<bool>,
    /// attach client에 이 kind 문서의 HTTP 제공·자원 전송을 허용한다.
    #[serde(default)]
    pub remote_http_serve: Option<bool>,
    /// attach에서 원격 문서를 이 kind의 viewer로 보인다(`remote.html_entry` 생성 인자).
    #[serde(default)]
    pub mirror_viewer: Option<bool>,
    /// attach에서 서버가 이 kind의 파일 원문을 제공하고, client가 이 kind로 원문 mirror를 만든다
    /// (`remote.file` 생성 인자).
    #[serde(default)]
    pub mirror_content: Option<bool>,
    /// 포커스를 띠가 아니라 바탕색으로 보인다. 페이지가 바탕을 host 테마 값으로 칠해야 한다.
    #[serde(default)]
    pub bed_focus: Option<bool>,
}

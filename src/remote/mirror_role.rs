//! attach descriptor의 문서 mirror role. role은 연결 방식(원문 요청·viewer 협상)을 고르고,
//! client가 만들 kind는 descriptor의 `kind`와 그 kind가 선언한 능력으로 고른다.
//! role 값은 다른 빌드의 상대와 계속 붙도록 처음 정한 글자를 그대로 쓴다.

/// 서버가 파일 원문을 제공하는 문서(`mirror_content`).
pub(crate) const CONTENT: &str = "markdown";
/// 서버의 문서를 HTTP viewer로 보이는 문서(`mirror_viewer`).
pub(crate) const VIEWER: &str = "html";

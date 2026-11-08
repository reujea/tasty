//! 매니페스트의 detector·handler JSON을 파일 도메인 타입으로 읽고 검증한다.
//! manifest 크레이트가 본체의 파일 도메인에 의존하지 않도록 검증을 여기서 보완한다.

use anyhow::Result;

/// Manifest::load 뒤에 공식 id 이름 공간과 detector·handler, surface kind·IPC namespace 참조를
/// 검증한다. 설치와 Add 미리보기가 함께 부르므로 둘이 같은 판정과 같은 문장을 쓴다.
pub fn validate_bin_extras(manifest: &crate::plugin::Manifest) -> Result<()> {
    // 서명 판정과 신뢰 기록보다 먼저 거부한다. 오류 형식은 IPC 응답의 원인 이름에 쓰인다.
    tasty_host_plugin::official::admit_plugin_id(&manifest.id)?;
    validate_detector_actual(&manifest.contributes.detector)?;
    let surface_kinds: Vec<String> = manifest
        .surface_kinds
        .iter()
        .map(|k| k.kind.clone())
        .collect();
    let ipc_prefixes: Vec<String> = manifest
        .contributes
        .ipc_namespace
        .iter()
        .map(|n| n.prefix.clone())
        .collect();
    validate_handler_actual(&manifest.contributes.handler, &surface_kinds, &ipc_prefixes)?;
    Ok(())
}

pub fn validate_detector_actual(raw: &[serde_json::Value]) -> Result<()> {
    for v in raw {
        let decl: crate::file::format::config::DetectorDecl = serde_json::from_value(v.clone())
            .map_err(|e| anyhow::anyhow!("invalid detector entry shape: {e}"))?;
        crate::file::format::config::validate_detector_decl(&decl, true)
            .map_err(|e| anyhow::anyhow!("contributes.detector '{}': {e}", decl.id))?;
    }
    Ok(())
}

pub fn validate_handler_actual(
    raw: &[serde_json::Value],
    surface_kinds: &[String],
    ipc_prefixes: &[String],
) -> Result<()> {
    for v in raw {
        let decl: crate::file::handler::config::HandlerDecl<
            crate::file::handler::config::PluginHandlerActionDecl,
        > = serde_json::from_value(v.clone())
            .map_err(|e| anyhow::anyhow!("invalid handler entry shape: {e}"))?;
        crate::file::handler::config::validate_plugin_handler_decl(&decl)
            .map_err(|e| anyhow::anyhow!("contributes.handler: {e}"))?;
        crate::file::handler::config::validate_plugin_handler_refs(
            &decl,
            surface_kinds,
            ipc_prefixes,
        )
        .map_err(|e| anyhow::anyhow!("contributes.handler: {e}"))?;
    }
    Ok(())
}

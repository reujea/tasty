use serde_json::json;

use crate::runtime::engine_access::EngineMut;
use tasty_ipc::output_cursor;
use tasty_ipc::protocol::JsonRpcResponse;

use super::require_surface_id;
use crate::ipc::handler::params::opt_int;
use tasty_terminal::OUTPUT_RETENTION_MAX_BYTES;

pub(crate) fn handle_set_mark(
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let _ = engine; // handler 는 enqueue 만. cascade 가 적용.
    out.push(crate::app::command::DomainIntent::SetTerminalMark { surface_id }.from_agent_ipc());
    JsonRpcResponse::success(id, json!({ "ok": true, "surface_id": surface_id }))
}

/// cursor가 없으면 공유 마크부터, 있으면 호출자가 지정한 위치부터 원문 출력을 읽는다.
/// cursor를 쓰는 소비자는 서버의 공유 마크를 바꾸지 않아 서로 독립적으로 읽을 수 있다.
/// 두 방식 모두 보관 범위·다음 위치·유실 바이트·스트림 식별자를 반환한다.
/// surface ID 재사용이나 터미널 재시작을 구분하도록 cursor에는 stream도 필요하다(docs/features/terminal-output/index.md).
pub(crate) fn handle_read_since_mark(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };

    let OutputReadParams { req, continuing } = match OutputReadParams::parse(params, &id) {
        Ok(v) => v,
        Err(e) => return *e,
    };
    read_output_response(engine, id, surface_id, &req, continuing)
}

/// 원문 조회 한 번의 응답. attach holder 의 원본 읽기(`crate::remote::terminal_read`)도 같은 응답을 쓴다.
pub(crate) fn read_output_response(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    surface_id: u32,
    req: &tasty_terminal::OutputReadRequest,
    continuing: bool,
) -> JsonRpcResponse {
    match engine.read_output(surface_id, req) {
        Some(Ok(read)) => answered(id, surface_id, read),
        Some(Err(tasty_terminal::OutputReadError::StreamMismatch { expected, actual })) => refused(
            id,
            -32602,
            format!(
                "'stream' was given as '{expected}' but surface {surface_id} is \
                 streaming '{actual}' — the surface id was reused or its terminal \
                 was respawned. Read once without 'cursor' to start over on this \
                 stream"
            ),
            "stream_mismatch",
            json!({ "stream": actual }),
        ),
        Some(Err(tasty_terminal::OutputReadError::AheadOfStream {
            cursor,
            retention_end,
        })) => refused(
            id,
            -32602,
            format!(
                "'cursor' was given as {cursor} but surface {surface_id} has only \
                 produced {retention_end} bytes. Answering it empty would leave \
                 the consumer waiting for a continuation that, once that much \
                 output arrives, is unrelated to what it last saw"
            ),
            "cursor_ahead_of_stream",
            json!({ "retention_end": retention_end }),
        ),
        // 이어 읽기는 터미널 부재를 거절한다. 위치 없는 기존 요청에는 빈 text를 반환한다.
        None if continuing => refused(
            id,
            -32000,
            format!("surface {surface_id} has no terminal to read a position from"),
            "no_terminal",
            serde_json::Value::Null,
        ),
        None => JsonRpcResponse::success(id, json!({ "text": "", "surface_id": surface_id })),
    }
}

/// next_cursor와 raw_bytes는 버퍼의 값을 그대로 쓴다. 디코딩·ANSI 제거 후 text 길이는
/// 원문 바이트 수와 다르므로 다음 위치 계산에 사용할 수 없다.
fn answered(
    id: serde_json::Value,
    surface_id: u32,
    read: tasty_terminal::OutputRead,
) -> JsonRpcResponse {
    JsonRpcResponse::success(id, read_fields(surface_id, read))
}

/// 원문 읽기 응답의 공통 키. 위치 읽기와 스캔 읽기가 같은 이름·뜻으로 싣는다.
fn read_fields(surface_id: u32, read: tasty_terminal::OutputRead) -> serde_json::Value {
    json!({
        "text": read.text,
        "surface_id": surface_id,
        "stream": read.stream,
        "cursor": read.cursor,
        "next_cursor": read.next_cursor,
        "raw_bytes": read.raw_bytes,
        "retention_start": read.retention_start,
        "retention_end": read.retention_end,
        "skipped": read.skipped,
    })
}

/// GUI와 헤드리스가 공유하는 원문 출력 조회 인자.
pub(super) struct OutputReadParams {
    pub(super) req: tasty_terminal::OutputReadRequest,
    /// cursor/stream 중 하나라도 지정했는지. 터미널이 없으면 이어 읽기를 거절한다.
    pub(super) continuing: bool,
}

impl OutputReadParams {
    pub(super) fn parse(
        params: &serde_json::Value,
        id: &serde_json::Value,
    ) -> Result<Self, Box<JsonRpcResponse>> {
        let strip_ansi = params
            .get("strip_ansi")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let cursor = opt_int::<u64>(params, output_cursor::PARAM_CURSOR, id).map_err(Box::new)?;
        // 보관 크기보다 많이 읽을 수 없으므로 같은 상수를 상한으로 쓴다.
        let max_bytes = opt_int::<usize>(params, output_cursor::PARAM_MAX_BYTES, id)
            .map_err(Box::new)?
            .unwrap_or(OUTPUT_RETENTION_MAX_BYTES);
        let stream = params
            .get(output_cursor::PARAM_STREAM)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        if cursor.is_some() && stream.is_none() {
            return Err(Box::new(refused(
                id.clone(),
                -32602,
                "'cursor' was given without 'stream'. A surface id is reused when a \
                 surface closes and another opens, and surface.respawn_terminal \
                 replaces the terminal under an id that does not change, so without \
                 the token a stale position would be applied to another stream's \
                 output in silence. Read once without 'cursor' to get both",
                "cursor_without_stream",
                serde_json::Value::Null,
            )));
        }

        Ok(Self {
            continuing: cursor.is_some() || stream.is_some(),
            req: tasty_terminal::OutputReadRequest {
                from: match cursor {
                    Some(at) => tasty_terminal::OutputCursor::At(at),
                    None => tasty_terminal::OutputCursor::Mark,
                },
                max_bytes,
                strip_ansi,
                expect_stream: stream,
            },
        })
    }
}

/// 호출자가 메시지를 파싱하지 않도록 오류별 reason도 반환한다.
fn refused(
    id: serde_json::Value,
    code: i32,
    message: impl Into<String>,
    reason: &str,
    extra: serde_json::Value,
) -> JsonRpcResponse {
    let mut data = json!({ "reason": reason });
    if let (Some(obj), Some(more)) = (data.as_object_mut(), extra.as_object()) {
        for (k, v) in more {
            obj.insert(k.clone(), v.clone());
        }
    }
    JsonRpcResponse::error_with_data(id, code, message, data)
}

/// 스캐너 전용 커서를 읽고 전진시킨다. 사용자 마크와 별개다.
/// 소비자가 둘이면 서로 읽을 구간을 건너뛰게 되므로 하나만 사용해야 한다(docs/features/terminal-output/index.md).
/// 응답은 위치 읽기와 같은 연속성 키를 늘 싣고, 출처 정보(`text_origins`)는 로컬 터미널에서 정할 수 있을
/// 때만 싣는다. mirror는 로컬 사본이라 출처를 믿을 수 없어 계산하지 않는다(docs/reference/api.md).
pub(crate) fn handle_read_since_scan_mark(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };

    let strip_ansi = params
        .get("strip_ansi")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let with_origins = !engine.is_mirror_surface(surface_id);
    let Some(scan) = engine.take_since_output_scan_mark(surface_id, strip_ansi, with_origins)
    else {
        return JsonRpcResponse::success(id, json!({ "text": "", "surface_id": surface_id }));
    };
    let mut result = read_fields(surface_id, scan.read);
    if let Some(origins) = scan.origins {
        result["text_origins"] = text_origins_json(&origins);
    }
    JsonRpcResponse::success(id, result)
}

fn text_origins_json(origins: &tasty_terminal::TextOrigins) -> serde_json::Value {
    let ranges: Vec<serde_json::Value> = origins
        .ranges
        .iter()
        .map(|r| json!({ "start": r.start, "end": r.end, "origin": r.origin.as_str() }))
        .collect();
    let gaps: Vec<serde_json::Value> = origins
        .gaps
        .iter()
        .map(|g| json!({ "at": g.at, "reason": g.reason.as_str() }))
        .collect();
    json!({ "ranges": ranges, "gaps": gaps })
}

/// `surface.parse_since_mark` — read_since_mark 결과를 `tasty-output` 빌트인
/// 파서들로 분해. `parsers` 가 생략되면 `DEFAULT_PARSER_IDS` 사용. `prompt_boundary`
/// /`exit_code` 같이 ANSI escape 자체가 의미인 파서를 쓸 수 있도록 raw 텍스트
/// (strip_ansi=false) 를 항상 입력으로 한다.
pub(crate) fn handle_parse_since_mark(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };

    let parser_ids: Vec<String> = match params.get("parsers") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => tasty_output::DEFAULT_PARSER_IDS
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };

    let text = engine.read_since_mark_of(surface_id, false);
    let items = match tasty_output::parse_buffer(&text, parser_ids.iter().map(String::as_str)) {
        Ok(v) => v,
        Err(unknown) => {
            return JsonRpcResponse::invalid_params(id, format!("unknown parser: '{unknown}'"));
        }
    };

    JsonRpcResponse::success(
        id,
        json!({
            "surface_id": surface_id,
            "parsers": parser_ids,
            "items": items,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{OUTPUT_RETENTION_MAX_BYTES, OutputReadParams};
    use serde_json::json;

    fn ok(params: serde_json::Value) -> OutputReadParams {
        OutputReadParams::parse(&params, &json!(1)).expect("정상 인자")
    }

    fn reason(params: serde_json::Value) -> String {
        let refusal = match OutputReadParams::parse(&params, &json!(1)) {
            Ok(_) => panic!("거절할 인자를 받아들였다"),
            Err(e) => *e,
        };
        refusal
            .error
            .expect("에러 몸통")
            .data
            .expect("사유")
            .get("reason")
            .and_then(|v| v.as_str())
            .expect("reason 칸")
            .to_string()
    }

    #[test]
    fn no_cursor_arguments_reads_the_server_held_mark_exactly_as_before() {
        let p = ok(json!({}));
        assert!(matches!(p.req.from, tasty_terminal::OutputCursor::Mark));
        assert!(!p.continuing, "이어 읽기 인자가 없는데 continuing이 true다");
        assert!(p.req.expect_stream.is_none());
        assert!(!p.req.strip_ansi);
        assert_eq!(
            p.req.max_bytes, OUTPUT_RETENTION_MAX_BYTES,
            "상한을 안 주면 보존 전체 — 예전 응답과 같은 text 가 나와야 한다"
        );
    }

    #[test]
    fn a_cursor_without_a_stream_token_is_refused_rather_than_guessed() {
        assert_eq!(reason(json!({ "cursor": 12 })), "cursor_without_stream");
    }

    #[test]
    fn a_stream_token_alone_is_a_continuation_too() {
        let p = ok(json!({ "stream": "abc" }));
        assert!(matches!(p.req.from, tasty_terminal::OutputCursor::Mark));
        assert!(p.continuing);
        assert_eq!(p.req.expect_stream.as_deref(), Some("abc"));
    }

    #[test]
    fn an_empty_stream_token_is_the_same_as_not_giving_one() {
        assert_eq!(
            reason(json!({ "cursor": 3, "stream": "" })),
            "cursor_without_stream"
        );
    }

    #[test]
    fn a_position_that_is_not_a_whole_number_is_refused_rather_than_coerced() {
        for bad in [json!("12"), json!(-1), json!(1.5)] {
            let r = OutputReadParams::parse(&json!({ "cursor": bad, "stream": "s" }), &json!(1));
            assert!(r.is_err(), "cursor={bad} 를 받아들였다");
        }
    }

    #[test]
    fn max_bytes_is_carried_through_and_the_buffer_clamps_it() {
        assert_eq!(ok(json!({ "max_bytes": 0 })).req.max_bytes, 0);
        assert_eq!(
            ok(json!({ "max_bytes": 999_999_999u64 })).req.max_bytes,
            999_999_999
        );
    }

    // capability에서 선언한 인자 이름을 실제 파서가 읽는지 확인한다.
    #[test]
    fn the_declared_cursor_contract_names_the_arguments_this_parser_reads() {
        use tasty_ipc::output_cursor::{PARAM_CURSOR, PARAM_MAX_BYTES, PARAM_STREAM};
        let p = ok(json!({ PARAM_CURSOR: 7, PARAM_STREAM: "s", PARAM_MAX_BYTES: 9 }));
        assert!(matches!(p.req.from, tasty_terminal::OutputCursor::At(7)));
        assert_eq!(p.req.expect_stream.as_deref(), Some("s"));
        assert_eq!(p.req.max_bytes, 9);
        assert!(
            tasty_ipc::capability::CAPABILITIES
                .iter()
                .any(|c| c.name == tasty_ipc::output_cursor::CAPABILITY),
            "파서가 읽는 계약이 협상 목록에 없다"
        );
    }

    #[test]
    fn the_wire_carries_the_buffer_s_raw_advance_and_not_the_length_of_the_text() {
        let read = tasty_terminal::OutputRead {
            text: "red".to_string(),
            raw_bytes: 12,
            cursor: 100,
            next_cursor: 112,
            retention_start: 0,
            retention_end: 112,
            skipped: 7,
            stream: "s1".to_string(),
        };
        let resp = super::answered(json!(1), 3, read);
        let r = resp.result.expect("성공 응답");
        assert_eq!(r["text"].as_str(), Some("red"));
        assert_eq!(r["raw_bytes"].as_u64(), Some(12));
        assert_eq!(
            r["next_cursor"].as_u64(),
            Some(112),
            "반환 text 길이(3)가 아닌 읽은 구간 길이(12)만큼 커서를 이동해야 한다"
        );
        assert_eq!(r["cursor"].as_u64(), Some(100));
        assert_eq!(r["skipped"].as_u64(), Some(7));
        assert_eq!(r["retention_start"].as_u64(), Some(0));
        assert_eq!(r["retention_end"].as_u64(), Some(112));
        assert_eq!(r["stream"].as_str(), Some("s1"));
        assert_eq!(r["surface_id"].as_u64(), Some(3));
    }
}

#[cfg(test)]
#[path = "mark_scan_tests.rs"]
mod scan_tests;

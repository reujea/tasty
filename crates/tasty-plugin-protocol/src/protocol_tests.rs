//! `protocol_tests` 단위 테스트.

use super::*;
use serde_json::Value;

#[test]
fn auth_round_trip() {
    let msg = AuthMessage {
        plugin_id: "com.example.x".into(),
        token: "abc123".into(),
    };
    let s = serde_json::to_string(&msg).unwrap();
    let parsed: AuthMessage = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed.plugin_id, "com.example.x");
    assert_eq!(parsed.token, "abc123");
}

#[test]
fn event_hello_round_trip() {
    let ev = PluginEvent::Hello {
        plugin_id: "com.example.x".into(),
        version: "0.1.0".into(),
    };
    let s = serde_json::to_string(&ev).unwrap();
    assert!(s.contains("\"kind\":\"hello\""));
    let parsed: PluginEvent = serde_json::from_str(&s).unwrap();
    match parsed {
        PluginEvent::Hello { plugin_id, version } => {
            assert_eq!(plugin_id, "com.example.x");
            assert_eq!(version, "0.1.0");
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn response_with_error() {
    let s = r#"{"id":42,"error":"boom"}"#;
    let parsed: PluginResponse = serde_json::from_str(s).unwrap();
    assert_eq!(parsed.id, 42);
    assert_eq!(parsed.error.as_deref(), Some("boom"));
    assert!(parsed.result.is_none());
    assert!(parsed.error_code.is_none());
}

#[test]
fn response_with_result() {
    let s = r#"{"id":7,"result":{"ok":true}}"#;
    let parsed: PluginResponse = serde_json::from_str(s).unwrap();
    assert_eq!(parsed.id, 7);
    assert!(parsed.result.is_some());
    assert!(parsed.error.is_none());
    assert!(parsed.error_code.is_none());
}

#[test]
fn response_with_error_code() {
    let resp = PluginResponse {
        id: 1,
        result: None,
        error: Some("method not found".into()),
        error_code: Some(-32601),
    };
    let s = serde_json::to_string(&resp).unwrap();
    assert!(s.contains("\"error_code\":-32601"));
    let parsed: PluginResponse = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed.error_code, Some(-32601));
}

#[test]
fn auth_ack_envelope_round_trip() {
    let env = AuthAckEnvelope {
        auth_ack: AuthAck {
            ok: false,
            reason: Some("token mismatch".into()),
        },
    };
    let s = serde_json::to_string(&env).unwrap();
    assert!(s.contains("\"auth_ack\""));
    assert!(s.contains("\"ok\":false"));
    let parsed: AuthAckEnvelope = serde_json::from_str(&s).unwrap();
    assert!(!parsed.auth_ack.ok);
    assert_eq!(parsed.auth_ack.reason.as_deref(), Some("token mismatch"));
}

#[test]
fn auth_ack_omits_reason_when_ok() {
    let env = AuthAckEnvelope {
        auth_ack: AuthAck {
            ok: true,
            reason: None,
        },
    };
    let s = serde_json::to_string(&env).unwrap();
    assert!(!s.contains("reason"));
}

#[test]
fn response_omits_error_code_when_none() {
    let resp = PluginResponse {
        id: 1,
        result: Some(Value::Null),
        error: None,
        error_code: None,
    };
    let s = serde_json::to_string(&resp).unwrap();
    assert!(!s.contains("error_code"));
}

#[test]
fn shared_buffer_id_serializes_as_bare_u64() {
    let id = SharedBufferId(42);
    let s = serde_json::to_string(&id).unwrap();
    assert_eq!(s, "42");
    let parsed: SharedBufferId = serde_json::from_str("42").unwrap();
    assert_eq!(parsed, SharedBufferId(42));
}

#[test]
fn shared_buffer_create_params_round_trip() {
    let p = SharedBufferCreateParams { size: 1_048_576 };
    let s = serde_json::to_string(&p).unwrap();
    let parsed: SharedBufferCreateParams = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed.size, 1_048_576);
}

#[test]
fn shared_buffer_dirty_omits_rect_when_none() {
    let p = SharedBufferDirtyParams {
        id: SharedBufferId(7),
        rect: None,
    };
    let s = serde_json::to_string(&p).unwrap();
    // serde_json은 None을 null로 직렬화한다 (default skip을 명시하지 않은 경우).
    // 우리는 rect를 명시적으로 두지 않고 None일 때 null로 보내도 됨 — 디코딩 호환.
    assert!(s.contains("\"rect\":null") || !s.contains("rect"));
    let parsed: SharedBufferDirtyParams = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed.id, SharedBufferId(7));
    assert!(parsed.rect.is_none());
}

#[test]
fn shared_buffer_dirty_with_rect_round_trip() {
    let p = SharedBufferDirtyParams {
        id: SharedBufferId(11),
        rect: Some(PixelRect {
            x: 10,
            y: 20,
            w: 40,
            h: 30,
        }),
    };
    let s = serde_json::to_string(&p).unwrap();
    let parsed: SharedBufferDirtyParams = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed.rect.unwrap().w, 40);
    assert_eq!(parsed.rect.unwrap().h, 30);
}

#[test]
fn handle_channel_ping_round_trip() {
    let msg = HandleChannelMessage::Ping { seq: 7 };
    let s = serde_json::to_string(&msg).unwrap();
    assert!(s.contains("\"kind\":\"ping\""));
    assert!(s.contains("\"seq\":7"));
    let parsed: HandleChannelMessage = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed, HandleChannelMessage::Ping { seq: 7 });
}

#[test]
fn handle_channel_pong_round_trip() {
    let s = r#"{"kind":"pong","seq":42}"#;
    let parsed: HandleChannelMessage = serde_json::from_str(s).unwrap();
    assert_eq!(parsed, HandleChannelMessage::Pong { seq: 42 });
}

#[test]
fn handle_channel_handle_attach_round_trip() {
    // Unix: handle 없음 — wire에 handle 키가 나타나지 않아야 한다(하위호환 유지).
    let msg = HandleChannelMessage::HandleAttach {
        request_id: 9,
        id: SharedBufferId(1),
        size: 4096,
        handle: None,
    };
    let s = serde_json::to_string(&msg).unwrap();
    assert!(s.contains("\"kind\":\"handle_attach\""));
    assert!(
        !s.contains("\"handle\""),
        "None handle must be skipped: {s}"
    );
    let parsed: HandleChannelMessage = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed, msg);

    // Windows: handle in-band. 라운드트립으로 값 보존 확인.
    let win = HandleChannelMessage::HandleAttach {
        request_id: 9,
        id: SharedBufferId(1),
        size: 4096,
        handle: Some(0xDEAD_BEEF),
    };
    let s = serde_json::to_string(&win).unwrap();
    assert!(s.contains("\"handle\":3735928559"));
    let parsed: HandleChannelMessage = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed, win);
}

#[test]
fn handle_channel_dirty_with_rect_round_trip() {
    let msg = HandleChannelMessage::Dirty {
        id: SharedBufferId(2),
        rect: Some(PixelRect {
            x: 1,
            y: 2,
            w: 3,
            h: 4,
        }),
    };
    let s = serde_json::to_string(&msg).unwrap();
    assert!(s.contains("\"kind\":\"dirty\""));
    let parsed: HandleChannelMessage = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed, msg);
}

#[test]
fn handle_channel_dirty_full_buffer_round_trip() {
    let s = r#"{"kind":"dirty","id":3}"#;
    let parsed: HandleChannelMessage = serde_json::from_str(s).unwrap();
    assert_eq!(
        parsed,
        HandleChannelMessage::Dirty {
            id: SharedBufferId(3),
            rect: None,
        }
    );
}

#[test]
fn popup_open_params_round_trip_with_context() {
    let s = r#"{"popup_id":"search","instance_id":42,"context":{"q":"a"}}"#;
    let p: PopupOpenParams = serde_json::from_str(s).unwrap();
    assert_eq!(p.popup_id, "search");
    assert_eq!(p.instance_id, 42);
    assert_eq!(p.context["q"], "a");
}

#[test]
fn popup_open_params_context_defaults_to_null() {
    let s = r#"{"popup_id":"search","instance_id":1}"#;
    let p: PopupOpenParams = serde_json::from_str(s).unwrap();
    assert!(p.context.is_null());
}

#[test]
fn popup_open_result_decodes_empty_object() {
    // egui-mesh popup 은 open 응답에 콘텐츠가 없다 — 빈 객체가 유효한 결과.
    let s = r#"{}"#;
    let _r: PopupOpenResult = serde_json::from_str(s).unwrap();
}

#[test]
fn popup_close_reason_serializes_snake_case() {
    let cases = [
        (PopupCloseReason::OutsideClick, "outside_click"),
        (PopupCloseReason::Escape, "escape"),
        (PopupCloseReason::PluginRequest, "plugin_request"),
        (PopupCloseReason::HostShutdown, "host_shutdown"),
    ];
    for (r, expected) in cases {
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, format!("\"{expected}\""));
        let parsed: PopupCloseReason = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, r);
    }
}

#[test]
fn popup_method_names_stable() {
    assert_eq!(METHOD_POPUP_OPEN, "popup.open");
    assert_eq!(METHOD_POPUP_CLOSED, "popup.closed");
}

// ── egui-mesh (A1-S3) set_context / paint_frame wire ──

#[test]
fn set_context_method_name_stable() {
    assert_eq!(METHOD_SURFACE_SET_CONTEXT, "surface.set_context");
}

#[test]
fn set_context_params_round_trip() {
    let params = SurfaceSetContextParams {
        surface_id: 7,
        width_px: 1280,
        height_px: 720,
        pixels_per_point: 2.0,
        raw_input: RawInputWire {
            time: Some(12.5),
            focused: true,
            modifiers: ModifiersWire {
                command: true,
                ..Default::default()
            },
            events: vec![
                RawInputEventWire::PointerMoved { x: 10.0, y: 20.0 },
                RawInputEventWire::PointerButton {
                    x: 10.0,
                    y: 20.0,
                    button: PointerButtonWire::Primary,
                    pressed: true,
                    modifiers: ModifiersWire::default(),
                },
                RawInputEventWire::Scroll { x: 0.0, y: -4.0 },
                RawInputEventWire::Key {
                    key: "Enter".into(),
                    pressed: true,
                    repeat: false,
                    modifiers: ModifiersWire::default(),
                },
                RawInputEventWire::Text { text: "hi".into() },
                RawInputEventWire::PointerGone,
            ],
        },
        theme: None,
        need_full_textures: false,
    };
    let s = serde_json::to_string(&params).unwrap();
    let parsed: SurfaceSetContextParams = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed.surface_id, 7);
    assert_eq!(parsed.width_px, 1280);
    assert_eq!(parsed.height_px, 720);
    assert_eq!(parsed.pixels_per_point, 2.0);
    assert_eq!(parsed.raw_input, params.raw_input);
    // theme 미동봉(None) 은 직렬화 누락(serde default) 후 다시 None 으로 복원된다.
    assert!(parsed.theme.is_none());
}

/// set_context 에 Theme 스냅샷을 실어 보내면 색 집합/is_light/zoom 이 round-trip 한다.
#[test]
fn set_context_theme_snapshot_round_trips() {
    use tasty_type_appearance::theme::ThemeColors;
    // 직렬화 왕복을 확인할 테마 색 집합. JSON 매크로의 재귀 확장을 피한다.
    const COLORS_JSON: &str = r##"{
        "crust":"#11111b","mantle":"#181825","base":"#1e1e2e","surface0":"#313244",
        "surface1":"#45475a","surface2":"#585b70","overlay0":"#6c7086","overlay1":"#7f849c",
        "overlay2":"#9399b2","text":"#cdd6f4","subtext1":"#bac2de","subtext0":"#a6adc8",
        "placeholder":"#9399b2","blue":"#89b4fa","green":"#a6e3a1","red":"#f38ba8",
        "yellow":"#f9e2af","peach":"#fab387","mauve":"#cba6f7","teal":"#94e2d5",
        "sky":"#89dceb","lavender":"#b4befe","flamingo":"#f2cdcd","pink":"#f5c2e7",
        "maroon":"#eba0ac","rosewater":"#f5e0dc","selection_bg":"#585b70",
        "vi_cursor_bg":"#f9e2af","search_match_bg":"#f9e2af","search_match_active_bg":"#fab387","search_match_active_fg":"#1e1e2e",
        "ansi_black":"#45475a","ansi_red":"#f38ba8","ansi_green":"#a6e3a1","ansi_yellow":"#f9e2af",
        "ansi_blue":"#89b4fa","ansi_magenta":"#f5c2e7","ansi_cyan":"#94e2d5","ansi_white":"#bac2de",
        "ansi_bright_black":"#585b70","ansi_bright_red":"#f38ba8","ansi_bright_green":"#a6e3a1",
        "ansi_bright_yellow":"#f9e2af","ansi_bright_blue":"#89b4fa","ansi_bright_magenta":"#f5c2e7",
        "ansi_bright_cyan":"#94e2d5","ansi_bright_white":"#a6adc8"
    }"##;
    let colors: ThemeColors = serde_json::from_str(COLORS_JSON).unwrap();
    let params = SurfaceSetContextParams {
        surface_id: 1,
        width_px: 100,
        height_px: 100,
        pixels_per_point: 1.0,
        raw_input: RawInputWire::default(),
        theme: Some(crate::protocol::ThemeWire {
            colors,
            is_light: false,
            ui_zoom: 1.25,
        }),
        need_full_textures: false,
    };
    let s = serde_json::to_string(&params).unwrap();
    let parsed: SurfaceSetContextParams = serde_json::from_str(&s).unwrap();
    let t = parsed.theme.expect("theme present");
    assert!(!t.is_light);
    assert_eq!(t.ui_zoom, 1.25);
    assert_eq!(t.colors, params.theme.unwrap().colors);
}

/// search_match_active_fg 가 생기기 전의 host 가 보낸 색 집합도 읽히고, 그 글자색은
/// active 채움 위에서 base·text 중 대비가 큰 색으로 도출된다.
#[test]
fn theme_colors_without_active_match_ink_still_deserialize() {
    use tasty_type_appearance::theme::{Theme, ThemeColors};
    const OLD_HOST_COLORS_JSON: &str = r##"{
        "crust":"#11111b","mantle":"#181825","base":"#1e1e2e","surface0":"#313244",
        "surface1":"#45475a","surface2":"#585b70","overlay0":"#6c7086","overlay1":"#7f849c",
        "overlay2":"#9399b2","text":"#cdd6f4","subtext1":"#bac2de","subtext0":"#a6adc8",
        "placeholder":"#9399b2","blue":"#89b4fa","green":"#a6e3a1","red":"#f38ba8",
        "yellow":"#f9e2af","peach":"#fab387","mauve":"#cba6f7","teal":"#94e2d5",
        "sky":"#89dceb","lavender":"#b4befe","flamingo":"#f2cdcd","pink":"#f5c2e7",
        "maroon":"#eba0ac","rosewater":"#f5e0dc","selection_bg":"#585b70",
        "vi_cursor_bg":"#f9e2af","search_match_bg":"#f9e2af4d","search_match_active_bg":"#f9e2afb3",
        "ansi_black":"#45475a","ansi_red":"#f38ba8","ansi_green":"#a6e3a1","ansi_yellow":"#f9e2af",
        "ansi_blue":"#89b4fa","ansi_magenta":"#f5c2e7","ansi_cyan":"#94e2d5","ansi_white":"#bac2de",
        "ansi_bright_black":"#585b70","ansi_bright_red":"#f38ba8","ansi_bright_green":"#a6e3a1",
        "ansi_bright_yellow":"#f9e2af","ansi_bright_blue":"#89b4fa","ansi_bright_magenta":"#f5c2e7",
        "ansi_bright_cyan":"#94e2d5","ansi_bright_white":"#a6adc8"
    }"##;
    let mocha: ThemeColors = serde_json::from_str(OLD_HOST_COLORS_JSON).unwrap();
    let theme = Theme::with_colors(mocha.clone(), false);
    assert_eq!(theme.search_match_active_fg, mocha.base);

    // 밝은 테마(Latte 원색, 이전 host 의 70% 채움)는 text 가 고른다.
    let latte_json = OLD_HOST_COLORS_JSON
        .replace("\"base\":\"#1e1e2e\"", "\"base\":\"#eff1f5\"")
        .replace("\"text\":\"#cdd6f4\"", "\"text\":\"#4c4f69\"")
        .replace(
            "\"search_match_active_bg\":\"#f9e2afb3\"",
            "\"search_match_active_bg\":\"#df8e1db3\"",
        );
    let latte: ThemeColors = serde_json::from_str(&latte_json).unwrap();
    let theme = Theme::with_colors(latte.clone(), true);
    assert_eq!(theme.search_match_active_fg, latte.text);
}

#[test]
fn raw_input_event_tags_stable() {
    let s = serde_json::to_string(&RawInputEventWire::PointerGone).unwrap();
    assert!(s.contains("\"t\":\"pointer_gone\""), "{s}");
    let s = serde_json::to_string(&RawInputEventWire::Scroll { x: 1.0, y: 2.0 }).unwrap();
    assert!(s.contains("\"t\":\"scroll\""), "{s}");
    // Ime 변형은 외부 `t` 태그와 내부 `ime` 태그가 둘 다 안정적으로 실린다.
    let s = serde_json::to_string(&RawInputEventWire::Ime {
        event: ImeWire::Preedit { text: "한".into() },
    })
    .unwrap();
    assert!(s.contains("\"t\":\"ime\""), "{s}");
    assert!(s.contains("\"ime\":\"preedit\""), "{s}");
}

/// IME 4단계가 모두 `RawInputEventWire::Ime` 로 wrap 되어 round-trip 한다.
#[test]
fn raw_input_ime_events_round_trip() {
    let events = vec![
        RawInputEventWire::Ime {
            event: ImeWire::Enabled,
        },
        RawInputEventWire::Ime {
            event: ImeWire::Preedit { text: "ㅎ".into() },
        },
        RawInputEventWire::Ime {
            event: ImeWire::Commit {
                text: "한글".into(),
            },
        },
        RawInputEventWire::Ime {
            event: ImeWire::Disabled,
        },
    ];
    let s = serde_json::to_string(&events).unwrap();
    let parsed: Vec<RawInputEventWire> = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed, events);
}

#[test]
fn paint_frame_event_round_trip() {
    let ev = PluginEvent::PaintFrame {
        surface_id: 42,
        buffer_id: SharedBufferId(9),
        generation: 1234,
        frame_seq: 56,
        full_textures: true,
        byte_len: 777,
        ime_cursor: None,
    };
    let s = serde_json::to_string(&ev).unwrap();
    assert!(s.contains("\"kind\":\"paint_frame\""), "{s}");
    let parsed: PluginEvent = serde_json::from_str(&s).unwrap();
    match parsed {
        PluginEvent::PaintFrame {
            surface_id,
            buffer_id,
            generation,
            frame_seq,
            full_textures,
            byte_len,
            ime_cursor,
        } => {
            assert_eq!(surface_id, 42);
            assert_eq!(buffer_id, SharedBufferId(9));
            assert_eq!(generation, 1234);
            assert_eq!(frame_seq, 56);
            assert!(full_textures);
            assert_eq!(byte_len, 777);
            assert_eq!(ime_cursor, None);
        }
        other => panic!("expected PaintFrame, got {other:?}"),
    }
}

/// ime_cursor 필드가 없는 요청도 None으로 읽는지 확인한다.
#[test]
fn paint_frame_without_ime_cursor_still_parses() {
    let s = r#"{"kind":"paint_frame","surface_id":1,"buffer_id":2,"generation":3,"frame_seq":4,"full_textures":false,"byte_len":5}"#;
    let parsed: PluginEvent = serde_json::from_str(s).unwrap();
    match parsed {
        PluginEvent::PaintFrame { ime_cursor, .. } => assert_eq!(ime_cursor, None),
        other => panic!("expected PaintFrame, got {other:?}"),
    }
}

/// IME 위젯과 캐럿 사각형이 직렬화 후에도 유지되는지 확인한다.
#[test]
fn paint_frame_ime_cursor_round_trips() {
    let ime = ImeCursorWire {
        rect: RectWire {
            x: 12.0,
            y: 34.0,
            width: 200.0,
            height: 24.0,
        },
        cursor_rect: RectWire {
            x: 56.0,
            y: 36.0,
            width: 1.0,
            height: 18.0,
        },
    };
    let ev = PluginEvent::PopupPaintFrame {
        instance_id: 7,
        buffer_id: SharedBufferId(1),
        generation: 2,
        frame_seq: 3,
        full_textures: false,
        ime_cursor: Some(ime),
    };
    let s = serde_json::to_string(&ev).unwrap();
    let parsed: PluginEvent = serde_json::from_str(&s).unwrap();
    match parsed {
        PluginEvent::PopupPaintFrame { ime_cursor, .. } => assert_eq!(ime_cursor, Some(ime)),
        other => panic!("expected PopupPaintFrame, got {other:?}"),
    }
}

/// frame_seq / full_textures / need_full_textures 는 serde default — 구버전 JSON
/// (필드 없음)이 그대로 파싱되고 기본값(0 / false)으로 복원된다 (additive 호환).
#[test]
fn texture_chain_fields_default_for_legacy_json() {
    let legacy = r#"{"kind":"paint_frame","surface_id":1,"buffer_id":2,"generation":3}"#;
    let parsed: PluginEvent = serde_json::from_str(legacy).unwrap();
    match parsed {
        PluginEvent::PaintFrame {
            frame_seq,
            full_textures,
            byte_len,
            ..
        } => {
            assert_eq!(frame_seq, 0);
            assert!(!full_textures);
            assert_eq!(byte_len, 0);
        }
        other => panic!("expected PaintFrame, got {other:?}"),
    }

    let legacy_ctx = r#"{"surface_id":1,"width_px":10,"height_px":10,"pixels_per_point":1.0}"#;
    let parsed: SurfaceSetContextParams = serde_json::from_str(legacy_ctx).unwrap();
    assert!(!parsed.need_full_textures);
}

/// error_code가 없을 때의 기본값, None일 때 필드 생략, 값이 있을 때 왕복을 검사한다.
#[test]
fn ipc_call_result_error_code_is_optional_in_both_directions() {
    use crate::protocol::IpcCallResult;

    // 구버전 호스트가 보낸 모양 — 필드가 아예 없다.
    let old: IpcCallResult = serde_json::from_str(r#"{"call_id":1,"error":"denied"}"#)
        .expect("구버전 모양을 읽어야 한다");
    assert_eq!(old.error_code, None);
    assert_eq!(old.error.as_deref(), Some("denied"));

    // 코드가 없으면 와이어에도 안 실린다 — 구버전 plugin 이 낯선 키를 안 본다.
    let none = IpcCallResult {
        call_id: 2,
        result: None,
        error: Some("denied".into()),
        error_code: None,
    };
    let s = serde_json::to_string(&none).unwrap();
    assert!(!s.contains("error_code"), "{s}");

    // 있으면 실리고 되읽힌다.
    let some = IpcCallResult {
        call_id: 3,
        result: None,
        error: Some("no live surface 999".into()),
        error_code: Some(-32602),
    };
    let s = serde_json::to_string(&some).unwrap();
    assert!(s.contains("\"error_code\":-32602"), "{s}");
    let back: IpcCallResult = serde_json::from_str(&s).unwrap();
    assert_eq!(back.error_code, Some(-32602));
}

/// dropped_requests의 생략과 기본값을 검사한다.
/// 이 시험은 구버전 바이너리를 실행하는 호환성 시험은 아니다.
#[test]
fn plugin_request_dropped_requests_is_optional_in_both_directions() {
    // 구버전 호스트가 보낸 줄 — 키가 아예 없다.
    let old: PluginRequest = serde_json::from_str(r#"{"method":"ping","params":{},"id":7}"#)
        .expect("구버전 모양을 읽어야 한다");
    assert_eq!(old.dropped_requests, 0);
    assert_eq!(old.method, "ping");

    // 버린 것이 없으면 와이어에 키가 안 실린다 — 구버전 plugin 이 낯선 키를 안 본다.
    // (모든 host→plugin 줄에 실리면 set_context 처럼 프레임마다 나가는 요청에
    // 빈 키가 얹힌다.)
    let none = PluginRequest::new("ping", serde_json::json!({}), 7);
    let s = serde_json::to_string(&none).unwrap();
    assert!(!s.contains("dropped_requests"), "{s}");

    // 있으면 실리고 되읽힌다.
    let mut some = PluginRequest::new("ipc.invoke", serde_json::json!({}), 8);
    some.dropped_requests = 12;
    let s = serde_json::to_string(&some).unwrap();
    assert!(s.contains("\"dropped_requests\":12"), "{s}");
    let back: PluginRequest = serde_json::from_str(&s).unwrap();
    assert_eq!(back.dropped_requests, 12);
}

/// 생성자는 드롭 수를 0 으로 둔다 — 그 값을 정하는 것은 본문이 아니라 송신 지점이다.
#[test]
fn a_freshly_built_request_carries_no_drop_count() {
    let req = PluginRequest::new("surface.create", serde_json::json!({"surface_id": 1}), 3);
    assert_eq!(req.dropped_requests, 0);
    assert_eq!(req.id, 3);
    assert_eq!(req.method, "surface.create");
}

/// 모르는 입력 이벤트가 섞인 set_context 도 읽을 수 있고, 같은 프레임의 나머지 이벤트는 남는다.
#[test]
fn set_context_with_an_unknown_input_event_keeps_the_other_events() {
    let raw = serde_json::json!({
        "surface_id": 7,
        "width_px": 100,
        "height_px": 50,
        "pixels_per_point": 1.0,
        "raw_input": {
            "events": [
                { "t": "key", "key": "A", "pressed": true },
                { "t": "some_future_event", "payload": 1 },
                { "t": "text", "text": "hi" }
            ]
        }
    });
    let parsed: SurfaceSetContextParams =
        serde_json::from_value(raw).expect("모르는 이벤트가 있어도 읽어야 한다");
    assert_eq!(
        parsed.raw_input.events,
        vec![
            RawInputEventWire::Key {
                key: "A".into(),
                pressed: true,
                repeat: false,
                modifiers: ModifiersWire::default(),
            },
            RawInputEventWire::Unknown,
            RawInputEventWire::Text { text: "hi".into() },
        ]
    );
}

/// 붙여넣기 텍스트와 잘라내기를 모르는 이전 정의의 축소 복제. 실제 이전 정의처럼
/// `t` 태그와 `#[serde(other)]` fallback 을 쓴다.
#[derive(Debug, serde::Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
enum PreviousRawInputEventWire {
    Copy,
    Paste,
    #[serde(other)]
    Unknown,
}

/// 새 붙여넣기 텍스트·잘라내기 이벤트는 이전 정의에서 오류 없이 Unknown 으로 읽힌다.
#[test]
fn paste_text_and_cut_read_as_unknown_in_the_previous_definition() {
    for raw in [
        serde_json::json!({ "t": "paste_text", "text": "x" }),
        serde_json::json!({ "t": "cut" }),
    ] {
        let old: PreviousRawInputEventWire =
            serde_json::from_value(raw.clone()).expect("이전 정의도 읽어야 한다");
        assert_eq!(old, PreviousRawInputEventWire::Unknown, "{raw}");
    }
}

/// 붙여넣기 텍스트와 잘라내기는 정한 와이어 모양으로 나가고 그대로 되읽힌다.
#[test]
fn paste_text_and_cut_round_trip_on_the_wire() {
    let paste = RawInputEventWire::PasteText {
        text: "/tmp/a.md".into(),
    };
    assert_eq!(
        serde_json::to_value(&paste).unwrap(),
        serde_json::json!({ "t": "paste_text", "text": "/tmp/a.md" })
    );
    assert_eq!(
        serde_json::to_value(RawInputEventWire::Cut).unwrap(),
        serde_json::json!({ "t": "cut" })
    );
    for ev in [paste, RawInputEventWire::Cut] {
        let back: RawInputEventWire =
            serde_json::from_value(serde_json::to_value(&ev).unwrap()).unwrap();
        assert_eq!(back, ev);
    }
}

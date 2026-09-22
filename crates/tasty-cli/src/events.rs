//! `tasty events follow` — 위치를 들고 long-poll 을 반복한다.
//!
//! **커서는 이쪽이 든다.** 매 요청이 직전 답의 `next_offset` 을 싣고, 호스트는
//! 소비자별 상태를 두지 않는다. 그래서 끊겼다 붙어도 같은 자리에서 이어진다.
//!
//! 모양의 선례는 `plugin audit-follow` 다. 다른 점 하나 — 그쪽은 간격을 두고 다시
//! 묻고, 이쪽은 **호스트가 기다려 준다**(`wait_ms`). 그래서 새 사건이 없는 동안
//! 왕복이 안 생기고, 생겼을 때의 지연이 폴링 간격에 안 묶인다.
//!
//! **세대는 연결을 넘어 이어진다.** 재시작하면 위치가 0 부터 다시 매겨지는데, 재시작은
//! 연결도 끊는다. 그래서 세대를 한 연결 안에서만 견주면 그 비교가 실제 재시작에서는
//! 한 번도 참이 되지 않는다. 옛 세대는 재부착 인자(`--epoch`)나 `--reconnect` 가
//! 넘겨 주고, 그것도 없으면 호스트가 다는 앞섬 표지(`ahead_of_stream`)로 안다
//! (ADR-0405 · ADR-0407).

use std::net::TcpStream;
use std::time::Duration;

use anyhow::Result;
use serde_json::{Map, Value, json};

use tasty_ipc::client::IpcConnection;

use crate::out::outln;

/// `--reconnect` 가 다시 붙기를 시도하는 간격. 재시작은 사람이나 감독 프로세스가 하는
/// 일이라 초 단위로 충분하고, 그보다 짧으면 호스트가 없는 동안 연결 시도가 루프를
/// 태운다.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);

/// `tasty events follow` 의 인자.
pub struct FollowArgs<'a> {
    pub offset: u64,
    pub filter: Option<&'a str>,
    pub batch: u64,
    pub wait_ms: u64,
    /// 재부착하는 위치가 온 세대. 없으면 첫 답이 정한다.
    pub epoch: Option<u64>,
    pub reconnect: bool,
}

/// 답 하나를 읽고 나서 stderr 로 알릴 것.
#[derive(Debug, PartialEq)]
enum Notice {
    /// 세대가 바뀌었다 — 처음부터 다시 잇는다.
    EpochChanged,
    /// 든 위치가 이 세대의 끝보다 뒤였다 — 다른 세대의 위치다. 처음부터 다시 잇는다.
    Ahead { asked: u64, end: u64 },
    /// 보존 밖이라 건너뛴 수.
    Skipped(u64),
}

/// 소비자가 드는 것 — 위치와 그 위치의 세대.
#[derive(Debug, PartialEq)]
struct Cursor {
    offset: u64,
    epoch: Option<u64>,
}

impl Cursor {
    /// 답 하나를 읽고 위치를 옮긴다. 알릴 것과 찍을 사건을 돌려준다.
    ///
    /// 세대가 바뀌었거나 위치가 끝보다 뒤면 **그 답의 사건을 찍지 않고** 위치를 0 으로
    /// 되돌린다 — 그 답은 옛 위치로 물은 것이라, 다시 묻는 답이 새 세대의 처음부터를 준다.
    fn absorb(&mut self, resp: &Value) -> (Vec<Notice>, Vec<Value>) {
        if let Some(got) = resp.get("epoch").and_then(|v| v.as_u64()) {
            match self.epoch {
                Some(had) if had != got => {
                    self.epoch = Some(got);
                    self.offset = 0;
                    return (vec![Notice::EpochChanged], Vec::new());
                }
                _ => self.epoch = Some(got),
            }
        }
        // 표지를 모르는 옛 호스트는 이 필드를 안 싣는다 — 없으면 예전처럼 읽는다.
        if resp
            .get("ahead_of_stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let end = resp.get("stream_end").and_then(|v| v.as_u64()).unwrap_or(0);
            let asked = self.offset;
            self.offset = 0;
            return (vec![Notice::Ahead { asked, end }], Vec::new());
        }
        let mut notices = Vec::new();
        // **조용히 넘어가지 않는다.** 건너뛴 수를 stderr 로 알린다 — stdout 은
        // `while read` 가 먹는 자리라 사건 줄만 간다.
        if resp
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            notices.push(Notice::Skipped(
                resp.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0),
            ));
        }
        let events = resp
            .get("events")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(next) = resp.get("next_offset").and_then(|v| v.as_u64()) {
            self.offset = next;
        }
        (notices, events)
    }

    /// 끊긴 뒤 같은 자리로 다시 붙는 인자. 세대를 아직 모르면(첫 답 전) 위치만 준다.
    fn reattach_args(&self) -> String {
        match self.epoch {
            Some(e) => format!("--offset {} --epoch {e}", self.offset),
            None => format!("--offset {}", self.offset),
        }
    }
}

fn print_notice(n: &Notice) {
    match n {
        Notice::EpochChanged => crate::out::errln!("{}", tasty_i18n::t("cli.events.epoch_changed")),
        Notice::Ahead { asked, end } => crate::out::errln!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.events.ahead_of_stream",
                &asked.to_string(),
                &end.to_string()
            )
        ),
        Notice::Skipped(n) => crate::out::errln!(
            "{}",
            tasty_i18n::t_fmt("cli.events.skipped", &n.to_string())
        ),
    }
}

fn connect(port_file: Option<&str>) -> Result<IpcConnection> {
    let port = crate::port_file::read_port(port_file)?;
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.request.connect_failed",
                &port.to_string(),
                &e.to_string()
            )
        )
    })?;
    IpcConnection::new(stream)
}

/// 위치를 들고 long-poll 을 반복한다. Ctrl-C 까지 돈다.
pub fn run_follow(args: FollowArgs<'_>, port_file: Option<&str>) -> Result<()> {
    let mut conn = connect(port_file)?;
    let session_token = std::env::var("TASTY_SESSION_TOKEN").ok();
    let mut next_id: i64 = 1;
    let mut cursor = Cursor {
        offset: args.offset,
        epoch: args.epoch,
    };
    // 연결마다 첫 요청은 기다리지 않는다 — 세대가 바뀌었거나 위치가 끝보다 뒤라는
    // 사실은 기다린 뒤의 답에도 실리지만, 그러면 알리는 것이 `wait_ms` 만큼 늦는다.
    let mut first_on_connection = true;
    loop {
        let mut params = Map::new();
        params.insert("offset".into(), json!(cursor.offset));
        params.insert("max".into(), json!(args.batch));
        let wait = if first_on_connection { 0 } else { args.wait_ms };
        params.insert("wait_ms".into(), json!(wait));
        if let Some(f) = args.filter {
            params.insert("filter".into(), json!(f));
        }
        let req = tasty_ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: "events.fetch".to_string(),
            params: Value::Object(params),
            id: Some(json!(next_id)),
            session_token: session_token.clone(),
        };
        next_id += 1;
        let resp = match conn.send(&req) {
            Ok(resp) => resp,
            // 호스트가 답한 오류는 연결 문제가 아니다 — 다시 붙어도 같은 답이 온다.
            Err(e) if e.is::<tasty_ipc::client::JsonRpcCallError>() => return Err(e),
            Err(e) => {
                if !args.reconnect {
                    crate::out::errln!(
                        "{}",
                        tasty_i18n::t_fmt("cli.events.connection_lost", &cursor.reattach_args())
                    );
                    return Err(e);
                }
                crate::out::errln!("{}", tasty_i18n::t("cli.events.reconnecting"));
                conn = loop {
                    std::thread::sleep(RECONNECT_INTERVAL);
                    if let Ok(c) = connect(port_file) {
                        break c;
                    }
                };
                first_on_connection = true;
                continue;
            }
        };
        first_on_connection = false;

        let (notices, events) = cursor.absorb(&resp);
        for n in &notices {
            print_notice(n);
        }
        for ev in &events {
            outln!("{}", serde_json::to_string(ev).unwrap_or_default())?;
        }
        if !events.is_empty() {
            crate::out::flush()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(epoch: u64, next: u64) -> Value {
        json!({
            "events": [], "next_offset": next, "epoch": epoch,
            "truncated": false, "skipped": 0,
            "ahead_of_stream": false, "stream_end": next,
        })
    }

    /// 재시작 뒤 `--epoch` 로 재부착하면 새 세대임을 알리고 처음부터 다시 잇는다.
    /// 옛 위치로 받은 답의 사건은 찍지 않는다.
    #[test]
    fn a_reattach_with_an_older_epoch_starts_the_new_generation_from_zero() {
        let mut c = Cursor {
            offset: 5,
            epoch: Some(100),
        };
        let mut resp = answer(200, 9);
        resp["events"] = json!([{ "offset": 5, "key": "agent.task_finished" }]);
        let (notices, events) = c.absorb(&resp);
        assert_eq!(notices, vec![Notice::EpochChanged]);
        assert!(
            events.is_empty(),
            "옛 위치로 받은 사건은 새 세대의 엉뚱한 사건이다"
        );
        assert_eq!(
            c,
            Cursor {
                offset: 0,
                epoch: Some(200)
            }
        );
    }

    /// 세대를 모르고 재부착해도, 위치가 새 세대의 끝보다 뒤면 호스트의 표지로 안다.
    #[test]
    fn a_position_past_the_end_is_reported_and_restarted_from_zero() {
        let mut c = Cursor {
            offset: 2404,
            epoch: None,
        };
        let mut resp = answer(200, 2404);
        resp["ahead_of_stream"] = json!(true);
        resp["stream_end"] = json!(1);
        let (notices, events) = c.absorb(&resp);
        assert_eq!(
            notices,
            vec![Notice::Ahead {
                asked: 2404,
                end: 1
            }]
        );
        assert!(events.is_empty());
        assert_eq!(c.offset, 0);
        assert_eq!(c.epoch, Some(200), "세대는 배운다");
    }

    /// 첫 답이 세대를 정하고, 같은 세대의 답은 위치만 옮긴다.
    #[test]
    fn the_first_answer_sets_the_epoch_and_later_answers_move_the_position() {
        let mut c = Cursor {
            offset: 0,
            epoch: None,
        };
        let mut resp = answer(7, 3);
        resp["events"] = json!([{ "offset": 0 }, { "offset": 1 }, { "offset": 2 }]);
        let (notices, events) = c.absorb(&resp);
        assert!(notices.is_empty());
        assert_eq!(events.len(), 3);
        assert_eq!(
            c,
            Cursor {
                offset: 3,
                epoch: Some(7)
            }
        );
        let (notices, _) = c.absorb(&answer(7, 3));
        assert!(notices.is_empty());
        assert_eq!(c.offset, 3);
    }

    /// 표지를 모르는 옛 호스트의 답(필드 없음)은 예전처럼 읽는다.
    #[test]
    fn an_answer_without_the_ahead_fields_reads_as_before() {
        let mut c = Cursor {
            offset: 4,
            epoch: None,
        };
        let resp = json!({
            "events": [{ "offset": 4 }], "next_offset": 5, "epoch": 1,
            "truncated": false, "skipped": 0,
        });
        let (notices, events) = c.absorb(&resp);
        assert!(notices.is_empty());
        assert_eq!(events.len(), 1);
        assert_eq!(c.offset, 5);
    }

    /// 보존 밖이면 건너뛴 수를 알리고, 사건은 그대로 찍는다.
    #[test]
    fn a_truncated_answer_is_reported_and_its_events_are_kept() {
        let mut c = Cursor {
            offset: 0,
            epoch: Some(1),
        };
        let mut resp = answer(1, 1073);
        resp["truncated"] = json!(true);
        resp["skipped"] = json!(1072);
        resp["events"] = json!([{ "offset": 1072 }]);
        let (notices, events) = c.absorb(&resp);
        assert_eq!(notices, vec![Notice::Skipped(1072)]);
        assert_eq!(events.len(), 1);
    }

    /// 끊겼을 때 찍는 재부착 인자 — 세대를 알면 함께 준다.
    #[test]
    fn the_reattach_arguments_carry_the_epoch_once_it_is_known() {
        let known = Cursor {
            offset: 2405,
            epoch: Some(1789973580035653967),
        };
        assert_eq!(
            known.reattach_args(),
            "--offset 2405 --epoch 1789973580035653967"
        );
        let unknown = Cursor {
            offset: 3,
            epoch: None,
        };
        assert_eq!(unknown.reattach_args(), "--offset 3");
    }
}

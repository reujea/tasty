//! 웹뷰 → plugin 메시지(docs/surfaces/mdedit/index.md#웹뷰-입력은-nav-fragment-로-보낸다). 편집기 스크립트가 `location.hash` 에
//! `tasty-nav:mdedit:<seq>:<op>:<encodeURIComponent(JSON)>` 을 대입하면 host 가 그 URL 을
//! `webview.navigation_attempt` 로 넘긴다.
//!
//! 큰 메시지는 `chunk` 로 쪼개 온다 — `{"i": 조각 번호, "n": 조각 수, "d": "<op>:<payload>"
//! 의 일부}`. 마지막 조각을 받으면 이어 붙여 원래 메시지로 되돌린다. 조각 사이에는 plugin 의
//! ack 가 끼므로 순서가 섞이지 않는다.

use serde_json::Value;

pub const MARKER: &str = "tasty-nav:mdedit:";

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub seq: u64,
    pub op: String,
    pub payload: Value,
}

/// URL 에서 메시지 하나를 꺼낸다. 이 plugin 의 표지가 없으면 `None`.
pub fn parse(url: &str) -> Option<Result<Message, String>> {
    let idx = url.rfind(MARKER)?;
    let rest = &url[idx + MARKER.len()..];
    Some(parse_body(rest))
}

fn parse_body(rest: &str) -> Result<Message, String> {
    let (seq, rest) = rest.split_once(':').ok_or("missing seq")?;
    let seq: u64 = seq.parse().map_err(|_| format!("bad seq '{seq}'"))?;
    let (op, enc) = rest.split_once(':').unwrap_or((rest, ""));
    let payload = if enc.is_empty() {
        Value::Null
    } else {
        let json = percent_decode(enc);
        serde_json::from_str(&json).map_err(|e| format!("payload: {e}"))?
    };
    Ok(Message {
        seq,
        op: op.to_string(),
        payload,
    })
}

/// 조각 재조립 상태(문서마다 하나).
#[derive(Debug, Default)]
pub struct Chunks {
    parts: Vec<String>,
    expect_n: usize,
}

impl Chunks {
    /// 조각 하나를 받는다. 마지막이면 원래 메시지의 `<op>:<payload>` 본문을 돌려준다.
    /// 번호가 어긋나면 지금까지를 버리고 `Err`.
    pub fn push(&mut self, payload: &Value) -> Result<Option<(String, Value)>, String> {
        let i = payload
            .get("i")
            .and_then(Value::as_u64)
            .ok_or("chunk without i")? as usize;
        let n = payload
            .get("n")
            .and_then(Value::as_u64)
            .ok_or("chunk without n")? as usize;
        let d = payload
            .get("d")
            .and_then(Value::as_str)
            .ok_or("chunk without d")?;
        if i == 0 {
            self.parts.clear();
            self.expect_n = n;
        }
        if i != self.parts.len() || n != self.expect_n || n == 0 {
            self.parts.clear();
            return Err(format!("chunk {i}/{n} out of order"));
        }
        self.parts.push(d.to_string());
        if self.parts.len() < n {
            return Ok(None);
        }
        let whole = std::mem::take(&mut self.parts).concat();
        let (op, json) = whole.split_once(':').unwrap_or((&whole, ""));
        let v = if json.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(json).map_err(|e| format!("chunked payload: {e}"))?
        };
        Ok(Some((op.to_string(), v)))
    }
}

/// UTF-16 오프셋 → 바이트 오프셋. 범위를 넘거나 서로게이트 중간이면 `None`.
pub fn utf16_to_byte(text: &str, u16_off: usize) -> Option<usize> {
    let mut acc = 0usize;
    for (b, ch) in text.char_indices() {
        if acc == u16_off {
            return Some(b);
        }
        acc += ch.len_utf16();
        if acc > u16_off {
            return None;
        }
    }
    (acc == u16_off).then_some(text.len())
}

pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// `{s, e, t, len}` splice 를 적용한다. `len` 은 스크립트 쪽 결과 길이(UTF-16) — 적용 결과와
/// 다르면 두 쪽이 이미 어긋난 것이라 `Err`(호출자가 전체 재동기를 요청한다).
pub fn apply_splice(text: &mut String, p: &Value) -> Result<(), String> {
    let get = |k: &str| p.get(k).and_then(Value::as_u64).map(|v| v as usize);
    let (s, e) = (
        get("s").ok_or("splice without s")?,
        get("e").ok_or("splice without e")?,
    );
    let t = p.get("t").and_then(Value::as_str).unwrap_or_default();
    let (bs, be) = (
        utf16_to_byte(text, s).ok_or("splice start out of range")?,
        utf16_to_byte(text, e).ok_or("splice end out of range")?,
    );
    if bs > be {
        return Err("splice start after end".into());
    }
    let mut next = String::with_capacity(text.len() + t.len());
    next.push_str(&text[..bs]);
    next.push_str(t);
    next.push_str(&text[be..]);
    if let Some(len) = get("len")
        && utf16_len(&next) != len
    {
        return Err(format!(
            "length mismatch after splice ({} != {len})",
            utf16_len(&next)
        ));
    }
    *text = next;
    Ok(())
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_messages_and_ignores_foreign_urls() {
        assert!(parse("about:blank#tasty-nav:link:x").is_none());
        let m = parse("about:blank#tasty-nav:mdedit:7:save:")
            .unwrap()
            .unwrap();
        assert_eq!(
            (m.seq, m.op.as_str(), m.payload.clone()),
            (7, "save", Value::Null)
        );
        let enc = "%7B%22s%22%3A1%2C%22t%22%3A%22%ED%95%9C%22%7D";
        let m = parse(&format!("about:blank#tasty-nav:mdedit:1:edit:{enc}"))
            .unwrap()
            .unwrap();
        assert_eq!(m.payload, json!({"s":1,"t":"한"}));
        assert!(parse("#tasty-nav:mdedit:x:save:").unwrap().is_err());
    }

    #[test]
    fn chunks_reassemble_in_order_only() {
        let mut c = Chunks::default();
        assert_eq!(
            c.push(&json!({"i":0,"n":2,"d":"full:{\"t\":\"ab"}))
                .unwrap(),
            None
        );
        let (op, v) = c.push(&json!({"i":1,"n":2,"d":"c\"}"})).unwrap().unwrap();
        assert_eq!((op.as_str(), v), ("full", json!({"t":"abc"})));
        assert!(c.push(&json!({"i":1,"n":2,"d":"x"})).is_err());
    }

    #[test]
    fn splice_uses_utf16_offsets() {
        let mut t = String::from("가\u{1F600}b");
        // '가'(1) + U+1F600(서로게이트 쌍 2) = 3 → 'b' 앞에 삽입
        apply_splice(&mut t, &json!({"s":3,"e":3,"t":"X","len":5})).unwrap();
        assert_eq!(t, "가\u{1F600}Xb");
        // 서로게이트 중간은 거부
        assert!(apply_splice(&mut t, &json!({"s":2,"e":2,"t":"!"})).is_err());
        // 길이 불일치는 거부하고 원문을 그대로 둔다
        assert!(apply_splice(&mut t, &json!({"s":0,"e":1,"t":"","len":99})).is_err());
        assert_eq!(t, "가\u{1F600}Xb");
        apply_splice(&mut t, &json!({"s":0,"e":5,"t":""})).unwrap();
        assert_eq!(t, "");
    }
}

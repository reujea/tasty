//! NDJSON 한 줄 송신.
//!
//! 호스트 ↔ plugin 메인 채널(TCP)의 양 끝이 메시지를 쓰는 자리는 전부 이 함수를 거친다.

use std::io::{self, Write};

/// `line` 과 개행을 **한 번의 `write_all`** 로 쓰고 flush 한다.
///
/// `writeln!` 은 버퍼링 없는 `TcpStream` 에서 본문과 개행을 두 번의 write 로 내보내
/// 메시지마다 세그먼트가 둘이 된다. Nagle 이 켜진 소켓이면 뒤 조각이 앞 조각의 ACK 를
/// 기다려 hop 마다 수십 ms 가 붙는다 — 양 끝의 `TCP_NODELAY` 와 함께 이중 방어다
/// (`docs/dev-guide/attach-behavior.md` "프레임 전송 지연").
pub fn write_line<W: Write + ?Sized>(w: &mut W, line: &str) -> io::Result<()> {
    let mut buf = Vec::with_capacity(line.len() + 1);
    buf.extend_from_slice(line.as_bytes());
    buf.push(b'\n');
    w.write_all(&buf)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 메시지 하나는 1 회의 `write` 로 나가야 한다 — 쪼개지면 위 문서의 지연이 돌아온다.
    #[test]
    fn write_line_emits_one_write_call() {
        struct CountingWriter {
            writes: usize,
            buf: Vec<u8>,
        }
        impl Write for CountingWriter {
            fn write(&mut self, data: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                self.buf.extend_from_slice(data);
                Ok(data.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        for line in ["", "{\"id\":1}", &"x".repeat(4096)] {
            let mut w = CountingWriter {
                writes: 0,
                buf: Vec::new(),
            };
            write_line(&mut w, line).unwrap();
            assert_eq!(
                w.writes,
                1,
                "{} 바이트 줄이 {} 번의 write 로 쪼개졌다",
                line.len(),
                w.writes
            );
            assert_eq!(w.buf, format!("{line}\n").into_bytes());
        }
    }
}

//! macOS TCP port enumeration with lsof -nP -a -iTCP -p <pids>.

use std::collections::HashSet;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::process::Command;

use crate::{ListeningPort, PortState, SystemListeningPort};

/// Map an `lsof` `(STATE)` token to [`PortState`].
fn state_from_lsof(s: &str) -> PortState {
    match s {
        "LISTEN" => PortState::Listen,
        "ESTABLISHED" => PortState::Established,
        "SYN_SENT" => PortState::SynSent,
        "SYN_RCVD" => PortState::SynRecv,
        "FIN_WAIT_1" => PortState::FinWait1,
        "FIN_WAIT_2" => PortState::FinWait2,
        "TIME_WAIT" => PortState::TimeWait,
        "CLOSED" => PortState::Closed,
        "CLOSE_WAIT" => PortState::CloseWait,
        "LAST_ACK" => PortState::LastAck,
        "CLOSING" => PortState::Closing,
        _ => PortState::Unknown,
    }
}

pub fn scan(pids: &HashSet<u32>) -> io::Result<Vec<ListeningPort>> {
    if pids.is_empty() {
        return Ok(Vec::new());
    }
    let pid_list: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
    let pid_arg = pid_list.join(",");

    // -a combines PID and TCP filters with AND; lsof otherwise combines them with OR.
    let output = Command::new("lsof")
        .args(["-nP", "-a", "-iTCP", "-p", &pid_arg])
        .output()?;
    // lsof returns 1 when *some* pids have no matching descriptors; treat
    // non-empty stdout as success regardless of exit code.
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_lsof(&text))
}

/// System-wide scan (all PIDs). Uses the same lsof parser.
pub fn scan_all() -> io::Result<Vec<SystemListeningPort>> {
    let output = Command::new("lsof").args(["-nP", "-iTCP"]).output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_lsof(&text)
        .into_iter()
        .map(|p| SystemListeningPort {
            pid: Some(p.pid),
            port: p.port,
            addr: p.addr,
            process_name: p.process_name,
            state: p.state,
        })
        .collect())
}

/// Parse human-readable lsof output. Each line looks like:
///   `node 1234 user 22u IPv4 ... TCP *:8080 (LISTEN)`
/// Established rows carry a remote endpoint: `... TCP 1.2.3.4:22->5.6.7.8:55 (ESTABLISHED)`.
fn parse_lsof(text: &str) -> Vec<ListeningPort> {
    let mut out = Vec::new();
    for line in text.lines() {
        // skip header line: "COMMAND   PID USER ..."
        if line.starts_with("COMMAND") {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        // Only rows with a TCP `(STATE)` token; everything else is skipped.
        let state = match parse_lsof_state(line) {
            Some(s) => s,
            None => continue,
        };
        let pid: u32 = match fields[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let process_name = Some(fields[0].to_string());
        // The `name` field looks like `*:8080`, `127.0.0.1:3000`, `[::1]:8080`,
        // `[::]:8080`, or for connected sockets `local->remote`. It's the 9th
        // column when -nP is used; we keep only the local endpoint.
        let local = fields[8].split("->").next().unwrap_or(fields[8]);
        if let Some((addr, port)) = parse_lsof_addr(local) {
            out.push(ListeningPort {
                pid,
                port,
                addr,
                process_name,
                state,
            });
        }
    }
    out
}

/// Extract the parenthesized TCP state token at the end of an lsof row
/// (e.g. `(LISTEN)`, `(ESTABLISHED)`). Returns `None` when absent.
fn parse_lsof_state(line: &str) -> Option<PortState> {
    let start = line.rfind('(')?;
    let rest = &line[start + 1..];
    let end = rest.find(')')?;
    Some(state_from_lsof(&rest[..end]))
}

fn parse_lsof_addr(s: &str) -> Option<(IpAddr, u16)> {
    // IPv6: "[<addr>]:<port>" or "[::]:port"
    if let Some(rest) = s.strip_prefix('[') {
        let (addr_part, port_part) = rest.split_once("]:")?;
        let port: u16 = port_part.parse().ok()?;
        if addr_part == "::" {
            return Some((IpAddr::V6(Ipv6Addr::UNSPECIFIED), port));
        }
        let addr: Ipv6Addr = addr_part.parse().ok()?;
        return Some((IpAddr::V6(addr), port));
    }
    // IPv4 or wildcard: "*:8080" or "127.0.0.1:8080"
    let (addr_part, port_part) = s.rsplit_once(':')?;
    let port: u16 = port_part.parse().ok()?;
    if addr_part == "*" {
        return Some((IpAddr::V4(Ipv4Addr::UNSPECIFIED), port));
    }
    let addr: Ipv4Addr = addr_part.parse().ok()?;
    Some((IpAddr::V4(addr), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wildcard_v4() {
        let (addr, port) = parse_lsof_addr("*:8080").unwrap();
        assert_eq!(port, 8080);
        assert_eq!(addr, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }

    #[test]
    fn parse_loopback_v4() {
        let (addr, port) = parse_lsof_addr("127.0.0.1:3000").unwrap();
        assert_eq!(port, 3000);
        assert_eq!(addr, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn parse_wildcard_v6() {
        let (addr, port) = parse_lsof_addr("[::]:9090").unwrap();
        assert_eq!(port, 9090);
        assert_eq!(addr, IpAddr::V6(Ipv6Addr::UNSPECIFIED));
    }

    #[test]
    fn parse_loopback_v6() {
        let (addr, port) = parse_lsof_addr("[::1]:8080").unwrap();
        assert_eq!(port, 8080);
        assert_eq!(addr, IpAddr::V6(Ipv6Addr::LOCALHOST));
    }

    #[test]
    fn parse_lsof_full_output() {
        let sample = "\
COMMAND   PID  USER  FD   TYPE             DEVICE SIZE/OFF NODE NAME
node    12345  dev   22u  IPv4 0x1234567890abcdef      0t0  TCP *:3000 (LISTEN)
node    12345  dev   23u  IPv6 0x1234567890abcdef      0t0  TCP [::1]:8080 (LISTEN)
ssh     54321  dev   3u   IPv4 0x1234                  0t0  TCP 1.2.3.4:22->5.6.7.8:55 (ESTABLISHED)
";
        let results = parse_lsof(sample);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].pid, 12345);
        assert_eq!(results[0].port, 3000);
        assert_eq!(results[0].state, PortState::Listen);
        assert_eq!(results[1].port, 8080);
        assert_eq!(results[2].port, 22);
        assert_eq!(results[2].state, PortState::Established);
        assert_eq!(results[2].addr, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn parse_lsof_state_extracts_token() {
        assert_eq!(
            parse_lsof_state("... TCP *:3000 (LISTEN)"),
            Some(PortState::Listen)
        );
        assert_eq!(
            parse_lsof_state("... TCP 1.2.3.4:22->5.6.7.8:55 (CLOSE_WAIT)"),
            Some(PortState::CloseWait)
        );
        assert_eq!(parse_lsof_state("... TCP *:3000"), None);
    }

    #[test]
    fn parse_lsof_fills_process_name() {
        let sample = "\
COMMAND   PID  USER  FD   TYPE             DEVICE SIZE/OFF NODE NAME
python3 12345 dev   22u  IPv4 0x1                  0t0 TCP *:3000 (LISTEN)
";
        let results = parse_lsof(sample);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].process_name.as_deref(), Some("python3"));
    }
}

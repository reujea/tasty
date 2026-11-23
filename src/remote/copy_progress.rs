//! attach mirror가 받는 바이트에서 원격 프로그램의 OSC 52 쓰기가 오가는 동안을 본다.
//!
//! 복사 접수는 쓰기 본문의 첫 바이트가 이 client에 도착한 때다. 그 전(원격 프로그램이 복사를 정하고
//! 시퀀스를 보내기 시작하기까지)은 바이트로 관측할 수 없다. 경계 판정은 mirror 터미널의 termwiz와
//! 같은 `vtparse` 상태 기계를 surface마다 따로 돌려 얻고, 끝난 시퀀스의 종류도 termwiz의
//! `OperatingSystemCommand::parse`로 정한다. 그래서 C1 도입자·UTF-8로 쓴 C1·ESC·CAN·SUB 중단도
//! termwiz와 같이 본다.
//!
//! 끝난 쓰기는 '결과 대기'로 남고 client가 같은 surface의 `TerminalClipboardSet`을 처리할 때 오래된
//! 것부터 하나씩 닫힌다. termwiz가 받지 않은 시퀀스(잘못된 base64 등)는 끝나는 즉시 실패다. 질의나
//! 지우기로 끝나면(앞머리 관측이 쓰기로 잘못 본 경우) 실패 없이 접수만 거둔다.
//!
//! 앞머리 판정은 원시 바이트를 본다. vtparse가 OSC 안에서 무시하는 C0 바이트는 건너뛰지만 그 밖의
//! 정규화 차이로 접수를 놓치면 진행 표시만 빠지고 기록·토스트는 그대로다. vtparse의 OSC 버퍼는 본문
//! 크기만큼 자라고 surface 상태를 지울 때 해제된다.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use termwiz::escape::osc::OperatingSystemCommand;

/// OSC 52 머리(도입자·`52;`·선택 인자·`;`·데이터 첫 두 바이트)를 판정하기에 충분한 길이.
const HEAD_LIMIT: usize = 64;

#[derive(Default)]
pub(crate) struct CopyProgress {
    surfaces: HashMap<u32, SurfaceScan>,
    /// 실패로 끝난 접수 건의 surface. 알릴 곳이 정해지면 꺼낸다.
    failures: Vec<u32>,
}

struct SurfaceScan {
    parser: vtparse::VTParser,
    head: Head,
    /// 본문을 받는 중인 쓰기의 접수 시각.
    receiving: Option<Instant>,
    /// 끝났고 client 기록 결과를 기다리는 쓰기의 접수 시각. 바이트 순서다.
    awaiting: VecDeque<Instant>,
}

impl Default for SurfaceScan {
    fn default() -> Self {
        Self {
            parser: vtparse::VTParser::new(),
            head: Head::Idle,
            receiving: None,
            awaiting: VecDeque::new(),
        }
    }
}

enum Head {
    /// 파서가 ground에 있다.
    Idle,
    /// ground를 떠난 뒤의 바이트. 아직 OSC 52 쓰기인지 모른다.
    Collect(Vec<u8>),
    /// OSC 52 쓰기 본문을 받는 중이거나, 다른 시퀀스라 끝날 때까지 볼 것이 없다.
    Decided,
}

enum Verdict {
    Undecided,
    Copy,
    Other,
}

/// vtparse가 OSC 문자열 안에서 버리는 C0. BEL·CAN·SUB·ESC는 시퀀스를 끝내므로 남긴다.
fn ignored_in_osc(byte: u8) -> bool {
    byte < 0x20 && !matches!(byte, 0x07 | 0x18 | 0x1a | 0x1b)
}

/// 도입자 뒤가 `52;<선택>;<데이터>`이고 데이터가 질의 `?` 하나가 아님이 확정됐는지.
fn classify(raw: &[u8]) -> Verdict {
    let mut head = Vec::with_capacity(raw.len());
    for (index, &byte) in raw.iter().enumerate() {
        // 도입자의 ESC는 남기고 그 뒤 OSC 안의 무시 바이트만 버린다.
        if index > 0 && ignored_in_osc(byte) {
            continue;
        }
        head.push(byte);
    }
    let head = head.as_slice();
    const INTRODUCERS: [&[u8]; 3] = [b"\x1b]", b"\x9d", b"\xc2\x9d"];
    let Some(rest) = INTRODUCERS
        .iter()
        .find_map(|intro| head.strip_prefix(*intro))
    else {
        return if INTRODUCERS.iter().any(|intro| intro.starts_with(head)) {
            Verdict::Undecided
        } else {
            Verdict::Other
        };
    };
    let command = b"52;";
    if rest.len() < command.len() {
        return if command.starts_with(rest) {
            Verdict::Undecided
        } else {
            Verdict::Other
        };
    }
    if !rest.starts_with(command) {
        return Verdict::Other;
    }
    let after = &rest[command.len()..];
    let Some(split) = after.iter().position(|&b| b == b';') else {
        return Verdict::Undecided;
    };
    match &after[split + 1..] {
        [] | [b'?'] => Verdict::Undecided,
        _ => Verdict::Copy,
    }
}

/// 끝난 OSC가 OSC 52였을 때 termwiz가 어떻게 받는지.
enum Ended {
    Write,
    /// 질의·지우기. 쓰기가 아니지만 실패도 아니다.
    NotWrite,
    /// termwiz가 받지 않았다(잘못된 base64 등).
    Invalid,
}

#[derive(Default)]
struct Dispatches(Vec<Ended>);

impl vtparse::VTActor for Dispatches {
    fn print(&mut self, _: char) {}
    fn execute_c0_or_c1(&mut self, _: u8) {}
    fn dcs_hook(&mut self, _: u8, _: &[i64], _: &[u8], _: bool) {}
    fn dcs_put(&mut self, _: u8) {}
    fn dcs_unhook(&mut self) {}
    fn esc_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: u8) {}
    fn csi_dispatch(&mut self, _: &[vtparse::CsiParam], _: bool, _: u8) {}
    fn osc_dispatch(&mut self, params: &[&[u8]]) {
        if params.first() != Some(&b"52".as_slice()) {
            return;
        }
        self.0.push(match OperatingSystemCommand::parse(params) {
            OperatingSystemCommand::SetSelection(..) => Ended::Write,
            OperatingSystemCommand::QuerySelection(..)
            | OperatingSystemCommand::ClearSelection(..) => Ended::NotWrite,
            _ => Ended::Invalid,
        });
    }
    fn apc_dispatch(&mut self, _: Vec<u8>) {}
}

impl CopyProgress {
    /// mirror 터미널에 넣기 전의 바이트를 본다.
    pub(crate) fn feed(&mut self, surface: u32, bytes: &[u8], now: Instant) {
        let Self { surfaces, failures } = self;
        let scan = surfaces.entry(surface).or_default();
        let mut ended = Dispatches::default();
        for &byte in bytes {
            let was_ground = scan.parser.is_ground();
            scan.parser.parse_byte(byte, &mut ended);
            // ESC가 OSC를 끝냈으면 그 ESC가 다음 시퀀스의 시작이다.
            let restarted = !ended.0.is_empty() && byte == 0x1b;
            for end in ended.0.drain(..) {
                let receiving = scan.receiving.take();
                match end {
                    Ended::Write => scan.awaiting.push_back(receiving.unwrap_or(now)),
                    Ended::Invalid if receiving.is_some() => failures.push(surface),
                    Ended::Invalid | Ended::NotWrite => {}
                }
            }
            if scan.parser.is_ground() {
                scan.head = Head::Idle;
                continue;
            }
            if restarted {
                scan.head = Head::Collect(vec![byte]);
                continue;
            }
            match &mut scan.head {
                Head::Idle if was_ground => scan.head = Head::Collect(vec![byte]),
                Head::Idle => scan.head = Head::Decided,
                Head::Collect(head) => {
                    head.push(byte);
                    match classify(head) {
                        Verdict::Copy => {
                            scan.receiving.get_or_insert(now);
                            scan.head = Head::Decided;
                        }
                        Verdict::Other => scan.head = Head::Decided,
                        Verdict::Undecided if head.len() >= HEAD_LIMIT => {
                            scan.head = Head::Decided;
                        }
                        Verdict::Undecided => {}
                    }
                }
                Head::Decided => {}
            }
        }
        if scan.receiving.is_none() && scan.awaiting.is_empty() && scan.parser.is_ground() {
            surfaces.remove(&surface);
        }
    }

    /// client가 이 surface의 OSC 52 쓰기 결과를 처리했다. 가장 오래된 결과 대기 건을 닫는다.
    pub(crate) fn settle_write(&mut self, surface: u32) -> bool {
        let Some(scan) = self.surfaces.get_mut(&surface) else {
            return false;
        };
        let settled = scan.awaiting.pop_front().is_some();
        self.forget_if_idle(surface);
        settled
    }

    /// 이 surface의 사건 처리가 끝났는데 결과가 나오지 않은 쓰기를 실패로 닫는다.
    pub(crate) fn fail_unanswered(&mut self, surface: u32) {
        let Some(scan) = self.surfaces.get_mut(&surface) else {
            return;
        };
        let failed = scan.awaiting.len();
        scan.awaiting.clear();
        self.failures.extend(std::iter::repeat_n(surface, failed));
        self.forget_if_idle(surface);
    }

    /// 스트림이 끊겼다. 본문을 받던 쓰기는 실패로 닫고 파서 상태를 버린다. 이미 끝나 결과를 기다리는
    /// 쓰기는 남겨 사건 처리로 정산한다.
    pub(crate) fn break_stream(&mut self, surface: u32) {
        let Some(scan) = self.surfaces.get_mut(&surface) else {
            return;
        };
        if scan.receiving.take().is_some() {
            self.failures.push(surface);
        }
        scan.parser = vtparse::VTParser::new();
        scan.head = Head::Idle;
        self.forget_if_idle(surface);
    }

    /// surface가 사라졌다. 알리지 않고 버린다.
    pub(crate) fn forget(&mut self, surface: u32) {
        self.surfaces.remove(&surface);
    }

    /// 상태가 있는 surface.
    pub(crate) fn surfaces(&self) -> Vec<u32> {
        self.surfaces.keys().copied().collect()
    }

    /// 실패로 끝난 접수 건의 surface를 꺼낸다. 한 건마다 한 항목이다.
    pub(crate) fn take_failures(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.failures)
    }

    fn forget_if_idle(&mut self, surface: u32) {
        if self.surfaces.get(&surface).is_some_and(|scan| {
            scan.receiving.is_none() && scan.awaiting.is_empty() && scan.parser.is_ground()
        }) {
            self.surfaces.remove(&surface);
        }
    }

    /// 표시할 진행 중 복사의 (surface, 접수 시각).
    pub(crate) fn waits(&self) -> impl Iterator<Item = (u32, Instant)> + '_ {
        self.surfaces.iter().flat_map(|(&surface, scan)| {
            scan.receiving
                .iter()
                .chain(scan.awaiting.iter())
                .map(move |&since| (surface, since))
        })
    }
}

#[cfg(test)]
#[path = "copy_progress_tests.rs"]
mod tests;

//! 실제로 그린 프레임을 그 프레임을 연 유발원별로 센다.
//! 요청 수만으로는 어느 유발원이 프레임을 만들었는지 알 수 없다. 직전 present 뒤에 들어온 요청의
//! 유발원(통과·지연 모두)을 모아 두었다가 present 때 그 집합의 유발원마다 한 프레임씩 센다.
//! 여러 유발원이 한 프레임을 함께 열면 각각 세므로 유발원별 합은 present 수보다 클 수 있다.
//! 요청 없이 그린 프레임(OS의 redraw)은 `unattributed`로 센다. 실패한 프레임의 유발원은 남아 있다가
//! 재시도 present에 귀속된다.

use super::RepaintSource;

pub(super) const SOURCES: usize = 5;

fn index(source: RepaintSource) -> usize {
    match source {
        RepaintSource::Interactive => 0,
        RepaintSource::TerminalOutput => 1,
        RepaintSource::InputEcho => 2,
        RepaintSource::EguiAnimation => 3,
        RepaintSource::AttachMirror => 4,
    }
}

/// 유발원별 누계. 창이 살아 있는 동안 계속 오른다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RepaintTotals {
    pub presents: u64,
    pub unattributed: u64,
    /// `Interactive`, `TerminalOutput`, `InputEcho`, `EguiAnimation`, `AttachMirror` 순서.
    pub requests: [u64; SOURCES],
    pub frames: [u64; SOURCES],
}

impl RepaintTotals {
    pub(crate) fn to_json(self) -> serde_json::Value {
        let per_source = |values: [u64; SOURCES]| {
            serde_json::json!({
                "interactive": values[0],
                "terminal_output": values[1],
                "input_echo": values[2],
                "egui": values[3],
                "attach_mirror": values[4],
            })
        };
        serde_json::json!({
            "presents": self.presents,
            "unattributed_frames": self.unattributed,
            "requests": per_source(self.requests),
            "frames": per_source(self.frames),
        })
    }
}

#[derive(Default)]
pub(super) struct FrameAttribution {
    pending: [bool; SOURCES],
    totals: RepaintTotals,
}

impl FrameAttribution {
    pub(super) fn note_request(&mut self, source: RepaintSource) {
        let i = index(source);
        self.pending[i] = true;
        self.totals.requests[i] += 1;
    }

    /// present 한 번을 센다. 돌려주는 값은 이 프레임을 연 유발원 집합이다(1초 창 집계용).
    pub(super) fn note_present(&mut self) -> [bool; SOURCES] {
        let opened = std::mem::take(&mut self.pending);
        self.totals.presents += 1;
        if opened.iter().any(|o| *o) {
            for (count, o) in self.totals.frames.iter_mut().zip(opened) {
                *count += u64::from(o);
            }
        } else {
            self.totals.unattributed += 1;
        }
        opened
    }

    pub(super) fn totals(&self) -> RepaintTotals {
        self.totals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_counts_every_source_since_the_previous_present() {
        let mut a = FrameAttribution::default();
        a.note_request(RepaintSource::TerminalOutput);
        a.note_request(RepaintSource::TerminalOutput);
        a.note_request(RepaintSource::Interactive);
        a.note_present();
        a.note_request(RepaintSource::TerminalOutput);
        a.note_present();
        a.note_present();
        let t = a.totals();
        assert_eq!(t.presents, 3);
        assert_eq!(t.unattributed, 1);
        assert_eq!(t.requests, [1, 3, 0, 0, 0]);
        assert_eq!(t.frames, [1, 2, 0, 0, 0]);
    }

    /// 입력 에코는 터미널 출력과 따로 세고 JSON에도 따로 나온다.
    #[test]
    fn input_echo_is_its_own_source() {
        let mut a = FrameAttribution::default();
        a.note_request(RepaintSource::InputEcho);
        a.note_present();
        let json = a.totals().to_json();
        assert_eq!(json["requests"]["input_echo"], 1);
        assert_eq!(json["frames"]["input_echo"], 1);
        assert_eq!(json["frames"]["terminal_output"], 0);
    }
}

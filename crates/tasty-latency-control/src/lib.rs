//! 지연 단정의 **대조군**.
//!
//! `assert!(elapsed < LIMIT)` 한 줄은 부하가 만든 값과 코드가 만든 값을 구분하지
//! 못한다. 이 크레이트는 그 자리에 **같은 자원을 지나되 측정 대상의 코드 경로는
//! 지나지 않는 값**을 하나 더 실어서, 실패 문장이 둘 중 어느 쪽인지 스스로 적게
//! 한다. 선택 규칙·거부한 대안·재검토 조건은
//! `docs/adr/0181-a-latency-assertion-must-carry-a-control-that-load-moves-and-code-does-not.md`.
//!
//! ★ **채널 왕복 계열은 아직 자리에 얹지 않는다.** 실측에서 이 계열의 왕복 비용은
//! **프로세스가 놀았는지에 좌우된다** — 200 ms 씩 놀린 뒤 왕복을 여섯 번 재니 배수가
//! 5.4·3.0·1.0… 에서 회차를 거듭할수록 32·33·32·37·32·17 → 36·37·36·37·35·36 으로
//! 굳었다(기준선 6.256 µs). 첫 왕복만 찬 것이 아니라 **전부**다. 즉 바쁠 때 잡은 기준선과
//! 논 뒤에 잡은 표본을 비교하면 부하가 없어도 30 배가 나오고, 그러면 문장이 "러너가
//! 굶었다" 를 **틀리게** 말한다. 이 계열은 표본을 측정 **뒤**가 아니라 측정과 **같은
//! 시간 창 안에서** 잡아야 한다 — 그 설계가 서기 전에는 얹지 않는다.
//!
//! **계열을 자리마다 고른다.** 측정 대상이 CPU 를 기다리면 스케줄러 계열
//! ([`ControlProbe::start`]), 자식 프로세스를 띄우고 기다리면 spawn 계열
//! ([`ControlProbe::start_spawn`]). 실측으로 writeback 이 포화돼 IPC 왕복이 5.4 초일 때
//! 스케줄러 쪽 지표는 7 ms 밖에 안 움직였다 — 계열을 잘못 고르면 대조군이 "부하 아님"
//! 이라고 **잘못 증언한다**. IPC 왕복처럼 채널 뒤에 줄 서는 값의 대조군은 이 크레이트가
//! 아직 안 준다; 같은 채널의 값싼 왕복이어야 하고 그건 호출자 쪽 물건이다.

// 이유: 이 lint 의 출력은 위반 목록이 아니라 **프로덕션 명부**다
// (docs/dev-guide/error-handling.md). 테스트 자리가 섞이면 새 프로덕션 자리가
// 묻히므로, 자리마다가 아니라 크레이트 루트에서 test 범위를 통째로 덮는다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]
use std::time::{Duration, Instant};

/// 계열 하나의 판정 구간. **구간이 셋인 이유가 이 크레이트의 핵심이다.**
///
/// 대조군을 하나의 문턱으로 가르면 "부하였다" 를 근거 없이 주장하게 되고, 그 주장은
/// **거짓 음성**을 만든다 — 진짜 회귀가 "러너 탓" 으로 면제된다. ADR-0181 이 지금
/// 상태(거짓 양성)보다 나쁘다고 못 박은 방향이다. 그래서 계열마다 **유휴만으로 오르는
/// 최악값**을 재고, 그 위를 못 넘는 계열은 굶주림을 **아예 주장하지 않는다.**
#[derive(Debug)]
pub struct Family {
    /// 계열 이름 — 실패 문장이 어느 대조군을 썼는지 밝힌다.
    kind: &'static str,
    /// 이 아래면 조용하다 — 실패의 원인은 측정 대상이다.
    quiet_below: f64,
    /// 이 위면 굶주림을 주장한다. `None` 이면 그 주장을 할 만큼 꼬리가 특성화되지
    /// 않은 계열이고, 그때는 "못 가른다" 로 끝난다.
    starved_at: Option<f64>,
    /// 유휴만으로 관측된 최악 배수 — 문장이 이 수를 싣는다.
    idle_worst: f64,
}

/// 유휴 12 회(각 200 ms) 뒤 최악 배수가 1.81 · 1.71 이었다(실측 2 회, 일감 `2_000_000`).
/// 문턱 3 은 그 위로 충분히 떨어져 있다.
///
/// **이 유휴 벌점은 선점이 아니라 클럭 상태다** — 12 회가 `0.99` 여섯 번 뒤 `1.7x` 여섯 번의
/// 계단으로 나온다. 그래서 같은 크기가 **반대 부호로도** 나타난다: 보정이 느린 클럭에서,
/// 표본이 빠른 클럭에서 잡히면 비율이 0.59(≈ 1/1.7)로 1 **밑**으로 내려간다. 비율이 1 보다
/// 작은 표본을 보고 "대조군이 죽었다" 로 읽으면 안 되는 이유다.
pub static CPU_FAMILY: Family = Family {
    kind: "고정 CPU 일감",
    quiet_below: 2.0,
    starved_at: Some(3.0),
    idle_worst: 1.81,
};

/// ★★ **이 계열은 어느 자리에도 안 얹혀 있다 — ADR-0181 의 규칙 3 을 못 지킨다.**
///
/// 계열 자체는 규칙 1·2 를 만족한다(fork/exec 포화에 반응하고, 측정 대상의 코드는 한 줄도
/// 안 부른다). 문제는 셋째 조건이다: 부하도 유휴도 없이 기준선 대비 **0.8~3.6 배**로
/// 흔들리고, 200 ms 유휴를 끼면 **3.73 · 4.81 배**까지 간다(실측). 그만큼 흔들리는 값에
/// 판정을 붙이면 "러너가 굶었다" 를 근거 없이 말하게 되고, 그것이 그 ADR 이 지금
/// 상태(거짓 양성)보다 나쁘다고 못 박은 **거짓 음성**이다.
///
/// 그래서 이 자리에 한때 얹었던 ssh 세 자리에서 도로 걷어냈다. 부하 창에서 꼬리를
/// 특성화해 `quiet_below` 와 `starved_at` 을 실측으로 정하기 전에는 다시 얹지 않는다.
pub static SPAWN_FAMILY: Family = Family {
    kind: "자식 하나 띄우기",
    quiet_below: 2.0,
    starved_at: None,
    idle_worst: 4.81,
};

/// ★ 유휴 뒤 왕복은 32~37 배까지 굳었다(실측). 비교 자체가 성립하지 않는 구간이라
/// 이 계열은 자리에 얹지 않는다 — 표본을 측정과 같은 시간 창 안에서 잡는 설계가 먼저다.
pub static CHANNEL_FAMILY: Family = Family {
    kind: "같은 상대편으로의 값싼 왕복",
    quiet_below: 2.0,
    starved_at: None,
    idle_worst: 37.0,
};

/// 대조군 한 표본이 어느 구간에 있나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// 조용하다 — 실패의 원인은 측정 대상이다.
    Quiet,
    /// 가를 수 없다 — 이 계열이 유휴만으로도 여기까지 오른다.
    Undecidable,
    /// 러너가 굶었다.
    Starved,
}

/// 대조군 한 번의 일감 크기. 유휴에서 이 크레이트의 test 프로필 기준선이 6.80 ms 였고
/// (`-O` 로 재면 1.56 ms), 실패 경로에서만 도는 값이라 이 비용은 예산에 들어가지 않는다
/// (보정 9 회를 합쳐 74 ms).
///
/// ★★ **이 값을 내려서 통과시키지 마라.** 내리면 ADR-0181 의 규칙 1("부하에 반응한다")이
/// 다시 거짓이 된다. 앞선 값 `50_000`(0.41 ms)에서 이 대조군은 **CPU 포화에 전혀 반응하지
/// 않았다** — 일감이 스케줄러 슬라이스 안에서 끝나 버려 포화가 그것을 밀지 못한다. 흡수가
/// 아니다: 부하를 보정 **뒤에** 켜도 같았다. 실측 2026-09-06, nproc 20:
///
/// | 일감 | 조용히 보정 → 부하 → 표본 (200 busy = 10 배 초과구독) |
/// |---|---|
/// | `50_000` (0.41 ms) | 1.00 · 0.59 · 0.59 · 1.00 — **안 움직인다** |
/// | `1_000_000` (3.4 ms) | 11.46 · 11.36 · 17.03 |
/// | `2_000_000` (6.8 ms) | 13.74 · 17.65 · 6.42 |
///
/// **내려도 되는지는 형용사가 아니라 절차가 답한다.** 내리기 전에 재라: 조용할 때
/// `calibrate()` 하고, 코어 수의 10 배만큼 busy 스레드를 띄운 뒤 `sample()` 한다.
/// 그 비율이 **3 회 연속 2.0 을 넘지 못하면 그 값은 못 쓴다** — 그 대조군은 포화를 못 본다.
///
/// `1_000_000` 도 위 문턱은 넘지만 고르지 않았다. `500_000`~`750_000` 구간이 **양방향으로
/// 불안정**해서다 — 부하 **중에** 보정했는데도 비율이 23.69 까지 튀었고, 그 튐은 아직
/// 설명되지 않았다. 설명 못 하는 구간에서 한 칸 떨어진 값을 고른 것이다.
///
/// ★★ **이 절차를 자동으로 도는 채널은 없다. 그리고 만들지 않는다.**
/// 위 표는 손으로 잰 값이고, 어떤 게이트도 "이 대조군이 부하에 반응하는가" 를 다시
/// 묻지 않는다. 그 사실을 적어 두는 이유는, 안 적으면 **이 크레이트의 초록이 규칙 1 을
/// 지켰다는 뜻으로 읽히기 때문**이다 — 그 초록은 규칙 2 와 계열 분리만 말한다.
///
/// 만들지 않는 이유도 값이다. 채널을 만들려면 시험이 **코어 수의 10 배로 부하를 만들어야**
/// 하고, 그것은 `cargo test --workspace --lib --bins` 안에서 수천 개 형제와 함께 돈다.
/// 작은 러너에서는 그 부하가 형제의 시간 단정을 흔들고, 큰 러너에서는 흡수돼 아무것도
/// 안 잰다. **재려는 성질이 곧 다른 시험을 깨뜨리는 성질**이라 그 자리는 자동 채널에
/// 맞지 않는다. 값을 내릴 때 사람이 위 절차를 밟는다.
const SPIN_ITERS: u64 = 2_000_000;

/// 기준선을 잡을 때 몇 번 재서 **최소**를 고르나. 최소를 쓰는 이유는 여러 번 중
/// 가장 덜 선점된 회차가 유휴 비용에 가장 가깝기 때문이다 — 평균은 그 순간의
/// 부하를 기준선에 섞어 넣어, 부하 속에서 만든 기준선이 부하를 못 보게 만든다.
const CALIBRATION_ROUNDS: usize = 9;

/// spawn 계열은 한 회가 밀리초 단위라 회차를 줄인다. 이 값이 곧 초록 경로에서
/// 띄우는 자식 수라, 대조군 자신이 러너에 부담이 되지 않게 3 으로 둔다.
///
/// **더 올려도 기준선이 수렴하지 않는다** — min-of-3/6/9/15 를 각각 8 번 반복해 재니
/// 기준선 자체가 회차 사이에서 2.05/2.51/2.58/2.27 배로 흔들렸다(실측). 회차를 늘리는 것이
/// 답이 아니라는 뜻이고, 비율이 자기 상대값이라 **회차 안에서는** 문제가 되지 않는다.
const SPAWN_CALIBRATION_ROUNDS: usize = 3;

/// 왕복 계열의 보정 회차. 왕복은 싸서(마이크로초) 회차를 넉넉히 준다.
const ROUND_TRIP_CALIBRATION_ROUNDS: usize = 9;

/// 표본 한 번을 **몇 회 중 최소**로 잡나 — 계열마다 꼬리가 다르다. 실측(40 표본):
///
/// | 계열 | best-of-1 최대 | best-of-3 최대 |
/// |---|---|---|
/// | 고정 CPU 일감 | 1.00 배 | 1.00 배 |
/// | 자식 띄우기 | 1.76 배 | 1.11 배 |
/// | in-process 왕복 | **11.7 배** | 1.1 배 |
///
/// 왕복을 단일 표본으로 재면 꼬리가 문턱 3 배를 넘겨 **"러너가 굶었다" 를 잘못 말한다.**
/// 그것이 이 축이 세고 있는 부류(측정값 자신이 배제하는 원인을 문장이 주장하는 것)라,
/// 꼬리가 있는 계열은 표본도 최소값으로 잡는다.
/// ★ **이 값 1 의 근거는 낡았다 — 지금 고치지 않는다, 다만 낡았다는 사실을 적어 둔다.**
///
/// 위 표의 "고정 CPU 일감 best-of-1 최대 1.01 배" 는 일감이 `50_000`(0.41 ms)이던
/// 시절에 잰 것이고, 그때 이 계열은 **CPU 부하에 아예 반응하지 않았다**(귀먹은 상태).
/// 꼬리가 없어 보였던 것이 계열의 성질이 아니라 **둔감함**이었다는 뜻이다. 형제 둘이
/// 3 인 이유(꼬리가 있다)가 이제 이 계열에도 해당될 수 있다.
///
/// 일감을 `2_000_000` 으로 올린 뒤 다시 재서 1.00/1.00 을 얻었지만, 그 측정도 **유휴
/// 20 코어 한 대**에서 나온 것이라 같은 한계를 갖는다 — 형제 시험 수천 개가 같은
/// 프로세스에서 병렬로 도는 조건(`cargo test --workspace --lib --bins`)에서는 안 쟀다.
/// 그 조건에서 best-of-1 과 best-of-3 의 최대를 각각 재기 전에는 이 값을 못 정한다.
const CPU_SAMPLE_ROUNDS: usize = 1;
const SPAWN_SAMPLE_ROUNDS: usize = 3;
const ROUND_TRIP_SAMPLE_ROUNDS: usize = 3;

/// 자식 하나를 띄우고 거둘 때까지. 측정 대상이 `Command::spawn` 뒤에 줄 서는 자리
/// (ssh 포트 발견 등)의 대조군이다 — fork/exec·스케줄러·바이너리 적재까지 같은 자원을
/// 지나면서, 측정 대상의 코드는 한 줄도 안 부른다.
///
/// 띄우지 못하면 `Duration::ZERO` 를 준다. 기준선이 0 이면 [`ControlSample::ratio`] 가
/// 1.0 을 주므로, **대조군이 없을 때 부하를 주장하지 않는다**.
fn spawn_probe() -> Duration {
    let started = Instant::now();
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/c", "exit"]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = std::process::Command::new("/bin/true");
    let Ok(mut child) = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return Duration::ZERO;
    };
    if child.wait().is_err() {
        return Duration::ZERO;
    }
    started.elapsed()
}

/// `work` 를 `rounds` 번 재서 최소를 준다.
fn calibrate_with(work: fn() -> Duration, rounds: usize) -> Duration {
    let mut baseline = work();
    for _ in 1..rounds {
        baseline = baseline.min(work());
    }
    baseline
}

/// **채널 왕복 계열의 기준선.** 앞의 두 계열과 달리 이 크레이트가 일감을 정할 수 없다 —
/// 이 계열이 잡으려는 포화는 **상대편이 막힌 것**이고, 상대편을 지나야만 잡히기 때문이다.
/// 그래서 왕복은 호출자가 준다.
///
/// `trip` 은 **측정 대상과 같은 상대편**으로의 값싼 왕복 한 번이어야 한다. 측정 대상의
/// 코드 경로는 지나지 않아야 한다(ADR-0181 규칙 2).
///
/// **측정 대상을 돌리기 전에** 부른다.
pub fn calibrate_round_trip(mut trip: impl FnMut() -> Duration) -> Duration {
    let mut baseline = trip();
    for _ in 1..ROUND_TRIP_CALIBRATION_ROUNDS {
        baseline = baseline.min(trip());
    }
    baseline
}

/// 채널 왕복 표본 하나. 꼬리가 커서 최소값으로 잡는다(`ROUND_TRIP_SAMPLE_ROUNDS`).
pub fn round_trip_sample(mut trip: impl FnMut() -> Duration, baseline: Duration) -> ControlSample {
    let mut cost = trip();
    for _ in 1..ROUND_TRIP_SAMPLE_ROUNDS {
        cost = cost.min(trip());
    }
    ControlSample::from_parts_in(&CHANNEL_FAMILY, cost, baseline)
}

/// 고정된 CPU 일감. 최적화로 사라지지 않게 결과를 `black_box` 로 붙잡는다.
fn spin() -> Duration {
    let started = Instant::now();
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for i in 0..SPIN_ITERS {
        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i);
    }
    std::hint::black_box(x);
    started.elapsed()
}

/// 스케줄러 대조군 — 고정된 CPU 일감이 지금 얼마나 걸리나.
///
/// ADR-0181 의 선택 규칙 셋을 이렇게 만족한다: ① 러너가 굶으면 같이 늘어난다
/// (같은 스레드·같은 CPU), ② 측정 대상의 코드를 한 줄도 안 부른다, ③ 싸고 변동이 작다.
#[derive(Debug, Clone, Copy)]
pub struct CpuControl {
    baseline: Duration,
}

impl CpuControl {
    /// 지금 자리의 기준선을 잡는다. **측정 대상을 돌리기 전에** 부른다.
    pub fn calibrate() -> Self {
        Self {
            baseline: calibrate_with(spin, CALIBRATION_ROUNDS),
        }
    }

    /// 기준선을 값으로 준다 — 산술을 부하 없이 고정하는 시험용.
    pub fn with_baseline(baseline: Duration) -> Self {
        Self { baseline }
    }

    pub fn baseline(&self) -> Duration {
        self.baseline
    }

    /// 지금 한 번 재서 기준선과 비교한다.
    pub fn sample(&self) -> ControlSample {
        ControlSample {
            cost: spin(),
            baseline: self.baseline,
            family: &CPU_FAMILY,
        }
    }
}

/// 대조군 한 번의 결과.
#[derive(Debug, Clone, Copy)]
pub struct ControlSample {
    cost: Duration,
    baseline: Duration,
    /// 어느 계열의 대조군인가 — 문장이 이것을 밝혀야 읽는 사람이 "그 대조군이 이 자리에
    /// 맞나" 를 다시 물을 수 있다. 계열을 잘못 고르는 것이 이 설계의 주된 사고다.
    family: &'static Family,
}

impl ControlSample {
    /// 잰 값으로 만든다 — 산술을 부하 없이 고정하는 시험용.
    pub fn from_parts(cost: Duration, baseline: Duration) -> Self {
        Self::from_parts_in(&CPU_FAMILY, cost, baseline)
    }

    pub fn from_parts_in(family: &'static Family, cost: Duration, baseline: Duration) -> Self {
        Self {
            cost,
            baseline,
            family,
        }
    }

    pub fn kind(&self) -> &'static str {
        self.family.kind
    }

    /// 이 표본이 어느 구간에 있나 — 세 구간의 정의는 [`Family`].
    pub fn band(&self) -> Band {
        let r = self.ratio();
        if r < self.family.quiet_below {
            return Band::Quiet;
        }
        match self.family.starved_at {
            Some(t) if r >= t => Band::Starved,
            _ => Band::Undecidable,
        }
    }

    pub fn cost(&self) -> Duration {
        self.cost
    }

    pub fn baseline(&self) -> Duration {
        self.baseline
    }

    /// 기준선 대비 배수. 기준선이 0 이면 비교가 성립하지 않으므로 1.0 을 준다
    /// (= "부풀지 않았다") — 없는 근거로 부하를 주장하지 않는다.
    pub fn ratio(&self) -> f64 {
        let base = self.baseline.as_secs_f64();
        if base <= 0.0 {
            return 1.0;
        }
        self.cost.as_secs_f64() / base
    }

    /// 조용한 구간을 벗어났나. **"굶주림이 확인됐다" 가 아니다** — 계열에 따라
    /// 그 위가 [`Band::Undecidable`] 일 수 있다. 굶주림 주장은 [`Band::Starved`] 만 한다.
    pub fn is_inflated(&self) -> bool {
        self.band() != Band::Quiet
    }
}

/// 변이 시험이 쓰는 문턱. **판정용 밴드([`Family::quiet_below`] = 2.0)와 다른 값이고,
/// 달라야 한다.**
///
/// 밴드는 *"이 비율로 굶주림을 말해도 되나"* 를 정한 값이다. 시험이 물어야 하는 것은
/// *"변이가 만드는 크기와 계열의 잡음이 갈리나"* 이고, 두 물음의 문턱이 같을 이유가 없다.
/// 실제로 200 배 다르다 — 실측:
///
/// | 시험 방향 | 변이가 만드는 크기 | 밴드(2.0) 로 쟀을 때 여유 |
/// |---|---|---|
/// | "움직였다" (양성) | 약 500 배 | **250 배** |
/// | "안 움직였다" (음성) | — (잡음 상한 1.81) | **1.10 배** |
///
/// 여유 1.13 배는 기계가 바뀌면 사라진다. 실제로 CI 에서 스케줄러 계열이 2.4 배로
/// 관측되어 음성 방향 단정이 죽었다 — 그 값은 위반이 아니라 **다른 기계의 유휴 꼬리**였다.
/// 그래서 음성 방향은 밴드가 아니라 이 문턱으로 잰다: 잡음 상한(1.81~2.4)보다 넉넉히
/// 위이고, 변이 크기(약 500 배)보다 한참 아래다.
pub const MUTATION_MARGIN: f64 = 10.0;

/// 대조군이 **측정 대상 경로를 안 지났나** (ADR-0181 규칙 2 의 음성 방향).
///
/// 지났다면 그 경로에 넣은 인위적 지연이 대조군 값에 통째로 실려 수백 배가 된다.
/// 그래서 [`MUTATION_MARGIN`] 아래면 "안 지났다" 로 읽는다 — 이 술어는 **잡음과 변이를**
/// 가르지, 조용함과 굶주림을 가르지 않는다(그건 [`ControlSample::band`] 의 일이다).
pub fn control_stayed_out_of_the_path(sample: &ControlSample) -> bool {
    sample.ratio() < MUTATION_MARGIN
}

/// 두 계열이 **서로 다른 비율로** 움직였나 — 계열 독립의 판정.
///
/// ★ 독립의 정의는 *"한쪽이 안 움직였다"* 가 **아니다.** 정의는 *"둘이 다른 비율로
/// 움직인다"* 이고, 같은 비율로 움직이면 계열이 둘이 아니라 하나라는 뜻이다.
/// 앞선 판은 정의보다 좁은 단정을 썼다 — 절대 밴드로 `held` 가 **전혀** 안 움직였음을
/// 요구했고, 그 여유가 1.13 배라 기계가 바뀌자 죽었다.
///
/// 이 술어는 두 비율의 **비**를 본다. 계열이 하나였다면 그 비가 1 근처이므로 변이가
/// 그대로 잡힌다. 실측 분리 — **모수가 기계다**:
///
/// | 기계 | 채널 | 스케줄러 | 분리 |
/// |---|---|---|---|
/// | 이 개발기(조용) | 506 배 | 1.0 배 | **505 배** |
/// | CI 러너(관측된 최악) | (미관측) | 2.4 배 | 약 **210 배** (채널 506 가정) |
///
/// 문턱 10 배는 그 아래로 한 자릿수, 계열이 하나일 때의 값(1 근처) 위로 한 자릿수다.
/// 잡음이 몇 배 흔들려도 살아남고, 변이는 그대로 죽는다 — 실측으로 양방향 확인했다.
/// 대조군이 **정말 다시 쟀나.**
///
/// ADR-0181 의 규칙 셋에 안 적혀 있던 전제다. "안 움직였다" 는 **죽은 대조군과 구별되지
/// 않는다** — `sample()` 이 기준선을 그대로 돌려주기만 해도 음성 방향 단정이 전부 통과한다.
/// 맞아서 초록인 것이 아니라 **빗나가서** 초록인 것이다. 진짜로 다시 잰 값은 나노초까지
/// 기준선과 같을 수 없으므로, 같으면 다시 안 잰 것으로 읽는다.
///
/// 이 술어가 인라인 비교가 아니라 함수인 이유는 하나다 — **죽은 표본을 먹여 볼 수 있어야
/// 한다.** 인라인이면 그 자리의 초록이 "산 것을 통과시켰다" 인지 "무엇이든 통과시킨다" 인지
/// 안 갈린다.
pub fn control_was_actually_resampled(sample: &ControlSample) -> bool {
    sample.cost() != sample.baseline()
}

pub fn families_separated(moved: &ControlSample, held: &ControlSample) -> bool {
    moved.ratio() > held.ratio() * MUTATION_MARGIN
}

/// 지연 단정이 실패할 때 실을 문장. **두 사건을 가른다.**
pub fn latency_verdict(
    what: &str,
    measured: Duration,
    limit: Duration,
    sample: &ControlSample,
) -> String {
    let ratio = sample.ratio();
    let kind = sample.family.kind;
    match sample.band() {
        Band::Starved => format!(
            "{what} 이 {measured:?} 걸려 상한 {limit:?} 를 넘었다 — 그런데 대조군({kind})도 \
             기준선의 {ratio:.1} 배로 부풀었다({:?} → {:?}). 러너가 굶은 것이라 \
             이 빨강은 코드에 대한 증거가 아니다. 상한 인상은 이 사건의 처방이 아니다",
            sample.baseline, sample.cost,
        ),
        Band::Undecidable => format!(
            "{what} 이 {measured:?} 걸려 상한 {limit:?} 를 넘었다. 대조군({kind})은 기준선의 \
             {ratio:.1} 배다({:?} → {:?}) — ★ **이 값으로는 못 가른다.** 이 계열은 부하가 \
             없어도 유휴만으로 {:.1} 배까지 오르는 것이 실측됐다. 코드 탓으로도 러너 탓으로도 \
             세지 말고, 조용한 상태에서 다시 재라",
            sample.baseline, sample.cost, sample.family.idle_worst,
        ),
        Band::Quiet => format!(
            "{what} 이 {measured:?} 걸려 상한 {limit:?} 를 넘었다. 대조군({kind})은 기준선의 \
             {ratio:.1} 배로 정상이다({:?} → {:?}) — 러너가 아니라 측정 대상 자신이 느려졌다",
            sample.baseline, sample.cost,
        ),
    }
}

/// 대조군을 들고 있다가, 어느 경로로 빠져나가든 값을 남긴다.
///
/// 이 헬퍼의 존재 이유가 "지연 단정이 실패할 때 대조군 값을 남기는 것" 인데,
/// 측정 도중 **다른** 단정이 먼저 패닉하면 그 값이 통째로 사라진다. 그래서 출력을
/// `Drop` 에 둔다 — 되감기(unwind) 중에도 값이 나온다. 초록일 때는 아무 말도 안 한다.
pub struct ControlProbe {
    label: String,
    work: fn() -> Duration,
    family: &'static Family,
    baseline: Duration,
    sample_rounds: usize,
    reported: bool,
}

impl ControlProbe {
    /// 스케줄러 계열. **측정 대상을 돌리기 전에** 만든다 — 기준선이 측정 전 상태를
    /// 담아야 한다. 측정 대상이 CPU·락만 기다리는 자리에 쓴다.
    pub fn start(label: impl Into<String>) -> Self {
        Self::with(
            label,
            spin,
            CALIBRATION_ROUNDS,
            CPU_SAMPLE_ROUNDS,
            &CPU_FAMILY,
        )
    }

    /// 프로세스 spawn 계열. 측정 대상이 자식을 띄우고 기다리는 자리에 쓴다 —
    /// 스케줄러 계열은 fork/exec·바이너리 적재의 포화를 못 본다.
    pub fn start_spawn(label: impl Into<String>) -> Self {
        Self::with(
            label,
            spawn_probe,
            SPAWN_CALIBRATION_ROUNDS,
            SPAWN_SAMPLE_ROUNDS,
            &SPAWN_FAMILY,
        )
    }

    fn with(
        label: impl Into<String>,
        work: fn() -> Duration,
        rounds: usize,
        sample_rounds: usize,
        family: &'static Family,
    ) -> Self {
        Self {
            label: label.into(),
            work,
            family,
            baseline: calibrate_with(work, rounds),
            sample_rounds,
            reported: false,
        }
    }

    fn sample(&self) -> ControlSample {
        let mut cost = (self.work)();
        for _ in 1..self.sample_rounds {
            cost = cost.min((self.work)());
        }
        ControlSample {
            cost,
            baseline: self.baseline,
            family: self.family,
        }
    }

    /// 실패 문장을 만든다. 이 값이 패닉 메시지에 실리므로 `Drop` 은 입을 다문다.
    pub fn verdict(&mut self, measured: Duration, limit: Duration) -> String {
        self.reported = true;
        let sample = self.sample();
        latency_verdict(&self.label, measured, limit, &sample)
    }
}

/// `Drop` 이 입을 여는 조건. 순수 함수로 빼둔 이유는 이 조건 자신이 "초록이면 침묵하고 패닉이면
/// 값을 낸다" 는 약속의 내용이라
/// 시험 대상이어야 하기 때문이다 — 초록일 때 떠들면 13 자리가 매 실행마다 잡음을 내고,
/// 패닉일 때 입을 다물면 이 크레이트가 존재할 이유가 없어진다.
fn drop_should_speak(reported: bool, panicking: bool) -> bool {
    !reported && panicking
}

impl Drop for ControlProbe {
    fn drop(&mut self) {
        if !drop_should_speak(self.reported, std::thread::panicking()) {
            return;
        }
        let sample = self.sample();
        eprintln!(
            "[대조군 {} · {}] 이 패닉은 지연 단정이 낸 것이 아니다 — 대조군은 기준선의 {:.1} 배다({:?} → {:?}). \
             3 배 이상이면 러너가 굶은 것이고, 그때 위 실패의 원인 지목을 믿으면 안 된다.",
            self.label,
            self.family.kind,
            sample.ratio(),
            sample.baseline(),
            sample.cost(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 초록일 때도 남는 관측 한 줄 — 이 부류의 물음은 "어느 기계에서 어느 시험의
    /// **여유가 얼마인가**" 이고, 지금은 초록이면 아무 수도 안 남는다.
    ///
    /// **채널이 짝이어야 성립한다.** libtest 는 통과한 테스트의 출력을 삼키므로 이 줄은
    /// `--show-output`(또는 `--nocapture`) 으로 부를 때만 보인다. 통합 회차가 이 크레이트를
    /// 그 플래그로 부르는 별도 스텝을 갖는다 — 본 leg 에 붙이면 수천 건 출력이 로그를 덮는다.
    /// 안 보인다고 "관측이 없다" 로 읽지 마라.
    ///
    /// 접두사가 고정이라 `grep '^\[latency-control\]'` 로 캔다.
    ///
    /// ★ 테스트 이름을 **문자열로 안 적는다** — 적으면 함수 이름과 조용히 어긋난다.
    /// 이 자리의 함수 경로에서 뽑으므로 이름을 바꾸면 출력도 따라간다.
    macro_rules! observe {
        ($($k:ident = $v:expr),+ $(,)?) => {{
            fn here() {}
            let path = std::any::type_name_of_val(&here);
            let name = path
                .strip_suffix("::here")
                .unwrap_or(path)
                .rsplit("::")
                .next()
                .unwrap_or(path);
            let mut line = format!("[latency-control] {name}");
            $( line.push_str(&format!(" {}={}", stringify!($k), $v)); )+
            println!("{line}");
        }};
    }

    /// 관측값의 표기를 한 곳으로 — 자릿수가 시험마다 다르면 추세를 못 캔다.
    fn n(v: f64) -> String {
        format!("{v:.2}")
    }

    /// 변이 시험이 측정 대상 경로에 넣는 지연의 크기 — **기준선의 몇 배인가**로 정한다.
    ///
    /// ★★ **절대 시간으로 되돌리지 마라.** 이 값이 한때 `200 ms` 고정이었고, 그때 양성 대조의
    /// 배수는 `1 + 200ms / baseline` 이었다 — 좌변을 **기계가** 정했다. baseline 이 22.2 ms 를
    /// 넘으면 문턱 [`MUTATION_MARGIN`] 을 구조적으로 못 넘고, baseline 은 부하가 올린다.
    /// 실측 2026-09-23(nproc 20, 시험 하나만 한 스레드로 10 회씩):
    ///
    /// | 조건 | 고정 200 ms 통과/실패 | baseline(ms) |
    /// |---|---|---|
    /// | 주변 부하만 | 10/0 | 12.7 ~ 16.6 |
    /// | +20 busy (1× 코어) | 2/8 | 16.9 ~ 46.5 |
    /// | +60 busy (3×) | 2/8 | 13.5 ~ 76.5 |
    /// | +200 busy (10×) | 0/10 | 38.8 ~ 147.9 |
    ///
    /// 그 빨강은 술어의 결함이 아니라 **시험의 검출력이 부하에 먹힌 것**이었다 — 경로를 지난
    /// 대조군도 그 회차에서는 통과했을 것이다. 배수로 정하면 검출력이 부하와 무관해진다.
    ///
    /// 문턱의 **두 배**인 이유: 경로를 지난 대조군의 값은 `스핀 + 지연` 인데, 스핀 자신이
    /// 클럭 상태로 기준선의 0.59 배까지 내려간다([`CPU_FAMILY`]). 그래도 `0.59 + 20` 이 문턱
    /// 10 을 두 배로 넘는다. 문턱에 묶어 두었으므로 문턱을 바꾸면 이 값도 따라간다.
    ///
    /// 결정과 대안은 `docs/adr/0552-a-positive-control-sizes-its-injection-from-the-calibrated-baseline.md`.
    const INJECTED_OVER_BASELINE: f64 = 2.0 * MUTATION_MARGIN;

    /// ★ ADR-0181 의 규칙 2 를 재는 변이 단정이다.
    ///
    /// 측정 대상 경로에 인위적 지연을 넣는다 — 여기서는 "그 경로가 막혀 있다" 를 바쁜 시간으로
    /// 흉내낸다. 대조군이 그 경로를 지나면 기준선(`calibrate` 가 잰 순수 스핀) 대비
    /// 비율이 통째로 튄다. 이 단정이 없으면 "대조군은 코드에 반응하지 않는다" 는
    /// **가정**이고, 그때 이 설계는 지금(거짓 양성)보다 나쁜 **거짓 음성**을 만든다.
    ///
    /// **앞뒤 차분으로는 이 변이를 못 잡는다** — 대조군이 그 경로를 지나면 앞 표본도
    /// 같이 부풀어 비가 상쇄된다. 그래서 기준선 대비 절대값으로 잰다.
    ///
    /// 넣는 지연은 **이 회차의 기준선에서 잰다** — 주입분 = [`INJECTED_OVER_BASELINE`] × 기준선.
    /// 그래서 주입으로 더해지는 시간이 부하에 따라 늘어난다. 이 판에서 잰 기준선 범위와 그 주입분은
    /// ADR-0552 의 Consequences("얻은 것" · "잃은 것")에 있다. 그 ADR Context 의 표는 고정 200 ms
    /// 판의 회차라 거기서 주입분은 200 ms 였다.
    #[test]
    fn an_artificial_delay_in_the_measured_path_does_not_move_the_control() {
        let control = CpuControl::calibrate();
        let injected = control.baseline().mul_f64(INJECTED_OVER_BASELINE);
        let before = control.sample();
        // 잠이 아니라 바쁜 시간으로 흉내낸다 — 유휴 반응과 코드 반응을 섞지 않으려는 것이다
        // (같은 이유가 spawn·채널 계열 시험에도 있다).
        busy_for(injected);
        let after = control.sample();

        // ★ 양성 대조 — 초록일 때 이 술어가 **무엇을 집는지** 보이게 한다.
        // 아래 음성 단정만 있으면 "통과했다" 가 "변이를 실제로 가른다" 를 뜻하지 않는다.
        // 대조군이 측정 경로를 지났다면 그 경로의 지연이 값에 통째로 실린다.
        let as_if_it_traversed = ControlSample::from_parts_in(
            &CPU_FAMILY,
            after.baseline() + injected,
            after.baseline(),
        );

        // 초록일 때도 이 회차의 검출력을 남긴다 — `baseline_ms` · `injected_ms` 가 이 회차의 값이다.
        // `injected_ratio` 는 측정값이 아니라 구성상 상수(`1 + INJECTED_OVER_BASELINE`)다 — 상수가
        // 바뀌면 달라지고, 부하로는 안 움직인다.
        observe!(
            before = n(before.ratio()),
            after = n(after.ratio()),
            margin = n(MUTATION_MARGIN),
            baseline_ms = n(control.baseline().as_secs_f64() * 1e3),
            injected_ms = n(injected.as_secs_f64() * 1e3),
            injected_ratio = n(as_if_it_traversed.ratio()),
        );
        assert!(
            control_stayed_out_of_the_path(&after),
            "{}",
            control_independence_verdict(&before, &after, injected)
        );
        // 주입이 기준선에 비례하므로 이 배수는 부하와 무관하게 `1 + INJECTED_OVER_BASELINE` 이다.
        // 여기가 빨간 것은 "이 회차가 굶었다" 가 아니라 **술어·문턱·주입 배수 중 하나가
        // 바뀌었다** 는 뜻이다.
        assert!(
            !control_stayed_out_of_the_path(&as_if_it_traversed),
            "술어가 경로를 지난 대조군({:.1} 배 = 기준선 {:?} + 주입 {:?})을 통과시킨다. 주입은 \
             기준선의 {INJECTED_OVER_BASELINE} 배로 정해지므로 이 배수는 부하가 못 움직인다 — \
             술어 `control_stayed_out_of_the_path` · 문턱 `MUTATION_MARGIN`({MUTATION_MARGIN}) · \
             주입 배수 `INJECTED_OVER_BASELINE` 중 하나가 바뀐 것이고, 그 상태에서 위 초록은 변이를 \
             가른 증거가 못 된다",
            as_if_it_traversed.ratio(),
            after.baseline(),
            injected,
        );
    }

    /// 이 시험의 빨강도 두 사건을 가려야 한다.
    ///
    /// ★ **정상 부하도 여기 걸린다** — 꾸준한 굶주림이 기준선(`calibrate`)에 흡수되리라는
    /// 기대는 코어 초과구독에서 반박됐다(리뷰어 실측 2026-09-23, nproc 20, busy 루프를 켜고 2 초
    /// 뒤부터 잼: +20 busy 2/20 · +200 busy 3/20 이 이 단정에서 빨강, 옛 고정 주입 판도 +200 busy
    /// 1/8). 그래서 이 시험이 빨간 것은 "보정 뒤에 무언가가 달라졌다" 는 뜻이지만 그 무언가가
    /// 코드인지 부하인지를 이 자리는 가르지 못한다. 앞 표본이 가르는 것은 그것이 보정 직후부터였는가
    /// 뿐이다. 경위는 `docs/adr/0552-a-positive-control-sizes-its-injection-from-the-calibrated-baseline.md`
    /// 의 "안 고친 것".
    fn control_independence_verdict(
        before: &ControlSample,
        after: &ControlSample,
        injected: Duration,
    ) -> String {
        if before.is_inflated() {
            return format!(
                "보정 **직후** 첫 표본이 이미 기준선의 {:.1} 배다({:?} → {:?}). 그 사이에는 \
                 아무것도 안 끼웠으므로 굶주림으로 설명되지 않는다 — 보정이 도는 일감과 표본이 \
                 도는 일감이 서로 다르다. 대조군 자신이 깨진 것이고, 이 상태의 비율은 부하 \
                 판정에 못 쓴다",
                before.ratio(),
                before.baseline(),
                before.cost(),
            );
        }
        format!(
            "지연을 넣기 전 대조군은 기준선의 {:.1} 배로 정상이었는데, 측정 대상 경로에 {injected:?} \
             지연을 넣은 뒤 {:.1} 배가 됐다({:?}). 대조군이 그 경로를 지난다 — ADR-0181 규칙 2 \
             위반이고, 이 상태에서는 진짜 회귀가 '부하였다' 로 덮인다. ★ 다만 이 자리는 \
             '그 {injected:?} 창에 굶주림이 **시작**한 경우' 와 원리적으로 구분되지 않는다. 조용할 때 \
             한 번 더 재서 재현되면 위반이다",
            before.ratio(),
            after.ratio(),
            after.cost(),
        )
    }

    /// 위 문장이 두 사건을 실제로 가르나 — 양방향.
    #[test]
    fn the_independence_verdict_separates_a_broken_control_from_a_reactive_one() {
        let base = Duration::from_micros(170);

        // 앞 표본이 이미 부풀었다 = 보정과 표본이 다른 일감을 돈다.
        let injected = base * 20;
        let broken = control_independence_verdict(
            &ControlSample::from_parts(base * 900, base),
            &ControlSample::from_parts(base * 900, base),
            injected,
        );
        assert!(broken.contains("대조군 자신이 깨진 것"), "{broken}");
        assert!(!broken.contains("규칙 2\n"), "{broken}");
        assert!(!broken.contains("그 경로를 지난다"), "{broken}");

        // 앞은 정상, 뒤만 부풀었다 = 주입한 지연에 반응했다.
        let reactive = control_independence_verdict(
            &ControlSample::from_parts(base, base),
            &ControlSample::from_parts(base * 900, base),
            injected,
        );
        assert!(reactive.contains("그 경로를 지난다"), "{reactive}");
        assert!(
            reactive.contains("원리적으로 구분되지 않는다"),
            "{reactive}"
        );
        assert!(!reactive.contains("대조군 자신이 깨진 것"), "{reactive}");
    }

    // spawn 계열의 규칙 2 변이 시험은 **여기 없다.** 벽시계 지연으로 흉내내면 잠은
    // 유휴 반응(3.7~4.8 배)을, 바쁜 시간은 CPU 경합(2.0~3.6 배)을 부르고, 둘 다 이 시험이
    // 재려는 "측정 대상 코드에 대한 반응" 과 섞인다. 즉 in-process 로는 못 재는 것이라,
    // 재는 방법이 생기기 전에는 이 계열을 자리에 얹지 않는다(위 `SPAWN_FAMILY`).

    // ── 채널 왕복 계열 ──────────────────────────────────────────────
    //
    // ★ 여기 쓰는 in-process 왕복은 **시험 매개이지 생산용 대조군이 아니다.**
    // 이 계열이 잡으려는 포화는 "상대편이 막힌 것" 인데, in-process 상대편은 디스크
    // 뒤에 줄 서지 않는다 — 실측에서 writeback 이 포화돼 진짜 IPC 왕복이 5453 ms 일 때
    // 스케줄러 지표는 7 ms 였다. 그래서 `calibrate_round_trip` 은 왕복을 **호출자에게
    // 받는다**. 아래 둘은 그 계약이 지켜지는지를 재는 것이지, gui 자리에 이 왕복을
    // 써도 된다는 뜻이 아니다.

    /// 답만 돌려주는 상대편 스레드. `delay` 를 올리면 상대편이 막힌 것을 흉내낸다.
    struct Peer {
        tx: std::sync::mpsc::Sender<u64>,
        rx: std::sync::mpsc::Receiver<u64>,
        delay: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    fn spawn_peer() -> Peer {
        let (tx, in_rx) = std::sync::mpsc::channel::<u64>();
        let (out_tx, rx) = std::sync::mpsc::channel::<u64>();
        let delay = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let peer_delay = delay.clone();
        std::thread::spawn(move || {
            while let Ok(v) = in_rx.recv() {
                let nanos = peer_delay.load(std::sync::atomic::Ordering::Relaxed);
                if nanos > 0 {
                    std::thread::sleep(Duration::from_nanos(nanos));
                }
                if out_tx.send(v).is_err() {
                    break;
                }
            }
        });
        Peer { tx, rx, delay }
    }

    /// 프로세스를 **놀리지 않고** 시간을 보낸다.
    fn busy_for(how_long: Duration) {
        let started = Instant::now();
        while started.elapsed() < how_long {
            spin();
        }
    }

    fn trip(peer: &Peer) -> Duration {
        let started = Instant::now();
        peer.tx.send(1).expect("상대편 살아 있음");
        peer.rx.recv().expect("상대편 응답");
        started.elapsed()
    }

    // ★★ 채널 계열의 규칙 2(음성 방향) 시험도 **여기 없다** — spawn 과 같은 이유다.
    // in-process 에서 "측정 대상 경로가 느리다" 를 흉내내는 방법이 둘뿐인데 둘 다 대조군을
    // 움직인다: 잠은 유휴 스케일링(왕복 32~37 배), 바쁜 시간은 CPU 경합(2.6~22 배).
    // 그래서 이 계열은 규칙 1(아래 양성 방향)만 재어져 있고 규칙 2 는 미측정이다 —
    // 미측정인 채로 자리에 얹지 않는다.

    /// ★ 양성 방향 — **상대편이 막히면** 왕복 대조군은 움직여야 한다. 부하를 안 만들고
    /// 재는 방법이 이것이다: 러너를 굶기는 대신 상대편에게 지연을 준다. 이 방향이 없으면
    /// "부하에 반응한다"(ADR-0181 규칙 1)가 이 계열에서는 가정으로 남는다.
    #[test]
    fn the_round_trip_control_moves_when_the_peer_is_blocked() {
        let peer = spawn_peer();
        let baseline = calibrate_round_trip(|| trip(&peer));
        // 조용할 때의 값은 **단정하지 않는다** — 그 값이 이 계열의 잡음 대상이고,
        // 여기서 재려는 것은 "상대편이 막히면 움직이나" 하나다.
        peer.delay.store(
            Duration::from_millis(5).as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        let blocked = round_trip_sample(|| trip(&peer), baseline);
        observe!(
            ratio = n(blocked.ratio()),
            band = format!("{:?}", blocked.band()),
        );
        assert!(
            blocked.is_inflated(),
            "상대편이 5 ms 막혔는데 대조군이 기준선의 {:.1} 배에 그쳤다({:?} → {:?}) — \
             이 계열은 상대편 포화에 반응하지 않는다는 뜻이고, 그러면 gui 자리에서 \
             '부하 아님' 이라고 잘못 증언한다",
            blocked.ratio(),
            blocked.baseline(),
            blocked.cost(),
        );
        assert!(blocked.kind() == CHANNEL_FAMILY.kind);

        // ★ 양성 대조 — `is_inflated()` 가 **늘 참이라서** 위가 초록인 것이 아니다.
        // 이 단정의 전부가 "움직였다" 하나라, 술어가 항진식이면 상대편을 안 막아도
        // 초록이고 그때 이 시험은 아무것도 안 잰다(조용한 1).
        let quiet =
            ControlSample::from_parts_in(&CHANNEL_FAMILY, blocked.baseline(), blocked.baseline());
        assert!(
            !quiet.is_inflated(),
            "술어가 기준선과 같은 표본을 '부풀었다' 로 판정한다({:.2} 배) — \
             그러면 위 초록은 상대편이 막혔다는 증거가 못 된다",
            quiet.ratio()
        );
    }

    /// ★★ 계열이 정말 셋인가 — **부하를 안 만들고** 재는 방법.
    ///
    /// "세 수가 부하 아래서 서로 다른 비율로 움직이면 독립" 이라는 판정은 부하를 요구한다.
    /// 그런데 **한 자원만 콕 집어 막으면** 같은 판정을 부하 없이 할 수 있다 — 상대편에게만
    /// 지연을 주고 셋을 동시에 본다. 계열이 하나였다면 셋 다 같이 움직였을 것이다.
    ///
    /// 이 시험이 가르는 짝은 **채널 ↔ 스케줄러** 하나다. spawn 은 흔들림이 커서(위
    /// `SPAWN_FAMILY`) 이 판정에 못 넣는다 — 그 계열이 얹히려면 부하 창에서 꼬리부터 재야 한다.
    #[test]
    fn blocking_only_the_peer_moves_only_the_channel_family() {
        let peer = spawn_peer();
        let trip_baseline = calibrate_round_trip(|| trip(&peer));
        let cpu = CpuControl::calibrate();

        // 상대편만 막는다. CPU 는 건드리지 않는다.
        peer.delay.store(
            Duration::from_millis(5).as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        let channel = round_trip_sample(|| trip(&peer), trip_baseline);
        let scheduler = cpu.sample();

        observe!(
            channel = n(channel.ratio()),
            scheduler = n(scheduler.ratio()),
            separation = n(channel.ratio() / scheduler.ratio().max(f64::MIN_POSITIVE)),
            margin = n(MUTATION_MARGIN),
        );
        assert!(
            channel.is_inflated(),
            "상대편을 막았는데 채널 계열이 {:.1} 배에 그쳤다",
            channel.ratio()
        );
        // ★ "안 움직였다" 는 **죽은 대조군과 구별되지 않는다.** `sample()` 이 기준선을
        // 그대로 돌려주기만 해도 아래 단정은 통과한다 — 그건 맞아서 초록인 것이 아니라
        // 빗나가서 초록인 것이다. 그래서 살아 있음을 먼저 묻는다: 진짜로 다시 잰 값은
        // 기준선과 정확히 같을 수 없다.
        assert!(
            control_was_actually_resampled(&scheduler),
            "스케줄러 대조군이 기준선을 그대로 돌려줬다({:?}) — 다시 재지 않았다는 뜻이고, \
             그러면 아래 '안 움직였다' 는 독립의 증거가 못 된다",
            scheduler.cost()
        );
        // ★ 생존 단정 자신의 대조 — 이 술어가 **죽은 대조군을 실제로 거부하는가.**
        // 없으면 위 초록은 "산 것을 통과시켰다" 가 아니라 "무엇이든 통과시킨다" 로도
        // 설명된다. 그것이 이 시험이 막으려던 바로 그 형태다.
        let never_resampled =
            ControlSample::from_parts_in(&CPU_FAMILY, scheduler.baseline(), scheduler.baseline());
        assert!(
            !control_was_actually_resampled(&never_resampled),
            "술어가 기준선을 그대로 돌려준 표본을 '다시 쟀다' 로 판정한다 — \
             위 생존 단정은 아무것도 안 지킨다"
        );
        // ★ 독립의 판정은 "스케줄러가 **안** 움직였다" 가 아니라 "둘이 **다른 비율로**
        // 움직였다" 이다. 앞선 판은 절대 밴드(2.0)로 전자를 요구했고, 그 여유가 1.13 배라
        // (잡음 상한 1.81) 기계가 바뀌자 죽었다 — CI 에서 스케줄러가 2.4 배로 관측됐다.
        // 그 2.4 는 위반이 아니라 **다른 기계의 유휴 꼬리**다. 이 시험은 계열 사이에
        // 15 ms 의 유휴 구간을 자기가 만든다(막힌 왕복 3 회) — 그 유휴가 곧 그 꼬리다.
        assert!(
            families_separated(&channel, &scheduler),
            "채널 {:.0} 배 · 스케줄러 {:.1} 배 — 분리가 {:.1} 배로 문턱 {:.0} 배에 못 미친다. \
             두 계열이 같은 자원을 보고 있다는 뜻이고, 그러면 '계열이 셋' 이라는 말이 \
             근거를 잃는다",
            channel.ratio(),
            scheduler.ratio(),
            channel.ratio() / scheduler.ratio().max(f64::MIN_POSITIVE),
            MUTATION_MARGIN
        );

        // ★ 양성 대조 — 계열이 **하나였다면** 스케줄러가 채널과 같은 비율로 움직였을 것이다.
        // 같은 술어에 그 표본을 먹여 거부되는 것을 보인다. 이것이 없으면 위 초록은
        // "분리를 실제로 가른다" 가 아니라 "잡음이 작았다" 로도 설명된다.
        let as_if_one_family = ControlSample::from_parts_in(
            &CPU_FAMILY,
            scheduler.baseline().mul_f64(channel.ratio()),
            scheduler.baseline(),
        );
        assert!(
            !families_separated(&channel, &as_if_one_family),
            "술어가 채널과 같은 비율({:.0} 배)로 움직인 스케줄러를 독립으로 판정한다 — \
             위 초록은 독립의 증거가 못 된다",
            as_if_one_family.ratio()
        );

        // ★ 남는 한계: 살아 있고 안 움직였다는 것까지는 재어졌지만, **CPU 가 실제로
        // 포화됐을 때 이 계열이 움직인다**(규칙 1)는 것은 부하 없이 못 잰다. 채널 계열은
        // 상대편에 지연을 주는 길이 있어서 쟀고, CPU 계열은 그런 길이 없다 —
        // CPU 를 포화시키는 것이 곧 부하 생성이기 때문이다. 부하 창에서 잰다.
    }

    /// 부하 없이 산술만 고정한다 — 양방향.
    #[test]
    fn the_verdict_separates_a_starved_runner_from_a_slow_code_path() {
        let base = Duration::from_micros(170);
        let limit = Duration::from_millis(50);
        let measured = Duration::from_millis(80);

        let starved = ControlSample::from_parts(base * 9, base);
        let msg = latency_verdict("await 즉시 반환", measured, limit, &starved);
        assert!(msg.contains("러너가 굶은 것"), "{msg}");
        assert!(msg.contains("증거가 아니다"), "{msg}");
        assert!(msg.contains("상한 인상은 이 사건의 처방이 아니다"), "{msg}");

        let quiet = ControlSample::from_parts(base * 3 / 2, base);
        let msg = latency_verdict("await 즉시 반환", measured, limit, &quiet);
        assert!(msg.contains("자신이 느려졌다"), "{msg}");
        assert!(!msg.contains("러너가 굶은 것"), "{msg}");

        // ★ 셋째 구간 — 못 가르는 자리는 그렇다고 말한다. 이 문장이 없으면 그 값이
        // 조용함이나 굶주림 중 하나로 잘못 접힌다.
        let murky = ControlSample::from_parts(base * 5 / 2, base);
        let msg = latency_verdict("await 즉시 반환", measured, limit, &murky);
        assert!(msg.contains("이 값으로는 못 가른다"), "{msg}");
        assert!(msg.contains("유휴만으로"), "{msg}");
        assert!(!msg.contains("자신이 느려졌다"), "{msg}");
        assert!(!msg.contains("러너가 굶은 것"), "{msg}");

        // spawn 계열은 아무리 커도 굶주림을 주장하지 않는다.
        let big_spawn = ControlSample::from_parts_in(&SPAWN_FAMILY, base * 50, base);
        let msg = latency_verdict("무엇", measured, limit, &big_spawn);
        assert!(msg.contains("이 값으로는 못 가른다"), "{msg}");
        assert!(!msg.contains("러너가 굶은 것"), "{msg}");
        // 계열이 문장에 실린다 — 잘못 고른 대조군을 읽는 사람이 알아볼 수 있어야 한다.
        assert!(msg.contains(SPAWN_FAMILY.kind), "{msg}");
        assert!(
            latency_verdict("무엇", measured, limit, &quiet).contains(CPU_FAMILY.kind),
            "CPU 계열 문장이 계열을 안 밝힌다"
        );
        let spawn_msg = latency_verdict(
            "무엇",
            measured,
            limit,
            &ControlSample::from_parts_in(&SPAWN_FAMILY, base * 2, base),
        );
        assert!(spawn_msg.contains(SPAWN_FAMILY.kind), "{spawn_msg}");
        assert!(!spawn_msg.contains(CPU_FAMILY.kind), "{spawn_msg}");
    }

    /// 문턱 양쪽 — 3 배 미만은 정상, 3 배는 부하.
    #[test]
    fn the_inflation_threshold_is_a_boundary_not_a_slope() {
        let base = Duration::from_micros(1000);
        // CPU 계열: 2 배 미만은 조용, 2~3 배는 못 가름, 3 배 이상은 굶주림.
        assert_eq!(
            ControlSample::from_parts(base * 2 - Duration::from_micros(1), base).band(),
            Band::Quiet
        );
        assert_eq!(
            ControlSample::from_parts(base * 2, base).band(),
            Band::Undecidable
        );
        assert_eq!(
            ControlSample::from_parts(base * 3, base).band(),
            Band::Starved
        );

        // ★ spawn 계열은 유휴만으로 4.81 배까지 오르므로 굶주림을 **주장하지 않는다**.
        let far = ControlSample::from_parts_in(&SPAWN_FAMILY, base * 50, base);
        assert_eq!(
            far.band(),
            Band::Undecidable,
            "spawn 계열이 굶주림을 주장했다"
        );
        assert_eq!(
            ControlSample::from_parts_in(&CHANNEL_FAMILY, base * 50, base).band(),
            Band::Undecidable
        );
    }

    /// 기준선이 0 이면 비교가 성립하지 않는다 — 없는 근거로 부하를 주장하지 않는다.
    #[test]
    fn a_zero_baseline_does_not_claim_load() {
        let sample = ControlSample::from_parts(Duration::from_secs(9), Duration::ZERO);
        assert!(!sample.is_inflated());
        let msg = latency_verdict(
            "무엇",
            Duration::from_secs(9),
            Duration::from_secs(1),
            &sample,
        );
        assert!(msg.contains("자신이 느려졌다"), "{msg}");
    }

    /// 패닉일 때만 입을 연다. 네 조합 전부.
    #[test]
    fn the_probe_speaks_only_on_the_panicking_path() {
        assert!(drop_should_speak(false, true));
        assert!(!drop_should_speak(true, true));
        assert!(!drop_should_speak(false, false));
        assert!(!drop_should_speak(true, false));
    }

    /// 문장을 가져간 뒤에는 `Drop` 이 다시 말하지 않는다.
    #[test]
    fn taking_the_verdict_silences_the_drop() {
        let mut probe = ControlProbe::start("무엇");
        // 문장 자체는 다른 시험이 본다 — 여기서 재는 것은 `verdict` 를 가져간 뒤
        // `Drop` 이 입을 다무는가 하나다.
        let _ = probe.verdict(Duration::from_millis(80), Duration::from_millis(50));
        assert!(!drop_should_speak(probe.reported, true));
    }
}

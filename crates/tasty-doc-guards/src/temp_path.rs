//! **공유 temp 아래 고정 이름 임시 경로**가 새로 생기는 것을 집는다 — ADR-0129 형태 B.
//!
//! `std::env::temp_dir()` 아래에 경로를 지으면 그것은 이 머신의 **모든 프로세스·모든
//! 완주가 공유하는 이름공간**이다. 거기에 인스턴스/완주를 가르는 성분 없이 고정 이름을
//! 쓰면, 같은 프로필 두 벌(또는 동시 두 완주)이 같은 파일을 truncate 하거나 서로의
//! 디렉터리를 지운다. 실측 사고: `disk_scrollback` 이 `surface-<id>` 만 써서 인스턴스
//! 둘이 스크롤백을 0 으로 만들었다.
//!
//! ## 극성 — "유일해야 하는가" 가 아니라 "유니크화됐는가" 를 묻는다
//!
//! "이 이름은 유일해야 하는가?" 는 **의도**를 읽어야 답한다 — 소스만 보고 못 푼다
//! (사용자 config 는 일부러 공유한다). 그래서 방향을 뒤집는다: **기본값은 "유니크화돼야
//! 한다"** 이고, 의도된 공유는 **예외**다. 예외는 명부가 아니라 **그 자리에 사유**로
//! 적는다(`이유:`/`reason:`/`사유:` — 마커를 쓰는 발상은 `check-allow-reason` 에서
//! 왔지만, **그 가드와 같다고 주장하지 않는다**. 아래 [`REASON_TOKENS`] 참조).
//! 의미 판단이 가드에서 소스로 옮겨가고, 그 자리가 그 판단을 할
//! 수 있는 유일한 자리다. 명부를 안 쓰는 이유: 명부는 자기 대상을 이름으로 지목해
//! "쓰이는 것" 으로 만들고, 정당한 예외가 구조적인 곳에서는 지키려는 표보다 빨리
//! 썩는다.
//!
//! ## 유니크화로 인정하는 것
//!
//! 아래 성분이 경로 짓는 창(temp_dir 호출 줄 ~ +6 줄) 안에 있으면 유니크화로 본다:
//! - `TempDir`/`tempfile`/`tempdir(`/`NamedTempFile` — **OS 가 유일 이름을 준다. 최선**
//!   (재사용 위험 없음).
//! - `process::id`/`pid` — 살아 있는 프로세스마다 유일. pid 는 프로세스가 죽은 뒤
//!   **재사용될 수 있으나**, 임시 스크래치는 Drop/TTL 로 청소되므로 그 창에서 충돌하지
//!   않는다. 그래서 인정하되 최선은 아니다.
//! - `path_for` — 공유 헬퍼(`prompt_file`)가 pid 로 유니크화한다.
//! - `nanos`/`SystemTime` — 시간 nonce. 동시성에서 같은 눈금에 겹칠 창이 있어 **가장
//!   약하다.** 그 창의 크기는 **플랫폼이 정한다**(시계 해상도) — 한 OS 에서 초록인
//!   것이 다른 OS 에서 깨진다.
//!
//! ## 등급은 판정에 **들어간다** (권고가 아니다)
//!
//! 위 목록은 한때 등급을 적어 두기만 했다 — "가장 약하다, 새 코드는 다른 둘을 쓰는 게
//! 낫다". 그런데 이 판정이 묻는 것은 **"유니크화됐는가"** 였지 "충분히 강한가" 가
//! 아니어서, **약한 성분만 쓴 자리는 전부 초록이었다.** 적혀 있는 등급과 집행되는
//! 등급이 갈라져 있었고, 읽는 사람은 그 문장을 강제로 오해한다.
//!
//! 예고된 사고가 예고된 자리에서 났다 — 시간 nonce 에만 기댄 픽스처가 **macOS 에서만**
//! 죽었고, 그 형태는 순회의 결함처럼 보였다. 플랫폼 의존이 판정 밖에 있으면 실패가
//! 엉뚱한 곳을 가리킨다.
//!
//! 그래서 등급을 [`FileClass::weak_only`] 로 판정에 넣었다. 유니크화 **여부**는 그대로
//! 합집합으로 묻고(약한 성분도 유니크화이긴 하다), 등급은 그 **다음** 물음이다.
//!
//! ## 그 판정의 첫 판은 사건 자신을 못 잡았다 (2026-09-08)
//!
//! 넣고 세었더니 걸리는 자리가 **0 곳**이었고, 그것을 "이미 지켜지던 규율을 값으로
//! 고정한다" 로 읽었다. **그 0 이 틀렸다.**
//!
//! 사건을 낸 코드(`floored_walk.rs` 의 `Tree::new`)는 키가 `process::id()` + 시간
//! nonce 였고, 첫 판의 규칙은 "강한 성분이 하나라도 있으면 강함" 이라 그 자리를
//! **강함으로 분류했다.** 즉 이 판정기는 **자기가 예고한 사고를, 그 사고가 난 뒤에도
//! 못 잡고 있었다.** 0 은 규율이 지켜졌다는 뜻이 아니라 **안 보고 있었다**는 뜻이다.
//!
//! 구멍의 정체는 성분이 아니라 [`Axes`] 다 — `process::id()` 는 프로세스 **간**만
//! 가르고, 같은 프로세스 안의 두 호출을 가르던 것은 시간 nonce 하나였다. **다른 축의
//! 강함은 이 축의 약함을 못 덮는다.** 축으로 다시 물었더니 같은 형태가 여섯 곳 더
//! 있었고(실측 2026-09-08), 전부 사건과 같은 수선(단조 카운터)으로 고쳤다.
//! 지금 걸리는 자리는 0 이고, 이번 0 은 **축을 보고 난 0** 이다.
//!
//! 빠져나가는 길은 명부가 아니라 **그 자리의 사유**다 — 위 극성과 같다. 시간 성분이
//! 의도된 선택이면 [`REASON_TOKENS`] 를 그 자리에 붙이면 등급 판정이 그것을 의도로
//! 읽는다(실측: 강한 성분을 끊고 `이유:` 를 붙이면 rc 0).
//!
//! 성분이 **변수 뒤에 숨은** 경우도 본다: `let unique = format!("{}-{}",
//! std::process::id(), ..nanos..)` 뒤에 `temp_dir().join(format!("x-{unique}"))`. 루트
//! 통합 테스트의 지배적 관용구라, 위 성분으로 바인딩된 지역 변수를 [`bindings::Bindings`]
//! 로 찾아 같은 함수·유효 블록의 선언 이후 참조에만 전파한다. 동명 shadowing은 이전
//! 성분을 가리며 다른 함수의 성분을 빌리지 않는다(인라인 `{unique}` 는
//! 문자열 안이라 raw 소스에서 단어 경계로 본다). 이 갈래가 없으면 루트 tests/ 를 편입한
//! 순간 25 곳이 거짓 위반이 됐다(실측) — 범위를 넓히자 판정기 사각이 드러난 형태다.
//!
//! ## 잡지 못하는 것
//!
//! - `temp_dir()` 를 받아 **멀리서** 고정 이름을 붙이는 형태(창 밖에서 `.join`) — 창 안에
//!   `.join(` 이 없으면 읽기 전용 사용과 구분되지 않아 보지 않는다.
//! - 소스에 리터럴로 안 보이는 이름(런타임 조합 문자열, 외부 도구가 짓는 이름)은 텍스트
//!   스캔의 사거리 밖이다.
//!
//! ## ★ 레포 스캔은 이 판정이 죽는 것을 **못 본다** — 아래 유닛 대조가 본다
//!
//! 실측 2026-09-06, 변이 여섯을 걸어 두 타깃을 따로 돌린 값이다. 세는 단위는
//! **빨개진 테스트 수**이고, 스캔은 `--test no_unshared_fixed_temp_path`(테스트 1 개),
//! 유닛은 `--lib temp_path`(테스트 12 개)다.
//!
//! | 변이 | 스캔 | 유닛 |
//! |---|---|---|
//! | `path_building_lines` 가 항상 빈 vec(당시 이름 `builds_path`) | 빨강 — 다만 `MIN_SITES` 하한이 잡는다 | 9 / 12 |
//! | `uniquified` 를 항상 true | 빨강 — 다만 `MIN_REASONED` 하한이 잡는다 | 5 / 12 |
//! | `reasoned` 를 항상 true | **초록. 조용하다.** | 4 / 12 |
//!
//! 셋째가 이 모듈의 존재 이유를 통째로 무력화하는 변이인데 **레포 스캔은 초록**이다.
//! 하한들은 순회가 죽는 방향만 보고 판정이 느슨해지는 방향은 안 본다. 둘째가 잡힌
//! 것도 설계가 아니라 파이프라인 순서의 부수효과다 — `uniquified` 가 먼저 삼켜
//! `reasoned` 가 0 이 되고 그 하한이 걸린다.
//!
//! **그래서 이 모듈의 보호는 아래 `#[cfg(test)] mod tests` 에 있다.** 스캔
//! 하나만 보고 "덮여 있다" 고 읽지 마라. 스캔에 상한을 다는 처방은 쓰지 않는다 —
//! 정당한 증가마다 값을 고쳐야 해서, 값을 고치는 것이 가장 싼 수선인 자리를 하나 더
//! 만들 뿐이다.
//!
//! 마스킹은 양방향으로 쟀다(같은 한 줄을 세 형태로 심고 스캔을 돌렸다):
//! 진짜 코드는 잡히고(rc=101, 그 파일을 지목), 문자열 리터럴 안과 주석 안은 안 잡힌다.

mod bindings;

#[cfg(test)]
mod scope_tests;

use std::path::Path;

use crate::source_text::{mask_literals, mask_non_code, rust_sources};

/// 유일성 키의 **축** — 그 성분이 무엇을 무엇으로부터 가르는가.
///
/// ★ 등급을 **성분 목록**으로만 매기면, 한 축을 홀로 지고 있는 약한 성분이 다른 축의
/// 강한 성분에 **가려진다.** 실측 사고(2026-09-08 · macOS 러너)가 정확히 그 형태였다:
/// `crates/tasty-doc-guards/src/floored_walk.rs` 의 `Tree::new` 가 키를
/// `process::id()` + 시간 nonce 로 지었고, 강한 성분이 하나 있으니 이 판정기는 그 자리를
/// **강함으로 분류했다**(`weak_only` 에 안 담았다). 그런데 `process::id()` 는 프로세스
/// **간**만 가르고, 같은 프로세스 안의 두 픽스처를 가르던 것은 시간 nonce 하나였다.
/// 그 축이 플랫폼 시계 해상도에 걸려 macOS 에서 겹쳤고, 먼저 끝난 쪽의 `Drop` 이 다른
/// 쪽이 순회 중인 트리를 지웠다. Linux 에서는 안 죽었다.
///
/// 그래서 성분이 아니라 축을 본다. **강한 성분이 있어도 다른 축의 약함을 안 가린다.**
/// ## 판정에 **없는** 축이 하나 있다 — pid 재사용 (2026-09-08 재고 못 넣기로 했다)
///
/// 아래 네 필드가 표현하는 축은 셋이다(프로세스 간 · 프로세스 안 · 둘 다). 그런데
/// [`CLOCK_TOKENS`] 주석이 드는 사례
/// (`crates/tasty-host-plugin/src/handle_channel.rs`)는 **넷째 축**을 쓴다: 재사용된
/// pid 로 도는 *다음* 프로세스를 옛 프로세스의 잔재와 가르는 축이다. 그 축은 여기
/// 없고, **앞으로도 못 넣는다.** 그 간격을 안 적으면 위 문단을 읽은 사람이 판정이
/// 넷을 본다고 생각한다.
///
/// ### 왜 못 넣나 — 소스에 그 축의 조건이 없다
///
/// 이 축이 발화해야 하는 조건은 "이 경로가 다음 프로세스와 충돌하는가" 이고, 그것은
/// 셋에 달렸다: **이 프로세스가 죽는 방식**(정상 종료냐 패닉·SIGKILL 이냐),
/// **OS 의 pid 재사용 정책**, **파일 소유자와 디렉터리 권한**(sticky `/tmp`).
/// 소스에는 셋 중 어느 것도 없다.
///
/// 대리 신호를 하나 재 봤다 — "자리 창 안에 정리 토큰(`remove_file`·`remove_dir_all`·
/// `TempDir`)이 있는가". 실측: 자리 **73** 중 **47**(`remove_dir_all` 39 ·
/// `remove_file` 8 · `TempDir` 0). 쓸 수 없다, 양방향으로 틀리기 때문이다:
/// - **있어도 안 남는다는 뜻이 아니다.** 정리는 정상 종료 경로에서만 돌고, 돌아도
///   실패할 수 있다. `handle_channel` 이 정확한 반례다 — `remove_file` 을 **갖고
///   있는데도** sticky `/tmp` 에서 남의 잔재를 못 지워 시계 항이 필요하다.
///   즉 이 신호는 그 자리에서 **정확히 반대**를 가리킨다.
/// - **없어도 남는다는 뜻이 아니다.** `Drop` 이나 별도 sweep 함수가 창 밖에서 치운다.
///
/// 그래서 이 축은 판정이 아니라 **그 자리의 주석**이 진다. `handle_channel` 의
/// 주석이 그 형태다 — 왜 정리 코드가 있는데도 시계 항이 필요한지를 그 자리에 적었다.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct Axes {
    /// 호출마다 유일 — 프로세스 간·안 두 축을 다 진다.
    per_call: bool,
    /// 프로세스 **간**만 가른다. 같은 프로세스의 두 호출은 못 가른다.
    process: bool,
    /// 프로세스 **안**을 가르고, 그 값에 해상도가 없다(단조 카운터).
    counter: bool,
    /// 프로세스 **안**을 가르지만 **그 창의 크기를 플랫폼이 정한다**(시계 해상도).
    clock: bool,
}

impl Axes {
    fn union(self, o: Self) -> Self {
        Self {
            per_call: self.per_call || o.per_call,
            process: self.process || o.process,
            counter: self.counter || o.counter,
            clock: self.clock || o.clock,
        }
    }

    /// 유니크화로 인정되는가 — 축이 하나라도 있으면 그렇다(등급은 다음 물음이다).
    fn any(self) -> bool {
        self.per_call || self.process || self.counter || self.clock
    }

    /// 시계가 **프로세스-내 축을 홀로** 지고 있는가. `process` 는 다른 축이라 못 덮는다.
    fn clock_stands_alone(self) -> bool {
        self.clock && !self.per_call && !self.counter
    }

    /// **재호출을 가르는 축이 하나도 없는가** — 정체성 축만 지고 있는 자리.
    ///
    /// pid 는 프로세스 **간**만 가른다. 같은 프로세스가 그 자리를 두 번 부르면 같은
    /// 경로가 나오고, 그 자리들의 지배적 관용구가 `let _ = remove_dir_all(&dir)` 로
    /// 시작하는 것이 그 사실의 자백이다 — 유일한 이름이라면 지울 잔여물이 있을 수 없다.
    ///
    /// ★ 이 술어의 이름이 한때 `process_stands_alone` 이었고, **그 이름이 틀렸다**
    ///   (2026-09-08, 통합에서 잡혔다). 이 칸에 든 자리 중 둘이 `process::id()` **와**
    ///   `thread::current().id()` 를 함께 쓰고 있었다 — "process 만" 이 사실이 아니다.
    ///   그런데 **판정 자체는 그대로가 맞다**: 스레드 축은 다른 물음에 답한다
    ///   ([`THREAD_TOKENS`]). 이 술어가 묻는 것은 "재호출을 가르는가" 하나이므로
    ///   이름이 그것을 말한다. 바뀐 것은 이름이지 좌변이 아니다.
    ///
    /// ★ [`clock_stands_alone`](Self::clock_stands_alone) 과 배타다. 시계는 약할지언정
    /// 프로세스-내 축을 **지고는 있다.**
    fn blind_to_recall(self) -> bool {
        self.process && !self.per_call && !self.counter && !self.clock
    }
}

/// 호출마다 유일한 이름을 **라이브러리가** 보증하는 성분.
const PER_CALL_TOKENS: &[&str] = &["TempDir", "tempfile", "tempdir(", "NamedTempFile"];

/// 프로세스 **간**만 가르는 성분. 이것만으로는 같은 프로세스의 두 호출이 겹친다.
///
/// `path_for` 가 여기 있는 이유(2026-09-08 측정): 레포 헬퍼
/// (`crates/tasty-plugin-agent-common/src/prompt_file.rs`)이고 짓는 이름이
/// `{prefix}{pid}-{surface_id}{SUFFIX}` 다. **같은 pid·같은 surface id 로 두 번 부르면
/// 같은 경로가 나오고, 그것이 설계 의도다** — 그 함수의 doc 이 "같은 프로세스 안에서는
/// 결정적이라 테스트가 프로덕션 헬퍼로 경로를 되짚을 수 있다" 고 명시한다. 즉 이것이
/// 지는 축은 pid 하나이고, surface id 는 **호출자가 주는 값이라 이 판정기가 못 읽는다.**
/// 한때 `PER_CALL_TOKENS` 에 있었는데 그것은 **재 보지 않은 배정**이었다.
const PROCESS_TOKENS: &[&str] = &["process::id", "pid", "path_for"];

/// 프로세스 **안**을 해상도 없이 가르는 성분 — 단조 카운터.
///
/// 위 사고의 수선이 이 형태였다. 시계와 같은 축을 지되 플랫폼을 안 읽는다.
const COUNTER_TOKENS: &[&str] = &["fetch_add", "AtomicUsize"];

/// 프로세스 **안**을 가르지만 해상도가 플랫폼의 성질인 성분 — 그 축에서 **약하다.**
///
/// ★ **약함은 성분의 성질이 아니라 축과의 짝이다.** 같은 `nanos` 가 다른 축에서는 안
/// 약하다 — `crates/tasty-host-plugin/src/handle_channel.rs` 의 socket 경로가 그
/// 사례다(2026-09-08 실측). 거기서 시계가 지는 것은 "재사용된 pid 로 도는 **다음
/// 프로세스**를 옛 프로세스의 잔재와 가르는" 축이고, 그 축은 **정의상 시간 간격이
/// 있어** 해상도 문제가 없다(sticky `/tmp` 에서는 남의 잔재를 unlink 도 못 한다).
///
/// 그래서 이 판정은 시계를 보는 것이 아니라 [`Axes::clock_stands_alone`] 을 본다 —
/// 시계가 **프로세스-내 축을 홀로 질 때만** 잡는다. 그 자리의 시계는 seq 가 그 축을
/// 지고 있어 안 걸리고, 그것이 옳다. 이 문단이 없으면 다음 사람이 "시계는 약하다" 만
/// 읽고 그 항을 지운다 — 그리고 그 크레이트의 시험 202 개는 **전부 초록으로 남는다.**
const CLOCK_TOKENS: &[&str] = &["nanos", "SystemTime"];

/// **유니크화 축이 아니다 — 보고에만 쓴다.** 같은 프로세스의 *다른 스레드*를 가르는 성분.
///
/// 물음이 셋인데 한 이름 아래 섞여 있었다:
/// - ㄱ 프로세스 **간** — [`PROCESS_TOKENS`] 가 가른다.
/// - ㄴ 같은 프로세스의 **다른 스레드**(= `cargo test` 가 병렬로 도는 다른 시험) — 이
///   목록이 가른다.
/// - ㄷ **같은 자리의 재호출** — [`COUNTER_TOKENS`](와 약하게 [`CLOCK_TOKENS`])만 가른다.
///
/// [`Axes`] 가 묻는 것은 ㄷ 다. 스레드 id 는 ㄷ 에 **답하지 않는다** — 같은 스레드가 그
/// 자리를 두 번 부르면 값이 같다. 그래서 축으로 안 센다. 이것만 가진 자리는 유니크화로도
/// 안 쳐준다: 프로세스가 둘이면 양쪽 다 `ThreadId(1)` 이라 그대로 겹친다.
///
/// **그런데 안 세는 것과 안 보는 것은 다르다.** 2026-09-08 통합에서 ㄱ+ㄴ 자리 둘이
/// [`Axes::blind_to_recall`] 칸에 들어왔는데 그 칸 이름이 `process_only` 여서 이름이
/// 사실과 어긋났다. 판정을 안 바꾸는 대신 **부분집합으로 보고**한다 —
/// [`FileClass::recall_blind_with_thread`].
///
/// ### 안 짓는 칸: **ㄴ③ 방향이 반대다** (ADR-0243)
///
/// 이 축을 [`Axes`] 에 더하면 실패가 아니라 **합격**이 는다. 열여섯 자리가
/// "프로세스 + 스레드로 유일화됨" 등급을 받고, 판정기가 같은 스레드 재호출 구멍을
/// 덮어 준다. (ㄷ)·(ㄱ) 을 볼 필요가 없다 — (ㄴ)에서 멈춘다.
///
/// **재진입 조건 — 있다.** 이 판정기가 물음 ㄴ("같은 프로세스의 **다른 시험**이
/// 갈리는가")을 **판정하는 칸을 새로 갖게 되면** 그 칸에서는 스레드 축이 옳은
/// 성분이다. 오늘 그런 칸이 없다. 바로 위 [`FileClass::recall_blind_with_thread`] 는
/// 보고일 뿐 판정이 아니라 이 조건을 아직 안 충족한다 — **그 칸이 판정에 들어가는
/// 날**이 조건이다.
const THREAD_TOKENS: &[&str] = &["thread::current", "ThreadId"];

/// 경로 짓는 창 안에 있으면 유니크화로 인정하는 성분 전부(네 축의 합집합).
///
/// 유니크화 **여부**는 이 합집합으로 묻는다 — 약한 성분도 유니크화이긴 하다. 등급은
/// 그 다음 물음이고 [`FileClass::weak_only`] 가 답한다. 합집합이 실제로 합집합인지는
/// `the_union_is_actually_the_union_of_the_four_axes` 가 지킨다 — 축 목록에 성분을
/// 더하고 여기 안 더하면 그 성분은 **변수 바인딩 경로에서만 조용히 빠진다**.
const UNIQ_TOKENS: &[&str] = &[
    "TempDir",
    "tempfile",
    "tempdir(",
    "NamedTempFile",
    "path_for",
    "process::id",
    "pid",
    "fetch_add",
    "AtomicUsize",
    "nanos",
    "SystemTime",
];

/// 한 줄이 어느 축을 지는가.
fn axes_of(line: &str) -> Axes {
    Axes {
        per_call: PER_CALL_TOKENS.iter().any(|t| line.contains(t)),
        process: PROCESS_TOKENS.iter().any(|t| line.contains(t)),
        counter: COUNTER_TOKENS.iter().any(|t| line.contains(t)),
        clock: CLOCK_TOKENS.iter().any(|t| line.contains(t)),
    }
}

/// 의도된 공유임을 그 자리에 밝히는 사유 마커.
///
/// ★ **여기는 한때 "`check-allow-reason` 과 같은 관례" 라고 적혀 있었고, 그 문장은
/// 틀렸다.** 실측: 그 셸 게이트의 마커는 `reason:|이유:|complexity-exempt:|SAFETY` 라
/// `사유:` 가 없고, 이쪽에는 뒤의 둘이 없다. 사유의 **위치** 규칙도 갈려 있었다(아래
/// [`reason_is_attached`]). 즉 두 축 모두에서 달랐는데 문서만 같다고 말했다.
///
/// ★ **무엇이 그 동일성을 지키는가 — 아무것도 지키지 않는다.** 한쪽은 awk, 한쪽은
/// Rust 이고 둘을 맞대는 가드가 없다. 그래서 이 문서는 동일성을 **다시 주장하지
/// 않는다** — 검증되지 않는 동일성 주장이 바로 그 표류를 만든 원인이다. 각 가드는
/// 자기 규칙을 자기 자리에 적고, 저자는 자기가 걸린 가드의 실패 메시지를 읽는다.
///
/// ## 이 마커 하나가 **서로 다른 세 물음**을 통과시킨다 (실측 2026-09-08)
///
/// 마커는 문자열이고 **그 뒤에 무엇이 오는지는 아무도 안 본다.** 그런데 마커가 붙는
/// 자리는 셋이고, 셋이 요구하는 답이 다르다:
///
/// | 칸 | 마커 없으면 | 그 자리가 답해야 하는 물음 |
/// |---|---|---|
/// | [`FileClass::reasoned`] | `silent`(위반) | 이 **고정 이름**을 다른 프로세스·다른 완주와 공유해도 왜 되는가 |
/// | [`Axes::blind_to_recall`] | 래칫 계수 +1 | 같은 프로세스가 이 자리를 **두 번 부르는 일**이 왜 없는가 |
/// | [`Axes::clock_stands_alone`] | [`FileClass::weak_only`](위반) | **시계 해상도 창**에 두 호출이 겹쳐도 왜 되는가 |
///
/// 실측(`integration-94`, 자리 72): 위에서부터 **7 · 2 · 0**, 합 9. 아홉을 손으로 읽어
/// 그 칸의 물음에 답하는지 판정했고 **답이 아닌 것은 0** 이었다. 그 0 의 종류를 밝힌다 —
/// "이 술어로는 원리적으로 안 걸린다" 가 **아니다**. 술어는 자연어를 읽는 사람이고,
/// 걸릴 수 있었다. 모수 아홉이 전부 이 가드가 생긴 뒤(2026-09-05 ~ 09-08) 그 실패
/// 메시지를 읽고 쓰인 것이라, 이 0 은 **아직 규율 밖에서 쓰인 사유가 없다**는 뜻이다.
///
/// ### 기계로 닿는 구멍은 하나이고, 오늘 그 모수가 0 이다
///
/// 세 칸은 서로 배타라 한 자리가 두 칸에 들지 않는다. 그래서 "A 의 답으로 B 를
/// 통과" 는 대개 불가능하다. **닿는 곳이 하나 있다**: `process` 와 `clock` 을 함께
/// 쓰는 자리는 셋째 칸에 드는데, 거기에 둘째 칸의 답("프로세스당 한 번만 불린다")을
/// 적으면 그것이 셋째 칸의 물음(시계가 겹쳐도 되는가)에 답하지 않은 채 통과한다.
/// 그 형태의 자리는 실측 **0** 이다(`pid` + 시계 0 · 시계만 0 — `handle_channel.rs` 의
/// 소켓 키는 `{pid}`·`{seq}` 를 인라인 중괄호로 지고 있어 셋째 칸이 아니다).
///
/// ### 그래서 판정기를 안 짓는다 — 못 지어서가 아니라 값이 음수라서다
///
/// 자연어가 그 물음의 답인지는 기계가 못 가른다. 갈래마다 **마커를 나누는** 것은
/// 기계로 되지만(예: `이유(재호출):`), 그러면 자리에 적는 사람이 **어느 칸에 드는지**를
/// 먼저 알아야 하고 그 칸 이름은 이 판정기의 내부 이름이다. 바로 이 회차에
/// `process_only` → `recall_blind` 로 고친 그 이름이다 — 낡은 이름 하나를 고친 회차에
/// 같은 이름을 마흔 자리에 새기는 것은 반대 방향이다. 게다가 갈래를 나눠도 "그 갈래의
/// 답인 척" 은 그대로 된다. 세는 것도 안 한다: 사유로 빠진 자리가 **늘어나는 것이 좋은
/// 일인지 나쁜 일인지 정해지지 않아**(문서화일 수도, 마커 채우기일 수도) 래칫의 극성이
/// 안 선다. 그래서 이 축은 **사람이 보는 축**이고, 그 사람이 볼 수 있게 위 표와 수를
/// 여기 적어 둔다.
///
/// ★ 같은 형태가 이 레포에 하나 더 있다 — `scripts/check-allow-reason.sh` 도
/// 마커(`reason:`·`이유:`·`complexity-exempt:`·`SAFETY`)만 보고 그 뒤를 안 본다.
/// **관측만 적는다**(저쪽은 소유가 다르다).
///
/// ### 안 짓는 칸이 둘이다 (ADR-0243), 그리고 **재진입 조건은 하나로 같다**
///
/// - **ㄴ③ 방향이 반대다** — 갈래마다 마커를 나누면(`이유(재호출):`) 자리에 적는
///   사람이 **어느 칸에 드는지**를 먼저 알아야 하고, 그 칸 이름은 이 판정기의 내부
///   이름이다. 바로 이 회차에 `process_only` → `recall_blind` 로 고친 그 이름이라,
///   낡은 이름 하나를 고친 회차에 같은 이름을 마흔 자리에 새기게 된다.
/// - **ㅂ 술어가 안 섰다** — 사유로 빠진 자리가 **느는 것이 좋은 일인지 나쁜 일인지**
///   안 정해졌다(문서화일 수도, 마커 채우기일 수도). 무엇을 위반이라 부를지가 없으니
///   실패문을 쓸 수가 없고, 래칫의 부호도 안 선다.
///
/// **재진입 조건 — 있고, 둘이 같은 관측 하나를 기다린다.** 위 표의 물음에 **답하지
/// 않는 사유가 하나라도 관측되면** 그 순간 (ㅂ)의 부호가 정해지고("늘면 나쁘다"),
/// 같은 관측이 (ㄴ③)의 전제도 무너뜨린다 — 그때는 갈래를 나눠서 얻는 것이 실물이
/// 된다. 오늘 그 수는 **0** 이고(아홉을 손으로 읽었다), 기계로 닿는 갈래의 모수도
/// **0** 이다(`process` + `clock` 자리 0). **둘 중 하나라도 0 이 아니게 되는 날이
/// 조건이다.**
const REASON_TOKENS: &[&str] = &["이유:", "reason:", "사유:"];

/// 유니크화 성분이 자리 둘레 어디까지 앉을 수 있는가(줄 수).
///
/// **이 창은 "경로를 짓는가" 를 묻지 않는다.** 그 물음의 답은 줄 거리가 아니라
/// 수신자 신원이고, [`path_building_lines`] 가 그것을 판정한다. 여기 남은 물음은
/// 다른 것이다 — 자리를 유일하게 만드는 성분(`process::id()` · nanos · 카운터)이
/// **그 자리에** 있는가. 성분은 경로를 짓는 식과 같은 줄에 있을 수도, 바로 위·아래
/// 줄에 있을 수도 있어서 자리 하나로는 못 묻는다.
const UNIQ_WINDOW: usize = 6;
/// 사유가 그 자리에 **붙어 있는지**를 정하는 규칙: 그 자리 줄 자신과, 위로 이어지는
/// 주석 줄 전부. 빈 줄이나 코드 줄에서 끊긴다.
///
/// 한때 이 자리는 고정 4 줄 창이었다. 그것은 "붙었다" 의 좁은 판이 아니라 **다른
/// 술어**였다 — 붙어 있어도 5 줄 위면 거부하고, 빈 줄과 코드 줄로 끊겨 있어도 3 줄
/// 위면 인정했다. 부분집합 관계가 아니라 서로 어긋난 집합이라, 저자가 어느 관례를
/// 배웠든 틀릴 수 있었다.
fn reason_is_attached(
    raw: &[&str],
    comments: &[&str],
    idx: usize,
    has_token: impl Fn(&str) -> bool,
) -> bool {
    if has_token(comments[idx]) {
        return true;
    }
    let mut j = idx;
    while j > 0 {
        j -= 1;
        if !raw[j].trim_start().starts_with("//") {
            return false;
        }
        if has_token(comments[j]) {
            return true;
        }
    }
    false
}

/// 한 파일을 분류한 결과. 줄 번호는 0 기반(`temp_dir()` 이 있는 줄).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileClass {
    /// 경로를 짓는(`.join` 이 창 안에 있는) `temp_dir()` 자리 전부.
    pub sites: Vec<usize>,
    /// 그중 유니크화된 줄.
    pub uniquified: Vec<usize>,
    /// 그중 사유로 공유가 명시된 줄.
    pub reasoned: Vec<usize>,
    /// 그중 유니크화도 사유도 없는 줄 — 고정 이름 공유(위반).
    pub silent: Vec<usize>,
    /// [`uniquified`](Self::uniquified) 의 부분집합 — **약한 성분만으로** 유일해진 줄.
    ///
    /// 사유가 그 자리에 붙어 있으면 여기 안 담는다(의도된 선택으로 본다). 즉 이 칸은
    /// "시간 nonce 에만 기댔고 그 선택을 아무도 안 밝힌 자리" 다.
    pub weak_only: Vec<usize>,
    /// [`uniquified`](Self::uniquified) 의 부분집합 — **재호출을 가르는 축이 빈** 줄.
    ///
    /// 사유가 그 자리에 붙어 있으면 여기 안 담는다(의도된 선택으로 본다).
    pub recall_blind: Vec<usize>,
    /// [`recall_blind`](Self::recall_blind) 의 **부분집합** — 그중 자리의 줄들
    /// ([`axes_span`]) 에 [`THREAD_TOKENS`] 도 있는 줄(ㄱ+ㄴ).
    ///
    /// 판정을 안 바꾼다 — 스레드 id 는 재호출을 못 가르므로 이 자리도 그대로 위 칸에
    /// 든다. 이 칸이 있는 이유는 하나다: 위 칸이 아직 **안 가른 수**이고, 이 부분집합이
    /// 그 분할의 첫 줄이다. 여기 든 자리는 "다른 시험과는 갈리고, 재호출만 못 가른다".
    pub recall_blind_with_thread: Vec<usize>,
    /// 경로를 짓는 `.join(` 의 **수신자로 안 이어져** 자리로도 안 세어진 `temp_dir()` 줄.
    ///
    /// 대부분은 정당하다 — 디렉터리를 그대로 넘기는 읽기 전용 용법이다. 남는 한 부류는
    /// **값이 이 파일 밖으로 나가는 것**이다(`path_for(&temp_dir(), ..)` 처럼 인자로
    /// 넘기면 경로를 어디서 짓는지가 여기서 안 보인다). 그쪽은 검사 없이 통과한다.
    /// 두 부류가 한 칸에 섞여 있어 소스만으로 못 가르므로, 수를 세어 **움직이는 것**을
    /// 본다.
    ///
    /// 한때 여기 세 번째 부류가 있었다 — 수신자가 여섯 줄 창 밖에 있어 빠진 자리.
    /// 그것은 판정을 [`path_building_lines`] 로 옮기면서 **없어졌다**(거리가 아니라
    /// 수신자로 묻는다).
    pub unpaired: Vec<usize>,
}

/// masked 코드 줄들과 masked 주석 줄들로 temp 경로 자리를 분류한다.
///
/// `code` 는 [`mask_non_code`](crate::source_text::mask_non_code)(주석·문자열 덮음),
/// `comments` 는 [`mask_literals`](crate::source_text::mask_literals)(문자열만 덮고 주석은
/// 남김)의 결과다 — 앞은 코드 토큰, 뒤는 사유 마커를 읽는다. 둘 다 줄 수가 같아야 한다.
pub fn classify(code: &[&str], comments: &[&str], raw: &[&str]) -> FileClass {
    assert_eq!(code.len(), comments.len(), "두 마스크의 줄 수가 다르다");
    assert_eq!(code.len(), raw.len(), "raw 줄 수가 다르다");
    // uniquifier 성분으로 바인딩된 지역 변수(`let unique = format!(.. process::id() .. nanos ..)`).
    // 창 밖의 변수 성분도 보되 함수·블록·선언 순서와 shadowing을 지킨다.
    let bindings = bindings::Bindings::new(code, raw, comments);
    let mut out = FileClass::default();
    for idx in 0..code.len() {
        if !code[idx].contains("temp_dir()") {
            continue;
        }
        // 경로를 짓는가 — **`.join(` 의 수신자**로 판정한다(줄 거리가 아니다).
        // 안 지으면(읽기 전용·다른 값의 join) 보지 않는다.
        let path_lines = path_building_lines(code, idx);
        if path_lines.is_empty() {
            out.unpaired.push(idx);
            continue;
        }
        out.sites.push(idx);

        // 유니크화 인정: ① 자리 둘레에 성분이 직접 있거나, ② 그 줄들이 uniquifier 로
        // 바인딩된 변수를 참조한다(인라인 `{unique}` 는 문자열 안이라 raw 에서 본다).
        let span = axes_span(code.len(), idx, &path_lines);
        let mut axes = Axes::default();
        for &j in &span {
            axes = axes.union(bindings.axes_on_line(idx, j));
        }
        if axes.any() {
            out.uniquified.push(idx);
            // 등급은 유니크화 **여부와 별개의 물음**이다. 약한 성분에만 기댄 자리는
            // 그 선택을 그 자리에 밝혀야 한다 — 밝히면 의도로 보고 넘긴다.
            let reasoned = reason_is_attached(raw, comments, idx, |line| {
                REASON_TOKENS.iter().any(|t| line.contains(t))
            });
            if axes.clock_stands_alone() && !reasoned {
                out.weak_only.push(idx);
            }
            // ★ 사유를 여기서 **다시** 묻는다. `reasoned` 칸은 축이 하나도 없을 때만
            //   도달하므로, 축이 있는 자리의 사유는 그 칸으로 안 빠진다.
            if axes.blind_to_recall() && !reasoned {
                out.recall_blind.push(idx);
                // 스레드 축은 판정에 안 들어간다 — 위 `if` 를 통과한 뒤에 묻는 이유가
                // 그것이다. 여기서 하는 일은 **가르는 것**뿐이다.
                if span
                    .iter()
                    .any(|&j| THREAD_TOKENS.iter().any(|t| code[j].contains(t)))
                {
                    out.recall_blind_with_thread.push(idx);
                }
            }
            continue;
        }
        // 사유는 그 자리에 붙어 있어야 한다 — 같은 줄이거나, 위로 이어지는 주석 블록 안.
        let reasoned = reason_is_attached(&raw, &comments, idx, |line| {
            REASON_TOKENS.iter().any(|t| line.contains(t))
        });
        if reasoned {
            out.reasoned.push(idx);
        } else {
            out.silent.push(idx);
        }
    }
    out
}

/// 이 `temp_dir()` 자리가 경로를 짓는지를 **`.join(` 의 수신자**로 판정한다.
///
/// 창(줄 거리)이 아닌 이유는 창이 답을 근사하기 때문이다 — `.join(` 이라는 토큰은
/// `Path::join` 과 `[T]::join` 을 못 가르고(`valid.join(", ")` 는 경로가 아니다),
/// 바로 아래 **다른 시험**의 `.join(` 을 이 자리 것으로 세기도 한다. 반대 방향의
/// 사각도 같은 뿌리다 — `let d = temp_dir();` 뒤 열 줄에 `d.join("고정이름")` 을
/// 쓰면 창을 넘어 조용히 빠진다. 두 오답이 다 "수신자가 누구인가" 를 거리로 물어서
/// 난다.
///
/// 그래서 판정은 둘 중 하나로만 선다:
/// ① 같은 문에서 `temp_dir()` **뒤에** `.join(` 이 이어진다(직접 수신자).
/// ② 그 문이 값을 이름에 묶고(`let d = ...temp_dir()...;`), 그 이름이 **감싸는 함수
///    안에서** `.join(` 의 수신자로 나온다.
///
/// 돌려주는 것은 그 판정에 쓰인 줄들이다 — 빈 vec 이면 경로를 안 짓는다. 함수 범위로
/// 자르는 이유는 이름이 파일 안에서 겹치기 때문이다(`dir` 은 시험마다 다시 묶인다).
fn path_building_lines(code: &[&str], idx: usize) -> Vec<usize> {
    let end = statement_end(code, idx);
    let lines: Vec<usize> = (idx..=end).collect();
    let (flat, _) = flatten_with_map(code, idx, end);
    if let Some(at) = flat.find("temp_dir()")
        && flat[at + "temp_dir()".len()..].contains(".join(")
    {
        return lines;
    }
    let Some(name) = let_binding_name(code, idx, end) else {
        return Vec::new();
    };
    if end + 1 >= code.len() {
        return Vec::new();
    }
    let hi = enclosing_fn(code, idx).map_or(code.len() - 1, |s| enclosing_fn_end(code, s));
    if end + 1 > hi {
        return Vec::new();
    }
    let (flat, map) = flatten_with_map(code, end + 1, hi);
    let pat = format!("{name}.join(");
    let bytes = flat.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(rel) = flat[from..].find(&pat) {
        let at = from + rel;
        if at == 0 || !is_word_byte(bytes[at - 1]) {
            found.push(map[at]);
        }
        from = at + 1;
    }
    if found.is_empty() {
        return Vec::new();
    }
    let mut out = lines;
    out.extend(found);
    out
}

/// 축을 묻는 줄들 — 자리 둘레의 [`UNIQ_WINDOW`] 와 경로를 짓는 줄들의 합집합.
///
/// 둘을 합치는 이유: 성분은 경로를 짓는 식에 있을 수도(`join(format!("x-{}", pid))`),
/// 그 위·아래 줄에 있을 수도 있다. 수신자가 창 밖에 있는 자리에서도 그 줄을 함께
/// 봐야 유니크화를 안 놓친다.
fn axes_span(len: usize, idx: usize, path_lines: &[usize]) -> Vec<usize> {
    let hi = (idx + UNIQ_WINDOW).min(len - 1);
    let mut out: Vec<usize> = (idx..=hi).collect();
    for &j in path_lines {
        if j < len && !out.contains(&j) {
            out.push(j);
        }
    }
    out
}

/// 이 줄에서 시작하는 문이 끝나는 줄 — 첫 `;` 가 있는 줄. 없으면 마지막 줄.
fn statement_end(code: &[&str], idx: usize) -> usize {
    (idx..code.len())
        .find(|&j| code[j].contains(';'))
        .unwrap_or(code.len() - 1)
}

/// `fn` 선언 줄부터 중괄호를 세어 그 본문이 끝나는 줄. 못 닫으면 마지막 줄.
fn enclosing_fn_end(code: &[&str], fn_start: usize) -> usize {
    let mut depth = 0i32;
    let mut opened = false;
    for (j, line) in code.iter().enumerate().skip(fn_start) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                opened = true;
            } else if ch == '}' {
                depth -= 1;
                if opened && depth <= 0 {
                    return j;
                }
            }
        }
    }
    code.len() - 1
}

/// 줄들을 공백 없이 이어 붙이고, 바이트마다 그 바이트가 온 줄 번호를 함께 돌려준다.
///
/// 이어 붙이는 이유는 `.join(` 이 줄바꿈을 건너뛰기 때문이다(`dir\n    .join("x")`).
/// 줄 번호 표는 찾은 자리를 다시 줄로 되돌리는 데 쓴다 — 창을 더 두지 않으려는 것이다.
fn flatten_with_map(code: &[&str], lo: usize, hi: usize) -> (String, Vec<usize>) {
    let mut flat = String::new();
    let mut map = Vec::new();
    for (j, line) in code.iter().enumerate().take(hi + 1).skip(lo) {
        for ch in line.chars() {
            if ch.is_whitespace() {
                continue;
            }
            let before = flat.len();
            flat.push(ch);
            for _ in before..flat.len() {
                map.push(j);
            }
        }
    }
    (flat, map)
}

/// `let [mut] <name> = ...` 의 이름. `let` 이 `temp_dir()` 보다 앞에 있을 때만 인정한다.
fn let_binding_name(code: &[&str], lo: usize, hi: usize) -> Option<String> {
    let text = code[lo..=hi].join(" ");
    let at_let = text.find("let ")?;
    let at_temp = text.find("temp_dir()")?;
    if at_let > at_temp {
        return None;
    }
    let rest = text[at_let + 4..].trim_start();
    let rest = rest.strip_prefix("mut ").unwrap_or(rest);
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// `line` 이 `word` 를 **단어 경계로** 포함하는가(부분 문자열 오인 방지 — `id` 가
/// `width` 안에서 매칭되지 않게).
fn references_word(line: &str, word: &str) -> bool {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(word) {
        let start = from + rel;
        let end = start + word.len();
        let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 워크스페이스 전역 census.
#[derive(Debug, Default)]
pub struct Census {
    pub files_scanned: usize,
    pub sites: usize,
    pub uniquified: usize,
    pub reasoned: usize,
    /// 창 안에 `.join(` 이 없어 자리로 안 세어진 `temp_dir()` 줄의 수.
    pub unpaired: usize,
    /// `"레포상대경로:1기반줄: 원문"` 형태의 위반 목록.
    pub silent: Vec<String>,
    /// 약한 성분에만 기댔고 그 선택을 안 밝힌 자리 — 같은 형태의 목록.
    pub weak_only: Vec<String>,
    /// 재호출을 가르는 축이 비었고 그 선택을 안 밝힌 자리 — 같은 형태의 목록.
    pub recall_blind: Vec<String>,
    /// 위 목록의 **부분집합** — 그중 스레드 id 도 함께 쓰는 자리(ㄱ+ㄴ).
    pub recall_blind_with_thread: Vec<String>,
}

/// `scan_roots` 아래를 훑어 census 를 만든다.
pub fn census(root: &Path, scan_roots: &[&str]) -> Census {
    let sources = rust_sources(root, scan_roots);
    let mut c = Census::default();
    for (rel, raw) in &sources {
        c.files_scanned += 1;
        let code_src = mask_non_code(raw);
        let comment_src = mask_literals(raw);
        let code: Vec<&str> = code_src.lines().collect();
        let comments: Vec<&str> = comment_src.lines().collect();
        let raw_lines: Vec<&str> = raw.lines().collect();
        let fc = classify(&code, &comments, &raw_lines);

        c.sites += fc.sites.len();
        c.uniquified += fc.uniquified.len();
        c.reasoned += fc.reasoned.len();
        c.unpaired += fc.unpaired.len();
        for &idx in &fc.silent {
            let text = raw_lines.get(idx).map(|s| s.trim()).unwrap_or("");
            c.silent
                .push(format!("{}:{}: {text}", rel.display(), idx + 1));
        }
        for &idx in &fc.weak_only {
            let text = raw_lines.get(idx).map(|s| s.trim()).unwrap_or("");
            c.weak_only
                .push(format!("{}:{}: {text}", rel.display(), idx + 1));
        }
        for &idx in &fc.recall_blind {
            let text = raw_lines.get(idx).map(|s| s.trim()).unwrap_or("");
            c.recall_blind
                .push(format!("{}:{}: {text}", rel.display(), idx + 1));
        }
        for &idx in &fc.recall_blind_with_thread {
            let text = raw_lines.get(idx).map(|s| s.trim()).unwrap_or("");
            c.recall_blind_with_thread
                .push(format!("{}:{}: {text}", rel.display(), idx + 1));
        }
    }
    c
}

// ── 판별자 사슬 ──────────────────────────────────────────────────────────────
//
// `recall_blind` 자리의 (가2) 부류는 유일화 성분을 **호출자에게서** 받는다
// (`fn tmp_pack(tag: &str)`). 그 자리만 보면 안전한지 알 수 없고, **호출자가 서로 다른
// 판별자를 주는가**가 답이다. 아래가 그것을 기계로 묻는다.

/// 판별자를 호출자에게서 받는 한 함수의 호출 실태.
#[derive(Debug, PartialEq, Eq)]
pub struct Chain {
    /// `"Tmp::new"` 처럼 호출에 쓰이는 이름(impl 이면 타입을 앞에 붙인다).
    pub callee: String,
    /// 판별자 자리에 넘어온 인자식 **텍스트** 목록(공백 정규화).
    pub args: Vec<String>,
    /// 그중 두 번 이상 나온 텍스트 — **이것이 위반이다.**
    pub duplicates: Vec<String>,
    /// 이 함수의 호출을 **이 파일에서 하나도 못 찾은** 경우 true.
    ///
    /// 통과가 아니다. 호출이 다른 파일에 있으면 이 판정은 그 자리를 **안 본 것**이다.
    pub no_call_seen: bool,
}

/// 한 파일에서 판별자 사슬을 모은다. `code` 는 마스킹된 코드 사본.
///
/// **모수**: 이 파일의 [`FileClass::recall_blind`] 자리 중, 감싸는 `fn` 의 매개변수
/// 이름이 경로 짓는 창 안에 나타나는 것(= (가2)). 나머지는 안 본다.
///
/// **한 단계가 아니라 사슬이다.** 인자식이 그 호출자의 매개변수를 그대로 담고 있으면
/// (`probe_root(tag, line)` · `Tmp::new(&format!("{tag}-root"))`) 그 호출자를 다시 물어
/// 위로 올라간다. 실측 2026-09-08: 여덟 자리 중 **셋이 2 단계**라 한 단계만 보면 그
/// 셋의 답이 `tag` 라는 이름뿐이다.
///
/// **사본**: 인자식은 마스킹 **안 한** 원문에서 읽는다 — 판별자는 대부분 문자열
/// 리터럴이라 마스킹된 사본에서는 지워진다. 자리를 찾는 일(어느 줄이 호출인가)만
/// 마스킹된 사본이 한다.
pub fn discriminator_chains(code: &[&str], raw: &[&str]) -> Vec<Chain> {
    let fc = classify(code, &mask_of_comments(raw), raw);
    let mut todo: Vec<(String, usize)> = Vec::new(); // (호출 이름, 판별자 인자 자리)
    for &idx in &fc.recall_blind {
        let Some(fj) = enclosing_fn(code, idx) else {
            continue;
        };
        let params = fn_params(code[fj]);
        let span = axes_span(raw.len(), idx, &path_building_lines(code, idx));
        let win = span.iter().map(|&j| raw[j]).collect::<Vec<_>>().join("\n");
        let Some(pos) = params.iter().position(|p| references_word(&win, p)) else {
            continue;
        };
        let name = call_name(code, fj);
        if !todo.iter().any(|(n, q)| n == &name && *q == pos) {
            todo.push((name, pos));
        }
    }
    let mut out = Vec::new();
    let mut seen = 0;
    while seen < todo.len() {
        let (name, pos) = todo[seen].clone();
        seen += 1;
        let mut args = Vec::new();
        for j in 0..code.len() {
            let Some(a) = nth_arg(code, raw, j, &name, pos) else {
                continue;
            };
            args.push(a);
        }
        let mut dups: Vec<String> = Vec::new();
        for a in &args {
            if args.iter().filter(|b| *b == a).count() > 1 && !dups.contains(a) {
                dups.push(a.clone());
            }
        }
        // 인자가 그 호출자의 매개변수를 담고 있으면 한 단계 위로.
        for j in 0..code.len() {
            if nth_arg(code, raw, j, &name, pos).is_none() {
                continue;
            }
            let Some(fj) = enclosing_fn(code, j) else {
                continue;
            };
            let a = nth_arg(code, raw, j, &name, pos).unwrap_or_default();
            let params = fn_params(code[fj]);
            if let Some(q) = params.iter().position(|p| references_word(&a, p)) {
                let up = call_name(code, fj);
                if !todo.iter().any(|(n, r)| n == &up && *r == q) {
                    todo.push((up, q));
                }
            }
        }
        out.push(Chain {
            no_call_seen: args.is_empty(),
            callee: name,
            args,
            duplicates: dups,
        });
    }
    out
}

/// `mask_literals` 를 이 모듈 안에서 부르기 위한 얇은 사본 — 줄 수를 맞춰 돌려준다.
fn mask_of_comments(raw: &[&str]) -> Vec<&'static str> {
    // classify 는 주석 사본을 사유 판정에만 쓴다. 여기서는 사유를 안 물으므로 빈 줄로
    // 채운다 — **사유가 붙은 자리도 좌변에 남는다**(그 자리도 판별자가 겹치면 겹친다).
    vec![""; raw.len()]
}

fn is_fn_decl(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("fn ")
        || t.starts_with("pub fn ")
        || t.starts_with("pub(crate) fn ")
        || t.starts_with("async fn ")
        || t.starts_with("pub async fn ")
}

fn enclosing_fn(code: &[&str], idx: usize) -> Option<usize> {
    let mut j = idx;
    while j > 0 {
        j -= 1;
        if is_fn_decl(code[j]) {
            return Some(j);
        }
    }
    None
}

/// `fn f(a: &str, b: u32)` → `["a", "b"]`.
fn fn_params(line: &str) -> Vec<String> {
    let Some(o) = line.find('(') else {
        return Vec::new();
    };
    let rest = &line[o + 1..];
    let c = rest.rfind(')').unwrap_or(rest.len());
    split_top(&rest[..c])
        .into_iter()
        .filter_map(|p| {
            p.split(':')
                .next()
                .map(|n| n.trim().trim_start_matches("mut ").to_string())
        })
        .filter(|n| !n.is_empty() && n != "&self" && n != "self")
        .collect()
}

/// 호출에 쓰이는 이름. `impl Tmp` 안의 `fn new` 이면 `"Tmp::new"`.
fn call_name(code: &[&str], fj: usize) -> String {
    let t = code[fj].trim_start();
    let after = t.split("fn ").nth(1).unwrap_or("");
    let bare: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let mut j = fj;
    while j > 0 {
        j -= 1;
        let l = code[j].trim_start();
        if l.starts_with("impl ") {
            let ty = l
                .trim_start_matches("impl ")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .to_string();
            if !ty.is_empty() {
                return format!("{ty}::{bare}");
            }
            break;
        }
        if is_fn_decl(l) {
            break;
        }
    }
    bare
}

/// `j` 줄이 `name(` 호출이면 `pos` 번째 인자식 텍스트를 원문에서 읽어 돌려준다.
fn nth_arg(code: &[&str], raw: &[&str], j: usize, name: &str, pos: usize) -> Option<String> {
    let pat = format!("{name}(");
    let at = code[j].find(&pat)?;
    // 이름 앞이 단어 문자면 다른 이름의 꼬리다(`with_new(` 안의 `new(`).
    if at > 0 && is_word_byte(code[j].as_bytes()[at - 1]) {
        return None;
    }
    // ★ 선언은 호출이 아니다. 이 배제를 한때 **줄 단위**로 했는데(`is_fn_decl(code[j])`)
    //   그러면 `fn a() { let d = tmp_pack("x"); }` 처럼 선언과 호출이 한 줄에 있는 자리를
    //   통째로 안 봤다. 양성 대조가 그것을 잡았다 — 이제 **그 자리 바로 앞**만 본다.
    if code[j][..at].trim_end().ends_with("fn") {
        return None;
    }
    let line = raw.get(j)?;
    let at_raw = line.find(&pat)?;
    let rest = &line[at_raw + pat.len()..];
    let mut depth = 0i32;
    let mut end = rest.len();
    for (k, ch) in rest.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    end = k;
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let inner = &rest[..end];
    let parts = split_top(inner);
    parts
        .get(pos)
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// 최상위 쉼표로 자른다(괄호·따옴표 안의 쉼표는 안 센다).
fn split_top(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut instr) = (Vec::new(), String::new(), 0i32, false);
    let mut prev = '\0';
    for ch in s.chars() {
        if instr {
            cur.push(ch);
            if ch == '"' && prev != '\\' {
                instr = false;
            }
            prev = ch;
            continue;
        }
        match ch {
            '"' => {
                instr = true;
                cur.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
        prev = ch;
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify_src(src: &str) -> FileClass {
        let code_src = mask_non_code(src);
        let comment_src = mask_literals(src);
        let code: Vec<&str> = code_src.lines().collect();
        let comments: Vec<&str> = comment_src.lines().collect();
        let raw: Vec<&str> = src.lines().collect();
        classify(&code, &comments, &raw)
    }

    /// 시간 nonce 에만 기댄 자리는 유니크화로는 인정하되 **등급으로 걸린다**.
    #[test]
    fn a_time_only_nonce_is_uniquified_but_graded_weak() {
        let fc = classify_src(
            "fn f() {\n    let n = nanos();\n    let p = std::env::temp_dir().join(format!(\"x-{n}\"));\n}",
        );
        assert_eq!(fc.sites.len(), 1);
        assert_eq!(fc.uniquified.len(), 1, "약한 성분도 유니크화이긴 하다");
        assert_eq!(fc.silent.len(), 0, "위반(고정 이름)이 아니다");
        assert_eq!(fc.weak_only.len(), 1, "등급이 판정에 안 들어갔다");
    }

    /// **실제 사고의 positive control.** `process::id()` 가 같은 창에 있어도 시계가
    /// 프로세스-내 축을 홀로 지면 약하다.
    ///
    /// 이 조각은 2026-09-08 macOS 러너에서 죽은 `floored_walk.rs` 의 `Tree::new` 를
    /// 그 사고 시점의 형태로 옮겨 온 것이다(`1aa1116c2` 직전). 이 판정기의 첫 판은
    /// 이것을 **통과**로 단정하고 있었고, 그 단정을 사건이 반증했다.
    #[test]
    fn the_macos_incident_shape_is_weak_even_though_a_pid_is_present() {
        let fc = classify_src(
            "fn f() {\n    let u = format!(\"{}-{}\", std::process::id(), nanos());\n    let p = std::env::temp_dir().join(format!(\"x-{u}\"));\n}",
        );
        assert_eq!(fc.uniquified.len(), 1, "유니크화이긴 하다");
        assert_eq!(
            fc.weak_only.len(),
            1,
            "pid 는 프로세스 **간** 축이라 시계가 지던 프로세스 **내** 축을 못 덮는다"
        );
    }

    /// `path_for` 는 프로세스 축만 진다 — 시계가 프로세스-내 축을 홀로 지면 약하다.
    ///
    /// 이 조각이 그 배정을 고정한다. `PER_CALL_TOKENS` 로 되돌리면 여기서 죽는다.
    #[test]
    fn path_for_does_not_cover_the_within_process_axis() {
        let fc = classify_src(
            "fn f() {\n    let p = std::env::temp_dir().join(path_for(\"tasty\", nanos()));\n}",
        );
        assert_eq!(fc.uniquified.len(), 1);
        assert_eq!(
            fc.weak_only.len(),
            1,
            "path_for 는 prefix+pid+surface_id 라 같은 프로세스·같은 id 면 같은 경로다"
        );
    }

    /// **양성 대조 — 등급이 바인딩 안의 줄 순서에 달려 있으면 안 된다.**
    ///
    /// 두 사본은 성분이 같다(`process::id()` 와 시계). 다른 것은 `format!` 안에서 어느
    /// 것이 먼저 오는가뿐이고, 그것은 그 코드의 안전성이 아니다. 한때 이 판정은 바인딩의
    /// **첫 성분 줄**만 보아서 pid 가 먼저 오면 시계를 못 봤고, 그래서 이 저장소의
    /// 지배적 관용구 열다섯 자리가 전부 조용히 통과했다(실측 2026-09-08).
    #[test]
    fn the_grade_does_not_depend_on_the_order_inside_the_binding() {
        let pid_first = classify_src(
            "fn f() {\n    let unique = format!(\n        \"{}-{}\",\n        \
             std::process::id(),\n        nanos(),\n    );\n    \
             let p = std::env::temp_dir().join(format!(\"x-{unique}\"));\n}",
        );
        let clock_first = classify_src(
            "fn f() {\n    let unique = format!(\n        \"{}-{}\",\n        \
             nanos(),\n        std::process::id(),\n    );\n    \
             let p = std::env::temp_dir().join(format!(\"x-{unique}\"));\n}",
        );
        assert_eq!(
            pid_first.weak_only, clock_first.weak_only,
            "같은 성분인데 `format!` 안 순서만으로 등급이 갈렸다 — 줄 순서는 안전성이 아니다"
        );
        assert!(
            !pid_first.weak_only.is_empty(),
            "pid + 시계는 프로세스-내 축을 시계가 홀로 진다 — 등급이 발화해야 한다"
        );
    }

    /// 단조 카운터는 시계와 같은 축을 지되 해상도가 없다 — 등급이 풀린다.
    ///
    /// 사건의 수선이 이 형태였다. 위 시험과 짝이다: 하나는 안 풀리고 하나는 풀린다.
    #[test]
    fn a_monotonic_counter_clears_the_grade_where_a_pid_does_not() {
        let fc = classify_src(
            "fn f() {\n    let p = std::env::temp_dir().join(format!(\"x-{}-{}\", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));\n}",
        );
        assert_eq!(fc.uniquified.len(), 1);
        assert!(fc.weak_only.is_empty(), "카운터가 프로세스-내 축을 진다");
    }

    /// 축 목록에 성분을 더하고 합집합에 안 더하면 **변수 바인딩 경로에서만** 조용히
    /// 빠진다 — 창 안 직접 토큰은 축 목록으로 보므로 티가 안 난다.
    #[test]
    fn the_union_is_actually_the_union_of_the_four_axes() {
        let mut parts: Vec<&str> = Vec::new();
        parts.extend_from_slice(PER_CALL_TOKENS);
        parts.extend_from_slice(PROCESS_TOKENS);
        parts.extend_from_slice(COUNTER_TOKENS);
        parts.extend_from_slice(CLOCK_TOKENS);
        let mut union: Vec<&str> = UNIQ_TOKENS.to_vec();
        parts.sort_unstable();
        union.sort_unstable();
        assert_eq!(
            parts, union,
            "UNIQ_TOKENS 가 네 축의 합집합이 아니다 — 어느 쪽에 더했는지 확인해라"
        );
    }

    /// **스레드 축은 유니크화 성분이 아니다** — 합집합에 새어 들어가면 여기서 죽는다.
    ///
    /// 위 시험은 "네 축 밖의 것이 [`UNIQ_TOKENS`] 에 있나" 를 못 묻는다(양쪽에 같이
    /// 더하면 통과한다). 이쪽이 그 갈래다.
    #[test]
    fn the_thread_axis_never_counts_as_uniquification() {
        for t in THREAD_TOKENS {
            assert!(
                !UNIQ_TOKENS.contains(t),
                "{t} 가 유니크화 성분으로 새어 들어갔다 — 스레드 id 는 재호출을 못 가른다"
            );
        }
    }

    /// **ㄱ+ㄴ 은 여전히 ㄷ 를 못 가른다 — 판정은 그대로, 목록만 갈린다.**
    ///
    /// 이 조각은 2026-09-08 통합에서 이 래칫을 짖게 만든 두 자리의 형태다
    /// (`crates/tasty-doc-guards/tests/cited_anchors_resolve.rs` ·
    /// `crates/tasty-doc-guards/tests/layering.rs` 의 픽스처). 두 자리는 `cargo test` 가
    /// 시험을 스레드로 병렬 실행한다는 사실에 기대 **다른 시험**과는 갈린다. 그것은
    /// 이 래칫이 묻는 물음이 아니다 — 같은 스레드가 그 자리를 두 번 부르면 값이 같다.
    ///
    /// 그래서 단정이 둘이다: 칸에 **그대로 있고**(판정 불변), 부분집합으로 **갈린다**.
    #[test]
    fn a_thread_id_splits_tests_but_not_recalls() {
        let fc = classify_src(
            "fn f() {\n    let root = std::env::temp_dir().join(format!(\n        \
             \"tasty-cited-anchors-fixture-{}-{:?}\",\n        std::process::id(),\n        \
             std::thread::current().id()\n    ));\n}",
        );
        assert_eq!(fc.uniquified.len(), 1, "pid 가 있으니 유니크화이긴 하다");
        assert_eq!(
            fc.recall_blind.len(),
            1,
            "스레드 id 가 재호출 축을 지지 않는다 — 판정은 안 바뀌어야 한다"
        );
        assert_eq!(
            fc.recall_blind_with_thread.len(),
            1,
            "ㄱ+ㄴ 인데 ㄱ만으로 보고되면 다음 회차의 분할이 이 자리를 못 찾는다"
        );
    }

    /// **스레드 id 만 있는 자리는 유니크화가 아니다** — 위 시험의 짝.
    ///
    /// 프로세스가 둘이면 양쪽 다 `ThreadId(1)` 이라 그대로 겹친다. 위 시험만 있으면
    /// "스레드 토큰을 본다" 가 "스레드 토큰을 인정한다" 로 읽힌다.
    #[test]
    fn a_thread_id_alone_is_not_uniquification() {
        let fc = classify_src(
            "fn f() {\n    let root = std::env::temp_dir().join(format!(\"x-{:?}\", std::thread::current().id()));\n}",
        );
        assert_eq!(fc.sites.len(), 1);
        assert!(
            fc.uniquified.is_empty(),
            "스레드 id 는 프로세스 간에 그대로 겹친다"
        );
        assert_eq!(fc.silent.len(), 1, "사유도 없으니 위반으로 나가야 한다");
    }

    /// 시간 성분이 의도된 선택이면 그 자리에 사유를 붙여 넘긴다 — 명부가 아니라 그 자리다.
    #[test]
    fn a_reason_at_the_site_clears_the_weak_grade() {
        let fc = classify_src(
            "fn f() {\n    // 이유: 같은 프로세스가 회차마다 다른 경로를 원한다.\n    let p = std::env::temp_dir().join(format!(\"x-{}\", nanos()));\n}",
        );
        assert_eq!(fc.uniquified.len(), 1);
        assert!(fc.weak_only.is_empty(), "그 자리 사유를 안 읽었다");
    }

    /// 고정 이름을 공유 temp 에 지으면 — 유니크화도 사유도 없으면 — 잡는다.
    #[test]
    fn a_fixed_name_under_temp_dir_is_caught() {
        let fc = classify_src(
            "fn f() {\n    let p = std::env::temp_dir().join(\"tasty-thing.toml\");\n}",
        );
        assert_eq!(fc.sites.len(), 1);
        assert_eq!(
            fc.silent.len(),
            1,
            "유니크화·사유 없는 고정 이름을 잡아야 한다"
        );
    }

    /// **인정하는 유니크화 성분 하나하나가 실제로 인식되는지** 묻는다.
    ///
    /// 실측 2026-09-06: 이 테스트가 없을 때 [`UNIQ_TOKENS`] 아홉 중 **여덟을 통째로
    /// 지워도** 레포 스캔(`--test no_unshared_fixed_temp_path`)도 이 모듈의 유닛
    /// 열둘도 전부 초록이었다. 오늘 레포에서 그 여덟이 **유일 근거인 자리가 0** 이라
    /// 수가 안 움직이기 때문이다 — 하중을 받는 것은 `process::id` 하나뿐이었다.
    /// 값을 움직이는 변이는 이미 여럿이 잡는다. **값이 안 움직이는 변이를 잡는 것이
    /// 이 테스트의 전부다.**
    ///
    /// 조각을 상수에서 만들지 않고 **손으로 적는다.** 목록을 순회해 조각을 지으면
    /// 오타가 난 항목(`NamedTempFilee`)도 자기 자신과는 맞아 통과한다 — 그러면 이
    /// 테스트가 목록의 사본이 될 뿐 목록을 검사하지 않는다.
    ///
    /// 조각마다 성분을 **하나만** 담는다. 둘을 담으면 하나를 지워도 다른 하나가
    /// 받쳐 주어 그 지움이 조용해진다.
    ///
    /// 여기 여덟뿐인 이유: `process::id` 는 아래 세 테스트가 이미 이름으로 부른다.
    /// 확인했다 — 아홉을 하나씩 빼면 전부 빨개지고, 여덟은 이 테스트가 성분 이름을
    /// 대며 잡고 `process::id` 는 그 셋이 잡는다.
    ///
    /// 목록의 원소를 리터럴 조각으로 박는 규율의 근거·대안:
    /// `docs/adr/0189-a-list-element-is-pinned-by-a-literal-snippet.md`.
    #[test]
    fn every_recognized_uniquifier_is_actually_recognized() {
        // 조각은 [`UNIQ_TOKENS`] 와 **같은 수**여야 한다 — 아래에서 단정한다. 성분을
        // 더하고 조각을 안 더하면 그 성분은 아무 시험도 안 받는다.
        let cases: [(&str, &str); 11] = [
            (
                "TempDir",
                "fn f() {\n    let base = std::env::temp_dir().join(\"tasty\");\n    let d = TempDir::new_in(&base).unwrap();\n}",
            ),
            (
                "tempfile",
                "fn f() {\n    let base = std::env::temp_dir().join(\"tasty\");\n    let b = tempfile::Builder::new().tempdir_in(&base).unwrap();\n}",
            ),
            (
                "tempdir(",
                "fn f() {\n    let base = std::env::temp_dir().join(\"tasty\");\n    let d = tempdir().unwrap();\n}",
            ),
            (
                "NamedTempFile",
                "fn f() {\n    let base = std::env::temp_dir().join(\"tasty\");\n    let h = NamedTempFile::new_in(&base).unwrap();\n}",
            ),
            (
                "process::id",
                "fn f() {\n    let p = std::env::temp_dir().join(format!(\"tasty-{}\", std::process::id()));\n}",
            ),
            (
                "fetch_add",
                "fn f() {\n    let p = std::env::temp_dir().join(format!(\"tasty-{}\", N.fetch_add(1, Ordering::Relaxed)));\n}",
            ),
            (
                "AtomicUsize",
                "fn f() {\n    let p = std::env::temp_dir().join(format!(\"tasty-{}\", AtomicUsize::new(0).load(Ordering::Relaxed)));\n}",
            ),
            (
                "pid",
                "fn f() {\n    let p = std::env::temp_dir().join(format!(\"tasty-{}\", pid));\n}",
            ),
            (
                "path_for",
                "fn f() {\n    let p = std::env::temp_dir().join(path_for(\"tasty\"));\n}",
            ),
            (
                "nanos",
                "fn f() {\n    let p = std::env::temp_dir().join(format!(\"tasty-{}\", nanos()));\n}",
            ),
            (
                "SystemTime",
                "fn f() {\n    let base = std::env::temp_dir().join(\"tasty\");\n    let t = SystemTime::now();\n}",
            ),
        ];
        assert_eq!(
            cases.len(),
            UNIQ_TOKENS.len(),
            "조각 수와 성분 수가 다르다 — 성분을 더했으면 조각도 더해라. 안 그러면 더한 \
             성분은 아무 시험도 안 받고, 이 시험의 이름(`every_…`)이 거짓이 된다"
        );
        for (name, src) in cases {
            let fc = classify_src(src);
            assert_eq!(
                fc.sites.len(),
                1,
                "{name}: 조각이 경로 짓는 자리 하나를 내야 한다 — 안 그러면 아래 단정이 \
                 공허하다"
            );
            assert!(
                fc.silent.is_empty(),
                "{name} 을 유니크화로 인정하지 않는다. 이 성분을 목록에서 뺐거나 철자를 \
                 바꿨으면 모듈 문서의 등급표도 함께 고쳐라 — 문서는 이것을 인정한다고 \
                 말하고 있다"
            );
            assert_eq!(fc.uniquified.len(), 1, "{name}: 유니크화 한 곳이어야 한다");
        }
    }

    /// pid 를 섞으면 유니크화로 통과한다.
    #[test]
    fn a_pid_keyed_name_passes() {
        let fc = classify_src(
            "fn f() {\n    let p = std::env::temp_dir()\n        .join(format!(\"x-{}.txt\", std::process::id()));\n}",
        );
        assert!(fc.silent.is_empty());
        assert_eq!(fc.uniquified.len(), 1);
    }

    /// 여러 줄 join 체인의 뒤쪽에 pid 가 있어도 창이 닿는다(disk_scrollback 형태).
    #[test]
    fn a_uniquifier_further_down_the_build_is_reached() {
        let fc = classify_src(
            "fn f() {\n    let dir = std::env::temp_dir().join(\"tasty-scrollback\");\n    let p = dir.join(format!(\n        \"surface-{}-{}\",\n        std::process::id(),\n        id\n    ));\n}",
        );
        assert!(fc.silent.is_empty(), "체인 아래 pid 를 창이 봐야 한다");
        assert_eq!(fc.uniquified.len(), 1);
    }

    /// 유니크화 성분이 **변수 뒤에 숨고**(창 밖 위) 경로가 인라인 `{unique}` 로
    /// 참조하면 통과한다 — 루트 통합 테스트의 지배적 관용구.
    #[test]
    fn an_inline_unique_var_bound_to_a_uniquifier_passes() {
        let fc = classify_src(
            "fn f() {\n    let unique = format!(\"{}-{}\", std::process::id(), nanos());\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let marker = std::env::temp_dir().join(format!(\"tasty-mark-{unique}.txt\"));\n}",
        );
        assert!(
            fc.silent.is_empty(),
            "창 밖 위에서 uniquifier 로 바인딩된 변수를 인라인 참조하면 통과해야 한다"
        );
        assert_eq!(fc.uniquified.len(), 1);
    }

    /// 위치 인자(`format!(\"{}\", unique)`)로 참조해도 같다.
    #[test]
    fn a_positional_unique_var_passes() {
        let fc = classify_src(
            "fn f() {\n    let unique = format!(\"{}\", std::process::id());\n    let p = std::env::temp_dir().join(format!(\"tasty-test-{}.port\", unique));\n}",
        );
        assert!(fc.silent.is_empty());
        assert_eq!(fc.uniquified.len(), 1);
    }

    /// 변수가 uniquifier 로 바인딩되지 **않았으면**(예: 고정 시나리오명) 참조해도
    /// 통과하지 않는다 — 변수 이름이 아니라 바인딩의 성분으로 판정한다.
    #[test]
    fn a_var_not_bound_to_a_uniquifier_does_not_pass() {
        let fc = classify_src(
            "fn f() {\n    let scenario = read_name();\n    let p = std::env::temp_dir().join(format!(\"tasty-{scenario}\"));\n}",
        );
        assert_eq!(
            fc.silent.len(),
            1,
            "uniquifier 로 바인딩 안 된 변수는 유니크화가 아니다"
        );
    }

    /// 사유(`이유:`)를 그 자리에 적으면 의도된 공유로 통과한다.
    #[test]
    fn a_reasoned_shared_path_passes() {
        let fc = classify_src(
            "fn f() {\n    // 이유: 사용자 config 라 의도된 공유다.\n    let p = std::env::temp_dir().join(\"tasty-config.toml\");\n}",
        );
        assert!(fc.silent.is_empty(), "사유가 붙으면 통과");
        assert_eq!(fc.reasoned.len(), 1);
    }

    /// 영문 `reason:` 마커도 인정한다.
    #[test]
    fn an_english_reason_marker_also_passes() {
        let fc = classify_src(
            "fn f() {\n    // reason: shared on purpose.\n    let p = std::env::temp_dir().join(\"shared\");\n}",
        );
        assert!(fc.silent.is_empty());
    }

    /// `사유:` 도 마커다. 실측 2026-09-06: 이 테스트가 없을 때 [`REASON_TOKENS`] 에서
    /// `사유:` 를 빼도 레포 스캔의 수(reasoned=7)가 안 움직이고 유닛 열셋도 전부
    /// 초록이었다 — 오늘 레포에 `사유:` 만으로 통과하는 자리가 없기 때문이다.
    /// 나머지 둘은 잡힌다(`이유:` 는 코퍼스와 유닛 둘 다, `reason:` 은 유닛이).
    /// 이 마커가 목록에 있다는 것은 문서가 **주장**하는 것이므로 그 주장을 여기서 건다.
    #[test]
    fn a_korean_sayu_marker_also_passes() {
        let fc = classify_src(
            "fn f() {\n    // 사유: 프로필 사이에 일부러 공유한다.\n    let p = std::env::temp_dir().join(\"tasty-shared\");\n}",
        );
        assert!(
            fc.silent.is_empty(),
            "`사유:` 를 사유 마커로 인정하지 않는다 — 목록에서 뺐으면 모듈 문서의 \
             마커 목록도 함께 고쳐라"
        );
        assert_eq!(fc.reasoned.len(), 1);
    }

    /// 경로를 안 짓는 `temp_dir()`(읽기 전용·전달)은 보지 않는다.
    #[test]
    fn a_bare_temp_dir_without_join_is_ignored() {
        let fc =
            classify_src("fn f() {\n    let dir = std::env::temp_dir();\n    read_only(dir);\n}");
        assert!(fc.sites.is_empty(), "join 이 없으면 자리로 세지 않는다");
    }

    /// 수신자가 창 밖에 있어도 자리로 센다 — 판정이 거리가 아니라 수신자 신원이다.
    ///
    /// 한때 이 자리는 `.join(` 을 여섯 줄 창에서만 찾아, 아래 배치(거리 8)가 **조용히
    /// 빠졌다**. 위반이 아니라 침묵이라 가드는 초록인 채로 이 자리를 안 봤다.
    #[test]
    fn a_join_on_the_bound_receiver_counts_however_far_it_sits() {
        let fc = classify_src(
            "fn f() {\n    let dir = std::env::temp_dir();\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let e = 5;\n    let g = 6;\n    let h = 7;\n    let p = dir.join(\"fixed-a\");\n}",
        );
        assert_eq!(fc.sites.len(), 1, "여덟 줄 아래의 수신자도 이 자리 것이다");
        assert_eq!(fc.silent.len(), 1, "고정 이름이라 위반으로 나와야 한다");
    }

    /// 가까이 있는 `.join(` 이라도 **다른 값의 것**이면 자리로 안 센다.
    ///
    /// `[T]::join` 은 경로를 짓지 않는다(`valid.join(", ")`). 창으로 물으면 이 배치가
    /// 자리가 되고, 그 뒤의 등급 판정이 전부 엉뚱한 줄을 읽는다.
    #[test]
    fn a_join_on_another_value_does_not_make_this_a_site() {
        let fc = classify_src(
            "fn f() {\n    let dir = std::env::temp_dir();\n    let valid = names();\n    let msg = valid.join(\", \");\n    read_only(dir, msg);\n}",
        );
        assert!(
            fc.sites.is_empty(),
            "문자열 join 은 경로를 짓는 것이 아니다"
        );
        assert_eq!(fc.unpaired.len(), 1);
    }

    /// 이름이 겹쳐도 **다른 함수**의 `.join(` 을 당겨오지 않는다.
    #[test]
    fn a_same_named_binding_in_another_fn_is_not_this_receiver() {
        let fc = classify_src(
            "fn f() {\n    let dir = std::env::temp_dir();\n    read_only(dir);\n}\nfn g() {\n    let dir = other();\n    let p = dir.join(\"fixed-a\");\n}",
        );
        assert!(fc.sites.is_empty(), "수신자는 감싸는 함수 안에서만 찾는다");
    }

    /// 사유는 붙은 주석 블록 **어디에 있어도** 인정된다 — 줄 수 제한이 없다.
    /// 한때 고정 4 줄 창이라 이 배치(첫 줄, 거리 8)가 거부됐다.
    #[test]
    fn a_reason_at_the_top_of_the_attached_block_counts() {
        let fc = classify_src(
            "fn f() {\n    // 이유: 공유가 의도다.\n    // 둘\n    // 셋\n    // 넷\n    // 다섯\n    // 여섯\n    // 일곱\n    let p = std::env::temp_dir().join(\"fixed-a\");\n}",
        );
        assert!(fc.silent.is_empty(), "붙은 블록의 첫 줄에 있는 사유");
    }

    /// 빈 줄이나 코드 줄에서 끊긴 **블록 밖**의 사유는 인정하지 않는다. 고정 4 줄
    /// 창은 이것을 인정했다 — 창이 "붙었다" 를 재지 않았다는 증거다.
    #[test]
    fn a_reason_outside_the_attached_block_does_not_count() {
        let fc = classify_src(
            "fn f() {\n    // 이유: 공유가 의도다.\n\n    let q = 1;\n    let p = std::env::temp_dir().join(\"fixed-b\");\n}",
        );
        assert_eq!(fc.silent.len(), 1, "블록 밖의 사유는 안 센다");
    }

    /// 사유가 문자열 안에만 있으면 인정하지 않는다(주석 마스크가 문자열을 덮는다).
    #[test]
    fn a_reason_inside_a_string_does_not_count() {
        let fc = classify_src(
            "fn f() {\n    let msg = \"이유: not a real marker\";\n    let p = std::env::temp_dir().join(\"fixed\");\n}",
        );
        assert_eq!(fc.silent.len(), 1, "문자열 속 이유: 는 사유가 아니다");
    }
}

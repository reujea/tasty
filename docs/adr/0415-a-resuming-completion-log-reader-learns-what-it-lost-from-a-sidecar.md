# ADR-0415: 재개하는 완료 로그 reader 는 옆 메타 파일에서 잃은 양을 안다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: notify, retention, reader-recovery, offset, plugin, compatibility, adr-0330, adr-0344
- **Group**: agent-integration

## Context

[ADR-0344](0344-the-completion-log-keeps-one-host-generation-and-says-what-it-threw-away.md)
는 완료 알림 로그(`<parent_home>/notify/<caller_surface>.log`)의 보존 범위를 정했다. 크기 축은
바이트 하나(256 KiB)이고, 닿으면 **전량 폐기**다. 버린 바이트 수는 `tracing::warn!` 로
남는다.

그 수는 **writer plugin 의 로그**에만 있다. 로그를 읽는 쪽(conductor 셸)은 그 수를 자기 읽기
위치와 이어 볼 수 없다. 그래서 reader 가 멈췄다가 재개하면 두 가지를 모른다.

- 그 사이 비우기가 있었는지.
- 있었다면 못 읽고 잃은 것이 얼마인지.

`tail -F` 는 파일이 줄어든 것을 보고 처음부터 다시 읽는다. 그러나 이것은 **arm 된 동안**만
그렇다. 멈췄다가 새로 붙는 reader 에게는 아무 신호도 없다.

값을 둘 자리는 둘이었다.

- **⒜ 줄 밖 메타** — `notify/<surface>.log.meta` 에 버린 바이트 누계를 둔다.
- **⒝ 줄 형식 변경** — 줄을 `<seq> <message>` 로 바꾸고 명시적 호환 전환을 거친다.

기준은 **호환 최대 보존**이다. 기존 reader 가 깨지지 않아야 한다. 기존 reader 는 셋이다.

- `tail -n0 -F` 로 줄을 받는 Monitor. 문서화된 사용자 경로다.
- 번들 plugin writer 두 개(claude · codex).
- 문구를 사람이 읽는 운영 관례.

[ADR-0330](0330-one-completion-line-is-one-write.md) 은 이미 로그 파일 **안**에 메타 줄을 두는
것을 기각했다. 이유는 "한 줄 = 완료 통지" 계약 때문이다.

어휘는 이미 있는 두 offset 계약에 맞춘다. `events.fetch` 는 `next_offset` · `truncated` ·
`skipped` 를 쓰고, `surface.read_since_mark` 는 `retention_start` 를 쓴다. 저장소는 공유하지
않는다 — 사건 버스와 PTY 버퍼는 이 파일과 다른 매체다.

## Decision

**⒜ 를 고른다. 로그 옆에 `<caller_surface>.log.meta` 를 두고, 거기에 `retention_start` —
이 세대에서 버린 바이트 누계 — 를 적는다. 줄 형식과 경로는 안 바꾼다.**

- **메타 파일 형식.**
  - 파일은 `key=value` 줄이다. 지금 키는 `retention_start` 하나다. 모르는 키는 건너뛴다.
  - 값은 **왼쪽 정렬 + 공백 채움, 폭 20**(u64 최대 자릿수)이다. 늘 같은 폭이라 제자리
    덮어쓰기가 파일을 줄이지 않는다. 0 채움을 안 쓰는 이유는 셸 `$((…))` 가 앞자리 0 을
    8 진수로 읽기 때문이다.
  - 파일이 없거나, 비었거나, 키가 없으면 0 이다(아직 안 비웠다).
- **논리 오프셋.**
  - reader 는 `next_offset` = `retention_start` + 파일 안 위치를 들고 있는다.
  - 비우기는 누계를 버린 양만큼 올린다. 누계는 줄지 않는다.
- **재개 절차.** `retention_start` 와 파일 길이 `len` 을 읽고 셋 중 하나로 간다.
  - `next_offset < retention_start` 면 `truncated` 다. `skipped = retention_start - next_offset`
    를 잃었고, 파일 처음부터 읽는다.
  - `next_offset - retention_start > len` 이면 **누계에 안 잡힌 비우기**가 있었다(아래 호환
    절). `truncated` 이고 `skipped` 는 **모른다**. 처음부터 읽는다.
  - 둘 다 아니면 파일 안 위치 `next_offset - retention_start` 부터 읽는다.
  - 어느 갈래든 **마지막 개행까지만** 소비하고, `next_offset` 을 그만큼 올린다.
- **정확성은 잠금이 지킨다.** advisory 잠금은 **메타 파일**에 건다.
  - append 는 공유 잠금을 쥔다.
  - 비우기(재기 · 비우기 · 누계 갱신)는 배타 잠금을 쥔다.
  - 그래서 비우기가 잰 크기가 곧 버린 양이다. 예전에 있던 창 — 비우기 직전 append 가 **아무
    수에도 안 잡힌 채** 사라지던 것 — 이 닫힌다.
  - 재개하는 reader 는 누계와 로그를 공유 잠금 아래에서 읽는다. 그러면 비우기가 그 사이에
    끼지 않는다.
  - 잠금을 로그 파일에 걸지 않는 이유는 Windows 의 파일 잠금이 강제형이기 때문이다. 로그에
    공유 잠금을 걸면 다른 writer 의 append 가 막힌다.
- **배타 잠금은 기다리지 않는다.**
  - 비우기는 `try_lock` 을 **200 ms** 동안 5 ms 간격으로 다시 시도한다. 못 잡으면 잠금 없이
    비우고 누계는 **올리지 않는다.** 잠금 없이 잰 크기는 버린 양과 다를 수 있으므로, 틀린
    수보다 "모른다" 를 남긴다. reader 에게는 위 두 번째 갈래다 — 새 어휘가 없다.
  - 이유: 공유 잠금을 쥐는 쪽에는 reader 도 있다. 그 reader 의 출력 소비자가 멈추면 잠금이
    계속 잡혀 있다. 블로킹 잠금이면 cap 에 닿은 writer — 곧 plugin 의 완료 통지 — 가 그 reader
    가 풀 때까지 선다(Gate 4 리뷰 실측: 2 s 뒤에도 안 끝났다). 통지 지연의 상한을 reader 가
    정하게 두지 않는다.
  - 공유 잠금(append)은 기다린다. 배타를 쥐는 것은 비우는 writer 뿐이고, 그 구간은 자기
    일(재기 · 비우기 · 누계 한 줄)로 끝난다.
  - **잠금 대기 상한(200 ms)은 파생되지 않는다 — 근거로 고른 값이다.** 아래로 누르는 쪽은 append
    의 공유 구간이다. 한 줄 write 한 번이라 µs 단위다(Gate 4 리뷰 실측: cap 아래 append 전체
    11.6 µs). writer 끼리의 경합으로는 이 상한에 닿으면 안 되므로 네 자릿수 이상 크게 잡았다.
    위로 누르는 쪽은 이 대기가 완료 통지 한 줄의 지연에 그대로 더해진다는 사실이다.
  - Windows 도 같은 의미다. `File::try_lock` 은 std 의 크로스 플랫폼 API 라 cfg 분기가 없다.
- **실패는 완료 통지를 막지도, 기다리게 하지도 않는다.**
  - 메타 파일을 못 열거나 공유 잠금이 실패하면 잠금 없이 예전처럼 append 한다.
  - 배타 잠금을 상한 안에 못 잡거나 실패하면 잠금 없이 비우고 누계는 안 올린다.
  - 누계 갱신이 실패하면 비우기는 그대로 한다(크기 축이 먼저다).
  - 모든 경우에 `warn` 이 남는다. reader 쪽에서는 위 두 번째 갈래("모른다")로 드러난다.
- **세대 경계는 그대로다.** 메타 파일은 `notify/` 안에 있어서 부팅 청소가 함께 지운다. 새
  세대의 누계는 0 부터 다시 센다. reader 의 `next_offset` 은 **한 호스트 세대 안에서만**
  뜻이 있다(ADR-0344 의 보존 범위).
- **새 IPC/CLI 는 두지 않는다.** 계약은 파일 두 개의 형식이다. 셸이 직접 읽을 수 있다.

## Consequences

- **얻은 것**: 재개하는 reader 가 "잃었나 / 얼마나" 를 값으로 안다. 그 수는 동시 writer 가
  있어도 정확하다. 시험 `under_concurrent_writers_read_plus_skipped_equals_written` 이 이를
  잰다: writer 6 개가 cap 512 에서 쓰는 동안 reader 가 재개를 반복하고, 끝에 **받은 바이트 +
  skipped = 쓴 바이트** 를 확인한다. **이 채널은 확률적이다** — 공유 잠금을 빼도 한 번 실행에
  약 30~40% 만 깨진다(변이 = `append_line_to` 의 `_shared` 줄 제거, 한 번 빌드 뒤 30 회 실행:
  12/30 · 9/30, 2026-09-22 두 사람 실측). 그래서 이 시험의 초록 한 번은 잠금이 있다는 증거가
  아니다. 재는 법: 그 줄을 지우고 `cargo test -p tasty-utils --lib --locked --no-run` 으로 한 번
  빌드한 뒤, 찍힌 `target/debug/deps/tasty_utils-<hash>` 를
  `--exact notify::tests::under_concurrent_writers_read_plus_skipped_equals_written` 로 30 회
  돌려 `test result: FAILED` 줄을 센다(매 회 `1 passed` 또는 `1 failed` 인지 함께 본다 — 이름이
  어긋나면 0 건이 돈다).
- **얻은 것**: `tail -n0 -F` 소비자, 줄 문구, 경로 규약이 하나도 안 바뀐다. 메타 파일을 모르는
  reader 는 예전 그대로 동작한다.
- **잃은 것**: `notify/` 에 파일이 surface 마다 하나 더 생긴다(37 바이트 — `retention_start=` 16 + 값 20 + 개행 1). `notify/*` 를
  glob 하는 소비자는 `.log.meta` 를 걸러야 한다. 레포 안에는 그런 소비자가 없다.
- **잃은 것 — 버전이 섞이면 정확성이 약해진다.** 잠금을 모르는 writer(이 결정 이전 plugin)가
  섞이면, 그 writer 가 한 비우기는 누계에 안 잡힌다. 대부분은 reader 의 "모른다" 갈래로
  드러난다. 다만 비운 뒤 파일이 옛 위치를 넘어 다시 자랐으면 **감지되지 않는다.** 번들 plugin
  은 같은 `tasty-utils` 를 링크하고 함께 bump 되므로, 섞이는 것은 업그레이드 과도기뿐이다.
- **잃은 것 — 오래 쥔 reader 가 정확성을 깎는다.** reader 가 공유 잠금을 200 ms 넘게 쥐고
  있는 동안 비우기가 나면, 그 비우기는 누계에 안 잡힌다. 통지는 서지 않는 대신 그 reader 가
  다음 재개에서 "모른다" 를 받는다. 그래서 reader 는 공유 잠금 구간을 짧게 해야 한다 — 누계와
  로그를 읽어 **파일로 복사한 뒤** 풀고, 소비는 잠금 밖에서 한다(dev-guide 의 reader 절).
- **운영 비용**: append 마다 메타 파일 open 한 번과 잠금 한 쌍이 든다. 완료 통지 빈도(분 단위)
  에서는 무시할 만하다. 비우기는 최악의 경우 200 ms 를 더 기다린다.

## Alternatives Considered

- **⒝ `<seq> <message>` 줄 형식** — reader 가 줄마다 순번을 보므로 틈이 줄 수로 바로 보인다.
  안 고른 이유:
  - 문서화된 `tail -F` 소비자가 받는 줄이 바뀐다. 그것은 명시적 호환 전환(옛 형식 병행 기간,
    capability, 문서 갱신)을 요구한다.
  - 순번은 잃은 **줄 수**만 준다. 이 로그의 크기 축은 바이트라서 ADR-0344 의 수와도 안 맞는다.
  - 여러 프로세스가 공유하는 순번을 만들려면 결국 같은 잠금 + 옆 파일이 필요하다.
- **메타를 로그 안의 줄로 둔다** — ADR-0330 이 기각했다. 이유는 그대로다.
- **잠금 없이 누계만 둔다** — 가장 싸다. 안 고른 이유: 비우기 직전 append 가 누계에 안 잡혀,
  "유실을 숨기지 않는다" 가 다시 확률적 약속이 된다. 위 시험이 잠금 없이 깨지는 것이 그 창의
  실측이다.
- **로그 파일 자체에 잠금** — 옆 파일이 필요 없다. 안 고른 이유: Windows 강제 잠금 때문에 공유
  잠금 동안 append 가 막힌다.
- **메타를 임시 파일 + rename 으로 갱신** — 잠금 없는 reader 도 원자적으로 본다. 안 고른 이유:
  잠금이 메타 파일에 걸려 있어서, rename 이 inode 를 바꾸면 잠금이 둘로 갈라진다. 고정 폭 제자리
  덮어쓰기로 같은 목적(빈 파일을 안 본다)을 얻었다.
- **읽기 CLI(`tasty notify read --since`)를 같이 낸다** — reader 절차를 코드 한 벌로 둘 수 있다.
  안 고른 이유: 이 결정의 범위는 값을 정하는 것이다. 소비자는 셸이라 파일 형식만으로 충분하다.
  CLI 를 내면 IPC 면까지 따라와야 한다(원칙 2). 필요가 생기면 그때 낸다.
- **세대 표식(`stream`)을 메타에 둔다** — 호스트 재시작을 넘어 살아 있는 reader 가 세대 교체를
  알 수 있다. 안 고른 이유: surface 에 붙은 reader 는 호스트와 같이 죽어서 세대를 넘지 못한다.
  세대 교체의 대부분은 두 번째 갈래("모른다")로도 드러난다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 누계가 정확하지 않게 되면(잠금이 빠지거나, 비우기가 누계 밖에서 일어나면)
  `notify::tests::under_concurrent_writers_read_plus_skipped_equals_written` 이 빨개질 수 있다
  — **확률적 채널**이다: 잠금을 빼도 한 번 실행에 약 30~40% 만 빨개진다(검출률과 재는 법은 위
  Consequences 의 첫 "얻은 것"). 초록 한 번으로 잠금이 있다고 판정하지 않는다.
- Windows 러너에서 같은 시험이 reader 휴지(재개 사이에 잠금 밖에서 `EXCLUSIVE_LOCK_RETRY` 만큼
  쉰다) 뒤에도 빨갛다 — append 공유 구간(`OpenOptions::open` 포함)을 Windows 에서 재서 수백 µs
  면 `notify.rs` 의 `EXCLUSIVE_LOCK_BUDGET` doc 이 말하는 "writer 끼리의 경합으로는 상한에 닿지
  않는다" 를 재판정한다. 채널은 CI `crossplatform-check` 의 `check-windows` 잡이다.
- 누계가 더해지지 않고 덮어써지면
  `notify::tests::retention_start_accumulates_what_every_truncation_threw_away` 가 빨개진다.
- 배타 잠금이 다시 블로킹이 되면(공유 잠금을 쥔 reader 가 비우는 writer 를 세우면)
  `notify::tests::a_reader_holding_the_shared_lock_does_not_stall_a_truncating_writer` 가
  시간 초과로 빨개진다. 같은 시험이 그때 누계를 안 올리는지와 reader 가 "모른다" 를
  받는지도 본다.
- 메타 한 줄의 폭이 값마다 달라지거나 앞자리 0 이 생기면
  `notify::tests::the_meta_line_has_a_fixed_width_and_no_leading_zeros` 가 빨개진다.
- `notify_meta_path` 가 `<log>.meta` 말고 다른 모양을 만들면
  `notify::tests::meta_path_sits_next_to_the_log` 가 빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **재개 reader 가 실제로 생기는가.** 지금 레포 안의 소비자는 `tail -F` 뿐이다. 재는 법:
  conductor 절차나 plugin 이 메타 파일을 읽기 시작하는지 본다. 둘 이상이 같은 절차를 따로
  구현하면 읽기 CLI 를 낼 때다.
- **"모른다" 갈래가 실제로 자주 나는가**(버전 혼재 · 잠금 실패 · 잠금 대기 상한 초과). 재는 법:
  writer plugin 로그(`<home>/plugins-logs/<plugin id>.log`)에서 `retention_start NOT advanced`,
  `lock failed`, `still locked by another holder` 줄을 센다. 상한 초과가 reader 없이도 나면
  200 ms 가 writer 끼리의 경합에 비해 작다는 뜻이다 — 그때 값을 다시 고른다.

## References

- 보존 범위(이 ADR 이 바꾸지 않는 것): [ADR-0344](0344-the-completion-log-keeps-one-host-generation-and-says-what-it-threw-away.md).
- 한 줄 = 한 write, 로그 안 메타 줄 기각: [ADR-0330](0330-one-completion-line-is-one-write.md).
- 어휘의 출처: `events.fetch`(`next_offset` · `truncated` · `skipped`)와
  `surface.read_since_mark`(`retention_start`) — [reference/api](../reference/api.md),
  [ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md).
- **코드 근거 (결정이 실현된 현재 위치)**: `crates/tasty-utils/src/notify.rs` 의
  `notify_meta_path` · `parse_retention_start` · `RETENTION_START_KEY` · `append_line_to` ·
  `truncate_and_account` · `advance_retention_start` · `MetaLock` · `EXCLUSIVE_LOCK_BUDGET` ·
  `EXCLUSIVE_LOCK_RETRY`. reader 절차의 참조 구현은
  같은 파일 `mod tests` 의 `resume` 이다.
- 사용자 경로: [dev-guide/external-interaction.md](../dev-guide/external-interaction.md#child-완료-알림--completion-log).

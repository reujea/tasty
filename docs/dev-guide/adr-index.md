# ADR 인덱스 다시 만들기

[`docs/adr/index.md`](../adr/index.md) 의 표 행은 ADR 헤더에서 **생성**한다. 사람이 쓰는 것은
마커 밖(인덱스 머리 목록 · 그룹 목록 · 그룹 머리말)뿐이다. 근거·대안·재검토 조건은
[ADR-0565](../adr/0565-the-adr-index-rows-are-generated-and-the-group-lives-in-the-adr-header.md).

## 파일 구조

그룹 절마다 생성 구역 하나가 있다.

```text
## <그룹 이름>                      ← 사람이 쓴다

<머리말: 결정 사슬 · 운영 문서>      ← 사람이 쓴다

<!-- adr-rows:begin <slug> -->
| # | Title | Status | Date | Tags |  ← 생성된다(머리글 포함)
…
<!-- adr-rows:end <slug> -->
```

ADR 이 어느 구역에 들어가는지는 그 ADR 헤더의 `- **Group**: <slug>` 가 정한다. 행의 값은
헤더에서 온다 — 제목(`# ADR-NNNN: …` 의 `:` 뒤, `**` 제거) · `Status`(괄호 밖 첫 ` — ` 앞까지,
링크와 `Superseded by ADR-` 의 `ADR-` 제거) · `Date` · `Tags`. 행 순서는 번호순이다.

## 명령

```bash
cargo run -p tasty-doc-guards --bin adr-index -- --write   # 생성 구역을 다시 쓴다
cargo run -p tasty-doc-guards --bin adr-index               # 대조만 — 다르거나 불완전하면 rc 1
```

종료코드: 0 = 같고(또는 `--write` 로 맞췄고) 생성 결과가 완전하다 · 1 = 다르다, 또는 생성 구역
밖에 표 줄이나 git 충돌 표지가 있다, 또는 생성 결과가 **불완전하다** — 생성 구역에 행이 없거나
둘 이상인 ADR 이 있다(`Group` 이 없거나 인덱스에 없는 slug 를 가리킨다. 뒤의 셋은 두 모드 모두) ·
2 = 만들 수 없다(모르는 인자 · 마커 짝이 깨짐 · ADR 0 개 · ADR 디렉토리 · ADR 파일 · 인덱스를
못 읽음 · `--write` 가 인덱스를 못 씀). 마커 밖은 한 글자도 안 바꾼다 —
그래서 마커 밖에 적힌 행과 머리말의 충돌 표지는 `--write` 로 안 없어지고, 손으로 푼다. 빠진 행도
`--write` 로 안 생긴다 — 그 ADR 헤더에 `Group` 을 적어야 생긴다. 파일이 생성 결과와 같아도 빠진
행이 있으면 rc 1 이다 — 행이 없는 ADR 은 생성 결과에서도 빠지므로 "같은가" 로는 안 잡힌다.

## 언제 돌리나

- **새 ADR 을 더했을 때** — 헤더에 `Group` 을 적고 돌린다. 그룹 고르는 기준은
  [`adr/template.md`](../adr/template.md) 의 "인덱스에 행을 추가할 때".
- **ADR 헤더(제목 · Status · Date · Tags · Group)를 고쳤을 때.** Supersede 로 옛 ADR 의 Status 를
  바꿨거나 그룹을 옮겼을 때도 같다.
- **rebase · 병합에서 `docs/adr/index.md` 가 충돌했을 때** — 인덱스는 두 층이라 푸는 법도
  둘이다. **생성 구역은 어느 쪽을 받아도 되지만, 머리말은 양쪽을 사람이 합쳐야 한다.** 그래서
  파일째 한쪽을 받지 않는다(`git checkout --ours` · `--theirs`) — 그러면 git 이 이미 자동 병합해
  둔 **상대 쪽 머리말 줄까지** 버려진다. 머리말은 사람이 쓰는 산문이라 생성기가 되살리지 못하고,
  그 유실은 어떤 판정에도 안 걸린다(다른 판정은 전부 초록이다). 순서:
  1. ADR 파일 자체가 충돌했으면 그것부터 푼다. 생성기는 헤더에 충돌 표지가 든 ADR 에서 첫 쪽
     값을 조용히 읽어 행에 싣는다.
  2. 충돌 표지가 든 인덱스를 **그대로 둔 채** `--write` 를 돌린다. 생성 구역은 안의 충돌 표지와
     양쪽 행째 새로 만든 행으로 덮인다 — 행은 ADR 파일에서 오므로 이 구역에서는 어느 쪽 내용이
     있었든 결과가 같다. 충돌 덩이가 마커 줄을 걸쳤으면 **마커 줄이 어느 쪽에 있느냐**로 갈린다.
     - **한쪽에만 있다** — 가장 흔한 형태다. 한쪽이 표 끝에 행을 더했고 다른 쪽은 그 자리에
       `adr-rows:end` 를 두었다(생성기 도입 전 브랜치와 도입 후 브랜치의 병합이 이렇다). 마커는 한
       벌이라 짝이 안 깨지고 `--write` 는 구역을 다시 쓴 뒤 **rc 1** 로 끝난다. 구역 밖에 남는 것은
       양쪽 산문이 아니라 덩이의 **표지 조각**이다 — `구역 밖 충돌 표지` 줄이 행 번호와 함께 찍는다.
       행을 더한 쪽이 앞(`<<<<<<<` 쪽)이면 짝 없는 `>>>>>>>` 한 줄이, 뒤(`>>>>>>>` 쪽)이면 `=======` ·
       그쪽 행 · `>>>>>>>` 가 end 마커 아래에 남는다(행은 `구역 밖 표 줄` 로도 찍힌다). 표지와 그 행을
       지운다 — 행은 그 ADR 에 `Group` 이 있으면 이미 구역 안에 생성돼 있고, 없으면 4 단계가 잡는다.
       실측(2026-09-23, 생성기 도입 lane 과 main 의 병합, 충돌 5 덩이 전부 이 형태): lane 을 main 에
       병합하면 rc 1 · 표지 5 · 표 줄 0, main 을 lane 에 병합하면 rc 1 · 표지 10 · 표 줄 6.
     - **양쪽 모두에 있다** — 두 쪽이 각자 end 마커 곁(마지막 행 · 뒤 머리말)을 고쳤다. 마커가 두
       벌이 되어 짝이 깨지고 `--write` · `--check` 는 **rc 2**(`열리지 않은 <slug> 구역을 닫았다`)로
       멈춘다. 마커 줄과 그 곁 머리말을 손으로 한 벌로 만든 뒤 다시 돌린다.
  3. 머리말(생성 구역 밖)에 남은 충돌은 **양쪽 산문을 합쳐** 푼다. 한쪽을 고르지 않는다 —
     양쪽이 각자 새 ADR 을 사슬에 올렸으면 둘 다 남아야 한다. 표지가 남아 있는 동안 `--check` ·
     `--write` 는 rc 1 이고 가드 `no_conflict_marker_is_left_in_the_preambles` 가 빨갛다.
  4. `--check` 가 0 으로 끝나는 것을 보고 `git add` 한다. 표지를 다 지웠는데 `생성 결과가
     불완전하다` 로 rc 1 이면, 찍힌 ADR 이 헤더에 `Group` 이 없는 것이다 — 흔히 상대 쪽이 생성기
     도입 전에 더한 ADR 이다. 헤더에 그룹을 적고 `--write` 를 다시 돌린다.

## 가드가 보는 것

`cargo test -p tasty-doc-guards --test adr_index_parity` 가 다음을 본다.

| 물음 | 시험 |
|------|------|
| 한 번호가 한 문서만 가리키는가 | `an_adr_number_names_exactly_one_document` |
| 본문 `# ADR-NNNN` 이 파일명 번호와 같은가 | `the_heading_number_matches_the_file_name` |
| 행을 만드는 헤더 값이 다 있는가 | `every_adr_header_carries_the_row_fields` |
| 생성 구역이 생성 결과와 같은가(손으로 고친 행 · 안 돌린 생성) | `the_index_rows_are_what_the_generator_renders` |
| 모든 ADR 이 정확히 한 그룹에 있고 그 그룹이 실재하는가 · 빈 그룹이 없는가 | `every_adr_sits_in_exactly_one_known_group` |
| 대체 · 부분 개정 · 중복 통합이 양 끝에 적혔는가 · 대체된 ADR 이 대체한 ADR 과 같은 그룹인가 | `decision_chains_are_written_on_both_ends` |
| 머리말이 부르는 네 자리 번호가 실재하는가 | `group_preambles_cite_only_existing_adrs` |
| 생성 구역 밖에 표 줄이 없는가(머리말에 끼운 행 · `adr-rows:end` 아래 덧붙인 행) | `no_table_line_sits_outside_the_generated_regions` |
| 머리말에 git 충돌 표지가 안 남았는가(병합 뒤 생성기만 돌리고 머리말을 안 합친 상태) | `no_conflict_marker_is_left_in_the_preambles` |

사슬의 짝 문구는 template 이 정한 것이다 — 새 ADR 쪽 `개정 대상:` ↔ 옛 ADR 쪽 `부분 개정:`(개정이
나중에 거둬졌으면 `부분 개정 후 철회:`), `통합 대상:` ↔ `중복 조항 통합:`. 줄 안에서 괄호 밖의
링크만 짝으로 센다 — 링크 뒤의 설명 ` (…)` · ` — …` 에 든 링크는 다른 관계다. 옛 ADR 이 새 ADR 로
대체됐으면(`Superseded by`) 그 Status 가 `부분 개정:` 을 대신한다.

머리말의 사슬 **서술이 완전한가**(어떤 A → B 를 머리말에 올릴지)는 안 본다 — 편집 판단이다.

생성기와 가드는 같은 모듈을 부르되 묻는 것이 다르다. **생성기는 자기 출력이 완전한가**(모든 ADR 이
생성 구역 안에 정확히 한 행을 갖는가)를 rc 로 내고, **가드는 그룹 배치와 결정 사슬**(왜 빠졌는가 ·
빈 그룹 · 대체 쌍의 같은 그룹 · 사슬의 양 끝)을 본다. 생성기의 완전성 판정은 합성 코퍼스 시험
`an_adr_left_out_of_the_rendered_rows_is_caught` 가 본다 — 행이 빠져도 파일이 생성 결과와 같다는
것을 먼저 보인 뒤 그 판정이 잡는지 묻는다.

## 채널

자동 채널은 `doc-guards.yml` 이다 — main push · PR 마다 `cargo test -p tasty-doc-guards --locked
--no-fail-fast` 를 경로 필터 없이 돈다(워크플로 파일 기준. 그 잡이 지금 초록인지는
[ci-gates](ci-gates.md) 의 방법으로 잰다). 로컬 조기 채널은 pre-push 훅의 `B.7` 이다. pre-commit 은
이 가드를 안 부른다 — 커밋 전에 잡으려면 위 명령을 직접 돌린다. 생성기 bin 자체에는 따로 도는
워크플로가 없고, 같은 가드 타깃의 시험 둘이 그것을 `--check` 로 돌린다:

- `the_generator_bin_agrees_with_the_repo` — 레포에서 rc 0 인가. 레포는 완전하므로 **rc 1 갈래를
  하나도 안 밟는다.**
- `the_generator_bin_fails_on_a_synthetic_corpus` — 임시 디렉토리에 ADR 둘과 마커를 가진 인덱스를
  쓰고 돌린다. 먼저 그 코퍼스가 rc 0 인 것을 보인 뒤, 파일이 생성 결과와 **같은데도** rc 1 이어야
  하는 세 갈래(`Group` 없는 ADR · `adr-rows:end` 아래 덧붙인 행 · 머리말의 충돌 표지)와 생성 구역
  행을 손으로 지운 갈래가 `--check` 에서 각각 rc 1 이고 그 사실을 찍는지 묻는다. 같은 코퍼스에
  `--write` 도 돌려, 앞의 세 갈래는 파일을 안 바꾸고 rc 1 이며 손으로 지운 갈래는 생성 결과로
  다시 쓰고 rc 0 인지 본다. 판정 함수가 옳아도 그 결과를 rc 로 바꾸는 줄이 빠지면 빨개지는 것은
  이 시험뿐이다 — 세 조건을 하나씩 지우는 변이와 `--write` 갈래를 지우는 변이에서 넷 다 이
  시험만 빨개졌다(2026-09-23).

여전히 안 재어지는 것: rc 2 갈래(모르는 인자 · ADR 디렉토리 · ADR 파일 · 인덱스를 못 읽음 · 마커 짝이 깨짐 ·
ADR 0 개 · `--write` 가 인덱스를 못 씀)의 bin 배선. 마커 구조 판정
자체는 `a_broken_marker_structure_is_refused` 가 라이브러리 함수로 본다.

요약 줄은 생성 구역 행을 **현재 파일**과 **생성 결과** 두 값으로 찍는다(`생성 구역 행 현재 N /
생성 결과 M`). `--check` 가 빨간 때는 둘이 다르다 — 파일에서 행을 지웠으면 현재가, ADR 헤더에서
`Group` 을 지웠으면 생성 결과가 작다.

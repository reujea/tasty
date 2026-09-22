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
cargo run -p tasty-doc-guards --bin adr-index               # 대조만 — 다르면 rc 1
```

종료코드: 0 = 같다(또는 `--write` 로 맞췄다) · 1 = 다르다 · 2 = 만들 수 없다(마커 짝이 깨짐 ·
ADR 0 개 · 파일을 못 읽음). 마커 밖은 한 글자도 안 바꾼다.

## 언제 돌리나

- **새 ADR 을 더했을 때** — 헤더에 `Group` 을 적고 돌린다. 그룹 고르는 기준은
  [`adr/template.md`](../adr/template.md) 의 "인덱스에 행을 추가할 때".
- **ADR 헤더(제목 · Status · Date · Tags · Group)를 고쳤을 때.** Supersede 로 옛 ADR 의 Status 를
  바꿨거나 그룹을 옮겼을 때도 같다.
- **rebase · 병합에서 `docs/adr/index.md` 가 생성 구역에서 충돌했을 때** — 충돌을 손으로 풀지
  않는다. 어느 쪽이든 받아(`git checkout --ours docs/adr/index.md` 등) 충돌 표지를 없앤 뒤
  `--write` 로 다시 만든다. 행은 ADR 파일에서 오므로 어느 쪽을 받았는지는 결과에 영향이 없다.
  머리말(생성 구역 밖)에서 충돌했으면 그 산문은 사람이 합친다.

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

사슬의 짝 문구는 template 이 정한 것이다 — 새 ADR 쪽 `개정 대상:` ↔ 옛 ADR 쪽 `부분 개정:`(개정이
나중에 거둬졌으면 `부분 개정 후 철회:`), `통합 대상:` ↔ `중복 조항 통합:`. 줄 안에서 괄호 밖의
링크만 짝으로 센다 — 링크 뒤의 설명 ` (…)` · ` — …` 에 든 링크는 다른 관계다. 옛 ADR 이 새 ADR 로
대체됐으면(`Superseded by`) 그 Status 가 `부분 개정:` 을 대신한다.

머리말의 사슬 **서술이 완전한가**(어떤 A → B 를 머리말에 올릴지)는 안 본다 — 편집 판단이다.

## 채널

자동 채널은 `doc-guards.yml` 이다 — main push · PR 마다 `cargo test -p tasty-doc-guards --locked
--no-fail-fast` 를 경로 필터 없이 돈다(워크플로 파일 기준. 그 잡이 지금 초록인지는
[ci-gates](ci-gates.md) 의 방법으로 잰다). 로컬 조기 채널은 pre-push 훅의 `B.7` 이다. pre-commit 은
이 가드를 안 부른다 — 커밋 전에 잡으려면 위 명령을 직접 돌린다. 생성기 bin 자체에는 따로 도는
채널이 없다 — 가드의 `the_generator_bin_agrees_with_the_repo` 가 그것을 `--check` 로 한 번 돌린다.

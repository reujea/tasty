# 테마

밝은 Latte와 어두운 Mocha 중 편한 테마를 골라보세요. 일부 색만 바꾸거나 TOML 파일로 나만의 테마를 만들 수도 있습니다. 테마 파일은 `~/.tasty/themes/`에 저장합니다.

## 번들 테마

| 테마 | 파일 | 밝기 | 비고 |
|------|------|------|------|
| **Catppuccin Mocha** | `~/.tasty/themes/mocha.toml` | 어두움 | 기본 테마. 지우거나 깨뜨려도 다음 실행 때 원본으로 복구됩니다 |
| **Catppuccin Latte** | `~/.tasty/themes/latte.toml` | 밝음 | 테마 폴더가 비어 있을 때 생성됩니다. 다른 테마가 남아 있으면 삭제한 Latte를 다시 만들지 않습니다 |

두 파일 모두 Tasty 가 관리하므로 **직접 편집하지 않습니다** — 실행할 때 Mocha와, 파일이 남아 있는 Latte를 원본 내용으로 맞춥니다. 색을 바꾸고 싶으면 아래 "색 몇 개만 바꾸기" 나 "직접 만들기" 를 씁니다.

읽기 권한이나 파일 인코딩 문제로 원본을 읽지 못하면 파일을 덮어쓰지 않습니다. 선택한 테마를 읽을 수 없으면 내장 Mocha 색상을 사용합니다. 실행 중이나 추가 윈도우에서 테마를 적용할 때는 파일을 복구하지 않습니다. 파일을 고친 뒤 설정의 테마 목록을 다시 열거나 Tasty를 다시 시작하세요.

밝은 Latte 테마에서 배너 본문과 카운트다운, 버튼, 키 표시와 태그, 선택·키보드 활성 행의 글자가 더 잘 읽히도록 대비를 높였습니다.

## 테마 바꾸기

가장 빠른 방법은 상태바 오른쪽 끝의 **테마 아이콘** 입니다. 클릭할 때마다 Latte 와 Mocha 를 오갑니다. 다른 테마를 쓰고 있을 때 누르면 Latte 로 갑니다.

목록에서 고르려면:

1. **설정** <!-- en: Settings --> (`Ctrl+,`) > **외관** <!-- en: Appearance --> > **테마** <!-- en: Theme -->.
2. **테마 프리셋** <!-- en: Theme Preset --> 아래 카드 중 하나를 클릭합니다. 카드에는 이름과 대표 색 다섯 개가 보입니다.
3. **저장** <!-- en: Save --> 을 누릅니다.

테마를 바꾸면 **색상** 탭과 **Tasty** 탭에서 덮어쓴 색, **터미널** 탭에서 정한 포커스 배경 · 비포커스 배경은 모두 초기화됩니다. 새 테마를 적용하면 그 테마의 색을 기준으로 시작합니다.

설정에 적힌 테마 파일이 없거나 읽을 수 없으면 Mocha 로 대신 시작하고 **테마를 찾을 수 없습니다** <!-- en: Theme not found --> 안내가 뜹니다.

## 색 몇 개만 바꾸기

테마 파일을 만들지 않고 현재 테마 위에 색을 덮어씁니다. 세 곳이 있습니다.

### 외관 > 색상

**색상** <!-- en: Colors --> 탭은 현재 테마의 모든 색을 그룹별로 나열합니다 — **배경** <!-- en: Surfaces --> · **오버레이** <!-- en: Overlays --> · **텍스트** <!-- en: Text --> · **강조색** <!-- en: Accents --> · **터미널 전용** <!-- en: Terminal-specific --> · **ANSI 16색** <!-- en: ANSI 16 -->.

1. 바꿀 행의 **기본값** <!-- en: Default --> 체크를 해제합니다.
2. 색을 고르거나 hex 값을 입력합니다.
3. 행의 **초기화** <!-- en: Reset --> 로 그 색만, 위의 **전체 초기화** <!-- en: Reset all --> 로 전부 되돌립니다. 바뀐 개수는 **n개 변경됨** <!-- en: n changed --> 으로 보입니다.
4. **저장**.

### 외관 > Tasty

버튼, 사이드바, 탭의 모양을 바꾸는 세 가지 항목입니다.

- **강조색** <!-- en: Accent --> — 활성 표시, 포커스 링, 버튼 등에 쓰이는 색.
- **사이드바 배경** <!-- en: Sidebar background -->.
- **활성 탭 표시** <!-- en: Active tab indicator --> — **밑줄** <!-- en: Underline --> / **채움** <!-- en: Fill --> / **점** <!-- en: Dot --> 중 하나.

**테마 기본값 사용** <!-- en: Use theme defaults --> 으로 세 항목을 한꺼번에 되돌립니다.

### 외관 > 터미널

터미널 서피스의 **포커스 배경** <!-- en: Focused background --> 과 **비포커스 배경** <!-- en: Unfocused background --> 을 따로 정합니다. 행의 **기본값 사용** <!-- en: Use default --> 을 끄면 편집할 수 있습니다. 포커스된 터미널과 아닌 터미널을 배경색으로 구분하는 것이 Tasty 의 기본 동작입니다 — Mocha 는 포커스 `#000000` / 비포커스 `#1e1e2e`.

이렇게 덮어쓴 값은 `~/.tasty/config.toml` 의 `[appearance]` 아래에 저장됩니다.

## 직접 만들기

`~/.tasty/themes/<id>.toml` 파일 하나가 테마 하나입니다. 파일 이름(확장자 제외)이 테마 id 가 되고, 설정의 테마 카드에는 `label` 이 표시됩니다. 다른 폴더의 파일은 읽지 않습니다.

**모든 색 항목은 선택** 입니다. 적지 않은 항목은 직전에 적용돼 있던 테마의 값이 그대로 남습니다. 그래서 몇 줄짜리 파일로도 테마가 됩니다.

```toml
# ~/.tasty/themes/my-theme.toml
label = "My Theme"      # 카드에 표시되는 이름
is_light = false        # 생략하면 이전 테마의 값 유지

[palette]
crust    = "#11111b"    # 가장 어두운 배경
mantle   = "#181825"    # 사이드바 등
base     = "#1e1e2e"    # 기본 배경
surface0 = "#313244"
surface1 = "#45475a"
surface2 = "#585b70"
overlay0 = "#6c7086"
overlay1 = "#7f849c"
overlay2 = "#9399b2"
text     = "#cdd6f4"
subtext1 = "#bac2de"
subtext0 = "#a6adc8"
placeholder = "#6c7086"

[accent]
blue = "#89b4fa"        # 기본 액센트
green = "#a6e3a1"
red = "#f38ba8"
yellow = "#f9e2af"
peach = "#fab387"
mauve = "#cba6f7"
teal = "#94e2d5"
sky = "#89dceb"
lavender = "#b4befe"
flamingo = "#f2cdcd"
pink = "#f5c2e7"
maroon = "#eba0ac"
rosewater = "#f5e0dc"

[terminal]
selection_bg = "#585b70"
vi_cursor_bg = "#b4befe"
search_match_bg = "#f9e2af4d"          # 8자리 = 마지막 두 자리가 투명도(그 칸의 배경 위에 섞인다)
search_match_active_bg = "#f9e2afb3"
search_match_active_fg = "#1e1e2e"     # 지금 결과 안의 글자색. 생략하면 이 값

[ansi]
black = "#45475a"
red = "#f38ba8"
green = "#a6e3a1"
yellow = "#f9e2af"
blue = "#89b4fa"
magenta = "#cba6f7"
cyan = "#94e2d5"
white = "#bac2de"
bright_black = "#6c7086"
bright_red = "#f38ba8"
bright_green = "#a6e3a1"
bright_yellow = "#f9e2af"
bright_blue = "#89b4fa"
bright_magenta = "#cba6f7"
bright_cyan = "#89dceb"
bright_white = "#cdd6f4"

[surfaces.terminal]
focused_bg   = "#000000"
focused_fg   = "#cdd6f4"
unfocused_bg = "#1e1e2e"
unfocused_fg = "#a6adc8"

[surfaces.markdown]
focused_bg   = "#11111b"
focused_fg   = "#cdd6f4"
```

규칙:

- 색은 `#RGB` · `#RRGGBB` · `#RRGGBBAA` 형식. 8자리면 마지막 두 자리가 투명도입니다.
- `[surfaces.<종류>]` 는 서피스 종류별 포커스 / 비포커스 배경 · 글자색입니다. `terminal` · `markdown` 외에 플러그인이 등록한 종류 이름도 쓸 수 있고, 정의하지 않은 종류는 안전한 기본색으로 그려집니다. 마크다운 문서 바탕은 포커스와 관계없이 `[surfaces.markdown]` 의 `focused_bg` 로 칠합니다.
- 호버 · 선택 강조 같은 반투명 효과는 `is_light`에 따라 자동으로 정해집니다. 여백 · 글자 크기는 테마 파일에서 설정하지 않습니다.
- 밝은 테마를 만들 때는 `is_light = true` 를 꼭 적습니다. 오버레이 색의 방향(검정 덧칠 / 흰색 덧칠)이 이 값으로 갈립니다.

만든 파일을 적용하려면 **설정** > **외관** > **테마** 를 엽니다. 이 탭을 열 때마다 폴더를 다시 읽으므로 재시작은 필요 없습니다. 카드가 보이면 클릭하고 **저장** 합니다.

Mocha 를 조금만 바꾼 변형을 만들려면 `mocha.toml` 을 복사해 **다른 이름** 으로 저장한 뒤 고칩니다. `mocha.toml` 자체를 고치면 다음 실행 때 원본으로 돌아갑니다.

## 문제 해결

| 증상 | 확인할 것 |
|------|-----------|
| 카드 목록에 내 테마가 안 보입니다 | 파일이 정확히 `~/.tasty/themes/` 에 있는지, 확장자가 `.toml` 인지. 설정 윈도우를 닫았다 다시 엽니다 |
| 시작하자마자 **테마를 찾을 수 없습니다** 가 뜹니다 | `config.toml` 의 `theme = "…"` 이 가리키는 파일이 없거나 문법 오류입니다. 파일을 고치거나 설정에서 다른 테마를 고릅니다 |
| 일부 색이 이전 테마 색으로 남아 있습니다 | 정상입니다 — 적지 않은 항목은 이전 값이 유지됩니다. 그 항목을 파일에 명시합니다 |
| 색상 탭에서 고친 값이 사라졌습니다 | 테마를 바꾸면 덮어쓴 값이 초기화됩니다. 테마 파일 쪽에 색을 옮겨 적습니다 |

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [설정](settings.md) — 외관 탭의 폰트 · UI 배율 · 투명도.
- [첫 화면 둘러보기](../getting-started/first-look.md) — 상태바의 테마 아이콘 위치.

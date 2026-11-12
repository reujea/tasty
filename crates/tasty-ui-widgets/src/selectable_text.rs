//! 선택 가능한 표시 텍스트. 편집 위젯이 아니다.
//!
//! 드래그로 글자를 선택하고 복사(Ctrl+C, macOS Cmd+C)할 수 있다. 캐럿을 그리지 않고 키 입력과
//! IME 조합을 받지 않는다. 포커스는 받으므로 포커스 중 화살표·Shift 선택 확장과 전체 선택은 동작한다.
//!
//! `egui::Label::selectable(true)` 대신 `TextEdit`을 쓰는 이유: Label의 드래그 선택은 포인터가 위젯
//! 밖으로 가로로 나가면 멈춘다(egui #3816). `TextEdit`은 범위 밖 좌표를 줄 끝으로 맞춘다.
//! `TextEdit::interactive(false)`는 선택까지 막으므로 쓰지 않는다. 대신 변경할 수 없는 `&str`
//! 버퍼를 넘긴다. egui는 `&str` 버퍼에 캐럿·IME 출력을 내지 않고 삽입·삭제를 하지 않는다.
//!
//! 회귀 시험: `crates/tasty-ui-widgets/tests/selectable_text.rs`.

/// [`selectable_text`] 의 줄바꿈 모드.
#[derive(Clone, Copy)]
pub enum TextWrap {
    /// 줄바꿈 없음, 콘텐츠 폭만큼만 차지(`egui::Label` 기본값과 동형).
    None,
    /// 가용 폭에서 여러 줄로 줄바꿈(`Label::wrap()` 과 동형).
    Wrap,
    /// 지정 폭에서 한 줄로 줄인다. 폭은 egui LayoutJob에 직접 전달한다.
    Truncate(f32),
}

/// 선택·복사 가능한 표시 텍스트를 그린다. 편집 위젯이 아니다.
///
/// 드래그 선택과 복사만 받고 캐럿·키 입력·IME 조합은 없다(모듈 문서).
pub fn selectable_text(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
    italic: bool,
    wrap: TextWrap,
) -> egui::Response {
    selectable_text_tracked(ui, text, color, size, monospace, italic, wrap, 0.0)
}

/// [`selectable_text`]에 논리 픽셀 단위 자간을 추가한다. 편집 위젯이 아니다.
#[allow(clippy::too_many_arguments)] // reason: 한 줄 텍스트의 표현 축이 그만큼이다
pub fn selectable_text_tracked(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
    italic: bool,
    wrap: TextWrap,
    tracking: f32,
) -> egui::Response {
    let color = color.into();
    let font_id = if monospace {
        egui::FontId::monospace(size)
    } else {
        egui::FontId::proportional(size)
    };
    // 변경할 수 없는 버퍼라 egui가 캐럿·IME를 내지 않고 키 입력을 넣지 않는다.
    let mut buffer: &str = text;
    let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color,
                italics: italic,
                extra_letter_spacing: tracking,
                ..Default::default()
            },
        );
        match wrap {
            TextWrap::None => job.wrap.max_width = f32::INFINITY,
            TextWrap::Wrap => {
                job.wrap.max_width = wrap_width;
                job.break_on_newline = true;
            }
            TextWrap::Truncate(w) => {
                job.wrap.max_width = w;
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
            }
        }
        ui.fonts(|f| f.layout_job(job))
    };
    // Wrap만 가용 폭을 채운다. 나머지는 내용 폭을 사용해 짧은 텍스트가 정렬 공간을 독점하지 않게 한다.
    let desired_width = if matches!(wrap, TextWrap::Wrap) {
        f32::INFINITY
    } else {
        0.0
    };
    ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .frame(false)
            .desired_rows(1)
            .desired_width(desired_width)
            .layouter(&mut layouter),
    )
}

/// [`selectable_text`] 축약형 — 줄바꿈 없음·italic 아님(가장 흔한 경우).
pub fn selectable_label(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
) -> egui::Response {
    selectable_text(ui, text, color, size, monospace, false, TextWrap::None)
}

/// [`selectable_label`] 에 자간을 더한 것 — 대문자 섹션 헤딩용.
pub fn selectable_label_tracked(
    ui: &mut egui::Ui,
    text: &str,
    color: impl Into<egui::Color32>,
    size: f32,
    monospace: bool,
    tracking: f32,
) -> egui::Response {
    selectable_text_tracked(
        ui,
        text,
        color,
        size,
        monospace,
        false,
        TextWrap::None,
        tracking,
    )
}

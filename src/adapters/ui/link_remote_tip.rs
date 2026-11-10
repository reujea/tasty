//! mirror 터미널의 파일 경로 링크에 수식키 hover 하면 링크 런 아래에 원격 위치 툴팁을 띄운다.
//! 내용은 `Remote` 낱말 + remote 글리프 + 호스트 글자다. 링크 셀의 모양은 바꾸지 않는다.
//! `tooltip-delay` 뒤에 나타나고 링크가 풀린 프레임에 사라진다. 그리기는 공용 위젯이 맡는다.

use std::time::Instant;

use tasty_type_geometry::length::PhysicalPx;
use tasty_type_geometry::rect::PhysicalRect;

use crate::state::MainViewState;
use crate::theme;

/// hover 한 링크 런의 화면 자리(물리 px)와 호스트 글자.
#[derive(Debug, Clone)]
pub(crate) struct LinkRemoteAnchor {
    pub(crate) anchor: PhysicalRect,
    pub(crate) host: String,
}

impl PartialEq for LinkRemoteAnchor {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.anchor, &other.anchor);
        self.host == other.host
            && a.x == b.x
            && a.y == b.y
            && a.width == b.width
            && a.height == b.height
    }
}

impl LinkRemoteAnchor {
    /// 화면에 보이는 링크 세그먼트를 감싸는 사각형. 보이는 세그먼트가 없으면 None.
    pub(crate) fn new(
        segments: &[tasty_terminal_link::LinkSegment],
        top_row: usize,
        surface_rect: &PhysicalRect,
        cell_w: f32,
        cell_h: f32,
        host: String,
    ) -> Option<Self> {
        let rows = (surface_rect.height.value() / cell_h).floor().max(0.0) as usize;
        let mut bounds: Option<(f32, f32, f32, f32)> = None;
        for seg in segments
            .iter()
            .filter(|s| s.absolute_row >= top_row && s.absolute_row < top_row + rows)
        {
            let x0 = surface_rect.x.value() + seg.start_col as f32 * cell_w;
            let x1 = surface_rect.x.value() + (seg.end_col + 1) as f32 * cell_w;
            let y0 = surface_rect.y.value() + (seg.absolute_row - top_row) as f32 * cell_h;
            let y1 = y0 + cell_h;
            bounds = Some(match bounds {
                None => (x0, y0, x1, y1),
                Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
            });
        }
        let (x0, y0, x1, y1) = bounds?;
        Some(Self {
            anchor: PhysicalRect {
                x: PhysicalPx(x0),
                y: PhysicalPx(y0),
                width: PhysicalPx(x1 - x0),
                height: PhysicalPx(y1 - y0),
            },
            host,
        })
    }
}

/// 띄우는 중인 툴팁. 같은 링크 위에 머문 시각부터 지연을 잰다.
#[derive(Debug, Clone)]
pub(crate) struct LinkRemoteTipState {
    surface_id: u32,
    tip: LinkRemoteAnchor,
    since: Instant,
}

/// 렌더 직전에 hover 링크로 상태를 맞춘다. 다른 링크로 옮기면 지연을 처음부터 잰다.
pub(crate) fn sync(
    state: &mut Option<LinkRemoteTipState>,
    hovered: Option<(u32, &LinkRemoteAnchor)>,
    now: Instant,
) {
    match hovered {
        None => *state = None,
        Some((surface_id, tip)) => {
            let same = state
                .as_ref()
                .is_some_and(|cur| cur.surface_id == surface_id && cur.tip == *tip);
            if !same {
                *state = Some(LinkRemoteTipState {
                    surface_id,
                    tip: tip.clone(),
                    since: now,
                });
            }
        }
    }
}

/// 지연이 지났으면 툴팁을 그린다. 아직이면 남은 시간 뒤에 다시 그리게 한다.
pub(crate) fn draw(ctx: &egui::Context, state: &MainViewState, scale_factor: f32) {
    let Some(tip) = &state.link_remote_tip else {
        return;
    };
    let th = theme::theme();
    let delay = th.tooltip_delay().to_duration();
    let waited = tip.since.elapsed();
    if waited < delay {
        ctx.request_repaint_after(delay - waited);
        return;
    }
    let logical = tip.tip.anchor.to_logical(scale_factor);
    let anchor = egui::Rect::from_min_size(
        egui::pos2(logical.x.value(), logical.y.value()),
        egui::vec2(logical.width.value(), logical.height.value()),
    );
    let bubble = tasty_ui_widgets::LinkRemoteTip {
        word: crate::i18n::t("attach.mirror_pill_label"),
        host: &tip.tip.host,
    };
    bubble.show(ctx, &th, anchor, ctx.screen_rect());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_terminal_link::LinkSegment;

    fn surface() -> PhysicalRect {
        PhysicalRect {
            x: PhysicalPx(100.0),
            y: PhysicalPx(50.0),
            width: PhysicalPx(800.0),
            height: PhysicalPx(200.0),
        }
    }

    #[test]
    fn the_anchor_wraps_the_visible_run() {
        let segments = [
            LinkSegment {
                absolute_row: 12,
                start_col: 70,
                end_col: 79,
            },
            LinkSegment {
                absolute_row: 13,
                start_col: 0,
                end_col: 4,
            },
        ];
        let a = LinkRemoteAnchor::new(&segments, 10, &surface(), 10.0, 20.0, "gx10".into())
            .expect("visible");
        assert_eq!(a.anchor.x.value(), 100.0);
        assert_eq!(a.anchor.y.value(), 50.0 + 2.0 * 20.0);
        assert_eq!(a.anchor.width.value(), 800.0);
        assert_eq!(a.anchor.height.value(), 40.0);
        // 화면 밖 세그먼트만 있으면 툴팁이 없다.
        assert!(
            LinkRemoteAnchor::new(&segments, 20, &surface(), 10.0, 20.0, "gx10".into()).is_none()
        );
    }

    #[test]
    fn moving_to_another_link_restarts_the_delay() {
        let a = LinkRemoteAnchor::new(
            &[LinkSegment {
                absolute_row: 0,
                start_col: 0,
                end_col: 3,
            }],
            0,
            &surface(),
            10.0,
            20.0,
            "gx10".into(),
        )
        .unwrap();
        let t0 = Instant::now();
        let mut state = None;
        sync(&mut state, Some((1, &a)), t0);
        let later = t0 + std::time::Duration::from_millis(500);
        sync(&mut state, Some((1, &a)), later);
        assert_eq!(
            state.as_ref().unwrap().since,
            t0,
            "같은 링크는 지연을 이어 간다"
        );
        sync(&mut state, Some((2, &a)), later);
        assert_eq!(state.as_ref().unwrap().since, later);
        sync(&mut state, None, later);
        assert!(state.is_none());
    }
}

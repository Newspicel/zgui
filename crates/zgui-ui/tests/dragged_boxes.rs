//! A box dragged under a zoomed layer leaves nothing behind.
//!
//! A square box with no shadow paints exactly its border box, so the damage its movement raises is
//! that rectangle where it was and where it is. Each partial repaint is compared with a complete
//! redraw of the same document, across zooms that put the box's edges on and between pixels.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: rgb(18, 52, 104) }
    .pane { position: relative; width: 800px; height: 600px; overflow: hidden }
    .layer { position: absolute; left: 0; top: 0; width: 0; height: 0; transform-origin: 0 0 }
    .node { position: absolute; left: 0; top: 0; width: 120px; height: 44px;
            background-color: rgb(24, 62, 120); border: 1px solid rgba(200, 222, 255, 0.8) }";

fn scene(zoom: RwSignal<f32>, dragged: RwSignal<(f32, f32)>) -> impl Fn() -> AnyView {
    move || {
        AnyView::new(view! {
            box(class = "pane") {
                box(class = "layer", style:transform = move || {
                    Some(format!("translate(13.3px, 7.7px) scale({})", zoom.get()))
                }) {
                    box(class = "node", style:transform = "translate(40px, 40px)") {}
                    box(class = "node", style:transform = move || {
                        let (x, y) = dragged.get();
                        Some(format!("translate({x}px, {y}px)"))
                    }) {}
                }
            }
        })
    }
}

fn picture(stage: &Stage) -> Vec<(u8, u8, u8)> {
    stage.composed_colours_in(Rect::<DevicePx, Device>::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(800.0), DevicePx(600.0)),
    ))
}

fn differing(a: &[(u8, u8, u8)], b: &[(u8, u8, u8)]) -> usize {
    a.iter()
        .zip(b)
        .filter(|(x, y)| {
            let d = |p: u8, q: u8| (i32::from(p) - i32::from(q)).abs();
            d(x.0, y.0) + d(x.1, y.1) + d(x.2, y.2) > 24
        })
        .count()
}

#[test]
fn a_dragged_box_leaves_no_trail_at_any_zoom() {
    crate::device::use_real_damage();
    let zoom = RwSignal::new(1.0f32);
    let dragged = RwSignal::new((200.0f32, 150.0f32));
    let Some(mut stage) = Stage::open(SHEET, scene(zoom, dragged)) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let mut failures = Vec::new();
    for level in [0.8f32, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
        zoom.set(level);
        dragged.set((200.0, 150.0));
        stage.wait_quietly(std::time::Duration::from_millis(34));
        stage.repaint();
        // Several partial frames in a row, and only then a comparison: a trail is what the frames
        // leave behind, not what any one of them draws.
        for step in 1..12 {
            let t = step as f32;
            dragged.set((200.0 + t * 3.37, 150.0 + t * 1.91));
            stage.wait_quietly(std::time::Duration::from_millis(34));
        }
        let partial = picture(&stage);
        stage.repaint();
        let full = picture(&stage);
        let wrong = differing(&partial, &full);
        if wrong > 0 {
            stage.capture_composed(&format!("trail-{level}"));
            failures.push(format!("zoom {level}: {wrong} pixels"));
        }
    }
    assert!(
        failures.is_empty(),
        "a drag left pixels behind: {failures:#?}"
    );
}

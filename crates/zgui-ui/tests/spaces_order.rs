//! Content drawn later covers content inside a transformed layer, wherever the layer moved it.

mod desktop;
mod device;
mod painted;

use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff }
    .page { position: relative; width: 900px; height: 600px }
    .layer { position: absolute; left: 0; top: 0; width: 0; height: 0; transform-origin: 0 0;
             }
    .node { position: absolute; left: 0; top: 0; width: 150px; height: 100px;
            background-color: #888888; transform: translate(10px, 10px); padding: 4px; gap: 4px }
    .row { height: 16px; background-color: #444444 }
    .popup { position: absolute; left: 400px; top: 300px; width: 120px; height: 80px;
             background-color: #ff00ff; z-index: 10 }";

fn scene(pan: RwSignal<f32>) -> impl Fn() -> AnyView {
    move || {
        AnyView::new(view! {
            box(class = "page") {
                box(class = "layer", style:transform = move || {
                    Some(format!("translate({}px, 300px) scale(0.8)", pan.get()))
                }) {
                    column(class = "node") {
                        box(class = "row") {}
                        box(class = "row") {}
                        box(class = "row") {}
                    }
                }
                box(class = "popup") {}
            }
        })
    }
}

#[test]
fn a_later_box_covers_a_node_in_a_moved_layer() {
    crate::device::use_real_damage();
    let pan = RwSignal::new(400.0f32);
    let Some(mut stage) = Stage::open(SHEET, scene(pan)) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    for step in 0..10 {
        pan.set(400.0 - step as f32 * 11.5);
        stage.wait_quietly(std::time::Duration::from_millis(34));
        for (x, y) in [(420.0, 320.0), (460.0, 340.0), (500.0, 370.0)] {
            let colour = stage
                .composed_colours_in(zgui::geom::Rect::new(
                    Point::new(DevicePx(x), DevicePx(y)),
                    zgui::geom::Size::new(DevicePx(1.0), DevicePx(1.0)),
                ))
                .first()
                .copied();
            assert_eq!(colour, Some((255, 0, 255)), "step {step}, at ({x}, {y})");
        }
    }
}

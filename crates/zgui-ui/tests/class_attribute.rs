//! Attribute selectors read the `class` attribute.

mod desktop;
mod device;
mod painted;

use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff }
    .swatch { width: 40px; height: 40px; background-color: #000000 }
    [class*=\"lue\"] { background-color: #0000ff }
    [class~=\"red\"] { background-color: #ff0000 }
    [class^=\"swatch gr\"] { background-color: #00ff00 }";

fn scene(late: RwSignal<bool>) -> impl Fn() -> AnyView {
    move || {
        AnyView::new(view! {
            box {
                box(class = "swatch blue") {}
                box(class = "swatch red") {}
                box(class = "swatch green") {}
                box(class = "swatch", class:blue = move || late.get()) {}
            }
        })
    }
}

#[test]
fn substring_word_and_prefix_selectors_match_the_class_list() {
    let late = RwSignal::new(false);
    let Some(mut stage) = Stage::open(SHEET, scene(late)) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let at = |stage: &Stage, y: f32| stage.colour_at(Point::new(DevicePx(20.0), DevicePx(y)));
    assert_eq!(at(&stage, 20.0), (0, 0, 255), "[class*=]");
    assert_eq!(at(&stage, 60.0), (255, 0, 0), "[class~=]");
    assert_eq!(at(&stage, 100.0), (0, 255, 0), "[class^=]");
    assert_eq!(at(&stage, 140.0), (0, 0, 0), "before the class changes");
    late.set(true);
    stage.wait(std::time::Duration::from_millis(34));
    assert_eq!(at(&stage, 140.0), (0, 0, 255), "after the class changes");
}

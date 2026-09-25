//! Colours written in `hsl()` reach the screen at the hue they name.

mod desktop;
mod device;
mod painted;

use zgui::geom::{DevicePx, Point};
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff }
    .swatch { width: 40px; height: 40px }
    .legacy { background-color: hsl(0, 70%, 55%) }
    .modern { background-color: hsl(120deg 70% 55%) }
    .inline { width: 40px; height: 40px }";

fn scene() -> AnyView {
    AnyView::new(view! {
        box {
            box(class = "swatch legacy") {}
            box(class = "swatch modern") {}
            box(class = "inline", style:background-color = move || Some("hsl(240, 70%, 55%)".to_owned())) {}
        }
    })
}

#[test]
fn each_swatch_shows_the_hue_it_names() {
    let Some(stage) = Stage::open(SHEET, scene) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let at = |y: f32| stage.colour_at(Point::new(DevicePx(20.0), DevicePx(y)));
    let (red, green, blue) = (at(20.0), at(60.0), at(100.0));
    assert!(red.0 > red.1 && red.0 > red.2, "hsl(0 ..) drew {red:?}");
    assert!(
        green.1 > green.0 && green.1 > green.2,
        "hsl(120deg ..) drew {green:?}"
    );
    assert!(
        blue.2 > blue.0 && blue.2 > blue.1,
        "hsl(240 ..) drew {blue:?}"
    );
}

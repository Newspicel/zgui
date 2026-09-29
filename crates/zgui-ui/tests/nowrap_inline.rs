//! A line that may not wrap stays one line when the text inside it sits in nested inline elements.
//!
//! The shaper marks a break after every inline box, and the edges of a nested inline element reach
//! it as inline boxes. A line that overflows its box must still stay whole, so that
//! `text-overflow` cuts it off at the box edge.

mod desktop;
mod device;
mod painted;

use zgui::view;
use zgui::view::NodeRef;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; font-size: 12px }
    .row { display: flex; flex-direction: row; align-items: center; gap: 7px; width: 360px; height: 22px; overflow: hidden }
    .mark { flex: 0 0 auto; width: 12px; height: 12px }
    .label { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap }
    .lit { font-weight: 600 }
    .note { flex: 1 1 0; min-width: 0; white-space: nowrap; overflow: hidden; text-align: right }";

/// Text far wider than the row, with a break opportunity after every slash.
const LONG: &str = "try.k8s.io/ingress-nginx/controller:v1.9.4@sha256:abcdef0123456789abcdef";

#[test]
fn an_overflowing_label_of_nested_runs_stays_one_line() {
    let nested = NodeRef::new();
    let single = NodeRef::new();
    let bare = NodeRef::new();
    let Some(stage) = Stage::open(SHEET, move || {
        view! {
            column {
                row(class = "row") {
                    box(class = "mark") {}
                    box(class = "label", node_ref = nested) {
                        text(class = "lit") {"regis"}
                        text {{LONG}}
                    }
                    text(class = "note") {"3 pods"}
                }
                row(class = "row") {
                    box(class = "mark") {}
                    box(class = "label", node_ref = single) {
                        text {{LONG}}
                    }
                }
                row(class = "row") {
                    box(class = "mark") {}
                    text(class = "label", node_ref = bare) {{LONG}}
                }
            }
        }
    }) else {
        return;
    };
    let host = &stage.handles().host;
    let height = |node: NodeRef| {
        let node = node.get_untracked().expect("the label is in the tree");
        host.window_box(node)
            .expect("the label has a box")
            .size
            .height
            .0
    };
    let line = height(bare);
    assert!(line > 0.0);
    assert_eq!(height(single), line, "one nested run stays on one line");
    assert_eq!(height(nested), line, "several nested runs stay on one line");
}

//! What a spinner puts on the screen.
//!
//! # What this measures
//!
//! A mark that turns has to be visibly *part* of a circle: a ring that is whole at every angle
//! turns without appearing to move. One quad carries one stroke, so the four sides of a border are
//! painted in one colour and a ring cannot have a transparent side — which is why the mark is a
//! second box on a fainter track rather than a border with a gap in it.
//!
//! The assertions are taken off the pixels, because that is the only reading that tells a whole
//! ring from a track with a mark on it.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::view;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{SETTLED, Stage};

/// A white page with black text on it, so the mark and the track are told apart by darkness alone.
const SHEET: &str = ":root { background-color: #ffffff; color: #000000; font-family: sans-serif }
                     .page { padding: 32px; align-items: flex-start }";

/// How dark a pixel of the mark is at most.
const MARK_CEILING: u8 = 90;

/// How light a pixel of the track is at least, and how dark it is at most.
const TRACK: (u8, u8) = (140, 240);

/// Where the spinner was built, kept across the closure the stage rebuilds.
type Held = Rc<RefCell<Option<NodeRef>>>;

/// Opens a page with one spinner on it, or reports the run skipped with no graphics device.
fn staged() -> Option<(Stage, NodeId)> {
    let held: Held = Rc::new(RefCell::new(None));
    let kept = Rc::clone(&held);
    let mut stage = Stage::open(SHEET, move || {
        let mark = NodeRef::new();
        *kept.borrow_mut() = Some(mark);
        view! {
            ThemeProvider {
                column(class = "page") {
                    row(node_ref = mark) {
                        Spinner(label = "Loading")
                    }
                }
            }
        }
    })?;
    stage.wait(SETTLED);
    let node = held
        .borrow()
        .expect("the spinner was built")
        .get_untracked()
        .expect("the spinner bound its reference");
    Some((stage, node))
}

/// Opens the fixture, or leaves the test without running it.
macro_rules! spinning {
    () => {
        match staged() {
            Some(held) => held,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

#[test]
fn a_spinner_draws_a_mark_on_a_fainter_track() {
    let (stage, node) = spinning!();
    stage.capture("spinner");
    let colours = stage.colours_in(stage.rect_of(node));
    assert!(!colours.is_empty(), "the spinner has a box");

    let mark = colours
        .iter()
        .filter(|(red, green, blue)| {
            *red <= MARK_CEILING && *green <= MARK_CEILING && *blue <= MARK_CEILING
        })
        .count();
    let track = colours
        .iter()
        .filter(|(red, green, blue)| {
            let (low, high) = TRACK;
            *red >= low
                && *red <= high
                && *green >= low
                && *green <= high
                && *blue >= low
                && *blue <= high
        })
        .count();

    assert!(mark > 0, "the mark is drawn in the current colour");
    assert!(track > 0, "the track is drawn fainter than the mark");
    assert!(
        track > mark,
        "the track runs the whole way round and the mark does not: {mark} mark, {track} track"
    );
}

#[test]
fn a_spinner_costs_no_path_rasteriser() {
    let (stage, node) = spinning!();
    assert!(
        stage.drawings_in(stage.rect_of(node)).is_empty(),
        "the mark is boxes, so no vector item reaches the renderer"
    );
}

#[test]
fn a_spinner_turns() {
    let (mut stage, node) = spinning!();
    let rect = stage.rect_of(node);
    let first = stage.colours_in(rect);
    stage.wait(std::time::Duration::from_millis(300));
    let later = stage.colours_in(rect);
    assert_eq!(first.len(), later.len());
    assert_ne!(
        first, later,
        "the mark is somewhere else a third of a turn on"
    );
}

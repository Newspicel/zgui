//! An event aimed at an element that has left the document.
//!
//! A press can ask for a click on the element it landed on, and the click is carried out after
//! the press has finished. Whatever else the press set off — a field that commits on blur, a list
//! that rebuilds — can take that element out of the document in between. The click then has no
//! element to reach, and the listeners it would have called belong to a scope that is gone.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerButton, PointerEvent, Timestamp};

/// The sheet the fixture is styled by: one element covering the whole window.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px }";

/// A press over the middle of the window.
fn press(action: PointerAction) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(150.0)))
            .with_button(PointerButton::Primary),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

#[test]
fn a_click_a_press_asked_for_reaches_nothing_once_the_press_took_its_element_away() {
    let clicks = Rc::new(Cell::new(0_u32));
    let shown = RwSignal::new(true);

    let counted = Rc::clone(&clicks);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let counted = Rc::clone(&counted);
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(move || {
                    let counted = Rc::clone(&counted);
                    shown.get().then(move || {
                        zgui_elements::column()
                            .class("key")
                            .on(zgui_view::events::POINTER_DOWN, move |ev| {
                                ev.activate_now();
                                shown.set(false);
                            })
                            .on(zgui_view::events::CLICK, move |_| {
                                counted.set(counted.get() + 1);
                            })
                    })
                })
                .into_view()
                .build(cx),
        )
    });

    app.settle(4);
    app.deliver_to_first(press(PointerAction::Moved));
    app.deliver_to_first(press(PointerAction::Pressed));
    app.settle(4);
    app.deliver_to_first(press(PointerAction::Released));
    app.settle(4);
    assert!(!shown.get_untracked(), "the press took the element away");
    assert_eq!(
        clicks.get(),
        0,
        "the click reached an element that had already left the document"
    );
}

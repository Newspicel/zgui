//! Asking a window which element holds a pointer's capture.
//!
//! A host that shares the pointer with other content reads this to keep a drag that started on
//! the interface with the interface, so the answer has to follow the capture a handler asked for
//! and its release.

mod support;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, PointerId, Timestamp};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px }";

/// A mouse event in the middle of the window.
fn mouse(action: PointerAction) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(150.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

#[test]
fn a_capture_a_handler_asked_for_is_reported_until_a_handler_releases_it() {
    let mut app = support::app(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .on(zgui_view::events::POINTER_DOWN, |cx| cx.capture_pointer())
                .on(zgui_view::events::POINTER_UP, |cx| cx.release_pointer())
                .into_view()
                .build(cx),
        )
    });
    app.settle(4);
    let window = || &app.app().windows()[0];
    assert!(!window().has_pointer_capture());
    assert_eq!(window().pointer_capture(PointerId::MOUSE), None);

    app.deliver_to_first(mouse(PointerAction::Pressed));
    app.settle(4);
    let window = &app.app().windows()[0];
    assert!(window.has_pointer_capture(), "the press captured the mouse");
    assert!(window.pointer_capture(PointerId::MOUSE).is_some());
    assert_eq!(window.pointer_capture(PointerId::new(7)), None);

    app.deliver_to_first(mouse(PointerAction::Released));
    app.settle(4);
    let window = &app.app().windows()[0];
    assert!(
        !window.has_pointer_capture(),
        "the handler on the release ended the capture"
    );
    app.shut_down();
}

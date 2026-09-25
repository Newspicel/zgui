//! What a pan and a zoom of a transformed layer cost.
//!
//! A node graph moves its whole content by writing one transform on one layer. Everything inside
//! the layer is laid out in the layer's own space, so a pan owes the layer's own fragments, the
//! damage where the content was and is, and nothing else: no card is composed again, no card's
//! painting is encoded again, and a view watching a card's size is not woken. A zoom changes the
//! scale every recording was made at, so the paintings are encoded again and the fragments are
//! still only carried.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_profile::{COUNTERS_ENABLED, Counter};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_view::{BuildCx, IntoView, NodeRef, View};

/// How many cards the layer holds.
const CARDS: usize = 8;

const CSS: &str = "
root { display: block; width: 400px; height: 300px; overflow: hidden }
.layer { position: absolute; left: 0; top: 0; width: 1200px; height: 900px; transform-origin: 0 0 }
.card { display: block; width: 90px; height: 24px; margin: 4px; background-color: #303030 }
";

#[test]
fn a_pan_carries_the_layer_and_a_zoom_encodes_its_paintings() {
    let _turn = zgui_profile::counter::exclusive();
    let view = RwSignal::new_local((0.0f32, 1.0f32));
    let runs = Rc::new(Cell::new(0u32));
    let seen = Rc::clone(&runs);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let watched = NodeRef::new();
        let mut layer =
            zgui_elements::column()
                .class("layer")
                .style_property("transform", move || {
                    let (pan, zoom) = view.get();
                    Some(format!("translate({pan}px, 7.5px) scale({zoom})"))
                });
        for index in 0..CARDS {
            let mut card = zgui_elements::r#box()
                .class("card")
                .child(zgui_elements::text().child(format!("card {index}")));
            if index == 0 {
                card = card.node_ref(watched);
            }
            layer = layer.child(card);
        }
        let size = watched.observe_content_size();
        let seen = Rc::clone(&seen);
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            let _ = size.get();
            seen.set(seen.get() + 1);
        }));
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(layer)
                .into_view()
                .build(cx),
        )
    });
    harness.settle(16);
    view.set((3.0, 1.0));
    harness.settle(16);

    let woken = runs.get();
    let before = zgui_profile::counter::snapshot();
    view.set((16.5, 1.0));
    harness.settle(16);
    let pan = before.delta(&zgui_profile::counter::snapshot());
    assert_eq!(
        runs.get(),
        woken,
        "a pan woke a view watching a card's size"
    );

    let before = zgui_profile::counter::snapshot();
    view.set((16.5, 1.25));
    harness.settle(16);
    let zoom = before.delta(&zgui_profile::counter::snapshot());

    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        pan.get(Counter::FragmentsRebuilt) < 4,
        "a pan composed {} fragments again",
        pan.get(Counter::FragmentsRebuilt)
    );
    assert!(
        pan.get(Counter::ChunksReencoded) < 4,
        "a pan encoded {} paintings again",
        pan.get(Counter::ChunksReencoded)
    );
    assert!(
        zoom.get(Counter::FragmentsRebuilt) < 4,
        "a zoom composed {} fragments again",
        zoom.get(Counter::FragmentsRebuilt)
    );
    assert!(
        zoom.get(Counter::ChunksReencoded) >= CARDS as u64,
        "a zoom encoded only {} paintings again, so a recording made at another scale replayed",
        zoom.get(Counter::ChunksReencoded)
    );
}

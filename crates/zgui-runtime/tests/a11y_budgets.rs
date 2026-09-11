//! What a geometry change costs the accessibility tree, in counters.
//!
//! The counters are process-global, so every case here takes the counter lock and no other target
//! in this crate asserts on the two it reads.

mod support;

use zgui_a11y::{NodeId, TreeUpdate};
use zgui_platform_headless::Harness;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_runtime::Runtime;
use zgui_view::{A11yBinding, IntoView, NodeRef, View};
use zgui_vocab::Role;

/// The last update the window's surface was handed.
fn published(harness: &Harness<Runtime>) -> TreeUpdate {
    harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .last_a11y_update()
        .expect("the frame published an accessibility update")
}

/// The accessibility identifier of whatever `node` is bound to, once the frame has settled.
fn bound_id(node: NodeRef) -> NodeId {
    NodeId(
        node.get_untracked()
            .expect("the node reference is bound after a frame")
            .as_u64(),
    )
}

/// Asserts that nothing the surface was ever handed names a node a consumer could not resolve.
fn assert_every_update_resolves(harness: &Harness<Runtime>) {
    support::replay_a11y(harness);
}

/// A control that grew is answered with its rectangle, and its parent is not sent again.
///
/// Nothing about the control an assistive technology reads changed except how large it is, so
/// the update carries that node alone — the same answer a control that moved gets — and not a
/// projection of its role, name and relations, nor its parent's child list.
#[test]
fn a_control_that_grew_is_re_measured_and_its_parent_is_not_re_sent() {
    let _turn = zgui_profile::counter::exclusive();
    let wide = RwSignal::new_local(false);
    let button = NodeRef::new();
    // The class moves on the root, so the control itself is edited by nothing: what reaches it
    // is a wider box from layout and only that.
    let mut harness = support::app_with_text(
        "root { display: block; width: 400px; height: 300px }
         control { display: block; width: 120px; height: 24px }
         .wide control { width: 200px }",
        move |cx: &mut zgui_view::BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .class_toggle(zgui_view::ClassName::new("wide"), move || wide.get())
                    .child(
                        zgui_elements::control()
                            .node_ref(button)
                            .a11y(A11yBinding::new(Role::Button).label("Save")),
                    )
                    .into_view()
                    .build(cx),
            )
        },
    );
    harness.settle(8);
    let target = bound_id(button);

    let before = zgui_profile::counter::snapshot();
    wide.set(true);
    harness.settle(8);
    let moved = before.delta(&zgui_profile::counter::snapshot());

    let update = published(&harness);
    assert_eq!(
        update.nodes.len(),
        1,
        "a control that grew re-sent more than itself:\n{}",
        zgui_a11y::dump(&update)
    );
    let bounds = update
        .nodes
        .iter()
        .find(|(id, _)| *id == target)
        .and_then(|(_, node)| node.bounds())
        .expect("the control that grew is the node sent");
    assert_eq!(bounds.width(), 200.0, "{bounds:?}");
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            moved.a11y_projected, 0,
            "a size change projected a node whole"
        );
        assert_eq!(moved.a11y_remeasured, 1);
    }
    assert_every_update_resolves(&harness);
}

/// A column of cells that all widen is a rectangle per cell and no projection at all.
#[test]
fn a_thousand_cells_that_widen_are_re_measured_and_not_projected() {
    const ROWS: usize = 1000;
    let _turn = zgui_profile::counter::exclusive();
    let wide = RwSignal::new_local(false);
    let mut harness = support::app_with_text(
        "root { display: block; width: 400px; height: 300px }
         .row { display: block; width: 400px; height: 10px }
         control { display: block; width: 120px; height: 10px }
         .wide control { width: 200px }",
        move |cx: &mut zgui_view::BuildCx<'_>| {
            let mut list = zgui_elements::column()
                .class("root")
                .class_toggle(zgui_view::ClassName::new("wide"), move || wide.get());
            for _ in 0..ROWS {
                list = list.child(zgui_elements::column().class("row").child(
                    zgui_elements::control().a11y(A11yBinding::new(Role::Button).label("Open")),
                ));
            }
            Box::new(list.into_view().build(cx))
        },
    );
    harness.settle(8);

    let before = zgui_profile::counter::snapshot();
    wide.set(true);
    harness.settle(8);
    let moved = before.delta(&zgui_profile::counter::snapshot());

    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(moved.a11y_remeasured, ROWS as u64, "{moved:?}");
        assert!(
            moved.a11y_projected <= 1,
            "{} nodes were projected whole for cells that only widened",
            moved.a11y_projected
        );
    }
    assert_every_update_resolves(&harness);
}

/// A font size that moved re-measures every cell and projects none of them whole.
#[test]
fn a_font_size_change_re_measures_cells_and_projects_none() {
    const ROWS: usize = 200;
    let _turn = zgui_profile::counter::exclusive();
    let size = RwSignal::new_local(14.0f32);
    let mut harness = support::app_with_text(
        "root { display: block; width: 400px; height: 300px }
         .row { display: flex; height: 22px; align-items: center }
         .c { display: block; padding: 0 6px; white-space: nowrap; overflow: hidden; width: 70px }",
        move |cx: &mut zgui_view::BuildCx<'_>| {
            let mut list = zgui_elements::column()
                .class("root")
                .style_property("font-size", move || Some(format!("{}px", size.get())));
            for index in 0..ROWS {
                list = list.child(
                    zgui_elements::row()
                        .class("row")
                        .a11y(A11yBinding::new(Role::Row))
                        .child(
                            zgui_elements::text()
                                .class("c")
                                .a11y(A11yBinding::new(Role::Cell))
                                .child(format!("cell {index}")),
                        ),
                );
            }
            Box::new(list.into_view().build(cx))
        },
    );
    harness.settle(8);

    let before = zgui_profile::counter::snapshot();
    size.set(15.0);
    harness.settle(8);
    let moved = before.delta(&zgui_profile::counter::snapshot());

    if zgui_profile::COUNTERS_ENABLED {
        assert!(
            moved.a11y_projected <= 2,
            "{} nodes were projected whole for a font size ({} re-measured); fragments rebuilt {} retired {} live {}",
            moved.a11y_projected,
            moved.a11y_remeasured,
            moved.fragments_rebuilt,
            moved.fragments_retired,
            moved.fragments_live
        );
    }
    assert_every_update_resolves(&harness);
}

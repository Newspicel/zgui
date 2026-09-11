//! What a pass that only carried the rows after a grown one arrives at, against a pass that
//! composed every box.
//!
//! A row that grows moves every row after it by exactly its growth and changes nothing else about
//! them, so the pass carries those subtrees instead of composing them again. The two are only
//! interchangeable if they produce the same fragment tree, the same clip chains and the same hit
//! answers — so that is what is asserted, by driving the same growth over two identical documents
//! and taking one of them down each path.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use support::{Element, Fixture, fragments, lay_out, lay_out_only, measurer};
use zgui_geom::{Device, DevicePx, Point};
use zgui_interned::ClassName;
use zgui_layout::fragment::diff::{DocumentMarks, Everything};
use zgui_layout::{Fragment, LayoutStore};
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};

/// The counter block is process-wide, so the cases that read it run alone.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

const ROWS: usize = 40;

/// A column of rows, each a flex row of two clipped cells holding text.
///
/// The clip is deliberate: a chain named by a rectangle that moves has to be re-interned by the
/// carrying path, which a document without one would never exercise.
fn page() -> Fixture {
    let rows: Vec<Element> = (0..ROWS)
        .map(|_| {
            Element::new("row").children(vec![
                Element::new("cell").text("alpha bravo"),
                Element::new("cell").text("delta gamma"),
            ])
        })
        .collect();
    Fixture::new(
        Element::new("root").children(vec![Element::new("column").children(rows)]),
        "root { display: block; width: 300px }
         column { display: flex; flex-direction: column }
         row { display: flex; height: 20px; flex-shrink: 0 }
         row.tall { height: 50px }
         cell { display: block; width: 100px; overflow: hidden; border-radius: 3px; padding: 2px }",
    )
}

/// One fragment's kind, border box, ink, clip chain and flags, as a value that compares.
type Row = (String, [f32; 4], [f32; 4], u32, u8);

fn snapshot(store: &LayoutStore) -> Vec<Row> {
    let root = store.root().expect("a root box");
    let mut out = Vec::new();
    for box_ in zgui_layout::fragment::stacking::paint_order(store, root) {
        for frag in store.fragments_of_box(box_) {
            let fragment = store.fragment(*frag).expect("a live fragment");
            out.push(row(fragment));
        }
    }
    out
}

fn row(fragment: &Fragment) -> Row {
    let rect = |rect: zgui_geom::Rect<DevicePx, Device>| {
        [
            rect.origin.x.0,
            rect.origin.y.0,
            rect.size.width.0,
            rect.size.height.0,
        ]
    };
    (
        format!("{:?}", fragment.kind),
        rect(fragment.border_box),
        rect(fragment.ink),
        fragment.clip.index(),
        fragment.flags.bits(),
    )
}

/// The `index`th row element of the column.
fn nth_row(fixture: &Fixture, index: usize) -> zgui_dom::NodeIndex {
    let store = fixture.document.store();
    let column = store.core(fixture.root).first_child().expect("the column");
    let mut next = store.core(column).first_child();
    for _ in 0..index {
        next = next.and_then(|node| store.core(node).next_sibling());
    }
    next.expect("the row")
}

/// Retires every obligation the build left on the document, as a frame's passes would have.
///
/// The first pass here composes everything without reading the marks, so the marks the build
/// wrote are still standing afterwards and every row would report itself owed.
fn retire_all(fixture: &mut Fixture) {
    let root = fixture.document.document_index();
    zgui_dom::dirty::walk::walk(
        fixture.document.store_mut(),
        root,
        zgui_bits::Dirty::all(),
        &mut |_, _| {},
    );
}

/// Grows row `index` by giving it the tall class, through the mutation protocol and a restyle.
fn grow(fixture: &mut Fixture, store: &mut LayoutStore, index: usize) {
    let target = nth_row(fixture, index);
    fixture.edit_restyle_and_patch(store, |edit| {
        edit.set_classes(target, &[ClassName::new("tall")]);
    });
}

/// The two documents grown at the same row, one carried and one composed.
///
/// Returns both snapshots and the hit answers for a point in every row, so a carried entry that
/// stayed where it was is a different answer.
/// One document's rows and the hit answer for a point in each of them.
type Snapshot = (Vec<Row>, Vec<Option<u32>>);

fn both(index: usize, grown: f32) -> (Snapshot, Snapshot) {
    let mut answers = Vec::new();
    for incremental in [false, true] {
        let mut fixture = page();
        let mut store = fixture.box_tree();
        let mut content = measurer();
        let mut frame = lay_out(&mut store, &mut content, 300.0, 2_000.0);
        retire_all(&mut fixture);
        grow(&mut fixture, &mut store, index);
        lay_out_only(&mut store, &mut content, 300.0, 2_000.0);
        let root = store.root().expect("a root box");
        if incremental {
            let mut dirty = DocumentMarks::for_document(&mut fixture.document);
            fragments(&mut frame, &mut store, root, &mut dirty);
        } else {
            fragments(&mut frame, &mut store, root, &mut Everything);
        }
        let hits: Vec<Option<u32>> = (0..ROWS)
            .map(|row| {
                let y = if row > index {
                    20.0 * row as f32 + grown + 10.0
                } else {
                    20.0 * row as f32 + 10.0
                };
                frame
                    .hit
                    .hit(
                        Point::new(DevicePx(10.0), DevicePx(y)),
                        &frame.clips,
                        &frame.spatial,
                    )
                    .first()
                    .map(|frag| frag.index())
            })
            .collect();
        answers.push((snapshot(&store), hits));
    }
    let carried = answers.pop().expect("both passes ran");
    let composed = answers.pop().expect("both passes ran");
    (composed, carried)
}

#[test]
fn carrying_the_rows_after_a_grown_one_lands_where_composing_them_would_have() {
    let _guard = exclusive();
    let (composed, carried) = both(20, 30.0);
    assert!(
        composed.0.len() > ROWS * 3,
        "the fixture produced {} fragments, too few for the comparison to mean anything",
        composed.0.len()
    );
    assert_eq!(
        composed.0, carried.0,
        "the pass that carried the rows and the pass that composed every box disagree"
    );
    assert_eq!(
        composed.1, carried.1,
        "a carried row answers a different fragment under the pointer than a composed one"
    );
}

#[test]
fn a_grown_row_carries_the_rows_after_it_instead_of_composing_them() {
    let _guard = exclusive();
    let mut fixture = page();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 2_000.0);
    retire_all(&mut fixture);
    grow(&mut fixture, &mut store, 20);
    lay_out_only(&mut store, &mut content, 300.0, 2_000.0);

    counter::reset();
    let root = store.root().expect("a root box");
    let mut dirty = DocumentMarks::for_document(&mut fixture.document);
    fragments(&mut frame, &mut store, root, &mut dirty);
    let rebuilt = counter::get(Counter::FragmentsRebuilt);
    let shifted = counter::get(Counter::BoxesShifted);
    let diffed = counter::get(Counter::FragmentsDiffed);

    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        rebuilt <= 8,
        "{rebuilt} fragments were composed again for one row that grew"
    );
    assert!(
        shifted >= (ROWS - 21) as u64,
        "{shifted} boxes were carried; the {} rows after the grown one should have been",
        ROWS - 21
    );
    // The rows before the grown one owe nothing and moved nowhere, and the rows after it were
    // carried: neither half is compared piece by piece.
    assert!(
        diffed <= 12,
        "{diffed} fragments were compared against their previous geometry: rows that owed \
         nothing were composed again"
    );
}

/// Every fragment's hit-order key, in painting order.
fn hit_orders(store: &LayoutStore, hit: &zgui_layout::HitIndex) -> Vec<u64> {
    let root = store.root().expect("a root box");
    let mut out = Vec::new();
    for box_ in zgui_layout::fragment::stacking::paint_order(store, root) {
        for frag in store.fragments_of_box(box_) {
            if let Some(order) = hit.order_of(*frag) {
                out.push(order);
            }
        }
    }
    out
}

/// A row born in the middle of the column takes a hit-order key between its neighbours, and the
/// index is neither rebuilt nor left disagreeing with painting order.
#[test]
fn an_inserted_row_takes_a_key_between_its_neighbours_without_a_rebuild() {
    let _guard = exclusive();
    let mut fixture = page();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 2_000.0);
    retire_all(&mut fixture);
    let before = hit_orders(&store, &frame.hit);
    assert!(
        before.windows(2).all(|pair| pair[0] < pair[1]),
        "the index's keys do not follow painting order to begin with"
    );

    let column = {
        let store = fixture.document.store();
        store.core(fixture.root).first_child().expect("the column")
    };
    let anchor = nth_row(&fixture, 20);
    fixture.edit_and_sync_boxes(&mut store, |edit| {
        let row = edit.create_element(zgui_interned::ElementName::new("row"));
        edit.set_classes(row, &[ClassName::new("tall")]);
        edit.insert_before(column, row, Some(anchor));
    });
    lay_out_only(&mut store, &mut content, 300.0, 2_000.0);

    counter::reset();
    let root = store.root().expect("a root box");
    let mut dirty = DocumentMarks::for_document(&mut fixture.document);
    fragments(&mut frame, &mut store, root, &mut dirty);
    let rebuilds = counter::get(Counter::HitIndexRebuilds);

    let after = hit_orders(&store, &frame.hit);
    assert_eq!(
        after.len(),
        before.len() + 1,
        "the born row indexed one piece"
    );
    assert!(
        after.windows(2).all(|pair| pair[0] < pair[1]),
        "the born row's key does not sort where it is painted: {after:?}"
    );
    if !COUNTERS_ENABLED {
        return;
    }
    assert_eq!(
        rebuilds, 0,
        "one row born in the middle rebuilt the whole hit index"
    );
}

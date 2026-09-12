//! That engine styles are interned rather than built per box, and that a ten-thousand-box layout
//! runs anyway.
//!
//! One engine style per box per frame would cost more than the layout it feeds, and nothing
//! about the resulting layout would look wrong. The property is therefore checked two ways: the
//! boxes of a uniform document share a handful of interned styles, and no source file in the
//! crate builds an engine style outside the one module that interns them.

mod support;

use std::path::{Path, PathBuf};

use zgui_arena::DocumentId;
use zgui_css::StyleDraft;
use zgui_layout::measure::NoContent;
use zgui_layout::node::box_node::BoxNode;
use zgui_layout::node::kind::{BoxKind, FormattingContext};
use zgui_layout::style::DeviceStyle;
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::store::LayoutStore;

/// How many boxes the fixture holds.
const BOXES: usize = 10_000;

/// How tall each of the children is, in device pixels.
const CHILD_HEIGHT: f32 = 7.0;

/// A flat block container with `BOXES - 1` block-level children, each of a fixed height.
///
/// The height is what makes the layout assertable: children with no height of their own all stack
/// at the same origin, and every "was it laid out" assertion over them is satisfied by a pass that
/// stopped after the first child.
fn fixture() -> LayoutStore {
    let mut store = LayoutStore::new(DocumentId::FIRST);
    let root_style = StyleDraft::initial().build();
    let style = {
        let mut draft = StyleDraft::initial();
        draft.position_group().height = zgui_css::values::size::SizeValue::LengthPercentage(
            zgui_css::values::length::NonNegative(
                zgui_css::values::length::LengthPercentage::new_length(
                    zgui_css::values::length::Length::new(CHILD_HEIGHT),
                ),
            ),
        );
        draft.build()
    };
    let root = store.insert(BoxNode::new(
        root_style,
        BoxKind::Element,
        FormattingContext::Block,
    ));
    store.get_mut(root).expect("live").block_level = true;
    let mut children = Vec::with_capacity(BOXES - 1);
    for _ in 0..BOXES - 1 {
        let child = store.insert(BoxNode::new(
            style.clone(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        let node = store.get_mut(child).expect("live");
        node.block_level = true;
        node.parent = Some(root);
        children.push(child);
    }
    let node = store.get_mut(root).expect("live");
    node.children = children.clone();
    node.paint_children = children;
    store.set_root(root);
    store
}

#[test]
fn a_ten_thousand_box_layout_runs_and_places_every_box() {
    let mut store = fixture();
    let mut content = NoContent;
    {
        let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
        assert!(tree.layout_viewport(1000.0, 800.0));
    }
    let root = store.root().expect("a root");
    assert_eq!(store.node(root).children.len(), BOXES - 1);
    // Every box was laid out, and each sits one child-height below the one before it. A layout that
    // stopped early — or one that never placed anything — leaves the tail at the origin, which is a
    // position a non-negativity check cannot tell apart from a real one.
    for (index, &child) in store.node(root).children.iter().enumerate() {
        let layout = store.layout_of(child).expect("laid out");
        #[expect(clippy::cast_precision_loss, reason = "ten thousand is exact in f32")]
        let expected = index as f32 * CHILD_HEIGHT;
        assert_eq!(layout.origin.y.0, expected, "child {index} sits wrong");
        assert_eq!(layout.size.height.0, CHILD_HEIGHT);
        assert_eq!(layout.size.width.0, 1000.0, "child {index} did not stretch");
    }
    assert_eq!(
        store.layout_of(root).expect("laid out").size.width.0,
        1000.0
    );
}

#[test]
fn a_uniform_document_shares_a_handful_of_engine_styles() {
    let mut store = fixture();
    let mut content = NoContent;
    {
        let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
        assert!(tree.layout_viewport(1000.0, 800.0));
    }
    // The root and the children: two cascade results, two variants, two entries. A store that
    // built one entry per box would hold ten thousand.
    assert!(
        store.interned_engine_styles() <= 4,
        "{} engine styles for {BOXES} boxes",
        store.interned_engine_styles()
    );
}

#[test]
fn no_source_file_in_this_crate_builds_an_engine_style() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    collect(&root, &mut sources);
    assert!(
        sources.len() > 20,
        "found only {} source files, so the scan covers nothing",
        sources.len()
    );

    let mut offenders = Vec::new();
    let mut saw_engine = false;
    for path in &sources {
        let text = std::fs::read_to_string(path).expect("a readable source file");
        if text.contains("cephal::") {
            saw_engine = true;
        }
        // The one module that builds engine styles is the template builder; everything else
        // borrows what the store interned from it. The interner's own tests build a default.
        let builds = path.ends_with("style/engine/mod.rs") || path.ends_with("engine_styles.rs");
        // A struct literal or the default is how one is built; a return type is not.
        let literal = text.split("cephal::Style {").skip(1).any(|_| true)
            && text
                .lines()
                .any(|line| line.contains("cephal::Style {") && !line.contains("-> &"));
        if !builds && (literal || text.contains("Style::DEFAULT")) {
            offenders.push(format!("{} builds an engine style", path.display()));
        }
    }
    // The control: if nothing in the crate mentioned the layout engine at all, the scan above
    // would pass while reading the wrong files.
    assert!(saw_engine, "no source file mentions the layout engine");
    assert!(offenders.is_empty(), "{offenders:#?}");
}

/// Every `.rs` file below `directory`.
fn collect(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a readable directory") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

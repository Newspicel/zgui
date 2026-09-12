//! Marking boxes for the next layout pass.
//!
//! A mark clears the box's cache and queues the box and its parent with the scheduler, which
//! recomputes them at the next pass and follows changed answers upward from there: an ancestor
//! whose cached answers still hold is left alone. Beside the scheduler's bits, a chain bit on
//! every ancestor keeps dirtiness readable from the root for the walks that stop at a clean box.

use zgui_dom::side::BoxKey;
use zgui_profile::{Counter, counter};

use crate::key::to_node_id;
use crate::tree::marker::Marker;
use crate::tree::store::LayoutStore;

/// The bit on a box's flags saying a box below it is queued: dirtiness as seen from the root.
///
/// The scheduler's own bits mark what it recomputes; this one keeps the old upward-closed
/// reading of "dirty" for the walks that start at the root and stop at a clean box. The pass
/// clears it from every box it was set on.
pub(crate) const CHAIN: u8 = 0x10;

/// Marks `box_` dirty: it is recomputed at the next pass, and so is its parent, which reads its
/// style and its answer. The pass then follows changed answers upward on its own.
///
/// Returns how many boxes were newly reached, the ancestor chain included.
pub fn mark_dirty(store: &mut LayoutStore, box_: BoxKey) -> u32 {
    if store.state(box_).is_none() {
        return 0;
    }
    let mut marked = 0;
    let mut scheduler = core::mem::take(&mut store.scheduler);
    let parent = store.get(box_).and_then(|node| node.parent);
    for key in [Some(box_), parent].into_iter().flatten() {
        if store.state(key).is_none() {
            continue;
        }
        store.state_mut(key).forget_layout();
        scheduler.mark(&mut Marker(store), to_node_id(key));
    }
    store.scheduler = scheduler;
    // The chain above: an intrinsic measurement taken over this box is stale, and a walk from
    // the root has to find its way down here.
    let mut next = Some(box_);
    while let Some(key) = next {
        let Some(state) = store.state(key) else {
            break;
        };
        if state.flags & CHAIN != 0 {
            break;
        }
        let state = store.state_mut(key);
        state.flags |= CHAIN;
        state.intrinsic = [None, None];
        if let Some(answers) = state.atomic.as_deref_mut() {
            answers.clear();
        }
        store.chain.push(key);
        marked += 1;
        next = store.get(key).and_then(|node| node.parent);
    }
    marked
}

/// Clears the chain bit the pass walked, after the pass.
pub(crate) fn clear_chain(store: &mut LayoutStore) {
    let chain = core::mem::take(&mut store.chain);
    for key in chain {
        if store.state(key).is_some() {
            store.state_mut(key).flags &= !CHAIN;
        }
    }
}

/// Whether `box_` is holding no layout answer, or is queued for one.
pub fn is_dirty(store: &LayoutStore, box_: BoxKey) -> bool {
    store.state(box_).is_none_or(|state| {
        state.holds_no_layout() || state.flags & (cephal::schedule::DIRTY | CHAIN) != 0
    })
}

/// Marks every box dirty, as after a change to the device scale.
///
/// Returns how many boxes held something to forget.
pub fn mark_all_dirty(store: &mut LayoutStore) -> u32 {
    // The viewport the held results belong to goes with them. A scale change leaves the surface the
    // same number of device pixels across while making every length inside it mean something else,
    // so a pass that compared viewports alone would find them equal and hold a document laid out at
    // the previous ratio.
    store.forget_root_layout();
    let keys = store.keys();
    let mut marked = 0;
    for key in keys {
        let held = store.state(key).is_none_or(|state| {
            !state.holds_no_layout()
                || state.first_baseline.is_some()
                || state.last_baseline.is_some()
                || state.inline.is_some()
        });
        if !held {
            continue;
        }
        store.take_inline_resolution(key);
        let state = store.state_mut(key);
        state.forget_layout();
        state.first_baseline = None;
        state.last_baseline = None;
        marked += 1;
    }
    if let Some(root) = store.root() {
        let mut scheduler = core::mem::take(&mut store.scheduler);
        scheduler.mark(&mut Marker(store), to_node_id(root));
        store.scheduler = scheduler;
    }
    counter::add(Counter::BoxesMarkedAllDirty, u64::from(marked));
    marked
}

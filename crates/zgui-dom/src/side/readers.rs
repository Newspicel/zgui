//! Which custom properties an element's subtree reads.
//!
//! A custom property that changes on an element reaches its descendants through `var()`, and only
//! through `var()`. Handing the change down costs a visit per descendant unless something says
//! which subtrees hold a reader at all — and that is this column: a small Bloom set of the names
//! the element's own rules read, and the union of the same over everything below it. A traversal
//! that changed `--accent` tests a child's union against the change and skips the subtree when
//! nothing in it can be listening.
//!
//! The union is a superset of the truth and never a subset. A subtree the set names as a reader
//! may hold none — a Bloom set answers "maybe" — and the traversal then does what it did before.
//! A subtree it does not name holds none, which rests on the union being kept current: an
//! element's own set is written whenever it is styled, and an ancestor whose own set or child list
//! moved is marked stale and recomputed before the next traversal reads it.

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;
use crate::node::kind::NodeKind;

/// The set that names every custom property.
///
/// What an element answers before it has ever been styled, what a declarer of custom properties
/// answers — it rebuilds its map from any change above it — and what a change too wide to name
/// tests against.
pub const ALL: u64 = u64::MAX;

/// One element's readers, own and below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Readers {
    /// The names the element's own rule chains read.
    pub own: u64,
    /// The union of `own` over the element and everything below it, as of the last settle.
    pub below: u64,
    /// Whether `below` has to be computed again: the element's own set or its child list moved.
    pub stale: bool,
}

/// The Bloom bits of one custom property name, without its `--` prefix.
///
/// Two bits out of sixty-four, so that a list of a few hundred distinct names still leaves most
/// of the word clear. The same function is used to build a set and to test one, which is the whole
/// of what makes the test sound.
pub fn name_bits(name: &str) -> u64 {
    use core::hash::{BuildHasher, Hasher};
    let mut hasher = rustc_hash::FxBuildHasher.build_hasher();
    hasher.write(name.as_bytes());
    let hash = hasher.finish();
    (1u64 << (hash & 63)) | (1u64 << ((hash >> 8) & 63))
}

/// The union of names read at or below `index`, as of the last settle.
///
/// An element nothing has written for answers [`ALL`]: it has never been styled, so nothing is
/// known about it and everything has to be assumed.
pub fn below(store: &DocumentStore, index: NodeIndex) -> u64 {
    store
        .columns()
        .readers
        .get(store.key_of(index))
        .map_or(ALL, |readers| readers.below)
}

/// Records the names `index`'s own rules read, marking its ancestors stale if that moved.
pub fn set_own(store: &mut DocumentStore, index: NodeIndex, own: u64) {
    let key = store.key_of(index);
    let held = store.columns().readers.get(key).copied();
    if held.is_some_and(|held| held.own == own) {
        return;
    }
    let below = held.map_or(ALL, |held| held.below);
    store.columns_mut().readers.insert(
        key,
        Readers {
            own,
            below,
            stale: true,
        },
    );
    mark_stale_above(store, index);
}

/// Marks `index` and every ancestor stale, stopping at the first that already is.
///
/// A child list that changed is a union that changed, so the node the list belongs to is marked
/// as well as everything above it. The climb stops at a stale ancestor because everything above
/// that one was marked when it was.
pub fn mark_stale(store: &mut DocumentStore, index: NodeIndex) {
    if !mark_one_stale(store, index) {
        return;
    }
    mark_stale_above(store, index);
}

/// Marks every ancestor of `index` stale, stopping at the first that already is.
fn mark_stale_above(store: &mut DocumentStore, index: NodeIndex) {
    let mut current = store.core(index).parent();
    while let Some(parent) = current {
        if !mark_one_stale(store, parent) {
            return;
        }
        current = store.core(parent).parent();
    }
}

/// Marks one node stale, answering whether it was not already.
///
/// A node nothing has written for gets an entry: an element assumes everything until its first
/// cascade says otherwise, and the document node reads nothing of its own.
fn mark_one_stale(store: &mut DocumentStore, index: NodeIndex) -> bool {
    let key = store.key_of(index);
    let kind = store.core(index).kind();
    match store.columns_mut().readers.get_mut(key) {
        Some(readers) => {
            if readers.stale {
                return false;
            }
            readers.stale = true;
            true
        }
        None => {
            let own = if kind == NodeKind::Element { ALL } else { 0 };
            store.columns_mut().readers.insert(
                key,
                Readers {
                    own,
                    below: ALL,
                    stale: true,
                },
            );
            true
        }
    }
}

/// Recomputes `below` for every stale node, from the document node down.
///
/// Descends only into stale entries: a frame that restyled nothing and moved no child costs the
/// document node's own test and nothing more, and a class toggled on one row costs the rows'
/// parent one pass over its children.
pub fn settle(store: &mut DocumentStore, document: NodeIndex) {
    settle_at(store, document);
}

/// Recomputes `below` for `index` if it is stale, and answers it either way.
fn settle_at(store: &mut DocumentStore, index: NodeIndex) -> u64 {
    let key = store.key_of(index);
    let Some(readers) = store.columns().readers.get(key).copied() else {
        return ALL;
    };
    if !readers.stale {
        return readers.below;
    }
    let mut below = readers.own;
    let mut next = store.core(index).first_child();
    while let Some(child) = next {
        next = store.core(child).next_sibling();
        if store.core(child).kind() != NodeKind::Element {
            continue;
        }
        below |= settle_at(store, child);
    }
    store.columns_mut().readers.insert(
        key,
        Readers {
            own: readers.own,
            below,
            stale: false,
        },
    );
    below
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use super::{ALL, below, mark_stale, name_bits, set_own, settle};
    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_name_sets_two_bits_and_the_same_two_every_time() {
        let bits = name_bits("accent");
        assert!(bits.count_ones() <= 2 && bits != 0);
        assert_eq!(bits, name_bits("accent"));
        assert_ne!(bits, name_bits("note-w"));
    }

    #[test]
    fn an_unstyled_element_reads_everything_and_a_settled_one_reads_what_its_subtree_does() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let row = document.append(root, NodeKind::Element, ElementName::new("row"));
        let cell = document.append(row, NodeKind::Element, ElementName::new("cell"));
        let other = document.append(root, NodeKind::Element, ElementName::new("row"));
        assert_eq!(below(document.store(), row), ALL);

        let accent = name_bits("accent");
        set_own(document.store_mut(), root, 0);
        set_own(document.store_mut(), row, 0);
        set_own(document.store_mut(), cell, accent);
        set_own(document.store_mut(), other, 0);
        let index = document.document_index();
        settle(document.store_mut(), index);

        assert_eq!(below(document.store(), cell), accent);
        assert_eq!(
            below(document.store(), row),
            accent,
            "a reader below is a reader"
        );
        assert_eq!(
            below(document.store(), other),
            0,
            "a subtree with no reader says so"
        );
        assert_eq!(below(document.store(), root), accent);

        // The reader stops reading: the union above it is recomputed on the next settle.
        set_own(document.store_mut(), cell, 0);
        settle(document.store_mut(), index);
        assert_eq!(below(document.store(), row), 0);

        // A child list that moved is a union that may have moved.
        mark_stale(document.store_mut(), root);
        set_own(document.store_mut(), other, accent);
        settle(document.store_mut(), index);
        assert_eq!(below(document.store(), root), accent);
    }
}

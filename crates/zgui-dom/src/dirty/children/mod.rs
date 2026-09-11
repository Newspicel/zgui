//! Which of a node's children owe work.
//!
//! Skipping a clean *subtree* is what the invalidation word does, and it is O(1) per subtree. That
//! is only half the bound a phase walk needs: a node with ten thousand children and one dirty one
//! still probes ten thousand invalidation words unless something narrows the child iteration too.
//! This record is that something.
//!
//! **Children are recorded by identity, never by position.** Positions among element siblings are
//! numbered lazily, so a structural change can invalidate every position in a child list *after* a
//! mark was recorded and *before* a walk reads it — a walk keyed by position would then descend
//! into the wrong child and the marked one would never be serviced. Identity also costs less:
//! resolving "the five-hundredth child" means following five hundred links, because a child list is
//! a chain and there is no position-to-node map anywhere.
//!
//! **Four children live in the record and the rest live in the document.** The commonest frame
//! there is moves a pointer, which clears a state bit on one row and sets it on another far away,
//! and four slots on the node cover that, focus-out/focus-in, an edge pair and a single insertion.
//! The fifth distinct child moves the set to the document's overflow table, keyed by the owner,
//! where it grows by identity: a mark costs one insertion whatever the width of the child list
//! and wherever the child sits in it, and a walk visits exactly the marked children. Past
//! [`OVERFLOW_CAP`] entries the record degrades to naming every child, which is the point at
//! which the set costs more than the probes it saves.
//!
//! **The record is a superset of the truth and never a subset.** A child that was marked and has
//! since left the owner is filtered out when the record is read, by the parent test in `iter`; a
//! child that is still there is always reached. Nothing here needs repairing when a child is
//! unlinked, because nothing here describes the child list's shape.
//!
//! | Module | Contents |
//! |---|---|
//! | `repr` | the four slots and the tag that says how to read them |
//! | `widen` | recording a mark, and moving to the overflow set |
//! | `iter` | reading the record back as the children it names |

mod iter;
mod repr;
mod widen;

#[cfg(test)]
mod tests;

use core::cell::Cell;

use crate::arena::store::DocumentStore;
use crate::dirty::children::repr::Repr;
use crate::id::node_key::NodeIndex;

pub use crate::dirty::children::repr::EXACT;

/// How many children the overflow set names before the record degrades to every child.
pub const OVERFLOW_CAP: usize = 4096;

/// Which of a node's children owe work.
///
/// Widened when a child is marked and rebuilt when a walk unwinds, so a walk descends only into
/// the children that have work rather than into all of them.
///
/// Written between frames, under an exclusive borrow of the document, which is what lets it be a
/// plain cell of copyable data next to fields that need atomics. Reading it during a traversal is
/// a shared read of memory nobody is writing.
#[derive(Debug)]
#[repr(transparent)]
pub struct DirtyChildren(Cell<Repr>);

impl DirtyChildren {
    /// A record naming no children.
    pub const fn empty() -> Self {
        Self(Cell::new(Repr::EMPTY))
    }

    /// Whether this record names no children.
    pub fn is_empty(&self) -> bool {
        let repr = self.0.get();
        !repr.is_wide() && repr.len == 0
    }

    /// Whether this record has left its own slots for the overflow set, or names every child.
    pub fn is_wide(&self) -> bool {
        self.0.get().is_wide()
    }

    /// Whether this record names every child of its owner.
    pub fn is_all(&self) -> bool {
        self.0.get().len == Repr::ALL
    }

    /// How many children the record names in its own slots, or [`None`] once it is wide.
    pub fn exact_len(&self) -> Option<usize> {
        let repr = self.0.get();
        (!repr.is_wide()).then_some(repr.len as usize)
    }

    /// Forgets every child named in the record's own slots.
    ///
    /// An overflow set the record had moved to is left to the next [`DirtyChildren::widen`] that
    /// moves there again, or to [`DirtyChildren::replace`], which has the store to release it.
    pub fn clear(&self) {
        self.0.set(Repr::EMPTY);
    }

    /// Replaces the record with exactly `children`, moving to the overflow set past the fourth.
    ///
    /// This is how a walk rebuilds the record as it unwinds, from the children it found still
    /// owing work. `owner` is the node whose record this is, and children it no longer parents are
    /// dropped rather than recorded — a callback that reparented a child mid-walk leaves exactly
    /// that.
    ///
    /// # Panics
    ///
    /// Panics if any of `children` names no live node of `store`.
    pub fn replace(
        &self,
        owner: NodeIndex,
        children: impl IntoIterator<Item = NodeIndex>,
        store: &DocumentStore,
    ) {
        if self.is_wide() {
            store.dirty_overflow().remove(&owner);
        }
        self.clear();
        for child in children {
            self.widen(owner, child, store);
        }
    }
}

impl Default for DirtyChildren {
    fn default() -> Self {
        Self::empty()
    }
}

// SAFETY: shape 2 — one cell of plain `Copy` data. `Cell::get` is a load and `Cell::set` a store,
// and the record is written only between frames, under an exclusive borrow of the document.
unsafe impl crate::node::discipline::CellDisciplined for DirtyChildren {}

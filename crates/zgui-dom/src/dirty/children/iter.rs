//! Reading the record back as the children it names.

use smallvec::SmallVec;

use crate::arena::store::DocumentStore;
use crate::dirty::children::DirtyChildren;
use crate::dirty::children::repr::{EXACT, Repr};
use crate::id::node_key::NodeIndex;

impl DirtyChildren {
    /// The children this record names, skipping any that `owner` no longer parents.
    ///
    /// The parent test is what makes a stale entry harmless: a child removed or reparented since
    /// the mark is simply not yielded, so nothing downstream has to check whether a record it was
    /// handed still describes the tree. A child whose slot has been reclaimed outright — a record
    /// held across the end of a frame — is dropped on the same grounds rather than panicking. The
    /// order is unspecified.
    pub fn iter<'doc>(
        &self,
        store: &'doc DocumentStore,
        owner: NodeIndex,
    ) -> impl Iterator<Item = NodeIndex> + 'doc {
        let repr = self.0.get();
        let mut named: SmallVec<[NodeIndex; EXACT]> = SmallVec::new();
        match repr.len {
            Repr::WIDE => {
                if let Some(set) = store.dirty_overflow().get(&owner) {
                    named.extend(set.iter().copied());
                }
            }
            Repr::ALL => {
                let mut next = store
                    .try_core(owner)
                    .and_then(|record| record.first_child());
                while let Some(child) = next {
                    named.push(child);
                    next = store
                        .try_core(child)
                        .and_then(|record| record.next_sibling());
                }
            }
            used => named.extend(
                repr.slots[..used as usize]
                    .iter()
                    .filter_map(|slot| slot.get()),
            ),
        }
        // A child the arena has recycled resolves to nothing and is dropped here rather than
        // panicking a walk that was handed a record older than the frame it is reading.
        named.into_iter().filter(move |child| {
            store
                .try_core(*child)
                .is_some_and(|record| record.parent() == Some(owner))
        })
    }
}

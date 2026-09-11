//! Recording a mark, and moving to the overflow set.

use crate::arena::store::DocumentStore;
use crate::dirty::children::repr::{EXACT, Repr};
use crate::dirty::children::{DirtyChildren, OVERFLOW_CAP};
use crate::id::node_key::{NodeIndex, OptIndex};

impl DirtyChildren {
    /// Adds `child` to the marked set, moving to the overflow set on the fifth distinct entry.
    ///
    /// `owner` is the node whose record this is. A child `owner` does not parent is ignored, so a
    /// mark that arrives after the child has been moved away records nothing under a parent it
    /// does not belong to.
    ///
    /// Adding a child that is already named changes nothing, which is what keeps repeated marks on
    /// one row from moving a record that only ever named one child.
    ///
    /// # Panics
    ///
    /// Panics if `child` names no live node of `store`.
    pub fn widen(&self, owner: NodeIndex, child: NodeIndex, store: &DocumentStore) {
        if store.core(child).parent() != Some(owner) {
            return;
        }
        let repr = self.0.get();
        match repr.len {
            Repr::ALL => {}
            Repr::WIDE => {
                let mut overflow = store.dirty_overflow();
                let Some(set) = overflow.get_mut(&owner) else {
                    // The set went with a `clear`; the record starts again from this child.
                    drop(overflow);
                    self.0.set(Repr::EMPTY);
                    self.widen(owner, child, store);
                    return;
                };
                set.insert(child);
                if set.len() > OVERFLOW_CAP {
                    overflow.remove(&owner);
                    self.0.set(Repr {
                        slots: [OptIndex::NONE; EXACT],
                        len: Repr::ALL,
                    });
                }
            }
            used => {
                let used = used as usize;
                if repr.slots[..used].contains(&OptIndex::some(child)) {
                    return;
                }
                if used < EXACT {
                    let mut slots = repr.slots;
                    slots[used] = OptIndex::some(child);
                    self.0.set(Repr {
                        slots,
                        len: repr.len + 1,
                    });
                    return;
                }
                let mut overflow = store.dirty_overflow();
                let set = overflow.entry(owner).or_default();
                set.clear();
                set.extend(repr.slots.iter().filter_map(|slot| slot.get()));
                set.insert(child);
                self.0.set(Repr {
                    slots: [OptIndex::NONE; EXACT],
                    len: Repr::WIDE,
                });
            }
        }
    }
}

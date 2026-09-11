//! The stored form of the record: four slots and a tag saying how to read them.

use crate::id::node_key::OptIndex;
use crate::plain_data;

/// How many children the record names in its own slots before it moves to the document's set.
pub const EXACT: usize = 4;

/// The stored form: four slots, and a tag saying how to read them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) struct Repr {
    /// The marked children, when `len` counts them.
    pub(super) slots: [OptIndex; EXACT],
    /// How many of `slots` are in use, or one of the two tags.
    pub(super) len: u32,
}

impl Repr {
    /// The tag that says the children are in the document's overflow set, under the owner.
    pub(super) const WIDE: u32 = u32::MAX;

    /// The tag that says every child of the owner is named.
    pub(super) const ALL: u32 = u32::MAX - 1;

    /// Nothing marked.
    pub(super) const EMPTY: Self = Self {
        slots: [OptIndex::NONE; EXACT],
        len: 0,
    };

    /// Whether the tag says the children are not in the slots.
    pub(super) fn is_wide(self) -> bool {
        self.len == Self::WIDE || self.len == Self::ALL
    }
}

plain_data!(Repr);

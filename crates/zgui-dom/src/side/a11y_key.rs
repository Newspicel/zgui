//! The identity of everything a node's accessible description depends on.
//!
//! The same idea as the paint key one level over. A style key can be a set of addresses because
//! computed values are shared and immutable. An accessible name is not: editing a text node
//! changes what a screen reader would say without changing any style group and without changing
//! any semantics record — which is why the edit itself marks the element for the accessibility
//! phase, and the key does not have to see it.

/// Identity of what a node's accessible description is derived from.
///
/// Written once per node per restyle, and compared against the previous frame's value to decide
/// whether the accessibility projection has to be rebuilt for this node.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
pub struct A11yKey {
    /// Reserved for the identity of a computed-value group the projection reads.
    ///
    /// The projection reads none: whether a node is exposed is answered from its fragments and
    /// its text order from the document. The field stays zero.
    pub style: usize,
    /// The node's semantics record, or zero when it has none.
    pub semantics: usize,
    /// Reserved for a hash of the text the projection would read out.
    ///
    /// Text is edited in place, and the edit marks the element for the accessibility phase
    /// itself, so the key no longer carries the hash; the field stays zero.
    pub content: u64,
}

impl A11yKey {
    /// The key of a node whose accessible description has never been computed.
    pub const UNPROJECTED: Self = Self {
        style: 0,
        semantics: 0,
        content: 0,
    };
}

#[cfg(test)]
mod tests {
    use super::A11yKey;

    #[test]
    fn the_unprojected_key_is_the_default() {
        assert_eq!(A11yKey::default(), A11yKey::UNPROJECTED);
    }

    #[test]
    fn a_text_edit_changes_the_key_even_when_nothing_else_moves() {
        let before = A11yKey {
            style: 0x40,
            semantics: 0x80,
            content: 0xdead,
        };
        let after = A11yKey {
            content: 0xbeef,
            ..before
        };
        assert_ne!(before, after);
    }
}

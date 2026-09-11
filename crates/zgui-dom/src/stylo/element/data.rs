//! Establishing, clearing and borrowing the engine's per-element data.
//!
//! The data lives inline in every record rather than behind a pointer, which costs the same bytes
//! on every node and saves an allocation and an indirection on every styled one. It also removes the
//! obvious answer to "does this element have data?": the storage is always there, so presence
//! answers nothing. The engine bounds one of its subtree-clearing walks on exactly that question, so
//! the real answer is a bit written by the same two calls that establish and clear the data — one
//! storage, and the two answers cannot drift apart because neither is written without the other.

use style::data::{ElementData, ElementDataMut, ElementDataRef};

use crate::node::atomics;
use crate::node::handle::Node;

impl<'doc> Node<'doc> {
    /// Establishes this element's style data and borrows it for writing.
    ///
    /// Called by the style engine for the element a worker currently owns, which is what makes the
    /// exclusive borrow underneath sound.
    pub fn ensure_style_data(self) -> ElementDataMut<'doc> {
        self.record().set_atomic(atomics::STYLED);
        self.record().data().borrow_mut()
    }

    /// Establishes this element's style data and keeps no borrow of it.
    ///
    /// What a caller wants when it needs the *storage* to exist and has nothing to write into it —
    /// an element the engine has not styled yet answers "has no data", and some questions are only
    /// asked of an element that has some. The borrow ends inside this call, so a caller does not
    /// have to end one it never wanted.
    pub fn establish_style_data(self) {
        let _data = self.ensure_style_data();
    }

    /// Resets this element's style data and records that it has none.
    ///
    /// Both halves happen here, in this order, because a cleared bit with stale data behind it makes
    /// the next cascade read a style nothing computed.
    pub fn clear_style_data(self) {
        *self.record().data().borrow_mut() = ElementData::default();
        self.record().clear_atomic(atomics::STYLED);
    }

    /// Clears the style data of everything below this element, leaving its own alone.
    ///
    /// What an element restyled to `display: none` owes its subtree: nothing below it has a style
    /// any more, and a style left behind would be read as one. Walked with a list rather than by
    /// recursion, because a worker's stack is small. An element without data has no descendant
    /// with data, which is the engine's own invariant and what bounds the walk.
    pub fn clear_style_data_below(self) {
        use style::dom::{TElement, TNode};
        let mut parents: Vec<Node<'doc>> = vec![self];
        while let Some(parent) = parents.pop() {
            for kid in parent.traversal_children() {
                if let Some(child) = kid.as_element()
                    && child.is_styled()
                {
                    child.clear_style_data();
                    parents.push(child);
                }
            }
        }
        self.clear_animation_work_below();
    }

    /// This element's style data, for reading, or [`None`] if it has none.
    pub fn borrow_style_data(self) -> Option<ElementDataRef<'doc>> {
        self.is_styled().then(|| self.record().data().borrow())
    }

    /// This element's style data, for writing, or [`None`] if it has none.
    pub fn mutate_style_data(self) -> Option<ElementDataMut<'doc>> {
        self.is_styled().then(|| self.record().data().borrow_mut())
    }
}

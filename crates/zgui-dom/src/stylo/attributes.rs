//! Attribute lookup, for the two callers that ask by name.
//!
//! Selector matching asks by name and compares the value itself; a container query asks by name and
//! wants the value as a string it can parse. Both go through the one lookup here, so an element's
//! attributes cannot answer one of them and not the other.
//!
//! `id` and `class` are not in the table [`Node::attr`] reads. They live in the node record — as a
//! copyable identifier handle and as a span into the document's class pool — because matching asks
//! about them far more often than about anything else, and neither answer should cost a column
//! lookup. [`Node::attr_text`] answers for them too, so `[class*=card]` and `attr(id)` read the
//! same values `.card` and `#id` do.

use std::borrow::Cow;

use style::dom::AttributeProvider;
use zgui_vocab::SharedString;

use crate::node::handle::Node;
use crate::side::attrs::Attr;

impl<'doc> Node<'doc> {
    /// This node's attributes other than `id` and `class`, in the order they were set.
    pub fn attrs(self) -> impl Iterator<Item = &'doc Attr> {
        self.store()
            .columns()
            .attrs
            .get(self.key())
            .and_then(Option::as_deref)
            .into_iter()
            .flat_map(crate::side::attrs::AttrMap::iter)
    }

    /// The value of the attribute called `name`, matched as text.
    pub fn attr(self, name: &str) -> Option<&'doc SharedString> {
        self.store()
            .columns()
            .attrs
            .get(self.key())
            .and_then(Option::as_deref)
            .and_then(|attrs| attrs.get_by_str(name))
    }

    /// The text of the attribute called `name`, `id` and `class` included.
    ///
    /// The class list is its names joined by single spaces, in the order they were set.
    pub fn attr_text(self, name: &str) -> Option<Cow<'doc, str>> {
        match name {
            "class" => {
                let classes = self.store().classes_of(self.index());
                (!classes.is_empty()).then(|| {
                    let names: Vec<&str> = classes.iter().map(|class| class.as_ref()).collect();
                    Cow::Owned(names.join(" "))
                })
            }
            "id" => self
                .record()
                .id_attr()
                .and_then(|ident| self.store().idents().resolve(ident))
                .map(|id| Cow::Owned(id.to_string())),
            _ => self.attr(name).map(|value| Cow::Borrowed(value.as_str())),
        }
    }
}

impl AttributeProvider for Node<'_> {
    /// The value of one attribute, as an owned string.
    ///
    /// The namespace is ignored because attributes in this document are unqualified: an attribute
    /// name is written once, by whoever set it, and there is no syntax anywhere above this crate
    /// that produces a prefixed one.
    fn get_attr(&self, attr: &style::LocalName, _namespace: &style::Namespace) -> Option<String> {
        self.attr_text(&attr.0).map(Cow::into_owned)
    }
}

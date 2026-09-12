//! Grid line and area names, as the small integers the incremental engine carries them by.
//!
//! The engine resolves `<area>-start` and `<area>-end` by asking for the suffixed name. The
//! answer is looked up here in constant time: every name interned records the suffixed names
//! that already exist, and a later name that is itself a suffixed form is filed back onto its base.
//! Names are never released; a document names a bounded number of lines.

use cephal::tree::IdentSuffix;
use rustc_hash::FxHashMap;
use zgui_interned::Ident;

/// The engine's handles for the names a document's grids use.
#[derive(Debug, Default)]
pub(crate) struct IdentTable {
    /// The engine handle of every name interned.
    by_name: FxHashMap<Ident, cephal::style::Ident>,
    /// Per handle: the handles of `<name>-start` and `<name>-end`, if those names were interned.
    suffixed: Vec<[Option<cephal::style::Ident>; 2]>,
}

impl IdentTable {
    /// The handle for one name, issued on first sight.
    pub(crate) fn intern(&mut self, name: Ident) -> cephal::style::Ident {
        if let Some(&id) = self.by_name.get(&name) {
            return id;
        }
        let id = cephal::style::Ident(u32::try_from(self.suffixed.len()).expect("fewer names"));
        let text = name.as_str();
        let mut slots = [None, None];
        for (slot, suffix) in [IdentSuffix::Start, IdentSuffix::End]
            .into_iter()
            .enumerate()
        {
            let candidate = Ident::new(&format!("{text}{}", suffix.as_str()));
            slots[slot] = self.by_name.get(&candidate).copied();
        }
        self.suffixed.push(slots);
        self.by_name.insert(name, id);
        for (slot, suffix) in [IdentSuffix::Start, IdentSuffix::End]
            .into_iter()
            .enumerate()
        {
            if let Some(base) = text.strip_suffix(suffix.as_str())
                && let Some(&base_id) = self.by_name.get(&Ident::new(base))
            {
                self.suffixed[base_id.0 as usize][slot] = Some(id);
            }
        }
        id
    }

    /// The handle of `<base>-start` or `<base>-end`, if such a name was interned.
    pub(crate) fn suffixed(
        &self,
        base: cephal::style::Ident,
        suffix: IdentSuffix,
    ) -> Option<cephal::style::Ident> {
        let slot = match suffix {
            IdentSuffix::Start => 0,
            IdentSuffix::End => 1,
        };
        self.suffixed.get(base.0 as usize)?[slot]
    }

    /// How many names are interned.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.suffixed.len()
    }
}

#[cfg(test)]
mod tests {
    use cephal::tree::IdentSuffix;
    use zgui_interned::Ident;

    use super::IdentTable;

    #[test]
    fn a_name_is_one_handle_however_often_it_is_interned() {
        let mut table = IdentTable::default();
        let first = table.intern(Ident::new("main"));
        let again = table.intern(Ident::new("main"));
        assert_eq!(first, again);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn a_suffixed_name_is_found_whichever_side_was_interned_first() {
        let mut table = IdentTable::default();
        let start = table.intern(Ident::new("main-start"));
        let main = table.intern(Ident::new("main"));
        let end = table.intern(Ident::new("main-end"));
        assert_eq!(table.suffixed(main, IdentSuffix::Start), Some(start));
        assert_eq!(table.suffixed(main, IdentSuffix::End), Some(end));
        assert_eq!(table.suffixed(start, IdentSuffix::Start), None);
    }
}

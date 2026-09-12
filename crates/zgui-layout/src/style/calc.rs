//! `calc()` expressions, and the identifiers the layout engine carries them by.
//!
//! The engine holds a `calc()` as a small integer and hands it back when it knows the percentage
//! basis; the expression itself stays here, evaluated by the style engine's own arithmetic.

use cephal::style::CalcId;
use zgui_css::values::length::{Length, LengthPercentage};

/// Where a conversion may intern a `calc()` it has no other representation for.
///
/// A trait rather than the table itself so a test can supply its own store, and so the conversions
/// stay the single statement of how a CSS value becomes an engine one whoever calls them.
pub(crate) trait InternCalc {
    /// The identifier for one expression.
    fn intern_calc_id(&mut self, value: &LengthPercentage) -> CalcId;
}

/// The `calc()` expressions the interned style lowerings refer to.
///
/// Entries are owned: each lowering records the identifiers it interned and gives them back when it
/// is replaced or dropped, so a handle embedded in a lowering stays meaningful for exactly as long
/// as the lowering itself.
#[derive(Debug, Default)]
pub(crate) struct CalcTable {
    /// The expressions, by identifier.
    exprs: Vec<Option<LengthPercentage>>,
    /// Identifiers whose expression was released and may be reissued.
    free: Vec<u32>,
    /// Identifiers interned since the last drain, in interning order.
    issued: Vec<u32>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl CalcTable {
    /// Prepares the table for lowerings at `scale` device pixels per CSS pixel.
    pub(crate) fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// The identifier for one expression, recorded for [`CalcTable::drain_issued`].
    pub(crate) fn intern_id(&mut self, value: &LengthPercentage) -> CalcId {
        let id = match self.free.pop() {
            Some(id) => {
                self.exprs[id as usize] = Some(value.clone());
                id
            }
            None => {
                let id =
                    u32::try_from(self.exprs.len()).expect("far fewer than four billion calcs");
                self.exprs.push(Some(value.clone()));
                id
            }
        };
        self.issued.push(id);
        CalcId(id)
    }

    /// Moves the identifiers interned since the last drain into `into`.
    pub(crate) fn drain_issued(&mut self, into: &mut Vec<u32>) {
        into.append(&mut self.issued);
    }

    /// Releases one identifier, whose handle stops being meaningful.
    pub(crate) fn release(&mut self, id: u32) {
        debug_assert!(
            self.exprs.get(id as usize).is_some_and(Option::is_some),
            "released a calc identifier twice"
        );
        if let Some(slot) = self.exprs.get_mut(id as usize) {
            *slot = None;
            self.free.push(id);
        }
    }

    /// What one identifier's expression evaluates to at `basis`, in device pixels.
    ///
    /// The basis arrives in device pixels because every length a layout pass handles is in device
    /// pixels, while the expression is written in CSS pixels — so the basis is converted down, the
    /// expression evaluated, and the result converted back up.
    ///
    /// # Panics
    ///
    /// If the identifier's expression was never interned here, or was released.
    pub(crate) fn resolve_id(&self, id: CalcId, basis: f32) -> f32 {
        let index = id.0;
        let expression = self
            .exprs
            .get(index as usize)
            .and_then(Option::as_ref)
            .expect("every calc handle names a live expression");
        expression.resolve(Length::new(basis / self.scale)).px() * self.scale
    }

    /// How many expressions are live.
    pub(crate) fn live(&self) -> usize {
        self.exprs.iter().filter(|it| it.is_some()).count()
    }
}

impl InternCalc for CalcTable {
    fn intern_calc_id(&mut self, value: &LengthPercentage) -> CalcId {
        self.intern_id(value)
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::values::length::{Length, LengthPercentage, percent};

    use super::CalcTable;

    fn table(scale: f32) -> CalcTable {
        let mut table = CalcTable::default();
        table.set_scale(scale);
        table
    }

    #[test]
    fn a_percentage_resolves_against_a_basis_measured_in_device_pixels() {
        let value = percent(0.25);
        let mut table = table(2.0);
        let handle = table.intern_id(&value);
        // A quarter of a 200-device-pixel basis, whatever the scale, because a percentage has no
        // unit of its own.
        assert_eq!(table.resolve_id(handle, 200.0), 50.0);
    }

    #[test]
    fn an_absolute_length_is_scaled_and_a_basis_does_not_change_it() {
        let value = LengthPercentage::new_length(Length::new(10.0));
        let mut table = table(2.0);
        let handle = table.intern_id(&value);
        assert_eq!(table.resolve_id(handle, 0.0), 20.0);
        assert_eq!(table.resolve_id(handle, 999.0), 20.0);
    }

    #[test]
    fn released_identifiers_are_reissued_and_their_owners_are_tracked() {
        let value = percent(0.5);
        let mut table = table(1.0);
        let first = table.intern_id(&value);
        let mut owned = Vec::new();
        table.drain_issued(&mut owned);
        assert_eq!(owned.len(), 1);
        assert_eq!(table.live(), 1);

        table.release(owned[0]);
        assert_eq!(table.live(), 0);

        let second = table.intern_id(&percent(0.75));
        assert_eq!(first, second, "a dead identifier grows the table forever");
        assert_eq!(table.resolve_id(second, 100.0), 75.0);
    }
}

//! A uniform grid over a pass's inks, for asking which of them a rectangle meets.
//!
//! Rule 3 and rule 5 each test non-vector primitives against the inks of a pass. A pass over a page
//! of labels holds thousands of both, and a test against every ink makes the plan quadratic. The
//! grid keeps each test to the inks near the rectangle asked about.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Rect};

/// The edge of one cell, in device pixels.
const CELL: f32 = 64.0;

/// The lowest and highest cell index on either axis.
///
/// Ink far outside the surface lands in the outermost cells. The mapping from pixels to cells stays
/// monotonic, so two rectangles that meet still share a cell.
const LOWEST: i32 = -64;
const HIGHEST: i32 = 256;

/// The inks of one pass, by the cells they cover.
#[derive(Debug, Default)]
pub(crate) struct InkCells {
    /// The indices of the inks that cover each cell.
    cells: FxHashMap<(i32, i32), SmallVec<[u32; 4]>>,
}

impl InkCells {
    /// Records that ink `index` covers `ink`.
    pub(crate) fn insert(&mut self, index: usize, ink: Rect<DevicePx, Device>) {
        let (columns, rows) = span(ink);
        for column in columns {
            for row in rows.clone() {
                self.cells
                    .entry((column, row))
                    .or_default()
                    .push(index as u32);
            }
        }
    }

    /// Whether `rect` meets one of `inks` whose index `admits` accepts.
    ///
    /// `inks` is the list the indices were recorded from.
    pub(crate) fn meets(
        &self,
        rect: Rect<DevicePx, Device>,
        inks: &[Rect<DevicePx, Device>],
        mut admits: impl FnMut(usize) -> bool,
    ) -> bool {
        if self.cells.is_empty() {
            return false;
        }
        let (columns, rows) = span(rect);
        for column in columns {
            for row in rows.clone() {
                let Some(held) = self.cells.get(&(column, row)) else {
                    continue;
                };
                let found = held.iter().any(|&index| {
                    let index = index as usize;
                    admits(index) && inks.get(index).is_some_and(|ink| rect.intersects(*ink))
                });
                if found {
                    return true;
                }
            }
        }
        false
    }
}

/// The cells `rect` covers, as a range of columns and a range of rows.
fn span(
    rect: Rect<DevicePx, Device>,
) -> (
    core::ops::RangeInclusive<i32>,
    core::ops::RangeInclusive<i32>,
) {
    let cell = |value: f32| ((value / CELL).floor() as i32).clamp(LOWEST, HIGHEST);
    (
        cell(rect.left().0)..=cell(rect.right().0),
        cell(rect.top().0)..=cell(rect.bottom().0),
    )
}

#[cfg(test)]
mod tests {
    use super::InkCells;
    use crate::pass::fixture::rect;

    #[test]
    fn a_rectangle_meets_only_the_inks_it_overlaps() {
        let inks = [rect(0.0, 0.0, 10.0, 10.0), rect(500.0, 500.0, 10.0, 10.0)];
        let mut cells = InkCells::default();
        for (index, ink) in inks.iter().enumerate() {
            cells.insert(index, *ink);
        }
        assert!(cells.meets(rect(5.0, 5.0, 2.0, 2.0), &inks, |_| true));
        assert!(!cells.meets(rect(20.0, 20.0, 2.0, 2.0), &inks, |_| true));
        assert!(cells.meets(rect(505.0, 505.0, 50.0, 50.0), &inks, |_| true));
        assert!(!cells.meets(rect(505.0, 505.0, 50.0, 50.0), &inks, |index| index == 0));
    }

    #[test]
    fn ink_far_off_the_surface_is_still_found() {
        let inks = [rect(-90_000.0, 40.0, 100_000.0, 10.0)];
        let mut cells = InkCells::default();
        cells.insert(0, inks[0]);
        assert!(cells.meets(rect(-80_000.0, 42.0, 5.0, 5.0), &inks, |_| true));
        assert!(cells.meets(rect(3000.0, 42.0, 5.0, 5.0), &inks, |_| true));
    }
}

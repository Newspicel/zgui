//! The computed style, as the layout engine sees it.
//!
//! No engine style is built per read. A [`lowered::LayoutStyle`] is one cascade result converted
//! once for one device, and the store interns one engine style per box from it and the box's
//! variant; the engine borrows that style by reference for as long as it needs it.

pub mod calc;
pub mod convert;
pub(crate) mod engine;
pub mod grid;
pub(crate) mod lowered;

use zgui_css::ComputedStyle;

use crate::style::convert::length::IntrinsicSizes;

/// Whether two computed styles are the same cascade result, as opposed to two that merely agree.
///
/// Allocation identity rather than value equality. A cascade result is a fresh allocation each time
/// it is computed and shared by every element it was computed for, so this is a pointer comparison
/// on the path a document full of similar elements takes. Two styles that do not share an
/// allocation may still agree on every property, and treating those as different costs one refcount
/// and no downstream work — every consumer keys on the property groups rather than on the style as
/// a whole.
pub(crate) fn same_cascade(held: &ComputedStyle, style: &ComputedStyle) -> bool {
    ::core::ptr::eq(
        ::core::ptr::from_ref(&**held),
        ::core::ptr::from_ref(&**style),
    )
}

/// The numbers a layout pass supplies that no style carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceStyle {
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// How wide a scrollbar is, in device pixels.
    ///
    /// This is not a CSS property in this build — the longhand that would carry it is generated
    /// only for another engine — so it comes from the theme.
    pub scrollbar_width: f32,
}

impl Default for DeviceStyle {
    fn default() -> Self {
        Self {
            scale: 1.0,
            scrollbar_width: 15.0,
        }
    }
}

/// What the intrinsic pre-pass measured for one box, if it measured anything.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeasuredSizes {
    /// The horizontal minimum and maximum.
    pub horizontal: Option<IntrinsicSizes>,
    /// The vertical minimum and maximum.
    pub vertical: Option<IntrinsicSizes>,
}

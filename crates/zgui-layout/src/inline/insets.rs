//! The margin, padding and border of one box, resolved against a basis.

use cephal::Rect;
use zgui_dom::side::BoxKey;

use crate::measure::MeasureContent;
use crate::tree::LayoutTree;

/// A box's margin, padding and border, percentages resolved against `basis`.
pub(crate) fn frame_of<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    basis: Option<f32>,
) -> (Rect<f32>, Rect<f32>, Rect<f32>) {
    let style = tree.engine_style(key);
    let calc = |id, basis| tree.resolve_calc(id, basis);
    (
        style.margin.map(|m| m.resolve_or_zero(basis, &calc)),
        style.padding.map(|p| p.resolve_or_zero(basis, &calc)),
        style.border.map(|b| b.resolve_or_zero(basis, &calc)),
    )
}

/// How much a box's start and end edges add on the inline axis: margin, padding and border.
pub(crate) fn edges_of<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    basis: Option<f32>,
) -> (f32, f32) {
    let (margin, padding, border) = frame_of(tree, key, basis);
    (
        margin.left + padding.left + border.left,
        margin.right + padding.right + border.right,
    )
}

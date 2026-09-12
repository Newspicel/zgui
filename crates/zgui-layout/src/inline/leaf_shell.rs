//! The box arithmetic around a leaf's measurement.
//!
//! The engine's own leaf path measures through a shared `measure` hook, which cannot run a nested
//! layout. A box that holds lines, an atomic inline and a custom element all need the tree while
//! they measure — an atomic's content is a nested layout of boxes this tree owns — so the leaf
//! sequence is written here over the tree instead: percentage resolution against the inline
//! size, the `box-sizing` adjustment, the min/max clamp, the transposed scrollbar gutter, the
//! available-space derivation and the aspect-ratio fix-up. The baselines the measurement reports
//! are carried out in the answer.

use cephal::compute::block::BlockContext;
use cephal::style::{BoxSizing, Overflow, Position};
use cephal::tree::{
    Baselines, CollapsibleMarginSet, LayoutInput, LayoutOutput, NodeId, RunMode, SizingMode,
};
use cephal::{AvailableSpace, MaybeMath, Rect, Size};

use crate::key::from_node_id;
use crate::measure::MeasureContent;
use crate::tree::LayoutTree;

/// Lays out one leaf, measuring its content through the tree.
pub(crate) fn compute<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    node: NodeId,
    inputs: LayoutInput,
    block: Option<&mut BlockContext<'_>>,
) -> LayoutOutput {
    let LayoutInput {
        known_dimensions,
        parent_size,
        available_space,
        sizing_mode,
        run_mode,
        ..
    } = inputs;
    // Everything up to the measurement is a pure function of the style and the inputs, so it runs
    // under a shared borrow that is released before the tree is used exclusively below.
    let prepared = {
        let style = tree.engine_style(from_node_id(node));
        let calc = |id, basis| tree.resolve_calc(id, basis);
        // Both axes resolve percentage padding and border against the containing block's *inline*
        // size. That is not an oversight in CSS; it is what the specification says.
        let margin = style
            .margin
            .map(|m| m.resolve_or_zero(parent_size.width, &calc));
        let padding = style
            .padding
            .map(|p| p.resolve_or_zero(parent_size.width, &calc));
        let border = style
            .border
            .map(|b| b.resolve_or_zero(parent_size.width, &calc));
        let padding_border = padding + border;
        let pb_sum = padding_border.sum_axes();
        let box_sizing_adjustment = if style.box_sizing == BoxSizing::ContentBox {
            pb_sum
        } else {
            Size::ZERO
        };
        let (node_size, node_min_size, node_max_size, aspect_ratio) = match sizing_mode {
            SizingMode::ContentSize => (known_dimensions, Size::NONE, Size::NONE, None),
            SizingMode::InherentSize => {
                let aspect_ratio = style.aspect_ratio;
                let style_size = Size {
                    width: style.size.width.resolve(parent_size.width, &calc),
                    height: style.size.height.resolve(parent_size.height, &calc),
                }
                .maybe_apply_aspect_ratio(aspect_ratio)
                .maybe_add(box_sizing_adjustment);
                let style_min_size = Size {
                    width: style.min_size.width.resolve(parent_size.width, &calc),
                    height: style.min_size.height.resolve(parent_size.height, &calc),
                }
                .maybe_apply_aspect_ratio(aspect_ratio)
                .maybe_add(box_sizing_adjustment);
                let style_max_size = Size {
                    width: style.max_size.width.resolve(parent_size.width, &calc),
                    height: style.max_size.height.resolve(parent_size.height, &calc),
                }
                .maybe_add(box_sizing_adjustment);
                (
                    known_dimensions.or(style_size),
                    style_min_size,
                    style_max_size,
                    aspect_ratio,
                )
            }
        };
        // The axes are transposed: a box that scrolls vertically needs *horizontal* space for the
        // scrollbar.
        let overflow = style.overflow;
        let scrollbar_gutter = Size {
            width: if overflow.y == Overflow::Scroll {
                style.scrollbar_width
            } else {
                0.0
            },
            height: if overflow.x == Overflow::Scroll {
                style.scrollbar_width
            } else {
                0.0
            },
        };
        let mut content_box_inset = padding_border;
        content_box_inset.right += scrollbar_gutter.width;
        content_box_inset.bottom += scrollbar_gutter.height;
        let cannot_collapse_through = !style.is_block()
            || overflow.x.is_scroll_container()
            || overflow.y.is_scroll_container()
            || style.position == Position::Absolute
            || padding.top > 0.0
            || padding.bottom > 0.0
            || border.top > 0.0
            || border.bottom > 0.0
            || matches!(node_size.height, Some(height) if height > 0.0)
            || matches!(node_min_size.height, Some(height) if height > 0.0);
        Prepared {
            scroll_container: overflow.x.is_scroll_container() || overflow.y.is_scroll_container(),
            padding,
            padding_border,
            content_box_inset,
            margin,
            node_size,
            node_min_size,
            node_max_size,
            aspect_ratio,
            cannot_collapse_through,
        }
    };
    // Both dimensions known and nothing to report: the content is never asked.
    if let (
        RunMode::ComputeSize,
        true,
        Size {
            width: Some(width),
            height: Some(height),
        },
    ) = (
        run_mode,
        prepared.cannot_collapse_through,
        prepared.node_size,
    ) {
        let size = Size { width, height }
            .maybe_clamp(prepared.node_min_size, prepared.node_max_size)
            .f32_max(prepared.padding_border.sum_axes());
        return LayoutOutput {
            size,
            scrollable_overflow_rect: Rect::ZERO,
            inflow_overflow_rect: Rect::ZERO,
            baselines: Baselines::NONE,
            top_margin: CollapsibleMarginSet::ZERO,
            bottom_margin: CollapsibleMarginSet::ZERO,
            margins_can_collapse_through: false,
        };
    }
    let measure_space = Size {
        width: known_dimensions
            .width
            .map(AvailableSpace::Definite)
            .unwrap_or(available_space.width)
            .maybe_sub(prepared.margin.horizontal_axis_sum())
            .maybe_set(known_dimensions.width)
            .maybe_set(prepared.node_size.width)
            .map_definite(|size| {
                size.maybe_clamp(prepared.node_min_size.width, prepared.node_max_size.width)
                    - prepared.content_box_inset.horizontal_axis_sum()
            }),
        height: known_dimensions
            .height
            .map(AvailableSpace::Definite)
            .unwrap_or(available_space.height)
            .maybe_sub(prepared.margin.vertical_axis_sum())
            .maybe_set(known_dimensions.height)
            .maybe_set(prepared.node_size.height)
            .map_definite(|size| {
                size.maybe_clamp(prepared.node_min_size.height, prepared.node_max_size.height)
                    - prepared.content_box_inset.vertical_axis_sum()
            }),
    };
    // The measurement takes the tree, because an atomic inline's content is a nested layout of
    // boxes this tree owns.
    let measured = crate::inline::measure_leaf(
        tree,
        node,
        match run_mode {
            RunMode::ComputeSize => known_dimensions,
            // A layout run passes no known dimensions, which is the signal to the content that the
            // lines it produces now are the ones that will be kept.
            RunMode::PerformLayout | RunMode::PerformHiddenLayout => Size::NONE,
        },
        measure_space,
        run_mode,
        inputs.axis,
        block,
    );
    let measured_size = measured.size;
    let clamped_size = known_dimensions
        .or(prepared.node_size)
        .unwrap_or(measured_size + prepared.content_box_inset.sum_axes())
        .maybe_clamp(prepared.node_min_size, prepared.node_max_size);
    let size = Size {
        width: clamped_size.width,
        height: clamped_size.height.max(
            prepared
                .aspect_ratio
                .map(|ratio| clamped_size.width / ratio)
                .unwrap_or(0.0),
        ),
    }
    .f32_max(prepared.padding_border.sum_axes());
    // A baseline the content reported is measured from the top of the *content* box, and every
    // consumer of it measures from the top of the border box.
    let offset = prepared.padding_border.top;
    // The content's extent from the padding-box corner: what the box scrolls, and what a content
    // size derived from this result reports. A scroll container's own end padding is part of it.
    let end = if prepared.scroll_container {
        Size {
            width: prepared.padding.right,
            height: prepared.padding.bottom,
        }
    } else {
        Size::ZERO
    };
    let scrollable_overflow_rect = Rect {
        left: 0.0,
        top: 0.0,
        right: prepared.padding.left + measured_size.width + end.width,
        bottom: prepared.padding.top + measured_size.height + end.height,
    };
    LayoutOutput {
        size,
        scrollable_overflow_rect,
        inflow_overflow_rect: scrollable_overflow_rect,
        baselines: Baselines {
            first: measured.first_baseline.map(|baseline| baseline + offset),
            last: measured.last_baseline.map(|baseline| baseline + offset),
        },
        top_margin: CollapsibleMarginSet::ZERO,
        bottom_margin: CollapsibleMarginSet::ZERO,
        margins_can_collapse_through: !prepared.cannot_collapse_through
            && size.height == 0.0
            && measured_size.height == 0.0,
    }
}

/// What the style and the inputs decide before the content is asked.
struct Prepared {
    scroll_container: bool,
    padding: Rect<f32>,
    padding_border: Rect<f32>,
    content_box_inset: Rect<f32>,
    margin: Rect<f32>,
    node_size: Size<Option<f32>>,
    node_min_size: Size<Option<f32>>,
    node_max_size: Size<Option<f32>>,
    aspect_ratio: Option<f32>,
    cannot_collapse_through: bool,
}

/// `maybe_set` for an available space: a known value replaces whatever the space was.
trait MaybeSet {
    fn maybe_set(self, value: Option<f32>) -> Self;
}

impl MaybeSet for AvailableSpace {
    fn maybe_set(self, value: Option<f32>) -> Self {
        value.map_or(self, AvailableSpace::Definite)
    }
}

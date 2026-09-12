//! Lengths, percentages and sizing keywords, as the incremental engine spells them.
//!
//! The same two rules as the other conversions: percentages stay fractions, absolute lengths are
//! scaled into device pixels. What differs is that the sizing keywords travel as themselves — the
//! engine resolves `min-content`, `max-content`, `fit-content` and `stretch` on a `size` or a
//! `flex-basis` without any prepass. `min-*` and `max-*` take no keyword in the engine's
//! vocabulary, so those keep the `auto` plus substitution slot of the lowering.

use cephal::style::{Dimension, Length, LengthPercentage, LengthPercentageAuto};
use zgui_css::values::border::{BorderSideWidthValue, BorderStyleValue};
use zgui_css::values::length::LengthPercentage as CssLengthPercentage;
use zgui_css::values::size::{
    FlexBasisValue, GapValue, InsetValue, MarginValue, MaxSizeValue, PaddingValue, SizeValue,
};

use crate::style::calc::InternCalc;

/// A length or percentage, with `calc()` handed to the interner.
pub(crate) fn raw(value: &CssLengthPercentage, scale: f32, calc: &mut impl InternCalc) -> Length {
    if let Some(length) = value.to_length() {
        Length::length(length.px() * scale)
    } else if let Some(percentage) = value.to_percentage() {
        Length::percent(percentage.0)
    } else {
        Length::calc(calc.intern_calc_id(value))
    }
}

/// One padding side.
pub(crate) fn padding(
    value: &PaddingValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentage {
    LengthPercentage(raw(&value.0, scale, calc))
}

/// One gap, `normal` being zero for flex and grid.
pub(crate) fn gap(value: &GapValue, scale: f32, calc: &mut impl InternCalc) -> LengthPercentage {
    match value {
        GapValue::Normal => LengthPercentage::ZERO,
        GapValue::LengthPercentage(inner) => LengthPercentage(raw(&inner.0, scale, calc)),
    }
}

/// One margin side; the anchor forms become `auto`.
pub(crate) fn margin(
    value: &MarginValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentageAuto {
    match value {
        MarginValue::LengthPercentage(inner) | MarginValue::AnchorContainingCalcFunction(inner) => {
            LengthPercentageAuto(raw(inner, scale, calc))
        }
        MarginValue::Auto | MarginValue::AnchorSizeFunction(_) => LengthPercentageAuto::AUTO,
    }
}

/// One of `top`, `right`, `bottom` and `left`.
pub(crate) fn inset(
    value: &InsetValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentageAuto {
    match value {
        InsetValue::LengthPercentage(inner) | InsetValue::AnchorContainingCalcFunction(inner) => {
            LengthPercentageAuto(raw(inner, scale, calc))
        }
        InsetValue::Auto | InsetValue::AnchorFunction(_) | InsetValue::AnchorSizeFunction(_) => {
            LengthPercentageAuto::AUTO
        }
    }
}

/// One border side's width, zero where the side draws nothing.
pub(crate) fn border_side(
    width: &BorderSideWidthValue,
    style: BorderStyleValue,
    scale: f32,
) -> LengthPercentage {
    match style {
        BorderStyleValue::None | BorderStyleValue::Hidden => LengthPercentage::ZERO,
        _ => LengthPercentage::length(width.0.to_f32_px() * scale),
    }
}

/// `width` or `height`, keywords carried as themselves.
pub(crate) fn size(value: &SizeValue, scale: f32, calc: &mut impl InternCalc) -> Dimension {
    match value {
        SizeValue::LengthPercentage(inner) | SizeValue::AnchorContainingCalcFunction(inner) => {
            Dimension(raw(&inner.0, scale, calc))
        }
        SizeValue::Auto | SizeValue::AnchorSizeFunction(_) => Dimension::AUTO,
        SizeValue::WebkitFillAvailable | SizeValue::Stretch => Dimension::STRETCH,
        SizeValue::MinContent => Dimension::MIN_CONTENT,
        SizeValue::MaxContent => Dimension::MAX_CONTENT,
        SizeValue::FitContent => Dimension::FIT_CONTENT,
        SizeValue::FitContentFunction(limit) => fit_content(&limit.0, scale, calc),
    }
}

/// `fit-content(<length-percentage>)`, whose argument is the upper bound.
fn fit_content(limit: &CssLengthPercentage, scale: f32, calc: &mut impl InternCalc) -> Dimension {
    if let Some(length) = limit.to_length() {
        Dimension::fit_content_px(length.px() * scale)
    } else if let Some(percentage) = limit.to_percentage() {
        Dimension::fit_content_percent(percentage.0)
    } else {
        // A `calc()` bound has no keyword form; the plain keyword is the nearest answer.
        let _ = calc;
        Dimension::FIT_CONTENT
    }
}

/// `min-width` or `min-height`: a keyword is `auto` here and substituted from the box's
/// measurement by the caller, `stretch` is the whole containing block.
pub(crate) fn min_size(
    value: &SizeValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentageAuto {
    match value {
        SizeValue::LengthPercentage(inner) | SizeValue::AnchorContainingCalcFunction(inner) => {
            LengthPercentageAuto(raw(&inner.0, scale, calc))
        }
        SizeValue::WebkitFillAvailable | SizeValue::Stretch => LengthPercentageAuto::percent(1.0),
        SizeValue::Auto
        | SizeValue::AnchorSizeFunction(_)
        | SizeValue::MinContent
        | SizeValue::MaxContent
        | SizeValue::FitContent
        | SizeValue::FitContentFunction(_) => LengthPercentageAuto::AUTO,
    }
}

/// `max-width` or `max-height`, `none` and the keywords being `auto`.
pub(crate) fn max_size(
    value: &MaxSizeValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentageAuto {
    match value {
        MaxSizeValue::LengthPercentage(inner)
        | MaxSizeValue::AnchorContainingCalcFunction(inner) => {
            LengthPercentageAuto(raw(&inner.0, scale, calc))
        }
        MaxSizeValue::WebkitFillAvailable | MaxSizeValue::Stretch => {
            LengthPercentageAuto::percent(1.0)
        }
        MaxSizeValue::None
        | MaxSizeValue::AnchorSizeFunction(_)
        | MaxSizeValue::MinContent
        | MaxSizeValue::MaxContent
        | MaxSizeValue::FitContent
        | MaxSizeValue::FitContentFunction(_) => LengthPercentageAuto::AUTO,
    }
}

/// `flex-basis`, `content` carried as itself.
pub(crate) fn flex_basis(
    value: &FlexBasisValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> Dimension {
    match value {
        FlexBasisValue::Size(inner) => size(inner, scale, calc),
        FlexBasisValue::Content => Dimension::CONTENT,
    }
}

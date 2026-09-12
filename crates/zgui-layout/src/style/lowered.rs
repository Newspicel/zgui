//! One computed style, lowered once into the form the layout engine reads.
//!
//! The lowering is done once per distinct cascade result and device, held by the store's style
//! table, and shared by every box that holds the cascade result. Most of it is the engine style's
//! template; what stays beside it is what a box has to add before the engine can read the style:
//! a `min-*`/`max-*` keyword resolves against the *box's* measured intrinsic sizes, an `auto`
//! aspect ratio defers to the box's natural ratio, and `position: fixed` is told apart from
//! absolute for overflow alone.

use zgui_css::ComputedStyle;
use zgui_css::values::size::{
    BoxSizingValue, MaxSizeValue, PositionValue, SizeValue, VisibilityValue,
};

use crate::style::calc::InternCalc;
use crate::style::convert::aspect;
use crate::style::convert::length::IntrinsicSizes;
use crate::style::grid::idents::IdentTable;
use crate::style::{DeviceStyle, MeasuredSizes};

/// Which per-box measurement a size slot substitutes, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    /// The packed value is final.
    None = 0,
    /// Substitute the content's minimum size.
    Min = 1,
    /// Substitute the content's maximum size. `fit-content` lands here too.
    Max = 2,
}

/// One two-bit keyword slot per substitutable size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Keywords(u8);

/// The substitutable size slots, two bits each.
#[derive(Clone, Copy, Debug)]
enum Slot {
    MinWidth = 0,
    MinHeight = 1,
    MaxWidth = 2,
    MaxHeight = 3,
}

impl Keywords {
    fn set(&mut self, slot: Slot, keyword: Keyword) {
        self.0 |= (keyword as u8) << ((slot as u8) * 2);
    }

    fn get(self, slot: Slot) -> Keyword {
        match (self.0 >> ((slot as u8) * 2)) & 0b11 {
            1 => Keyword::Min,
            2 => Keyword::Max,
            _ => Keyword::None,
        }
    }

    /// Whether no slot substitutes anything, which is the common box.
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One computed style in the layout engine's vocabulary, conversion already paid.
#[derive(Clone, Debug)]
pub(crate) struct LayoutStyle {
    /// The engine style before a box's variant is patched in.
    pub(crate) template: cephal::Style,
    /// How much of an intrinsic measurement is this style's own padding and border, per axis.
    ///
    /// Zero under `border-box`. Percentage and `calc()` components contribute nothing, exactly as
    /// resolving them against no basis contributed nothing per call.
    pub(crate) intrinsic_inset: cephal::Size<f32>,
    /// The written aspect ratio, degenerate ratios already discarded.
    aspect_explicit: Option<f32>,
    /// Whether `aspect-ratio` prefers the content's natural proportions.
    aspect_auto: bool,
    /// Which `min-*`/`max-*` slots substitute a measurement at read time.
    pub(crate) keywords: Keywords,
    /// Whether the box is `position: fixed`, told apart from absolute for overflow only.
    pub(crate) fixed: bool,
    /// Whether `visibility: collapse` removes this box when its parent is a flex container.
    pub(crate) collapses_as_flex_item: bool,
}

impl LayoutStyle {
    /// Lowers one computed style for one device.
    ///
    /// Every `calc()` met on the way is interned into `calc`, and the caller owns the identifiers
    /// the interner issued. Grid names are interned into `idents` and never released.
    pub(crate) fn lower(
        style: &ComputedStyle,
        device: DeviceStyle,
        calc: &mut impl InternCalc,
        idents: &mut IdentTable,
    ) -> Self {
        let box_ = style.get_box();
        let position_group = style.get_position();
        let mut keywords = Keywords::default();
        keywords.set(Slot::MinWidth, keyword_of_size(&position_group.min_width));
        keywords.set(Slot::MinHeight, keyword_of_size(&position_group.min_height));
        keywords.set(Slot::MaxWidth, keyword_of_max(&position_group.max_width));
        keywords.set(Slot::MaxHeight, keyword_of_max(&position_group.max_height));
        let template = crate::style::engine::template(style, device, calc, idents);
        let intrinsic_inset = if position_group.box_sizing == BoxSizingValue::ContentBox {
            (template.padding.map(|it| it.value_or_zero())
                + template.border.map(|it| it.value_or_zero()))
            .sum_axes()
        } else {
            cephal::Size::ZERO
        };
        let (aspect_explicit, aspect_auto) = aspect::split(&position_group.aspect_ratio);
        Self {
            template,
            intrinsic_inset,
            aspect_explicit,
            aspect_auto,
            keywords,
            fixed: box_.position == PositionValue::Fixed,
            collapses_as_flex_item: style.get_inherited_box().visibility
                == VisibilityValue::Collapse
                && box_.display.outside() != zgui_css::values::size::DisplayOutside::None,
        }
    }

    /// The four `min-*`/`max-*` slots' measured values, where a keyword was written and the
    /// content has been measured: minimum width and height, then maximum width and height.
    pub(crate) fn min_max_with(&self, measured: MeasuredSizes) -> [Option<f32>; 4] {
        if self.keywords.is_empty() {
            return [None; 4];
        }
        let pick = |slot: Slot, sizes: Option<IntrinsicSizes>| match self.keywords.get(slot) {
            Keyword::None => None,
            Keyword::Min => sizes.map(|sizes| sizes.min),
            Keyword::Max => sizes.map(|sizes| sizes.max),
        };
        [
            pick(Slot::MinWidth, measured.horizontal),
            pick(Slot::MinHeight, measured.vertical),
            pick(Slot::MaxWidth, measured.horizontal),
            pick(Slot::MaxHeight, measured.vertical),
        ]
    }

    /// The ratio of width to height a box should keep, given its content's natural proportions.
    pub(crate) fn aspect_ratio(&self, natural: Option<f32>) -> Option<f32> {
        if self.aspect_auto {
            natural.or(self.aspect_explicit)
        } else {
            self.aspect_explicit.or(natural)
        }
    }
}

/// The plain length of a value, or zero for a percentage or `calc()`.
trait ValueOrZero {
    fn value_or_zero(self) -> f32;
}

impl ValueOrZero for cephal::style::LengthPercentage {
    fn value_or_zero(self) -> f32 {
        if self.is_length() { self.value() } else { 0.0 }
    }
}

/// Which measurement a `min-width`-family value substitutes.
fn keyword_of_size(value: &SizeValue) -> Keyword {
    match value {
        SizeValue::MinContent => Keyword::Min,
        SizeValue::MaxContent | SizeValue::FitContent | SizeValue::FitContentFunction(_) => {
            Keyword::Max
        }
        _ => Keyword::None,
    }
}

/// Which measurement a `max-width`-family value substitutes.
fn keyword_of_max(value: &MaxSizeValue) -> Keyword {
    match value {
        MaxSizeValue::MinContent => Keyword::Min,
        MaxSizeValue::MaxContent
        | MaxSizeValue::FitContent
        | MaxSizeValue::FitContentFunction(_) => Keyword::Max,
        _ => Keyword::None,
    }
}

#[cfg(test)]
mod tests {
    use cephal::style::{Dimension, Float, LengthPercentageAuto};
    use zgui_css::StyleDraft;
    use zgui_css::values::length::{Length, LengthPercentage as CssLp, NonNegative};
    use zgui_css::values::size::{FloatValue, InsetValue, PositionValue, SizeValue};
    use zgui_css::values::text::Direction as CssDirection;

    use crate::style::calc::CalcTable;
    use crate::style::convert::length::IntrinsicSizes;
    use crate::style::grid::idents::IdentTable;
    use crate::style::{DeviceStyle, MeasuredSizes};

    use super::LayoutStyle;

    fn lower_with(mutate: impl FnOnce(&mut StyleDraft)) -> LayoutStyle {
        let mut draft = StyleDraft::initial();
        mutate(&mut draft);
        let style = draft.build();
        let mut calc = CalcTable::default();
        calc.set_scale(1.0);
        let mut idents = IdentTable::default();
        LayoutStyle::lower(&style, DeviceStyle::default(), &mut calc, &mut idents)
    }

    fn length(px: f32) -> CssLp {
        CssLp::new_length(Length::new(px))
    }

    #[test]
    fn a_min_max_keyword_substitutes_the_measurement_at_read_time() {
        let lowered = lower_with(|draft| {
            draft.position_group().width = SizeValue::MinContent;
            draft.position_group().min_height = SizeValue::MaxContent;
        });
        assert_eq!(
            lowered.template.size.width,
            Dimension::MIN_CONTENT,
            "the engine resolves it"
        );
        assert_eq!(
            lowered.template.min_size.height,
            LengthPercentageAuto::AUTO,
            "deferred"
        );
        assert_eq!(lowered.min_max_with(MeasuredSizes::default()), [None; 4]);
        let sizes = IntrinsicSizes {
            min: 30.0,
            max: 90.0,
        };
        let measured = MeasuredSizes {
            horizontal: Some(sizes),
            vertical: Some(sizes),
        };
        assert_eq!(
            lowered.min_max_with(measured),
            [None, Some(90.0), None, None]
        );
    }

    #[test]
    fn a_style_with_no_keywords_substitutes_nothing() {
        let lowered = lower_with(|draft| {
            draft.position_group().width = SizeValue::LengthPercentage(NonNegative(length(24.0)));
        });
        assert!(lowered.keywords.is_empty());
        let measured = MeasuredSizes {
            horizontal: Some(IntrinsicSizes { min: 1.0, max: 2.0 }),
            vertical: None,
        };
        assert_eq!(lowered.min_max_with(measured), [None; 4]);
        assert_eq!(lowered.template.size.width, Dimension::length(24.0));
    }

    #[test]
    fn a_static_box_has_no_inset_whatever_was_written() {
        let lowered = lower_with(|draft| {
            draft.position_group().left = InsetValue::LengthPercentage(length(10.0));
        });
        assert_eq!(lowered.template.inset.left, LengthPercentageAuto::AUTO);

        let positioned = lower_with(|draft| {
            draft.box_group().position = PositionValue::Relative;
            draft.position_group().left = InsetValue::LengthPercentage(length(10.0));
        });
        assert_eq!(
            positioned.template.inset.left,
            LengthPercentageAuto::length(10.0)
        );
    }

    #[test]
    fn flow_relative_float_bakes_the_writing_direction() {
        let ltr = lower_with(|draft| {
            draft.box_group().float = FloatValue::InlineStart;
        });
        assert_eq!(ltr.template.float, Float::Left);

        let rtl = lower_with(|draft| {
            draft.box_group().float = FloatValue::InlineStart;
            draft.inherited_box().direction = CssDirection::Rtl;
        });
        assert_eq!(rtl.template.float, Float::Right);
    }

    #[test]
    fn content_box_padding_joins_the_intrinsic_inset_and_border_box_does_not() {
        let content_box = lower_with(|draft| {
            draft.padding().padding_left = NonNegative(length(10.0));
            draft.padding().padding_right = NonNegative(length(5.0));
        });
        assert_eq!(content_box.intrinsic_inset.width, 15.0);
        assert_eq!(content_box.intrinsic_inset.height, 0.0);

        let border_box = lower_with(|draft| {
            draft.padding().padding_left = NonNegative(length(10.0));
            draft.position_group().box_sizing = zgui_css::values::size::BoxSizingValue::BorderBox;
        });
        assert_eq!(border_box.intrinsic_inset.width, 0.0);
    }
}

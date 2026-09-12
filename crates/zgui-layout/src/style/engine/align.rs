//! Box alignment in the incremental engine's vocabulary.
//!
//! The keyword set is the same as the other engine's, and so is what is degraded: `self-start`
//! and `self-end` become `start` and `end`, `last baseline` a first baseline, `anchor-center` a
//! centre.

use cephal::style::{
    AlignContent, AlignContentKeyword, AlignItems, AlignItemsKeyword, AlignmentSafety,
};
use zgui_css::values::align::AlignFlags;

/// Where a container puts its items, or nothing to let the algorithm choose.
pub(crate) fn items(flags: AlignFlags, rtl: bool) -> Option<AlignItems> {
    let safety = safety(flags);
    let keyword = match flags.value() {
        AlignFlags::AUTO | AlignFlags::NORMAL => return None,
        AlignFlags::START | AlignFlags::SELF_START => AlignItemsKeyword::Start,
        AlignFlags::END | AlignFlags::SELF_END => AlignItemsKeyword::End,
        AlignFlags::FLEX_START => AlignItemsKeyword::FlexStart,
        AlignFlags::FLEX_END => AlignItemsKeyword::FlexEnd,
        AlignFlags::CENTER | AlignFlags::ANCHOR_CENTER => AlignItemsKeyword::Center,
        AlignFlags::BASELINE | AlignFlags::LAST_BASELINE => AlignItemsKeyword::Baseline,
        AlignFlags::STRETCH => AlignItemsKeyword::Stretch,
        AlignFlags::LEFT => flow(rtl, AlignItemsKeyword::Start, AlignItemsKeyword::End),
        AlignFlags::RIGHT => flow(rtl, AlignItemsKeyword::End, AlignItemsKeyword::Start),
        _ => return None,
    };
    Some(AlignItems { keyword, safety })
}

/// Where a container puts the whole block of its content, or nothing to let the algorithm choose.
pub(crate) fn content(flags: AlignFlags, rtl: bool) -> Option<AlignContent> {
    let safety = safety(flags);
    let keyword = match flags.value() {
        AlignFlags::AUTO | AlignFlags::NORMAL => return None,
        AlignFlags::START
        | AlignFlags::BASELINE
        | AlignFlags::LAST_BASELINE
        | AlignFlags::SELF_START => AlignContentKeyword::Start,
        AlignFlags::END | AlignFlags::SELF_END => AlignContentKeyword::End,
        AlignFlags::FLEX_START => AlignContentKeyword::FlexStart,
        AlignFlags::FLEX_END => AlignContentKeyword::FlexEnd,
        AlignFlags::CENTER | AlignFlags::ANCHOR_CENTER => AlignContentKeyword::Center,
        AlignFlags::STRETCH => AlignContentKeyword::Stretch,
        AlignFlags::SPACE_BETWEEN => AlignContentKeyword::SpaceBetween,
        AlignFlags::SPACE_AROUND => AlignContentKeyword::SpaceAround,
        AlignFlags::SPACE_EVENLY => AlignContentKeyword::SpaceEvenly,
        AlignFlags::LEFT => flow(rtl, AlignContentKeyword::Start, AlignContentKeyword::End),
        AlignFlags::RIGHT => flow(rtl, AlignContentKeyword::End, AlignContentKeyword::Start),
        _ => return None,
    };
    Some(AlignContent { keyword, safety })
}

/// `justify-items`, whose `legacy` value means "no preference".
pub(crate) fn justify_items(flags: AlignFlags, rtl: bool) -> Option<AlignItems> {
    if flags.contains(AlignFlags::LEGACY) {
        return None;
    }
    items(flags, rtl)
}

fn safety(flags: AlignFlags) -> AlignmentSafety {
    if flags.contains(AlignFlags::SAFE) {
        AlignmentSafety::Safe
    } else {
        AlignmentSafety::Unsafe
    }
}

fn flow<T>(rtl: bool, ltr: T, rtl_answer: T) -> T {
    if rtl { rtl_answer } else { ltr }
}

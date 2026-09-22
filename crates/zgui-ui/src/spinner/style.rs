//! What a spinner looks like, in tokens.

use zgui::style;

style! { pub SpinnerStyle =>
    // The turn is linear because it repeats: an eased rotation visibly hesitates every time one
    // revolution meets the next, and a spinner that hesitates reads as a spinner that has stopped.
    //
    // The resting `rotate(0deg)` is load-bearing. Whether a box is transformed at all decides its
    // stacking context and what it contains, so those answers live in the shared style — and an
    // animation whose keyframes would *bring a transform into existence* is sent back through the
    // cascade instead of being sampled. With the identity rotation declared, the turn only moves a
    // transform the box already has.
    ":scope {
        display: flex;
        align-items: flex-start;
        justify-content: center;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
        border: 2px solid color-mix(in oklab, currentColor 20%, transparent);
        border-radius: var(--zui-radius-full);
        transform: rotate(0deg);
        animation: zui-spinner-turn 900ms var(--zui-motion-ease-linear) infinite;
    }"
    // The mark sits on the track it runs round, which is what the lift is: half of it covers the
    // border and half of it stands outside.
    ":scope .zui-spinner__mark {
        width: 4px;
        height: 4px;
        margin-top: -2px;
        flex: none;
        border-radius: var(--zui-radius-full);
        background-color: currentColor;
    }"
    "@keyframes zui-spinner-turn {
        0% { transform: rotate(0deg); }
        100% { transform: rotate(360deg); }
    }"
}

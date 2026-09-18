//! An application with no dock icon, and a panel that appears over whatever the user is doing.
//!
//! Run it with `cargo run -p zgui-examples --example accessory`.
//!
//! What it is worth reading for:
//!
//! * the application is [`AppPresence::Accessory`], so it has no dock icon and no place in the
//!   switcher. That belongs to the process rather than to a window, so it is chosen where the
//!   driver is — `run_on(desktop_as(…))` — and cannot be changed later;
//! * the panel is opened **hidden** and shown and hidden again with
//!   [`WindowHandle::set_visible`]. It keeps its graphics surface and its whole document while it
//!   is away, so showing it costs a frame rather than a window creation — which is the difference
//!   that matters for a window that appears many times a day;
//! * it asks for [`ShellBehavior::overlay`], so it is on every space and over a full-screen
//!   window rather than behind one;
//! * and `with_active(false)`, so showing it does not take the keyboard away from whatever the
//!   user was typing in. Each of those three is a preference: a desktop that cannot grant one
//!   leaves the window as it is, and nothing here branches on which desktop it is.

use zgui::app::{AppPresence, desktop_as};
use zgui::prelude::*;

#[component]
fn Controls() -> impl IntoView {
    let showing = RwSignal::new(false);

    // Opened once, hidden, and kept. A component body runs once, so this is the whole of it:
    // the point of the example is that the window is never made again.
    let panel = use_windows().open(
        WindowOptions::new("Panel")
            .with_size(280.0, 96.0)
            .with_decorations(Decorations::None)
            .with_transparent(true)
            .with_level(WindowLevel::AlwaysOnTop)
            .with_active(false)
            .with_shell_behavior(ShellBehavior::overlay())
            .with_stylesheet(PANEL),
        || view! { column(class = "panel") { "Over everything." } },
    );
    panel.set_visible(false);

    let toggle = move |_: &mut EventCx<'_, events::Click>| {
        let next = !showing.get_untracked();
        panel.set_visible(next);
        showing.set(next);
    };

    view! {
        column(class = "page") {
            label(class = "page__title") { "Accessory application" }
            label(class = "page__note") { "No dock icon. The panel keeps its surface while hidden." }
            control(class = "button", tabindex = Focus::Sequential, on:click = toggle) {
                {move || if showing.get() { "Hide the panel" } else { "Show the panel" }}
            }
        }
    }
}

const SHEET: &str = css!(
    r#"
    .page {
        padding: 24px;
        gap: 12px;
        align-items: flex-start;
        background-color: #fcfcfd;
        font-family: system-ui, sans-serif;
    }
    .page__title { font-size: 16px; font-weight: 600; color: #1c2024; }
    .page__note { font-size: 12px; color: #60646c; }
    .button {
        padding: 6px 12px;
        border-radius: 5px;
        background-color: #12a594;
        color: #ffffff;
        font-size: 12px;
        font-weight: 500;
    }
    .button:hover { background-color: #0d9b8a; }
"#
);

const PANEL: &str = css!(
    r#"
    :root { background-color: transparent; }
    .panel {
        flex: 1;
        margin: 8px;
        padding: 16px;
        border-radius: 14px;
        background-color: #18191b;
        color: #edeef0;
        font-family: system-ui, sans-serif;
        font-size: 13px;
        justify-content: center;
    }
"#
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Accessory")
        .with_title("Accessory")
        .with_size(360.0, 200.0)
        .with_stylesheet(SHEET)
        .run_on(desktop_as(AppPresence::Accessory), || view! { Controls() })
}

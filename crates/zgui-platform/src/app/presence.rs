//! What the application itself is to the desktop.

/// How much of a presence an application has outside its own windows.
///
/// A desktop distinguishes a program the user switches to from one that lives in a status area and
/// has no place in the switcher. That difference is not a window property — it belongs to the
/// process — so it is settled when the event loop is built and never after.
///
/// Only macOS acts on this today. Elsewhere it is a preference the desktop is free to ignore,
/// which is the contract [`WindowLevel`](crate::surface::WindowLevel) already carries.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[non_exhaustive]
pub enum AppPresence {
    /// An ordinary application: in the switcher, in the dock, with a menu of its own.
    #[default]
    Regular,
    /// A background application that still draws: no dock icon and no place in the switcher, and
    /// its windows still appear and still take the keyboard when asked.
    ///
    /// What a status-area utility wants. On macOS this is
    /// `NSApplicationActivationPolicyAccessory`, the same thing `LSUIElement` sets from a bundle.
    Accessory,
    /// No presence at all: the application cannot be activated and its windows cannot take the
    /// keyboard. For something that draws and is never interacted with.
    Background,
}

//! Light/dark appearance. The chosen `Theme` is applied to NSApp, so every window's
//! effective appearance (and the native title bar) follows it; views read the resolved
//! light/dark choice through `is_dark()` while rendering.

use crate::Globals;
use crate::settings::Theme;
use cocoa::appkit::NSApp;
use cocoa::base::{id, nil};
use cocoa::foundation::NSString;
use gpui::{App, WindowAppearance};
use objc::{class, msg_send, sel, sel_impl};
use std::sync::atomic::{AtomicBool, Ordering};

static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Call at the top of each root view's `render`. Resolved from the preference rather than
/// `Window::appearance()`: gpui caches that value and misses the change notification when
/// `apply` runs inside an update (a settings click), so the window would stay on the old theme.
pub fn sync(cx: &App) {
    let theme = cx.global::<Globals>().state.read(cx).prefs.theme;
    let dark = match theme {
        Theme::Light => false,
        Theme::Dark => true,
        Theme::System => matches!(
            cx.window_appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    };
    DARK.store(dark, Ordering::Relaxed);
}

/// Force the app's appearance, or hand it back to the system (`nil`).
pub fn apply(theme: Theme) {
    let name = match theme {
        Theme::System => None,
        Theme::Light => Some("NSAppearanceNameAqua"),
        Theme::Dark => Some("NSAppearanceNameDarkAqua"),
    };
    unsafe {
        let appearance: id = match name {
            Some(n) => msg_send![class!(NSAppearance), appearanceNamed: NSString::alloc(nil).init_str(n)],
            None => nil,
        };
        let _: () = msg_send![NSApp(), setAppearance: appearance];
    }
}

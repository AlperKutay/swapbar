//! User notifications through UNUserNotificationCenter (cocoa/objc, like statusbar.rs).
//! Notifications need a bundle identifier, so a bare `cargo run` binary skips them.
//! A notification can carry an account number; its "Switch" action switches to it.

use block::{Block, ConcreteBlock};
use cocoa::base::{BOOL, id, nil};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::cell::RefCell;
use std::sync::Once;

#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {}

/// Category whose notifications offer the "Switch" action.
const SWITCH_CATEGORY: &str = "switch";
const SWITCH_ACTION: &str = "switch";
const DEFAULT_ACTION: &str = "com.apple.UNNotificationDefaultActionIdentifier";
const ACCOUNT_KEY: &str = "account";

/// What the user did with a notification.
pub enum Action {
    /// Clicked the notification itself.
    Open,
    /// Pressed "Switch" on a notification about this account.
    Switch(u32),
}

type Callback = Box<dyn Fn(Action)>;

thread_local! {
    static ON_ACTION: RefCell<Option<Callback>> = RefCell::new(None);
}

static REGISTER: Once = Once::new();
static mut DELEGATE_CLASS: *const Class = std::ptr::null();

fn ns(s: &str) -> id {
    unsafe {
        let s = NSString::alloc(nil).init_str(s);
        msg_send![s, autorelease]
    }
}

fn to_string(s: id) -> String {
    if s == nil {
        return String::new();
    }
    unsafe {
        let p: *const std::os::raw::c_char = msg_send![s, UTF8String];
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

/// Whether this process can use the notification center (it runs from an .app bundle).
pub fn available() -> bool {
    unsafe {
        let bundle: id = msg_send![class!(NSBundle), mainBundle];
        let ident: id = msg_send![bundle, bundleIdentifier];
        ident != nil
    }
}

fn center() -> Option<id> {
    available()
        .then(|| unsafe { msg_send![class!(UNUserNotificationCenter), currentNotificationCenter] })
}

// Banners also show while the popover has focus (the app is frontmost then).
extern "C" fn will_present(_this: &Object, _sel: Sel, _center: id, _notification: id, handler: id) {
    let handler = handler as *mut Block<(usize,), ()>;
    // UNNotificationPresentationOptionSound | List | Banner
    unsafe { (*handler).call(((1 << 1) | (1 << 3) | (1 << 4),)) };
}

// The delegate may be called off the main thread; hand the action to the main thread,
// where the gpui callback lives.
extern "C" fn did_receive(this: &Object, _sel: Sel, _center: id, response: id, handler: id) {
    unsafe {
        let action = to_string(msg_send![response, actionIdentifier]);
        let notification: id = msg_send![response, notification];
        let request: id = msg_send![notification, request];
        let content: id = msg_send![request, content];
        let info: id = msg_send![content, userInfo];
        let account: id = msg_send![info, objectForKey: ns(ACCOUNT_KEY)];
        let code: i64 = match (action.as_str(), account) {
            (SWITCH_ACTION, a) if a != nil => msg_send![a, longLongValue],
            (DEFAULT_ACTION, _) => 0,
            _ => -1,
        };
        if code >= 0 {
            let arg: id = msg_send![class!(NSNumber), numberWithLongLong: code];
            let _: () = msg_send![this, performSelectorOnMainThread: sel!(handleAction:)
                withObject: arg
                waitUntilDone: false];
        }
        (*(handler as *mut Block<(), ()>)).call(());
    }
}

extern "C" fn handle_action(_this: &Object, _sel: Sel, arg: id) {
    let code: i64 = unsafe { msg_send![arg, longLongValue] };
    let action = match u32::try_from(code) {
        Ok(n) if n > 0 => Action::Switch(n),
        _ => Action::Open,
    };
    ON_ACTION.with(|cb| {
        if let Some(f) = cb.borrow().as_ref() {
            f(action);
        }
    });
}

fn delegate_class() -> &'static Class {
    REGISTER.call_once(|| {
        let mut decl =
            ClassDecl::new("SwapbarNotificationDelegate", class!(NSObject)).expect("class decl");
        unsafe {
            decl.add_method(
                sel!(userNotificationCenter:willPresentNotification:withCompletionHandler:),
                will_present as extern "C" fn(&Object, Sel, id, id, id),
            );
            decl.add_method(
                sel!(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:),
                did_receive as extern "C" fn(&Object, Sel, id, id, id),
            );
            decl.add_method(
                sel!(handleAction:),
                handle_action as extern "C" fn(&Object, Sel, id),
            );
            DELEGATE_CLASS = decl.register();
        }
    });
    unsafe { &*DELEGATE_CLASS }
}

/// Install the delegate and the "Switch" action. Call once at launch, on the main thread.
pub fn init(on_action: Box<dyn Fn(Action)>) {
    let Some(center) = center() else {
        crate::debug_log(format_args!(
            "notifications unavailable: no bundle identifier"
        ));
        return;
    };
    ON_ACTION.with(|cb| *cb.borrow_mut() = Some(on_action));
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        // never released: the center keeps only a weak reference to its delegate
        let delegate: id = msg_send![delegate_class(), new];
        let _: () = msg_send![center, setDelegate: delegate];

        let action: id = msg_send![class!(UNNotificationAction),
            actionWithIdentifier: ns(SWITCH_ACTION)
            title: ns(crate::i18n::l("Switch", "Geç"))
            options: 0usize];
        let actions: id = msg_send![class!(NSArray), arrayWithObject: action];
        let empty: id = msg_send![class!(NSArray), array];
        let category: id = msg_send![class!(UNNotificationCategory),
            categoryWithIdentifier: ns(SWITCH_CATEGORY)
            actions: actions
            intentIdentifiers: empty
            options: 0usize];
        let set: id = msg_send![class!(NSSet), setWithObject: category];
        let _: () = msg_send![center, setNotificationCategories: set];
    }
}

/// Ask for permission to show alerts with sound. macOS prompts only the first time.
pub fn request_authorization() {
    let Some(center) = center() else { return };
    let done = ConcreteBlock::new(|granted: BOOL, _error: id| {
        crate::debug_log(format_args!("notification permission granted={granted:?}"));
    })
    .copy();
    unsafe {
        // UNAuthorizationOptionSound | UNAuthorizationOptionAlert
        let _: () = msg_send![center, requestAuthorizationWithOptions: (1usize << 1) | (1 << 2)
            completionHandler: &*done];
    }
}

/// Show a notification. A notification with the same `ident` replaces the previous one.
/// With `switch_to`, it offers a "Switch" action for that account.
pub fn post(ident: &str, title: &str, body: &str, switch_to: Option<u32>) {
    let Some(center) = center() else { return };
    crate::debug_log(format_args!("notify {ident}: {title} / {body}"));
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        let content: id = msg_send![class!(UNMutableNotificationContent), new];
        let _: () = msg_send![content, setTitle: ns(title)];
        let _: () = msg_send![content, setBody: ns(body)];
        let sound: id = msg_send![class!(UNNotificationSound), defaultSound];
        let _: () = msg_send![content, setSound: sound];
        if let Some(n) = switch_to {
            let _: () = msg_send![content, setCategoryIdentifier: ns(SWITCH_CATEGORY)];
            let num: id = msg_send![class!(NSNumber), numberWithLongLong: n as i64];
            let info: id = msg_send![class!(NSDictionary),
                dictionaryWithObject: num
                forKey: ns(ACCOUNT_KEY)];
            let _: () = msg_send![content, setUserInfo: info];
        }
        let request: id = msg_send![class!(UNNotificationRequest),
            requestWithIdentifier: ns(ident)
            content: content
            trigger: nil];
        let _: () = msg_send![center, addNotificationRequest: request withCompletionHandler: nil];
        let _: () = msg_send![content, release];
    }
}

/// Open the Notifications pane of System Settings.
pub fn open_system_settings() {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension")
        .spawn();
}

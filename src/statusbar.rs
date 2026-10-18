//! NSStatusItem glue (cocoa/objc, the same bindings gpui uses on macOS).
//! The click handler is a thread-local callback set from the gpui App.

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicy, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSString};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::cell::RefCell;
use std::sync::Once;

const ICON_2X: &[u8] = include_bytes!("../assets/menubar-36.png");

thread_local! {
    static ON_CLICK: RefCell<Option<Box<dyn Fn()>>> = RefCell::new(None);
}

static REGISTER: Once = Once::new();
static mut TARGET_CLASS: *const Class = std::ptr::null();

extern "C" fn clicked(_this: &Object, _sel: Sel, _sender: id) {
    ON_CLICK.with(|cb| {
        if let Some(f) = cb.borrow().as_ref() {
            f();
        }
    });
}

fn target_class() -> &'static Class {
    REGISTER.call_once(|| {
        let mut decl =
            ClassDecl::new("AIMenubarStatusTarget", class!(NSObject)).expect("class decl");
        unsafe {
            decl.add_method(sel!(clicked:), clicked as extern "C" fn(&Object, Sel, id));
            TARGET_CLASS = decl.register();
        }
    });
    unsafe { &*TARGET_CLASS }
}

/// Screen geometry of the status item button, in gpui's top-left-origin
/// coordinates relative to the screen that hosts the item.
#[derive(Debug, Clone, Copy)]
pub struct ButtonGeometry {
    pub screen_display_id: u32,
    pub x: f64,
    pub bottom_y: f64,
    pub width: f64,
    pub screen_width: f64,
    pub screen_height: f64,
}

pub struct StatusItem {
    item: id,
    _target: id,
}

impl StatusItem {
    pub fn new(title: &str, on_click: Box<dyn Fn()>) -> StatusItem {
        ON_CLICK.with(|cb| *cb.borrow_mut() = Some(on_click));
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);
            let app = NSApp();
            app.setActivationPolicy_(
                NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
            );
            let bar = NSStatusBar::systemStatusBar(nil);
            let item: id = bar.statusItemWithLength_(NSVariableStatusItemLength);
            let _: () = msg_send![item, retain];
            let target: id = msg_send![target_class(), new];
            let button: id = item.button();
            // Template image: macOS tints it for light/dark menu bars.
            let data: id =
                msg_send![class!(NSData), dataWithBytes: ICON_2X.as_ptr() length: ICON_2X.len()];
            let image: id = msg_send![class!(NSImage), alloc];
            let image: id = msg_send![image, initWithData: data];
            let _: () = msg_send![image, setSize: cocoa::foundation::NSSize::new(18.0, 18.0)];
            let _: () = msg_send![image, setTemplate: true];
            let _: () = msg_send![button, setImage: image];
            let _: () = msg_send![button, setImagePosition: 2u64]; // NSImageLeft
            let _: () = msg_send![button, setTarget: target];
            let _: () = msg_send![button, setAction: sel!(clicked:)];
            let si = StatusItem {
                item,
                _target: target,
            };
            si.set_title(title);
            si
        }
    }

    pub fn set_title(&self, title: &str) {
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);
            let button: id = self.item.button();
            let s = NSString::alloc(nil).init_str(title);
            let _: () = msg_send![button, setTitle: s];
            // Monospaced digits so the title doesn't jitter.
            let font: id =
                msg_send![class!(NSFont), monospacedDigitSystemFontOfSize: 12.0f64 weight: 0.0f64];
            let _: () = msg_send![button, setFont: font];
        }
    }

    pub fn set_highlighted(&self, on: bool) {
        unsafe {
            let button: id = self.item.button();
            let _: () = msg_send![button, highlight: on];
        }
    }

    pub fn geometry(&self) -> Option<ButtonGeometry> {
        unsafe {
            let button: id = self.item.button();
            let window: id = msg_send![button, window];
            if window == nil {
                return None;
            }
            let frame: NSRect = msg_send![window, frame];
            let screen: id = msg_send![window, screen];
            let screen = if screen == nil {
                msg_send![class!(NSScreen), mainScreen]
            } else {
                screen
            };
            let sframe: NSRect = msg_send![screen, frame];
            let desc: id = msg_send![screen, deviceDescription];
            let key = NSString::alloc(nil).init_str("NSScreenNumber");
            let num: id = msg_send![desc, objectForKey: key];
            let display_id: u32 = if num == nil {
                0
            } else {
                msg_send![num, unsignedIntValue]
            };
            let top_left = NSPoint::new(
                frame.origin.x - sframe.origin.x,
                sframe.origin.y + sframe.size.height - frame.origin.y,
            );
            Some(ButtonGeometry {
                screen_display_id: display_id,
                x: top_left.x,
                bottom_y: top_left.y,
                width: frame.size.width,
                screen_width: sframe.size.width,
                screen_height: sframe.size.height,
            })
        }
    }
}

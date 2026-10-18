//! System-wide shortcuts through Carbon's RegisterEventHotKey, which needs no
//! Accessibility permission. The handler runs on the main thread inside AppKit's
//! event loop and calls a thread-local callback set from the gpui App.

use std::cell::RefCell;
use std::ffi::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// ⌃⌥S: open or close the popover.
    Toggle = 1,
    /// ⌃⌥⇧S: switch to the account with the most quota left.
    SwitchBest = 2,
}

pub const TOGGLE_LABEL: &str = "⌃⌥S";
pub const SWITCH_BEST_LABEL: &str = "⌃⌥⇧S";

type OsStatus = i32;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type Handler = extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OsStatus;

#[repr(C)]
#[derive(Default)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: Handler,
        num_types: usize,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut EventHandlerRef,
    ) -> OsStatus;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: EventTargetRef,
        options: u32,
        out_ref: *mut EventHotKeyRef,
    ) -> OsStatus;
    fn UnregisterEventHotKey(hotkey: EventHotKeyRef) -> OsStatus;
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OsStatus;
}

const fn four_cc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SIGNATURE: u32 = four_cc(b"SWPB");
const K_EVENT_CLASS_KEYBOARD: u32 = four_cc(b"keyb");
const K_EVENT_HOT_KEY_PRESSED: u32 = 5;
const K_EVENT_PARAM_DIRECT_OBJECT: u32 = four_cc(b"----");
const TYPE_EVENT_HOT_KEY_ID: u32 = four_cc(b"hkid");

const SHIFT: u32 = 0x0200;
const OPTION: u32 = 0x0800;
const CONTROL: u32 = 0x1000;
const KEY_S: u32 = 0x01; // kVK_ANSI_S

type Callback = Box<dyn Fn(Hotkey)>;

thread_local! {
    static ON_HOTKEY: RefCell<Option<Callback>> = RefCell::new(None);
    static INSTALLED: RefCell<bool> = const { RefCell::new(false) };
    static REGISTERED: RefCell<Vec<EventHotKeyRef>> = const { RefCell::new(Vec::new()) };
}

extern "C" fn handler(_call: EventHandlerCallRef, event: EventRef, _data: *mut c_void) -> OsStatus {
    let mut hk = EventHotKeyId::default();
    let status = unsafe {
        GetEventParameter(
            event,
            K_EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyId>(),
            std::ptr::null_mut(),
            (&mut hk as *mut EventHotKeyId).cast(),
        )
    };
    if status != 0 || hk.signature != SIGNATURE {
        return status;
    }
    let key = match hk.id {
        1 => Hotkey::Toggle,
        2 => Hotkey::SwitchBest,
        _ => return 0,
    };
    crate::debug_log(format_args!("hotkey {key:?}"));
    ON_HOTKEY.with(|cb| {
        if let Some(f) = cb.borrow().as_ref() {
            f(key);
        }
    });
    0
}

pub fn set_handler(f: Box<dyn Fn(Hotkey)>) {
    ON_HOTKEY.with(|cb| *cb.borrow_mut() = Some(f));
}

/// Register or unregister both shortcuts. Safe to call repeatedly.
pub fn set_enabled(on: bool) {
    REGISTERED.with(|reg| {
        let mut reg = reg.borrow_mut();
        if on == !reg.is_empty() {
            return;
        }
        unsafe {
            if !on {
                for r in reg.drain(..) {
                    UnregisterEventHotKey(r);
                }
                return;
            }
            let target = GetApplicationEventTarget();
            INSTALLED.with(|installed| {
                if !*installed.borrow() {
                    let spec = EventTypeSpec {
                        event_class: K_EVENT_CLASS_KEYBOARD,
                        event_kind: K_EVENT_HOT_KEY_PRESSED,
                    };
                    let status = InstallEventHandler(
                        target,
                        handler,
                        1,
                        &spec,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    );
                    *installed.borrow_mut() = status == 0;
                }
            });
            for (key, mods, id) in [
                (KEY_S, CONTROL | OPTION, Hotkey::Toggle),
                (KEY_S, CONTROL | OPTION | SHIFT, Hotkey::SwitchBest),
            ] {
                let mut r: EventHotKeyRef = std::ptr::null_mut();
                let hk = EventHotKeyId {
                    signature: SIGNATURE,
                    id: id as u32,
                };
                let status = RegisterEventHotKey(key, mods, hk, target, 0, &mut r);
                if status == 0 {
                    reg.push(r);
                } else {
                    // another app owns the combination
                    eprintln!("hotkey {id:?} not registered: OSStatus {status}");
                }
            }
        }
    });
}

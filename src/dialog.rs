//! Native NSAlert prompts: confirmations and one-line text input (gpui has no text field).
//! The alert runs modally from a main-queue block, outside any gpui update, and the answer
//! comes back over a channel.

use cocoa::base::{YES, id, nil};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;
use std::sync::mpsc;

unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

type Job = Box<dyn FnOnce() + Send>;

extern "C" fn run_job(ctx: *mut c_void) {
    let job = unsafe { Box::from_raw(ctx as *mut Job) };
    job();
}

/// Run `f` on the main thread on a later turn of the run loop.
fn on_main(f: impl FnOnce() + Send + 'static) {
    let job: Box<Job> = Box::new(Box::new(f));
    unsafe {
        dispatch_async_f(
            &_dispatch_main_q as *const c_void,
            Box::into_raw(job) as *mut c_void,
            run_job,
        )
    };
}

pub struct Alert {
    pub title: String,
    pub message: String,
    pub ok: String,
    /// Red OK button, for removals.
    pub destructive: bool,
    /// Show a text field with this initial value.
    pub input: Option<String>,
    pub placeholder: Option<String>,
}

impl Alert {
    pub fn confirm(
        title: impl Into<String>,
        message: impl Into<String>,
        ok: impl Into<String>,
    ) -> Alert {
        Alert {
            title: title.into(),
            message: message.into(),
            ok: ok.into(),
            destructive: false,
            input: None,
            placeholder: None,
        }
    }

    pub fn destructive(mut self) -> Alert {
        self.destructive = true;
        self
    }

    pub fn with_input(mut self, value: impl Into<String>, placeholder: impl Into<String>) -> Alert {
        self.input = Some(value.into());
        self.placeholder = Some(placeholder.into());
        self
    }
}

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

/// Show the alert. The receiver yields `Some(text)` for OK (empty without a text
/// field) and `None` for Cancel.
pub fn show(alert: Alert) -> mpsc::Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    on_main(move || {
        let _ = tx.send(run_modal(&alert));
    });
    rx
}

fn run_modal(a: &Alert) -> Option<String> {
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let alert: id = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setMessageText: ns(&a.title)];
        let _: () = msg_send![alert, setInformativeText: ns(&a.message)];
        let ok: id = msg_send![alert, addButtonWithTitle: ns(&a.ok)];
        if a.destructive {
            let _: () = msg_send![ok, setHasDestructiveAction: YES];
        }
        let _: id = msg_send![alert, addButtonWithTitle: ns(crate::i18n::l("Cancel", "İptal"))];
        let field: id = match &a.input {
            Some(value) => {
                let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(280.0, 24.0));
                let field: id = msg_send![class!(NSTextField), alloc];
                let field: id = msg_send![field, initWithFrame: frame];
                let _: () = msg_send![field, setStringValue: ns(value)];
                if let Some(p) = &a.placeholder {
                    let _: () = msg_send![field, setPlaceholderString: ns(p)];
                }
                let _: () = msg_send![alert, setAccessoryView: field];
                let _: () = msg_send![alert, layout];
                let window: id = msg_send![alert, window];
                let _: () = msg_send![window, setInitialFirstResponder: field];
                let _: id = msg_send![field, autorelease];
                field
            }
            None => nil,
        };
        // Swapbar is an accessory app; bring the alert to the front.
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, activateIgnoringOtherApps: YES];
        let response: isize = msg_send![alert, runModal];
        // NSAlertFirstButtonReturn
        let answer = (response == 1000).then(|| {
            if field == nil {
                String::new()
            } else {
                to_string(msg_send![field, stringValue])
            }
        });
        let _: () = msg_send![alert, release];
        pool.drain();
        answer
    }
}

/// Show `alert` and call `then` with the answer on the gpui side.
pub fn ask(
    cx: &mut gpui::App,
    alert: Alert,
    then: impl FnOnce(Option<String>, &mut gpui::App) + 'static,
) {
    let rx = show(alert);
    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
        let answer = cx
            .background_executor()
            .spawn(async move { rx.recv().ok().flatten() })
            .await;
        let _ = cx.update(|cx| then(answer, cx));
    })
    .detach();
}

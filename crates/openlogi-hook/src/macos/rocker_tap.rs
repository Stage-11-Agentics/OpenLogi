//! The tap side of the right-to-left rocker (see [`crate::rocker`]): read a
//! tapped `CGEvent` into a rocker input, and apply the step the rocker names.
//!
//! Nothing here waits. A held-back right press is kept as a copy of its
//! event. A replay posts that copy with `CGEventTapPostEvent`, which returns
//! at once and inserts it just past this tap, so the tap never sees it
//! again; per CoreGraphics, a posted event enters the system before the
//! event the callback returns, so the current event, returned as it is,
//! follows the replay. Every posted event carries OpenLogi's synthetic
//! marker.

use std::cell::RefCell;

use core_graphics::event::{CGEvent, CGEventType, CallbackResult, EventField};
use core_graphics::geometry::CGPoint;
use foreign_types_shared::ForeignType as _;
use tracing::warn;

use super::sender::event_sender_id;
use super::translate::{button_source, is_openlogi_synthetic};
use super::{deliver, run_tap_callback};
use crate::rocker::{Input, RightPress, Rocker, Step};
use crate::{ButtonId, EventDisposition, HookEvent, MouseEvent};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    // `core-graphics` clones a `CGEvent` by retaining the same object; a held
    // press needs an independent copy the window server cannot reuse.
    fn CGEventCreateCopy(event: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

/// An independent copy of `event`, marked as OpenLogi's own.
fn marked_copy(event: &CGEvent) -> Option<CGEvent> {
    // SAFETY: `event` is a live CGEventRef; `CGEventCreateCopy` returns a +1
    // copy (or null), whose ownership `from_ptr` takes.
    let copy = unsafe { CGEventCreateCopy(event.as_ptr().cast()) };
    if copy.is_null() {
        return None;
    }
    // SAFETY: non-null, +1 retained, owned from here on.
    let copy = unsafe { CGEvent::from_ptr(copy.cast()) };
    copy.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        openlogi_inject::SYNTHETIC_EVENT_USER_DATA,
    );
    Some(copy)
}

/// The held-back right press, owned by the tap closure and only ever touched
/// on the tap thread.
struct HeldEvent(CGEvent);

// SAFETY: `CGEvent` is a Core Foundation object, safe to retain and release
// from any thread. The value is created, read and dropped only on the tap
// thread; `Send` is needed only because the tap closure as a whole is.
unsafe impl Send for HeldEvent {}

/// The rocker and the event it holds back, for one tap.
#[derive(Default)]
pub(super) struct TapRocker {
    rocker: Rocker,
    held: Option<HeldEvent>,
}

impl Drop for TapRocker {
    fn drop(&mut self) {
        // The tap is gone (stopped, or Accessibility revoked) with a right
        // press held back. It cannot be posted any more; say so. The app is
        // left with nothing stuck: it never saw the press.
        if self.held.is_some() {
            warn!("event tap stopped while holding a right press back — that right click is lost");
        }
    }
}

/// Reduce a tapped event to a rocker input. Only button edges read fields;
/// drags read their location. A click OpenLogi posted never counts.
fn input(etype: CGEventType, event: &CGEvent) -> Option<Input> {
    Some(match etype {
        CGEventType::MouseMoved => Input::Moved,
        CGEventType::LeftMouseDragged | CGEventType::RightMouseDragged => {
            let at = event.location();
            Input::Dragged { x: at.x, y: at.y }
        }
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            Input::Interrupted
        }
        CGEventType::LeftMouseDown
        | CGEventType::LeftMouseUp
        | CGEventType::RightMouseDown
        | CGEventType::RightMouseUp
            if is_openlogi_synthetic(event) =>
        {
            return None;
        }
        CGEventType::RightMouseDown => {
            let at = event.location();
            Input::RightDown(RightPress {
                x: at.x,
                y: at.y,
                sender: event_sender_id(event),
            })
        }
        CGEventType::RightMouseUp => Input::RightUp,
        CGEventType::LeftMouseDown => Input::LeftDown {
            sender: event_sender_id(event),
        },
        CGEventType::LeftMouseUp => Input::LeftUp,
        _ => return None,
    })
}

fn rocker_edge(pressed: bool, device: Option<crate::EventDevice>) -> HookEvent {
    HookEvent::Mouse(MouseEvent::Button {
        id: ButtonId::RockerRightLeft,
        pressed,
        device,
    })
}

fn rocker_lost() -> HookEvent {
    HookEvent::Mouse(MouseEvent::ButtonLost {
        id: ButtonId::RockerRightLeft,
    })
}

/// Where a replayed event is posted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Route {
    /// `CGEventTapPostEvent` through this tap's proxy: the event enters the
    /// stream just past the tap, before the event the callback returns.
    AfterTap,
    /// `CGEventPost` at the HID level, for the one moment the proxy may not
    /// serve: the tap-disabled notice. The tap sees the event again, and
    /// ignores it by its synthetic marker.
    Hid,
}

/// Post the held press (and with `click`, a release for it), then let the
/// current event take its ordinary path, which puts it after the replay.
/// With nothing held, which the state machine never allows, only the current
/// event goes through.
///
/// The current event always reaches the app here: once its held press is
/// replayed, it is the app's, and a press the app got must get its release.
fn replay(
    held: Option<HeldEvent>,
    click: bool,
    post: &dyn Fn(&CGEvent, Route),
    cb: &dyn Fn(HookEvent) -> EventDisposition,
    etype: CGEventType,
    event: &CGEvent,
) -> CallbackResult {
    let route = if matches!(
        etype,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        Route::Hid
    } else {
        Route::AfterTap
    };
    if let Some(HeldEvent(down)) = held {
        if click {
            // A whole click recovers a lost edge, when the pointer may be
            // far from the press: click where it is, so posting at the HID
            // level never warps it back.
            if let Some(at) = crate::cursor_position() {
                down.set_location(CGPoint::new(at.x, at.y));
            }
        }
        post(&down, route);
        if click && let Some(up) = marked_copy(&down) {
            up.set_type(CGEventType::RightMouseUp);
            up.set_double_value_field(EventField::MOUSE_EVENT_PRESSURE, 0.0);
            post(&up, route);
        }
    }
    let _ = run_tap_callback(cb, etype, event);
    CallbackResult::Keep
}

/// Whether `input` may move the rocker. A bare move proves every button up
/// only when a device made it: while a press is held or a chord is under
/// way, a move another program posted (a KVM, a remote-control tool) must
/// not end it. Idle, motion costs no FFI call at all.
fn counts(input: Input, tap: &RefCell<TapRocker>, event: &CGEvent) -> bool {
    input != Input::Moved
        || tap.try_borrow().is_ok_and(|tap| tap.rocker.is_idle())
        || event_sender_id(event).is_some()
}

/// Run one tapped event through the rocker, then through `cb`. `post`
/// puts a replayed event back into the stream.
///
/// The state is borrowed only to step it or to record a decision, never
/// across a callback. A borrow that cannot be taken (it never can be, short
/// of a re-entrant tap) leaves the event on its ordinary path.
pub(super) fn tap_event(
    cb: &dyn Fn(HookEvent) -> EventDisposition,
    post: &dyn Fn(&CGEvent, Route),
    tap: &RefCell<TapRocker>,
    etype: CGEventType,
    event: &CGEvent,
) -> CallbackResult {
    let step = input(etype, event)
        .filter(|input| counts(*input, tap, event))
        .and_then(|input| Some(tap.try_borrow_mut().ok()?.rocker.step(input)))
        .unwrap_or(Step::Pass);
    match step {
        Step::Pass => run_tap_callback(cb, etype, event),
        Step::Swallow => CallbackResult::Drop,
        Step::SwallowAndRelease => {
            deliver(cb, rocker_edge(false, None));
            CallbackResult::Drop
        }
        Step::AsMove => {
            event.set_type(CGEventType::MouseMoved);
            run_tap_callback(cb, CGEventType::MouseMoved, event)
        }
        Step::CancelThenPass => {
            deliver(cb, rocker_lost());
            run_tap_callback(cb, etype, event)
        }
        Step::Replay { click } => {
            let held = tap
                .try_borrow_mut()
                .ok()
                .and_then(|mut tap| tap.held.take());
            replay(held, click, post, cb, etype, event)
        }
        Step::Query(press) => {
            let device = button_source(event);
            let wanted = deliver(cb, HookEvent::Mouse(MouseEvent::RockerQuery { device }))
                == EventDisposition::Suppress;
            // Hold the press back only with a copy in hand to replay: a
            // press held without one could never be given back.
            let copy = if wanted { marked_copy(event) } else { None };
            let held = copy.is_some_and(|copy| {
                tap.try_borrow_mut().is_ok_and(|mut tap| {
                    tap.rocker.hold(press);
                    tap.held = Some(HeldEvent(copy));
                    true
                })
            });
            if held {
                CallbackResult::Drop
            } else {
                run_tap_callback(cb, etype, event)
            }
        }
        Step::Offer => {
            let accepted =
                deliver(cb, rocker_edge(true, button_source(event))) == EventDisposition::Suppress;
            // Record the decision and take the held press in one borrow.
            let decided = tap.try_borrow_mut().ok().map(|mut tap| {
                if accepted {
                    tap.rocker.accept();
                } else {
                    tap.rocker.decline();
                }
                tap.held.take()
            });
            match decided {
                // Accepted: the app never sees either press.
                Some(_) if accepted => CallbackResult::Drop,
                Some(held) => replay(held, false, post, cb, etype, event),
                None => {
                    if accepted {
                        deliver(cb, rocker_lost());
                    }
                    run_tap_callback(cb, etype, event)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

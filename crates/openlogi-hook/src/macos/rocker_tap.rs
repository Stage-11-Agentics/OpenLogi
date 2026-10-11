//! The tap side of the left-to-right rocker (see [`crate::rocker`]): read a
//! tapped `CGEvent` into a rocker input, and apply the step the rocker names.
//!
//! Everything here edits the event in place and returns at once. Nothing is
//! posted, so nothing re-enters the tap, and nothing waits: the rewritten
//! event simply continues down the event stream.

use std::cell::RefCell;

use core_graphics::event::{CGEvent, CGEventType, CallbackResult, EventField};
use core_graphics::geometry::CGPoint;

use super::sender::event_sender_id;
use super::translate::{button_source, is_openlogi_synthetic};
use super::{deliver, run_tap_callback};
use crate::rocker::{Input, PrimaryPress, Rocker, Step};
use crate::{ButtonId, EventDisposition, HookEvent, MouseEvent};

/// Reduce a tapped event to a rocker input. Pointer motion costs no FFI call;
/// only primary and secondary presses read fields off the event, and only
/// physical ones count: a click OpenLogi synthesised never starts a chord.
fn input(etype: CGEventType, event: &CGEvent) -> Option<Input> {
    Some(match etype {
        CGEventType::MouseMoved => Input::Moved,
        CGEventType::LeftMouseDragged | CGEventType::RightMouseDragged => Input::Dragged,
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
        CGEventType::LeftMouseDown => {
            let at = event.location();
            Input::PrimaryDown(PrimaryPress {
                x: at.x,
                y: at.y,
                click_state: event.get_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE),
                event_number: event.get_integer_value_field(EventField::MOUSE_EVENT_NUMBER),
                sender: event_sender_id(event),
            })
        }
        CGEventType::LeftMouseUp => Input::PrimaryUp,
        CGEventType::RightMouseDown => Input::SecondaryDown {
            sender: event_sender_id(event),
        },
        CGEventType::RightMouseUp => Input::SecondaryUp,
        _ => return None,
    })
}

/// Turn the accepted secondary press into the release of `press`: same
/// location, click count and event number, so the app ends the click it
/// already started instead of seeing a second button.
fn rewrite_as_primary_release(event: &CGEvent, press: PrimaryPress) {
    event.set_type(CGEventType::LeftMouseUp);
    event.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, 0);
    event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, press.click_state);
    event.set_integer_value_field(EventField::MOUSE_EVENT_NUMBER, press.event_number);
    event.set_double_value_field(EventField::MOUSE_EVENT_PRESSURE, 0.0);
    event.set_location(CGPoint::new(press.x, press.y));
}

fn rocker_edge(pressed: bool, device: Option<crate::EventDevice>) -> HookEvent {
    HookEvent::Mouse(MouseEvent::Button {
        id: ButtonId::RockerLeftRight,
        pressed,
        device,
    })
}

/// Run one tapped event through the rocker, then through `cb`.
///
/// The rocker state is borrowed only to step it, never across a callback. A
/// borrow that cannot be taken (it never can be, short of a re-entrant tap)
/// leaves the event on its ordinary path.
pub(super) fn tap_event(
    cb: &dyn Fn(HookEvent) -> EventDisposition,
    rocker: &RefCell<Rocker>,
    etype: CGEventType,
    event: &CGEvent,
) -> CallbackResult {
    let step = input(etype, event)
        .and_then(|input| Some(rocker.try_borrow_mut().ok()?.step(input)))
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
            deliver(cb, HookEvent::Mouse(MouseEvent::CaptureInterrupted));
            run_tap_callback(cb, etype, event)
        }
        Step::Offer(press) => {
            if deliver(cb, rocker_edge(true, button_source(event))) == EventDisposition::Suppress {
                if let Ok(mut rocker) = rocker.try_borrow_mut() {
                    rocker.accept();
                }
                rewrite_as_primary_release(event, press);
                CallbackResult::Keep
            } else {
                run_tap_callback(cb, etype, event)
            }
        }
    }
}

#[cfg(test)]
mod tests;

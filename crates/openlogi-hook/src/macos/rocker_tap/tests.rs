//! The rocker's tap glue against real (never posted) `CGEvent`s.

use std::sync::Mutex;

use core_graphics::event::CGMouseButton;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

use super::*;

fn mouse_event(etype: CGEventType, x: f64, y: f64, button: CGMouseButton) -> CGEvent {
    let source = CGEventSource::new(CGEventSourceStateID::Private)
        .expect("CGEventSourceCreate must succeed");
    CGEvent::new_mouse_event(source, etype, CGPoint::new(x, y), button)
        .expect("CGEventCreateMouseEvent must succeed")
}

/// `CGEventType` has no `PartialEq`; compare discriminants.
fn kind(event: &CGEvent) -> u32 {
    event.get_type() as u32
}

/// A consumer that records every event it is handed and accepts (or declines)
/// the rocker's down edge.
fn record(seen: &Mutex<Vec<String>>, accept: bool, event: &HookEvent) -> EventDisposition {
    let label = match event {
        HookEvent::Mouse(MouseEvent::Button { id, pressed, .. }) => {
            format!("{id:?} {}", if *pressed { "down" } else { "up" })
        }
        HookEvent::Mouse(MouseEvent::Moved { .. }) => "moved".to_owned(),
        other => format!("{other:?}"),
    };
    seen.lock().expect("seen").push(label);
    match event {
        HookEvent::Mouse(MouseEvent::Button {
            id: ButtonId::RockerLeftRight,
            pressed: true,
            ..
        }) if accept => EventDisposition::Suppress,
        _ => EventDisposition::PassThrough,
    }
}

fn left_down(rocker: &RefCell<Rocker>, cb: &dyn Fn(HookEvent) -> EventDisposition) {
    let down = mouse_event(CGEventType::LeftMouseDown, 10.0, 20.0, CGMouseButton::Left);
    down.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2);
    down.set_integer_value_field(EventField::MOUSE_EVENT_NUMBER, 77);
    assert!(matches!(
        tap_event(cb, rocker, CGEventType::LeftMouseDown, &down),
        CallbackResult::Keep
    ));
    assert_eq!(kind(&down), CGEventType::LeftMouseDown as u32, "untouched");
}

#[test]
fn an_accepted_rocker_rewrites_the_right_press_into_the_left_release() {
    let seen = Mutex::new(Vec::new());
    let cb = |event| record(&seen, true, &event);
    let rocker = RefCell::new(Rocker::default());
    left_down(&rocker, &cb);

    let right = mouse_event(
        CGEventType::RightMouseDown,
        13.0,
        24.0,
        CGMouseButton::Right,
    );
    assert!(matches!(
        tap_event(&cb, &rocker, CGEventType::RightMouseDown, &right),
        CallbackResult::Keep
    ));
    assert_eq!(kind(&right), CGEventType::LeftMouseUp as u32);
    assert_eq!(
        right.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER),
        0
    );
    assert_eq!(
        right.get_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE),
        2
    );
    assert_eq!(
        right.get_integer_value_field(EventField::MOUSE_EVENT_NUMBER),
        77
    );
    let at = right.location();
    assert!(
        (at.x - 13.0).abs() < f64::EPSILON && (at.y - 24.0).abs() < f64::EPSILON,
        "released where the pointer is, never warped, got ({}, {})",
        at.x,
        at.y
    );

    let drag = mouse_event(
        CGEventType::LeftMouseDragged,
        15.0,
        26.0,
        CGMouseButton::Left,
    );
    assert!(matches!(
        tap_event(&cb, &rocker, CGEventType::LeftMouseDragged, &drag),
        CallbackResult::Keep
    ));
    assert_eq!(
        kind(&drag),
        CGEventType::MouseMoved as u32,
        "hover, not a drag"
    );

    let right_up = mouse_event(CGEventType::RightMouseUp, 15.0, 26.0, CGMouseButton::Right);
    assert!(matches!(
        tap_event(&cb, &rocker, CGEventType::RightMouseUp, &right_up),
        CallbackResult::Drop
    ));
    let left_up = mouse_event(CGEventType::LeftMouseUp, 15.0, 26.0, CGMouseButton::Left);
    assert!(matches!(
        tap_event(&cb, &rocker, CGEventType::LeftMouseUp, &left_up),
        CallbackResult::Drop
    ));

    // The next ordinary click passes untouched again.
    let click = mouse_event(CGEventType::LeftMouseDown, 30.0, 30.0, CGMouseButton::Left);
    assert!(matches!(
        tap_event(&cb, &rocker, CGEventType::LeftMouseDown, &click),
        CallbackResult::Keep
    ));
    assert_eq!(kind(&click), CGEventType::LeftMouseDown as u32);

    assert_eq!(
        *seen.lock().expect("seen"),
        [
            "LeftClick down",
            "RockerLeftRight down",
            "moved",
            "RockerLeftRight up",
            "LeftClick down",
        ]
    );
}

#[test]
fn a_declined_rocker_leaves_the_right_click_untouched() {
    let seen = Mutex::new(Vec::new());
    let cb = |event| record(&seen, false, &event);
    let rocker = RefCell::new(Rocker::default());
    left_down(&rocker, &cb);

    let right = mouse_event(
        CGEventType::RightMouseDown,
        13.0,
        24.0,
        CGMouseButton::Right,
    );
    assert!(matches!(
        tap_event(&cb, &rocker, CGEventType::RightMouseDown, &right),
        CallbackResult::Keep
    ));
    assert_eq!(kind(&right), CGEventType::RightMouseDown as u32);
    let drag = mouse_event(
        CGEventType::LeftMouseDragged,
        15.0,
        26.0,
        CGMouseButton::Left,
    );
    tap_event(&cb, &rocker, CGEventType::LeftMouseDragged, &drag);
    assert_eq!(kind(&drag), CGEventType::LeftMouseDragged as u32);

    assert_eq!(
        *seen.lock().expect("seen"),
        [
            "LeftClick down",
            "RockerLeftRight down",
            "RightClick down",
            "moved",
        ]
    );
}

#[test]
fn openlogi_synthetic_clicks_never_start_a_chord() {
    let seen = Mutex::new(Vec::new());
    let cb = |event| record(&seen, true, &event);
    let rocker = RefCell::new(Rocker::default());

    let down = mouse_event(CGEventType::LeftMouseDown, 10.0, 20.0, CGMouseButton::Left);
    down.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        openlogi_inject::SYNTHETIC_EVENT_USER_DATA,
    );
    tap_event(&cb, &rocker, CGEventType::LeftMouseDown, &down);
    let right = mouse_event(
        CGEventType::RightMouseDown,
        13.0,
        24.0,
        CGMouseButton::Right,
    );
    tap_event(&cb, &rocker, CGEventType::RightMouseDown, &right);
    assert_eq!(kind(&right), CGEventType::RightMouseDown as u32);
    assert_eq!(*seen.lock().expect("seen"), ["RightClick down"]);
}

#[test]
fn a_lost_release_cancels_only_the_rocker_never_every_hold() {
    let seen = Mutex::new(Vec::new());
    let cb = |event| record(&seen, true, &event);
    let rocker = RefCell::new(Rocker::default());
    left_down(&rocker, &cb);
    let right = mouse_event(
        CGEventType::RightMouseDown,
        13.0,
        24.0,
        CGMouseButton::Right,
    );
    tap_event(&cb, &rocker, CGEventType::RightMouseDown, &right);

    // Both releases went missing: a bare move proves the buttons are up.
    let moved = mouse_event(CGEventType::MouseMoved, 20.0, 30.0, CGMouseButton::Left);
    assert!(matches!(
        tap_event(&cb, &rocker, CGEventType::MouseMoved, &moved),
        CallbackResult::Keep
    ));
    assert_eq!(
        *seen.lock().expect("seen"),
        [
            "LeftClick down",
            "RockerLeftRight down",
            "Mouse(ButtonLost { id: RockerLeftRight })",
            "moved",
        ]
    );
}

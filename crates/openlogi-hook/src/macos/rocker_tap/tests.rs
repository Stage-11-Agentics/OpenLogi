//! The rocker's tap glue against real `CGEvent`s. Nothing is ever posted to
//! the system: the poster under test only records what it is handed.

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

fn name(etype: u32) -> &'static str {
    match etype {
        t if t == CGEventType::LeftMouseDown as u32 => "left down",
        t if t == CGEventType::LeftMouseUp as u32 => "left up",
        t if t == CGEventType::RightMouseDown as u32 => "right down",
        t if t == CGEventType::RightMouseUp as u32 => "right up",
        t if t == CGEventType::RightMouseDragged as u32 => "right dragged",
        t if t == CGEventType::MouseMoved as u32 => "moved",
        _ => "other",
    }
}

/// One tap under test: the consumer's view, what was posted, and what the
/// app receives in order (kept events and posted ones).
struct Harness {
    accept: bool,
    seen: Mutex<Vec<String>>,
    posted: Mutex<Vec<String>>,
    app: Mutex<Vec<String>>,
    tap: RefCell<TapRocker>,
}

impl Harness {
    fn new(accept: bool) -> Self {
        Self {
            accept,
            seen: Mutex::new(Vec::new()),
            posted: Mutex::new(Vec::new()),
            app: Mutex::new(Vec::new()),
            tap: RefCell::new(TapRocker::default()),
        }
    }

    fn consumer(&self, event: &HookEvent) -> EventDisposition {
        let label = match event {
            HookEvent::Mouse(MouseEvent::Button { id, pressed, .. }) => {
                format!("{id:?} {}", if *pressed { "down" } else { "up" })
            }
            HookEvent::Mouse(MouseEvent::Moved { .. }) => "moved".to_owned(),
            HookEvent::Mouse(MouseEvent::RockerQuery { .. }) => "query".to_owned(),
            other => format!("{other:?}"),
        };
        self.seen.lock().expect("seen").push(label);
        match event {
            HookEvent::Mouse(
                MouseEvent::Button {
                    id: ButtonId::RockerRightLeft,
                    pressed: true,
                    ..
                }
                | MouseEvent::RockerQuery { .. },
            ) if self.accept => EventDisposition::Suppress,
            _ => EventDisposition::PassThrough,
        }
    }

    /// Run one event and return what the tap did with it.
    fn run(&self, etype: CGEventType, x: f64, y: f64) -> CallbackResult {
        let button = match etype {
            CGEventType::LeftMouseDown | CGEventType::LeftMouseUp => CGMouseButton::Left,
            _ => CGMouseButton::Right,
        };
        let event = mouse_event(etype, x, y, button);
        let cb = |event| self.consumer(&event);
        // A posted event enters the system before the event the callback
        // returns, so the app log puts posts first.
        let post = |posted: &CGEvent, route: Route| {
            assert!(
                is_openlogi_synthetic(posted),
                "every posted event is marked"
            );
            assert_eq!(route, Route::AfterTap, "only an interruption posts at HID");
            let label = name(kind(posted)).to_owned();
            self.posted.lock().expect("posted").push(label.clone());
            self.app.lock().expect("app").push(label);
        };
        let result = tap_event(&cb, &post, &self.tap, etype, &event);
        if matches!(result, CallbackResult::Keep) {
            self.app
                .lock()
                .expect("app")
                .push(name(kind(&event)).to_owned());
        }
        result
    }

    fn app(&self) -> Vec<String> {
        self.app.lock().expect("app").clone()
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("seen").clone()
    }
}

#[test]
fn the_chord_reaches_the_app_as_hover_only() {
    let h = Harness::new(true);
    assert!(matches!(
        h.run(CGEventType::RightMouseDown, 10.0, 20.0),
        CallbackResult::Drop
    ));
    assert!(matches!(
        h.run(CGEventType::RightMouseDragged, 12.0, 21.0),
        CallbackResult::Keep
    ));
    assert!(matches!(
        h.run(CGEventType::LeftMouseDown, 12.0, 21.0),
        CallbackResult::Drop
    ));
    h.run(CGEventType::RightMouseDragged, 40.0, 60.0);
    assert!(matches!(
        h.run(CGEventType::LeftMouseUp, 40.0, 60.0),
        CallbackResult::Drop
    ));
    assert!(matches!(
        h.run(CGEventType::RightMouseUp, 40.0, 60.0),
        CallbackResult::Drop
    ));

    assert_eq!(h.app(), ["moved", "moved"], "no click of the chord");
    assert!(h.posted.lock().expect("posted").is_empty());
    assert_eq!(
        h.seen(),
        [
            "query",
            "moved",
            "RockerRightLeft down",
            "moved",
            "RockerRightLeft up",
        ]
    );
}

#[test]
fn a_right_click_alone_is_replayed_in_order() {
    let h = Harness::new(true);
    h.run(CGEventType::RightMouseDown, 10.0, 20.0);
    h.run(CGEventType::RightMouseDragged, 11.0, 20.0);
    assert!(matches!(
        h.run(CGEventType::RightMouseUp, 11.0, 20.0),
        CallbackResult::Keep
    ));
    assert_eq!(h.app(), ["moved", "right down", "right up"]);
}

#[test]
fn a_right_drag_past_the_slop_replays_the_press_then_drags() {
    let h = Harness::new(true);
    h.run(CGEventType::RightMouseDown, 10.0, 20.0);
    h.run(CGEventType::RightMouseDragged, 13.0, 20.0);
    h.run(CGEventType::RightMouseDragged, 30.0, 20.0);
    h.run(CGEventType::RightMouseDragged, 50.0, 20.0);
    h.run(CGEventType::RightMouseUp, 50.0, 20.0);
    assert_eq!(
        h.app(),
        [
            "moved",
            "right down",
            "right dragged",
            "right dragged",
            "right up",
        ]
    );
}

#[test]
fn a_device_without_a_rocker_gets_its_right_click_at_once() {
    let h = Harness::new(false);
    assert!(matches!(
        h.run(CGEventType::RightMouseDown, 10.0, 20.0),
        CallbackResult::Keep
    ));
    h.run(CGEventType::LeftMouseDown, 10.0, 20.0);
    h.run(CGEventType::LeftMouseUp, 10.0, 20.0);
    h.run(CGEventType::RightMouseUp, 10.0, 20.0);
    assert_eq!(h.app(), ["right down", "left down", "left up", "right up"]);
    assert!(h.posted.lock().expect("posted").is_empty());
}

#[test]
fn left_clicks_never_wait_on_anything() {
    let h = Harness::new(true);
    assert!(matches!(
        h.run(CGEventType::LeftMouseDown, 10.0, 20.0),
        CallbackResult::Keep
    ));
    // A right press made while the left is held is not even queried.
    assert!(matches!(
        h.run(CGEventType::RightMouseDown, 10.0, 20.0),
        CallbackResult::Keep
    ));
    h.run(CGEventType::RightMouseUp, 10.0, 20.0);
    h.run(CGEventType::LeftMouseUp, 10.0, 20.0);
    assert_eq!(h.app(), ["left down", "right down", "right up", "left up"]);
    assert!(!h.seen().contains(&"query".to_owned()));
}

#[test]
fn a_declined_offer_replays_the_right_press_before_the_left() {
    // Holds right presses back but declines the rocker itself, as when the
    // press queue is full.
    let h = Harness::new(true);
    h.run(CGEventType::RightMouseDown, 10.0, 20.0);
    let declining = |event: HookEvent| match event {
        HookEvent::Mouse(MouseEvent::RockerQuery { .. }) => EventDisposition::Suppress,
        _ => EventDisposition::PassThrough,
    };
    let left = mouse_event(CGEventType::LeftMouseDown, 10.0, 20.0, CGMouseButton::Left);
    let posted = Mutex::new(Vec::new());
    let post = |event: &CGEvent, _: Route| posted.lock().expect("posted").push(name(kind(event)));
    // The held right press goes first; the left press itself follows.
    assert!(matches!(
        tap_event(&declining, &post, &h.tap, CGEventType::LeftMouseDown, &left),
        CallbackResult::Keep
    ));
    assert_eq!(*posted.lock().expect("posted"), ["right down"]);
}

#[test]
fn a_lost_release_mid_chord_cancels_only_the_rocker() {
    let h = Harness::new(true);
    h.run(CGEventType::RightMouseDown, 10.0, 20.0);
    h.run(CGEventType::LeftMouseDown, 10.0, 20.0);
    // The right went down again: its release was lost. The new press is the
    // app's; only the rocker's lifecycle is canceled.
    assert!(matches!(
        h.run(CGEventType::RightMouseDown, 20.0, 30.0),
        CallbackResult::Keep
    ));
    assert_eq!(
        h.seen(),
        [
            "query",
            "RockerRightLeft down",
            "Mouse(ButtonLost { id: RockerRightLeft })",
            "RightClick down",
        ]
    );
}

#[test]
fn a_held_press_whose_release_was_lost_is_replayed_as_a_click() {
    let h = Harness::new(true);
    h.run(CGEventType::RightMouseDown, 10.0, 20.0);
    // A second press proves the first one's release went missing.
    h.run(CGEventType::RightMouseDown, 20.0, 30.0);
    assert_eq!(h.app(), ["right down", "right up", "right down"]);
}

#[test]
fn openlogi_synthetic_clicks_never_start_or_join_a_chord() {
    let h = Harness::new(true);
    let right = mouse_event(
        CGEventType::RightMouseDown,
        10.0,
        20.0,
        CGMouseButton::Right,
    );
    right.set_integer_value_field(
        EventField::EVENT_SOURCE_USER_DATA,
        openlogi_inject::SYNTHETIC_EVENT_USER_DATA,
    );
    let cb = |event| h.consumer(&event);
    let post = |_: &CGEvent, _: Route| panic!("nothing to replay");
    assert!(matches!(
        tap_event(&cb, &post, &h.tap, CGEventType::RightMouseDown, &right),
        CallbackResult::Keep
    ));
    assert!(h.seen().is_empty(), "never queried, never translated");
}

#[test]
fn an_interruption_while_holding_replays_a_click_at_the_hid_level() {
    let h = Harness::new(true);
    h.run(CGEventType::RightMouseDown, 10.0, 20.0);
    let notice = mouse_event(CGEventType::MouseMoved, 10.0, 20.0, CGMouseButton::Left);
    let cb = |event| h.consumer(&event);
    let posted = Mutex::new(Vec::new());
    let post = |event: &CGEvent, route: Route| {
        posted
            .lock()
            .expect("posted")
            .push((name(kind(event)), route));
    };
    tap_event(
        &cb,
        &post,
        &h.tap,
        CGEventType::TapDisabledByTimeout,
        &notice,
    );
    assert_eq!(
        *posted.lock().expect("posted"),
        [("right down", Route::Hid), ("right up", Route::Hid)]
    );
    // The real release, arriving after the gap, is not handed out twice.
    assert!(matches!(
        h.run(CGEventType::RightMouseUp, 10.0, 20.0),
        CallbackResult::Drop
    ));
}

#[test]
fn a_move_another_program_posted_never_ends_a_held_press() {
    let h = Harness::new(true);
    h.run(CGEventType::RightMouseDown, 10.0, 20.0);
    // A sender-less move (a KVM, a remote-control tool) while held.
    h.run(CGEventType::MouseMoved, 30.0, 30.0);
    assert!(
        h.posted.lock().expect("posted").is_empty(),
        "nothing replayed"
    );
    // The chord still works.
    assert!(matches!(
        h.run(CGEventType::LeftMouseDown, 30.0, 30.0),
        CallbackResult::Drop
    ));
}

//! The right-to-left rocker's state machine, one ordering per test.

use super::*;

const MOUSE: Option<u64> = Some(7);
const TRACKPAD: Option<u64> = Some(9);

fn right(sender: Option<u64>) -> RightPress {
    RightPress {
        x: 100.0,
        y: 200.0,
        sender,
    }
}

const REPLAY_THEN_CURRENT: Step = Step::Replay { click: false };

/// A right press the consumer agreed to hold back.
fn holding() -> Rocker {
    let mut rocker = Rocker::default();
    assert_eq!(
        rocker.step(Input::RightDown(right(MOUSE))),
        Step::Query(right(MOUSE))
    );
    rocker.hold(right(MOUSE));
    assert!(rocker.is_holding());
    rocker
}

/// Hold the right, press the left, accept.
fn rocking() -> Rocker {
    let mut rocker = holding();
    assert_eq!(rocker.step(Input::LeftDown { sender: MOUSE }), Step::Offer);
    rocker.accept();
    assert!(rocker.is_rocking());
    rocker
}

#[test]
fn left_clicks_and_drags_are_never_touched() {
    let mut rocker = Rocker::default();
    for input in [
        Input::Moved,
        Input::LeftDown { sender: MOUSE },
        Input::Dragged { x: 300.0, y: 300.0 },
        Input::LeftUp,
        Input::LeftDown { sender: TRACKPAD },
        Input::LeftUp,
        Input::Interrupted,
    ] {
        assert_eq!(rocker.step(input), Step::Pass, "{input:?}");
    }
}

#[test]
fn a_declined_query_leaves_the_right_click_alone() {
    let mut rocker = Rocker::default();
    assert!(matches!(
        rocker.step(Input::RightDown(right(TRACKPAD))),
        Step::Query(_)
    ));
    // Not held: everything after it is ordinary.
    for input in [
        Input::Dragged { x: 101.0, y: 200.0 },
        Input::LeftDown { sender: TRACKPAD },
        Input::LeftUp,
        Input::RightUp,
    ] {
        assert_eq!(rocker.step(input), Step::Pass, "{input:?}");
    }
}

#[test]
fn a_right_press_made_while_the_left_is_held_is_never_held_back() {
    let mut rocker = Rocker::default();
    rocker.step(Input::LeftDown { sender: MOUSE });
    assert_eq!(rocker.step(Input::RightDown(right(MOUSE))), Step::Pass);
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
    assert_eq!(rocker.step(Input::LeftUp), Step::Pass);
    // Once the left is up, a right press is a candidate again.
    assert!(matches!(
        rocker.step(Input::RightDown(right(MOUSE))),
        Step::Query(_)
    ));
}

#[test]
fn a_held_right_press_released_alone_is_replayed_as_a_right_click() {
    let mut rocker = holding();
    assert_eq!(rocker.step(Input::RightUp), REPLAY_THEN_CURRENT);
    assert!(!rocker.is_holding());
    assert!(matches!(
        rocker.step(Input::RightDown(right(MOUSE))),
        Step::Query(_)
    ));
}

#[test]
fn small_motion_while_held_is_hover_and_a_real_drag_replays_the_press() {
    let mut rocker = holding();
    assert_eq!(
        rocker.step(Input::Dragged { x: 104.0, y: 204.0 }),
        Step::AsMove,
        "within the slop"
    );
    assert_eq!(
        rocker.step(Input::Dragged { x: 106.0, y: 200.0 }),
        Step::AsMove,
        "exactly the slop is still a candidate"
    );
    assert_eq!(
        rocker.step(Input::Dragged { x: 106.5, y: 200.0 }),
        REPLAY_THEN_CURRENT
    );
    // From here on it is an ordinary right drag.
    assert_eq!(
        rocker.step(Input::Dragged { x: 140.0, y: 200.0 }),
        Step::Pass
    );
    assert_eq!(rocker.step(Input::LeftDown { sender: MOUSE }), Step::Pass);
    assert_eq!(rocker.step(Input::LeftUp), Step::Pass);
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
}

#[test]
fn a_left_press_while_the_right_is_held_back_is_offered() {
    let mut rocker = holding();
    rocker.step(Input::Dragged { x: 102.0, y: 201.0 });
    assert_eq!(rocker.step(Input::LeftDown { sender: MOUSE }), Step::Offer);
}

#[test]
fn a_declined_offer_hands_both_presses_to_the_app() {
    let mut rocker = holding();
    assert_eq!(rocker.step(Input::LeftDown { sender: MOUSE }), Step::Offer);
    rocker.decline();
    assert!(!rocker.is_holding() && !rocker.is_rocking());
    for input in [
        Input::Dragged { x: 140.0, y: 200.0 },
        Input::LeftUp,
        Input::RightUp,
    ] {
        assert_eq!(rocker.step(input), Step::Pass, "{input:?}");
    }
}

#[test]
fn a_left_press_from_another_device_replays_the_held_press() {
    let mut rocker = holding();
    assert_eq!(
        rocker.step(Input::LeftDown { sender: TRACKPAD }),
        REPLAY_THEN_CURRENT
    );
    assert_eq!(rocker.step(Input::LeftUp), Step::Pass);
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
}

#[test]
fn the_chord_is_swallowed_whole_and_the_right_release_ends_it() {
    let mut rocker = rocking();
    assert_eq!(
        rocker.step(Input::Dragged { x: 160.0, y: 240.0 }),
        Step::AsMove
    );
    assert_eq!(rocker.step(Input::LeftUp), Step::Swallow);
    // Another left press during the hold is the chord's, not the app's.
    assert_eq!(
        rocker.step(Input::LeftDown { sender: MOUSE }),
        Step::Swallow
    );
    assert_eq!(rocker.step(Input::LeftUp), Step::Swallow);
    assert_eq!(
        rocker.step(Input::Dragged { x: 170.0, y: 240.0 }),
        Step::AsMove
    );
    assert_eq!(rocker.step(Input::RightUp), Step::SwallowAndRelease);
    assert!(!rocker.is_rocking());
    // Ordinary again.
    assert_eq!(rocker.step(Input::LeftDown { sender: MOUSE }), Step::Pass);
    assert_eq!(rocker.step(Input::LeftUp), Step::Pass);
}

#[test]
fn right_released_first_ends_the_lifecycle_and_still_hides_the_left_release() {
    let mut rocker = rocking();
    assert_eq!(rocker.step(Input::RightUp), Step::SwallowAndRelease);
    assert_eq!(
        rocker.step(Input::Dragged { x: 150.0, y: 200.0 }),
        Step::AsMove
    );
    assert_eq!(rocker.step(Input::LeftUp), Step::Swallow);
    assert!(!rocker.is_rocking());
}

#[test]
fn the_up_edge_is_delivered_exactly_once() {
    let mut rocker = rocking();
    assert_eq!(rocker.step(Input::RightUp), Step::SwallowAndRelease);
    assert_eq!(rocker.step(Input::LeftUp), Step::Swallow);
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
}

#[test]
fn a_new_right_press_after_the_release_reaches_the_app_and_hides_the_chords_left() {
    let mut rocker = rocking();
    rocker.step(Input::RightUp);
    // The left is still down from the chord; the right goes down again.
    assert_eq!(rocker.step(Input::RightDown(right(MOUSE))), Step::Pass);
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
    // The left release the app never saw the press of stays hidden.
    assert_eq!(rocker.step(Input::LeftUp), Step::Swallow);
    assert_eq!(rocker.step(Input::LeftDown { sender: MOUSE }), Step::Pass);
    assert_eq!(rocker.step(Input::LeftUp), Step::Pass);
}

#[test]
fn an_interruption_mid_chord_closes_the_lifecycle_but_keeps_swallowing() {
    let mut rocker = rocking();
    assert_eq!(rocker.step(Input::Interrupted), Step::Pass);
    assert_eq!(
        rocker.step(Input::Dragged { x: 150.0, y: 200.0 }),
        Step::AsMove
    );
    // The consumer canceled on the interruption: no second end.
    assert_eq!(rocker.step(Input::RightUp), Step::Swallow);
    assert_eq!(rocker.step(Input::LeftUp), Step::Swallow);
    assert!(!rocker.is_rocking());
}

#[test]
fn a_move_with_no_button_held_cancels_a_chord_whose_releases_were_lost() {
    let mut rocker = rocking();
    assert_eq!(rocker.step(Input::Moved), Step::CancelThenPass);
    assert!(!rocker.is_rocking());
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
}

#[test]
fn a_fresh_right_press_mid_chord_cancels_and_reaches_the_app() {
    let mut rocker = rocking();
    assert_eq!(
        rocker.step(Input::RightDown(right(MOUSE))),
        Step::CancelThenPass
    );
    assert!(!rocker.is_rocking() && !rocker.is_holding());
}

#[test]
fn a_held_press_whose_release_was_lost_is_replayed_as_a_whole_click() {
    let mut rocker = holding();
    assert_eq!(rocker.step(Input::Moved), Step::Replay { click: true });
    assert!(!rocker.is_holding());
}

#[test]
fn a_second_right_press_replays_the_first_as_a_click() {
    let mut rocker = holding();
    assert_eq!(
        rocker.step(Input::RightDown(right(MOUSE))),
        Step::Replay { click: true }
    );
    // The second press itself reached the app; its release is ordinary.
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
}

#[test]
fn an_interruption_replays_the_held_press_and_hides_a_late_release() {
    let mut rocker = holding();
    assert_eq!(
        rocker.step(Input::Interrupted),
        Step::Replay { click: true }
    );
    // The replayed click already ended it: the real release is swallowed.
    assert_eq!(rocker.step(Input::RightUp), Step::Swallow);
    assert_eq!(rocker.step(Input::RightUp), Step::Pass);
}

#[test]
fn an_orphaned_release_is_forgotten_once_a_move_proves_the_buttons_up() {
    let mut rocker = holding();
    rocker.step(Input::Interrupted);
    assert_eq!(rocker.step(Input::Moved), Step::Pass);
    assert!(matches!(
        rocker.step(Input::RightDown(right(MOUSE))),
        Step::Query(_)
    ));
}

#[test]
fn an_interruption_forgets_a_held_left_so_the_rocker_is_not_refused() {
    let mut rocker = Rocker::default();
    rocker.step(Input::LeftDown { sender: MOUSE });
    // The left release may have been lost in the gap.
    rocker.step(Input::Interrupted);
    assert!(matches!(
        rocker.step(Input::RightDown(right(MOUSE))),
        Step::Query(_)
    ));
}

#[test]
fn hold_accept_and_decline_without_their_step_change_nothing() {
    let mut rocker = Rocker::default();
    rocker.accept();
    rocker.decline();
    assert!(!rocker.is_rocking() && !rocker.is_holding());
    let mut rocker = rocking();
    rocker.hold(right(MOUSE));
    assert!(rocker.is_rocking());
}

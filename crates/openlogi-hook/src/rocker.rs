//! The left-to-right rocker chord, as pure state.
//!
//! Hold the primary button, press the secondary: that press becomes the down
//! edge of [`ButtonId::RockerLeftRight`](crate::ButtonId::RockerLeftRight),
//! and the primary's later release becomes its up edge. Nothing about an
//! ordinary click changes: every event passes through untouched until a
//! secondary press arrives while the primary is held, and even then only if
//! the consumer accepts the rocker for that device.
//!
//! Once accepted, the app must never see the secondary press, and must not
//! be left mid-drag. So the accepted secondary press is rewritten into the
//! primary release, at the primary press's location; drags are retyped into
//! plain moves; and every remaining edge of the chord is swallowed. This
//! module only decides each step. The platform tap applies it, so the logic
//! is testable without a single FFI call.

/// The primary press a rocker may grow from: everything needed to write its
/// release later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PrimaryPress {
    /// Location of the press, in global display points.
    pub(crate) x: f64,
    /// Location of the press, in global display points.
    pub(crate) y: f64,
    /// The press's click count (2 for the second press of a double click).
    pub(crate) click_state: i64,
    /// The press's mouse event number, which its release must repeat.
    pub(crate) event_number: i64,
    /// Registry id of the device that produced the press, when known.
    pub(crate) sender: Option<u64>,
}

/// One input event, reduced to what the rocker needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Input {
    /// A physical primary press.
    PrimaryDown(PrimaryPress),
    /// A physical primary release.
    PrimaryUp,
    /// A physical secondary press, with the registry id of its device.
    SecondaryDown { sender: Option<u64> },
    /// A physical secondary release.
    SecondaryUp,
    /// Pointer motion with the primary or secondary button held.
    Dragged,
    /// Pointer motion with no button held.
    Moved,
    /// The OS stopped delivering events for a while; edges may be lost.
    Interrupted,
}

/// What the tap does with the event that produced an [`Input`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    /// The event takes its ordinary path, untouched.
    Pass,
    /// A secondary press while the primary is held. Offer the rocker's down
    /// edge to the consumer. If it accepts, call [`Rocker::accept`] and
    /// rewrite this event into the release of the given primary press; if it
    /// declines, the event takes its ordinary path (a plain secondary click).
    Offer(PrimaryPress),
    /// Drop the event: the app never sees it.
    Swallow,
    /// Drop the event and deliver the rocker's up edge: the primary came back
    /// up and the rocker's lifecycle is still open.
    SwallowAndRelease,
    /// Retype the drag into a plain move. The cursor still moves; the app
    /// sees hover, not a drag.
    AsMove,
    /// The rocker lost a button edge: cancel the consumer's open lifecycle
    /// (it gets no release, so it selects nothing), then let the event take
    /// its ordinary path.
    CancelThenPass,
}

/// The physical state of an accepted chord.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Chord {
    primary_down: bool,
    secondary_down: bool,
    /// Whether the consumer's lifecycle still waits for its up edge.
    open: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum State {
    /// No primary press is known to be held.
    #[default]
    Idle,
    /// The primary is held and the app has its press: a candidate rocker.
    PrimaryHeld(PrimaryPress),
    /// An accepted rocker. The app has already seen the primary release, so
    /// every remaining edge of the chord is swallowed until both buttons are
    /// physically up.
    Rocking(Chord),
}

/// Rocker state for one event-tap thread.
#[derive(Debug, Default)]
pub(crate) struct Rocker {
    state: State,
}

impl Rocker {
    /// Advance on one input and say what to do with its event.
    pub(crate) fn step(&mut self, input: Input) -> Step {
        match self.state {
            State::Idle => {
                if let Input::PrimaryDown(press) = input {
                    self.state = State::PrimaryHeld(press);
                }
                Step::Pass
            }
            State::PrimaryHeld(held) => match input {
                // A second press without a release proves the release was
                // lost; the newer press is the candidate now.
                Input::PrimaryDown(press) => {
                    self.state = State::PrimaryHeld(press);
                    Step::Pass
                }
                // A move with no button held proves the primary is up.
                Input::PrimaryUp | Input::Moved => {
                    self.state = State::Idle;
                    Step::Pass
                }
                // A chord on one mouse only: a trackpad press held under a
                // mouse's right click is not a rocker.
                Input::SecondaryDown { sender } if sender == held.sender => Step::Offer(held),
                Input::SecondaryDown { .. }
                | Input::SecondaryUp
                | Input::Dragged
                | Input::Interrupted => Step::Pass,
            },
            State::Rocking(chord) => self.step_rocking(input, chord),
        }
    }

    fn step_rocking(&mut self, input: Input, chord: Chord) -> Step {
        let Chord {
            primary_down,
            secondary_down,
            open,
        } = chord;
        match input {
            Input::PrimaryUp => {
                self.settle(false, secondary_down, false);
                if open {
                    Step::SwallowAndRelease
                } else {
                    Step::Swallow
                }
            }
            Input::SecondaryUp => {
                self.settle(primary_down, false, open);
                Step::Swallow
            }
            // Further secondary presses while the primary stays down belong
            // to the chord already under way: swallowed, never acted on.
            Input::SecondaryDown { .. } => {
                self.settle(primary_down, true, open);
                Step::Swallow
            }
            Input::Dragged => Step::AsMove,
            // A move with no button held, or a fresh primary press: a release
            // was lost (the tap was interrupted, or the OS dropped it). The
            // app already has its primary release, so nothing is stuck; end
            // the rocker, and cancel its lifecycle if it is still open.
            Input::Moved => {
                self.state = State::Idle;
                Self::lost_edge(open)
            }
            Input::PrimaryDown(press) => {
                self.state = State::PrimaryHeld(press);
                Self::lost_edge(open)
            }
            // The consumer cancels its own lifecycle on an interruption, so
            // the rocker's lifecycle is closed now. Keep swallowing the
            // chord: the buttons may well still be down.
            Input::Interrupted => {
                self.settle(primary_down, secondary_down, false);
                Step::Pass
            }
        }
    }

    /// Record the consumer accepting the rocker offered by the last
    /// [`Step::Offer`]. Both buttons are down, and the lifecycle is open.
    pub(crate) fn accept(&mut self) {
        if matches!(self.state, State::PrimaryHeld(_)) {
            self.state = State::Rocking(Chord {
                primary_down: true,
                secondary_down: true,
                open: true,
            });
        }
    }

    /// Whether an accepted rocker is under way.
    #[cfg(test)]
    pub(crate) fn is_rocking(&self) -> bool {
        matches!(self.state, State::Rocking(_))
    }

    fn settle(&mut self, primary_down: bool, secondary_down: bool, open: bool) {
        self.state = if primary_down || secondary_down {
            State::Rocking(Chord {
                primary_down,
                secondary_down,
                open,
            })
        } else {
            State::Idle
        };
    }

    fn lost_edge(open: bool) -> Step {
        if open {
            Step::CancelThenPass
        } else {
            Step::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUSE: Option<u64> = Some(7);
    const TRACKPAD: Option<u64> = Some(9);

    fn press(sender: Option<u64>) -> PrimaryPress {
        PrimaryPress {
            x: 100.0,
            y: 200.0,
            click_state: 1,
            event_number: 41,
            sender,
        }
    }

    /// Hold the primary on the mouse, press the secondary, accept.
    fn rocking() -> Rocker {
        let mut rocker = Rocker::default();
        assert_eq!(rocker.step(Input::PrimaryDown(press(MOUSE))), Step::Pass);
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Offer(press(MOUSE))
        );
        rocker.accept();
        assert!(rocker.is_rocking());
        rocker
    }

    #[test]
    fn ordinary_clicks_and_drags_pass_untouched() {
        let mut rocker = Rocker::default();
        for input in [
            Input::Moved,
            Input::PrimaryDown(press(MOUSE)),
            Input::Dragged,
            Input::PrimaryUp,
            Input::SecondaryDown { sender: MOUSE },
            Input::Dragged,
            Input::SecondaryUp,
            Input::Moved,
            Input::Interrupted,
        ] {
            assert_eq!(rocker.step(input), Step::Pass, "{input:?}");
        }
        assert!(!rocker.is_rocking());
    }

    #[test]
    fn a_secondary_press_without_the_primary_held_is_a_plain_right_click() {
        let mut rocker = Rocker::default();
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Pass
        );
        assert_eq!(rocker.step(Input::SecondaryUp), Step::Pass);
        // The chord is primary first: a primary press made while the
        // secondary is already down is not a rocker either.
        for input in [
            Input::SecondaryDown { sender: MOUSE },
            Input::PrimaryDown(press(MOUSE)),
            Input::Dragged,
            Input::PrimaryUp,
            Input::SecondaryUp,
        ] {
            assert_eq!(rocker.step(input), Step::Pass, "{input:?}");
        }
        assert!(!rocker.is_rocking());
    }

    #[test]
    fn the_secondary_press_is_offered_with_the_primary_press_to_release() {
        let mut rocker = Rocker::default();
        rocker.step(Input::PrimaryDown(press(MOUSE)));
        rocker.step(Input::Dragged);
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Offer(press(MOUSE))
        );
    }

    #[test]
    fn a_declined_offer_leaves_the_right_click_alone() {
        let mut rocker = Rocker::default();
        rocker.step(Input::PrimaryDown(press(MOUSE)));
        assert!(matches!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Offer(_)
        ));
        // Not accepted: the secondary edges and the drags are ordinary.
        assert_eq!(rocker.step(Input::Dragged), Step::Pass);
        assert_eq!(rocker.step(Input::SecondaryUp), Step::Pass);
        assert_eq!(rocker.step(Input::PrimaryUp), Step::Pass);
        assert!(!rocker.is_rocking());
    }

    #[test]
    fn a_chord_across_two_devices_is_never_offered() {
        let mut rocker = Rocker::default();
        rocker.step(Input::PrimaryDown(press(TRACKPAD)));
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Pass
        );
    }

    #[test]
    fn primary_released_first_ends_the_lifecycle_then_swallows_the_secondary() {
        let mut rocker = rocking();
        assert_eq!(rocker.step(Input::Dragged), Step::AsMove);
        assert_eq!(rocker.step(Input::PrimaryUp), Step::SwallowAndRelease);
        // The secondary is still down: its drags are still not the app's.
        assert_eq!(rocker.step(Input::Dragged), Step::AsMove);
        assert_eq!(rocker.step(Input::SecondaryUp), Step::Swallow);
        assert!(!rocker.is_rocking());
        assert_eq!(rocker.step(Input::Moved), Step::Pass);
    }

    #[test]
    fn secondary_released_first_keeps_the_hold_until_the_primary_comes_up() {
        let mut rocker = rocking();
        assert_eq!(rocker.step(Input::SecondaryUp), Step::Swallow);
        assert_eq!(rocker.step(Input::Dragged), Step::AsMove);
        // Another secondary press during the hold is swallowed, not offered.
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Swallow
        );
        assert_eq!(rocker.step(Input::SecondaryUp), Step::Swallow);
        assert_eq!(rocker.step(Input::PrimaryUp), Step::SwallowAndRelease);
        assert!(!rocker.is_rocking());
        // The next click is ordinary again.
        assert_eq!(rocker.step(Input::PrimaryDown(press(MOUSE))), Step::Pass);
        assert_eq!(rocker.step(Input::PrimaryUp), Step::Pass);
    }

    #[test]
    fn the_up_edge_is_delivered_exactly_once() {
        let mut rocker = rocking();
        assert_eq!(rocker.step(Input::PrimaryUp), Step::SwallowAndRelease);
        assert_eq!(rocker.step(Input::SecondaryUp), Step::Swallow);
        assert_eq!(rocker.step(Input::PrimaryUp), Step::Pass);
    }

    #[test]
    fn an_interruption_closes_the_lifecycle_but_keeps_swallowing_the_chord() {
        let mut rocker = rocking();
        assert_eq!(rocker.step(Input::Interrupted), Step::Pass);
        assert_eq!(rocker.step(Input::Dragged), Step::AsMove);
        // The consumer already canceled on the interruption: no second end.
        assert_eq!(rocker.step(Input::PrimaryUp), Step::Swallow);
        assert_eq!(rocker.step(Input::SecondaryUp), Step::Swallow);
        assert!(!rocker.is_rocking());
    }

    #[test]
    fn a_move_with_no_button_held_cancels_a_rocker_whose_releases_were_lost() {
        let mut rocker = rocking();
        assert_eq!(rocker.step(Input::Moved), Step::CancelThenPass);
        assert!(!rocker.is_rocking());
        // A late release from the lost chord is then an ordinary event.
        assert_eq!(rocker.step(Input::PrimaryUp), Step::Pass);
    }

    #[test]
    fn a_fresh_primary_press_mid_rocker_cancels_and_starts_a_new_candidate() {
        let mut rocker = rocking();
        let next = PrimaryPress {
            x: 5.0,
            ..press(MOUSE)
        };
        assert_eq!(rocker.step(Input::PrimaryDown(next)), Step::CancelThenPass);
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Offer(next)
        );
    }

    #[test]
    fn a_lost_primary_release_never_turns_a_later_right_click_into_a_rocker() {
        let mut rocker = Rocker::default();
        rocker.step(Input::PrimaryDown(press(MOUSE)));
        // The release was lost, but the pointer then moved with no button
        // held, which proves the primary is up.
        assert_eq!(rocker.step(Input::Moved), Step::Pass);
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Pass
        );
    }

    #[test]
    fn accept_without_an_offer_changes_nothing() {
        let mut rocker = Rocker::default();
        rocker.accept();
        assert!(!rocker.is_rocking());
        assert_eq!(
            rocker.step(Input::SecondaryDown { sender: MOUSE }),
            Step::Pass
        );
    }
}

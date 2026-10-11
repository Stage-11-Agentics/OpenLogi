//! The right-to-left rocker chord, as pure state.
//!
//! Hold the right button, press the left: that left press becomes the down
//! edge of [`ButtonId::RockerRightLeft`](crate::ButtonId::RockerRightLeft),
//! and the right's later release becomes its up edge.
//!
//! macOS opens a context menu on right-mouse-*down*, so the right press
//! cannot reach the app first and be repaired afterwards. When the device
//! that made it has the rocker bound, the right press is held back instead,
//! and only the right press: left clicks are never delayed or touched. Then
//! exactly one of these happens:
//!
//! - A left press from the same device arrives: the rocker fires. The app
//!   never sees either button; the rest of the chord is swallowed, and drags
//!   become plain moves.
//! - The right comes back up first: the held press is replayed, then the
//!   release, which is an ordinary right click (the menu opens on release).
//! - The right drags away past [`DRAG_SLOP`]: the held press is replayed and
//!   the drag goes on as an ordinary right drag.
//! - An edge is lost or capture is interrupted: the held press is replayed
//!   as a whole click, so it is never dropped and the app is never left with
//!   a right press it does not get the release for.
//!
//! This module only decides each step. The platform tap applies it (holds,
//! replays, swallows, retypes), so the logic is testable without FFI.

/// How far, in points, a held-back right press may drag before it stops
/// being a rocker candidate and becomes the start of an ordinary right drag.
pub(crate) const DRAG_SLOP: f64 = 6.0;

/// A right press the tap may hold back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RightPress {
    /// Location of the press, in global display points.
    pub(crate) x: f64,
    /// Location of the press, in global display points.
    pub(crate) y: f64,
    /// Registry id of the device that produced the press, when known.
    pub(crate) sender: Option<u64>,
}

/// One input event, reduced to what the rocker needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Input {
    /// A physical right press.
    RightDown(RightPress),
    /// A physical right release.
    RightUp,
    /// A physical left press, with the registry id of its device.
    LeftDown { sender: Option<u64> },
    /// A physical left release.
    LeftUp,
    /// Pointer motion with a button held, at this location.
    Dragged { x: f64, y: f64 },
    /// Pointer motion with no button held.
    Moved,
    /// The OS stopped delivering events for a while; edges may be lost.
    Interrupted,
}

/// What the tap does with the current event after replaying a held press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Then {
    /// Post a copy of the current event after the replay and drop the
    /// original, so the app sees the two in order.
    PostCurrent,
    /// The current event takes its ordinary path; its order relative to the
    /// replay does not matter (a bare move, an interruption notice).
    PassCurrent,
}

/// What the tap does with the event that produced an [`Input`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    /// The event takes its ordinary path, untouched.
    Pass,
    /// A right press that may start a rocker. Ask the consumer; if it says
    /// yes, call [`Rocker::hold`] and drop the event (the tap keeps a copy).
    /// Otherwise the event takes its ordinary path.
    Query(RightPress),
    /// A left press while a right press is held back. Offer the rocker's
    /// down edge; if accepted, call [`Rocker::accept`], forget the held copy
    /// and drop the event. If declined, call [`Rocker::decline`] and replay
    /// as [`Step::Replay`] with `click: false, then: PostCurrent` would.
    Offer,
    /// Drop the event: the app never sees it.
    Swallow,
    /// Drop the event and deliver the rocker's up edge.
    SwallowAndRelease,
    /// Retype the drag into a plain move: the cursor moves, the app sees
    /// hover, not a drag.
    AsMove,
    /// Replay the held right press (and, with `click`, a release for it),
    /// then handle the current event as `then` says.
    Replay { click: bool, then: Then },
    /// The rocker lost a button edge: cancel the consumer's open rocker
    /// lifecycle alone (no release, so it selects nothing), then let the
    /// event take its ordinary path.
    CancelThenPass,
}

/// The physical state of an accepted chord.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Chord {
    right_down: bool,
    left_down: bool,
    /// Whether the consumer's lifecycle still waits for its up edge.
    open: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum State {
    /// Nothing held back.
    #[default]
    Idle,
    /// A right press is held back: the app has not seen it.
    Holding(RightPress),
    /// An accepted rocker. The app saw neither button; every remaining edge
    /// of the chord is swallowed until both are physically up.
    Rocking(Chord),
}

/// Rocker state for one event-tap thread.
#[derive(Debug, Default)]
pub(crate) struct Rocker {
    state: State,
    /// The app has a left press of its own down: a right press now is not a
    /// rocker, and must reach the app at once.
    left_held: bool,
    /// A button still down from a finished chord, whose press the app never
    /// saw: its release must not reach the app either.
    orphan_left_up: bool,
    orphan_right_up: bool,
}

impl Rocker {
    /// Advance on one input and say what to do with its event.
    pub(crate) fn step(&mut self, input: Input) -> Step {
        match self.state {
            State::Idle => self.step_idle(input),
            State::Holding(held) => self.step_holding(input, held),
            State::Rocking(chord) => self.step_rocking(input, chord),
        }
    }

    fn step_idle(&mut self, input: Input) -> Step {
        match input {
            Input::RightDown(press) if !self.left_held => {
                self.orphan_right_up = false;
                Step::Query(press)
            }
            Input::RightDown(_) => {
                self.orphan_right_up = false;
                Step::Pass
            }
            Input::RightUp if self.orphan_right_up => {
                self.orphan_right_up = false;
                Step::Swallow
            }
            Input::LeftDown { .. } => {
                self.left_held = true;
                self.orphan_left_up = false;
                Step::Pass
            }
            Input::LeftUp if self.orphan_left_up => {
                self.orphan_left_up = false;
                Step::Swallow
            }
            // A left release, or an interruption in which one may have been
            // lost: forget the hold rather than refuse every rocker until the
            // next move.
            Input::LeftUp | Input::Interrupted => {
                self.left_held = false;
                Step::Pass
            }
            // A move with no button held proves every button is up.
            Input::Moved => {
                self.left_held = false;
                self.orphan_left_up = false;
                self.orphan_right_up = false;
                Step::Pass
            }
            Input::RightUp | Input::Dragged { .. } => Step::Pass,
        }
    }

    fn step_holding(&mut self, input: Input, held: RightPress) -> Step {
        let replay = |rocker: &mut Self, click: bool, then: Then| {
            rocker.state = State::Idle;
            Step::Replay { click, then }
        };
        match input {
            // A chord on one mouse only: a trackpad click under a mouse's
            // held right button is not a rocker.
            Input::LeftDown { sender } if sender == held.sender => Step::Offer,
            Input::LeftDown { .. } => {
                self.left_held = true;
                replay(self, false, Then::PostCurrent)
            }
            // The right came back up first: an ordinary right click.
            Input::RightUp => replay(self, false, Then::PostCurrent),
            Input::Dragged { x, y } => {
                if (x - held.x).hypot(y - held.y) > DRAG_SLOP {
                    // An ordinary right drag: the app gets the press, then
                    // the drag from where it is now.
                    replay(self, false, Then::PostCurrent)
                } else {
                    Step::AsMove
                }
            }
            // A move with no button held: the right release was lost.
            // Replay the whole click rather than drop it.
            Input::Moved => replay(self, true, Then::PassCurrent),
            // A second right press: the first one's release was lost. Replay
            // it as a click; the new press then reaches the app as it is.
            Input::RightDown(_) => replay(self, true, Then::PostCurrent),
            // The release may be lost in the gap, or still to come. Replay
            // the click now, and swallow a release that arrives later.
            Input::Interrupted => {
                self.orphan_right_up = true;
                replay(self, true, Then::PassCurrent)
            }
            // A left release with no left press since the hold began: a
            // stray, and nothing to do with the held press.
            Input::LeftUp => Step::Pass,
        }
    }

    fn step_rocking(&mut self, input: Input, chord: Chord) -> Step {
        let Chord {
            right_down,
            left_down,
            open,
        } = chord;
        match input {
            Input::RightUp => {
                self.settle(false, left_down, false);
                if open {
                    Step::SwallowAndRelease
                } else {
                    Step::Swallow
                }
            }
            // Further left presses during the hold belong to the chord.
            Input::LeftDown { .. } => {
                self.settle(right_down, true, open);
                Step::Swallow
            }
            Input::LeftUp => {
                self.settle(right_down, false, open);
                Step::Swallow
            }
            Input::Dragged { .. } => Step::AsMove,
            // A move with no button held: a release was lost. The app saw no
            // press of the chord, so nothing is stuck; end the rocker, and
            // cancel its lifecycle if it is still open.
            Input::Moved => {
                self.state = State::Idle;
                Self::lost_edge(open)
            }
            // A fresh right press. With the right still down its release was
            // lost; with it already released, the chord is over and this is a
            // new press. Either way it reaches the app as it is, and a left
            // still down from the chord stays the chord's to swallow.
            Input::RightDown(_) => {
                self.state = State::Idle;
                self.left_held = false;
                self.orphan_left_up = left_down;
                Self::lost_edge(open)
            }
            // The consumer cancels its own lifecycle on an interruption.
            // Keep swallowing the chord: the buttons may still be down.
            Input::Interrupted => {
                self.settle(right_down, left_down, false);
                Step::Pass
            }
        }
    }

    /// Record the consumer agreeing to hold back the right press `press`
    /// that the last [`Step::Query`] asked about.
    pub(crate) fn hold(&mut self, press: RightPress) {
        if self.state == State::Idle {
            self.state = State::Holding(press);
        }
    }

    /// Record the consumer accepting the rocker offered by the last
    /// [`Step::Offer`]. Both buttons are down, and the lifecycle is open.
    pub(crate) fn accept(&mut self) {
        if matches!(self.state, State::Holding(_)) {
            self.state = State::Rocking(Chord {
                right_down: true,
                left_down: true,
                open: true,
            });
        }
    }

    /// Record the consumer declining the rocker offered by the last
    /// [`Step::Offer`]: the held press is replayed, and the left press that
    /// was offered belongs to the app.
    pub(crate) fn decline(&mut self) {
        if matches!(self.state, State::Holding(_)) {
            self.state = State::Idle;
            self.left_held = true;
        }
    }

    /// Whether a right press is held back.
    #[cfg(test)]
    pub(crate) fn is_holding(&self) -> bool {
        matches!(self.state, State::Holding(_))
    }

    /// Whether an accepted rocker is under way.
    #[cfg(test)]
    pub(crate) fn is_rocking(&self) -> bool {
        matches!(self.state, State::Rocking(_))
    }

    fn settle(&mut self, right_down: bool, left_down: bool, open: bool) {
        self.state = if right_down || left_down {
            State::Rocking(Chord {
                right_down,
                left_down,
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
mod tests;

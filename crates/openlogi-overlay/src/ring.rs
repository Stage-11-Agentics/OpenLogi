//! The ring itself: the GPUI view, and the window it is drawn in.
//!
//! Placement is the interesting part — the panel is centred on the cursor and
//! clamped to the display it came up on, so a ring raised near a screen edge
//! stays whole instead of being cut off.

#[cfg(any(not(target_os = "windows"), test))]
use gpui::{Bounds, Pixels, Point, Size, point};
use gpui::{
    Context, Div, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window, WindowBackgroundAppearance,
    WindowKind, WindowOptions, div, hsla, img, prelude::FluentBuilder as _, px, rgb, svg,
};
use openlogi_core::binding::{Action, ActionRingSlot};
use openlogi_ipc::ActionRingInvocation;
use openlogi_ui::action_icons::RING_CANCEL_ICON;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::ipc::OverlayCommand;
use crate::session::{ClickAwaySession, ShowingRing};

/// The ring is drawn as a three-by-three keypad: the eight positions sit
/// around the centre cell (cancel, where the cursor lands) one gap away, so the
/// travel from the cursor to any action is a single cell.
const CELL: f32 = 48.0;
const GAP: f32 = 8.0;
const GRID: f32 = 3.0 * CELL + 2.0 * GAP;
/// Sheet padding around the keypad, and the ruled readout row under it.
const PAD: f32 = 10.0;
const READOUT: f32 = 28.0;
/// Room for the floating shadow between the sheet and the window edge.
const MARGIN: f32 = 18.0;
/// The window is square and centred on the cursor, so it is sized by the
/// sheet's longest reach from the keypad centre: down through the readout.
pub(crate) const WINDOW_SIZE: f32 = 2.0 * (GRID / 2.0 + PAD + 1.0 + READOUT + MARGIN);
const CENTER: f32 = WINDOW_SIZE / 2.0;
const SHEET_LEFT: f32 = CENTER - GRID / 2.0 - PAD;
const SHEET_TOP: f32 = CENTER - GRID / 2.0 - PAD;
const SHEET_WIDTH: f32 = GRID + 2.0 * PAD;
const SHEET_HEIGHT: f32 = GRID + 2.0 * PAD + 1.0 + READOUT;
const GLYPH_SIZE: f32 = 22.0;
/// Edge of the square box a user-supplied slot image is fitted into.
const CUSTOM_ICON_SIZE: f32 = 32.0;
/// Hover brackets: four L marks outside the cell's corners.
const BRACKET_GAP: f32 = 2.5;
const BRACKET_ARM: f32 = 8.0;
const BRACKET_STROKE: f32 = 1.5;
const MONO: &str = "JetBrains Mono";
const READOUT_TEXT: f32 = 10.5;

/// A user-supplied image for a slot with a custom label, drawn in full colour
/// in place of the built-in glyph: `<config dir>/ring-icons/<label>.svg` or
/// `.png`. Labels that could escape that directory are ignored.
fn custom_icon_path(presentation: &openlogi_ipc::ActionRingPresentation) -> Option<PathBuf> {
    let label = presentation.label.trim();
    if !presentation.literal
        || label.is_empty()
        || label.starts_with('.')
        || label.contains(['/', '\\'])
    {
        return None;
    }
    let directory = openlogi_core::paths::config_dir().ok()?.join("ring-icons");
    ["svg", "png"]
        .iter()
        .map(|extension| directory.join(format!("{label}.{extension}")))
        .find(|candidate| candidate.is_file())
}

/// Datasheet Mono, dark: square solid surfaces, 1 px rules, one orange. The ring
/// floats over whatever is on the desktop, so it commits to the dark sheet
/// rather than following the OS appearance.
fn surface() -> Hsla {
    rgb(0x0011_1214).into()
}
fn chip() -> Hsla {
    rgb(0x001A_1B1F).into()
}
fn edge() -> Hsla {
    rgb(0x002C_2E33).into()
}
fn accent() -> Hsla {
    rgb(0x00FF_4F1F).into()
}
fn text() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.92)
}
fn text_2() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.58)
}
fn text_dim() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.46)
}
fn bracket_ink() -> Hsla {
    hsla(0.0, 0.0, 1.0, 0.78)
}

/// Keypad column and row of a ring position, each in `-1..=1`.
const fn grid_position(slot: ActionRingSlot) -> (f32, f32) {
    match slot {
        ActionRingSlot::Top => (0.0, -1.0),
        ActionRingSlot::TopRight => (1.0, -1.0),
        ActionRingSlot::Right => (1.0, 0.0),
        ActionRingSlot::BottomRight => (1.0, 1.0),
        ActionRingSlot::Bottom => (0.0, 1.0),
        ActionRingSlot::BottomLeft => (-1.0, 1.0),
        ActionRingSlot::Left => (-1.0, 0.0),
        ActionRingSlot::TopLeft => (-1.0, -1.0),
    }
}

/// Top-left corner of a cell in window coordinates.
fn cell_origin(column: f32, row: f32) -> (f32, f32) {
    (
        CENTER - CELL / 2.0 + column * (CELL + GAP),
        CENTER - CELL / 2.0 + row * (CELL + GAP),
    )
}

/// Compass code shown in the readout for a hovered position.
const fn compass(slot: ActionRingSlot) -> &'static str {
    match slot {
        ActionRingSlot::Top => "N",
        ActionRingSlot::TopRight => "NE",
        ActionRingSlot::Right => "E",
        ActionRingSlot::BottomRight => "SE",
        ActionRingSlot::Bottom => "S",
        ActionRingSlot::BottomLeft => "SW",
        ActionRingSlot::Left => "W",
        ActionRingSlot::TopLeft => "NW",
    }
}

/// Four L marks drawn outside a cell's corners. Pure paint: no handlers, so
/// they never change layout or hit testing.
fn brackets(left: f32, top: f32) -> Vec<Div> {
    let near = BRACKET_GAP + BRACKET_STROKE;
    let (x0, y0) = (left - near, top - near);
    let (x1, y1) = (left + CELL + BRACKET_GAP, top + CELL + BRACKET_GAP);
    let (x1_arm, y1_arm) = (x1 + BRACKET_STROKE - BRACKET_ARM, y1 + BRACKET_STROKE - BRACKET_ARM);
    let bar = |x: f32, y: f32, w: f32, h: f32| {
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .bg(bracket_ink())
    };
    let (arm, stroke) = (BRACKET_ARM, BRACKET_STROKE);
    vec![
        bar(x0, y0, arm, stroke),
        bar(x0, y0, stroke, arm),
        bar(x1_arm, y0, arm, stroke),
        bar(x1, y0, stroke, arm),
        bar(x0, y1, arm, stroke),
        bar(x0, y1_arm, stroke, arm),
        bar(x1_arm, y1, arm, stroke),
        bar(x1, y1_arm, stroke, arm),
    ]
}

pub(crate) struct RingView {
    invocation: ActionRingInvocation,
    commands: mpsc::UnboundedSender<OverlayCommand>,
    hovered: Option<ActionRingSlot>,
    /// Pointer over the centre (cancel) cell; drives the readout only.
    cancel_hovered: bool,
    /// Publishes click-away identity for exactly this view's lifetime.
    _showing: ShowingRing,
}

impl RingView {
    /// Open a view on `invocation`, reporting interactions through `commands`.
    pub(crate) fn new(
        invocation: ActionRingInvocation,
        commands: mpsc::UnboundedSender<OverlayCommand>,
        live: &Arc<ClickAwaySession>,
    ) -> Self {
        let showing = live.showing(invocation.session_id);
        Self {
            invocation,
            commands,
            hovered: None,
            cancel_hovered: false,
            _showing: showing,
        }
    }

    /// Preview builds only: draw `slot` as hovered without a pointer.
    #[cfg(feature = "ring-preview")]
    pub(crate) fn preview_hovered(mut self, slot: Option<ActionRingSlot>) -> Self {
        self.hovered = slot;
        self
    }

    /// Preview builds only: draw the centre cell as hovered.
    #[cfg(feature = "ring-preview")]
    pub(crate) fn preview_cancel_hovered(mut self) -> Self {
        self.cancel_hovered = true;
        self
    }

    /// The ring session this view is showing.
    pub(crate) const fn session_id(&self) -> u64 {
        self.invocation.session_id
    }

    /// Report this ring cancelled. The window is closed by the caller, which
    /// is the only one holding the handle.
    pub(crate) fn cancel(&self) {
        let _ = self.commands.send(OverlayCommand::Cancel {
            session_id: self.invocation.session_id,
        });
    }

    fn slot_element(&self, slot: ActionRingSlot, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (column, row) = grid_position(slot);
        let (left, top) = cell_origin(column, row);
        let cell = div()
            .id(("ring-slot", slot.index()))
            .absolute()
            .left(px(left))
            .top(px(top))
            .size(px(CELL))
            .flex()
            .items_center()
            .justify_center();
        // An empty position keeps its cell as an outline, so the keypad never
        // changes shape between rings.
        let Some(presentation) = self.invocation.slots.get(&slot) else {
            return cell.border_1().border_color(edge()).into_any_element();
        };
        let session_id = self.invocation.session_id;
        let activate = self.commands.clone();
        cell.bg(chip())
            .text_color(text())
            .cursor_pointer()
            .child(match custom_icon_path(presentation) {
                Some(path) => img(path).size(px(CUSTOM_ICON_SIZE)).into_any_element(),
                None => svg()
                    .path(presentation.icon.asset_path())
                    .size(px(GLYPH_SIZE))
                    .text_color(text())
                    .into_any_element(),
            })
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered && this.hovered != Some(slot) {
                    this.hovered = Some(slot);
                    let _ = this
                        .commands
                        .send(OverlayCommand::Hover { session_id, slot });
                    cx.notify();
                } else if !*hovered && this.hovered == Some(slot) {
                    this.hovered = None;
                    cx.notify();
                }
            }))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                let _ = activate.send(OverlayCommand::Activate { session_id, slot });
                window.remove_window();
            })
            .into_any_element()
    }

    /// The label for a populated slot, localized unless user-authored.
    fn slot_label(&self, slot: ActionRingSlot) -> Option<String> {
        let presentation = self.invocation.slots.get(&slot)?;
        // User-authored labels render verbatim: passing them through the
        // localization table would translate any label that happens to
        // collide with a known key ("Copy" → "Copier" under fr).
        Some(if presentation.literal {
            presentation.label.clone()
        } else if let Some(key) = Action::translation_key_for_label(&presentation.label) {
            rust_i18n::t!(key).into_owned()
        } else {
            presentation.label.clone()
        })
    }

    /// The ruled row under the keypad: a status square and the armed action's
    /// name on the left, its compass position on the right. Both boxes are
    /// fixed, so hovering only swaps text in place.
    fn readout(&self) -> Div {
        let armed = self
            .hovered
            .and_then(|slot| Some((self.slot_label(slot)?, compass(slot))));
        let (square, label, label_color, code) = match armed {
            Some((label, code)) => (accent(), label, text(), code),
            None if self.cancel_hovered => (gpui::transparent_black(), "Cancel".into(), text_2(), ""),
            None => (gpui::transparent_black(), "Actions".into(), text_dim(), ""),
        };
        let square = div().flex_none().size(px(6.0)).bg(square).when(
            self.hovered.is_none(),
            |square| square.border_1().border_color(text_dim()),
        );
        div()
            .absolute()
            .left(px(SHEET_LEFT + PAD))
            .top(px(SHEET_TOP + GRID + 2.0 * PAD + 1.0))
            .w(px(GRID))
            .h(px(READOUT))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .font_family(MONO)
            .font_weight(FontWeight::MEDIUM)
            .text_size(px(READOUT_TEXT))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(7.0))
                    .min_w_0()
                    .child(square)
                    .child(
                        div()
                            .text_color(label_color)
                            .whitespace_nowrap()
                            .truncate()
                            .child(SharedString::from(label.to_uppercase())),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(18.0))
                    .text_right()
                    .text_color(text_2())
                    .child(SharedString::from(code)),
            )
    }
}

impl Render for RingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session_id = self.invocation.session_id;
        let root_commands = self.commands.clone();
        let center_commands = self.commands.clone();
        let cells = ActionRingSlot::ALL
            .into_iter()
            .map(|slot| self.slot_element(slot, cx))
            .collect::<Vec<_>>();
        let (center_left, center_top) = cell_origin(0.0, 0.0);
        let bracketed = match self.hovered {
            Some(slot) => {
                let (column, row) = grid_position(slot);
                Some(cell_origin(column, row))
            }
            None if self.cancel_hovered => Some((center_left, center_top)),
            None => None,
        };

        div()
            .id("ring-root")
            .relative()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(px(SHEET_LEFT))
                    .top(px(SHEET_TOP))
                    .w(px(SHEET_WIDTH))
                    .h(px(SHEET_HEIGHT))
                    .bg(surface())
                    .border_1()
                    .border_color(edge())
                    .shadow_lg(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(SHEET_LEFT))
                    .top(px(SHEET_TOP + GRID + 2.0 * PAD))
                    .w(px(SHEET_WIDTH))
                    .h(px(1.0))
                    .bg(edge()),
            )
            .children(cells)
            .child(
                div()
                    .id("ring-cancel")
                    .absolute()
                    .left(px(center_left))
                    .top(px(center_top))
                    .size(px(CELL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(edge())
                    .cursor_pointer()
                    // An svg paints with its own text colour; it does not
                    // inherit the parent's, so the glyph sets it directly.
                    .child(
                        svg()
                            .path(RING_CANCEL_ICON)
                            .size(px(16.0))
                            .flex_none()
                            .text_color(if self.cancel_hovered { text() } else { text_2() }),
                    )
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        if this.cancel_hovered != *hovered {
                            this.cancel_hovered = *hovered;
                            cx.notify();
                        }
                    }))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        let _ = center_commands.send(OverlayCommand::Cancel { session_id });
                        window.remove_window();
                    }),
            )
            .child(self.readout())
            .when_some(bracketed, |ring, (left, top)| ring.children(brackets(left, top)))
            .on_click(move |_, window, _| {
                let _ = root_commands.send(OverlayCommand::Cancel { session_id });
                window.remove_window();
            })
    }
}

/// Appearance shared by the platform-specific placement paths.
pub(crate) fn ring_window_options() -> WindowOptions {
    WindowOptions {
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("openlogi-action-ring".to_string()),
        ..WindowOptions::default()
    }
}

#[cfg(any(not(target_os = "windows"), test))]
pub(crate) fn clamp_window_origin(
    desired: Point<Pixels>,
    window_size: Size<Pixels>,
    display: Bounds<Pixels>,
) -> Point<Pixels> {
    let max = point(
        display.right() - window_size.width,
        display.bottom() - window_size.height,
    );
    desired.clamp(&display.origin, &max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_origin_is_clamped_to_the_display() {
        let display = Bounds::new(point(px(100.0), px(50.0)), Size::new(px(800.0), px(600.0)));
        let size = Size::new(px(400.0), px(400.0));
        assert_eq!(
            clamp_window_origin(point(px(-50.0), px(-50.0)), size, display),
            point(px(100.0), px(50.0))
        );
        assert_eq!(
            clamp_window_origin(point(px(700.0), px(500.0)), size, display),
            point(px(500.0), px(250.0))
        );
    }

    #[test]
    fn overlay_origin_stays_cursor_centered_away_from_edges() {
        let display = Bounds::new(Point::default(), Size::new(px(1600.0), px(1000.0)));
        let desired = point(px(600.0), px(300.0));
        assert_eq!(
            clamp_window_origin(desired, Size::new(px(400.0), px(400.0)), display),
            desired
        );
    }
}

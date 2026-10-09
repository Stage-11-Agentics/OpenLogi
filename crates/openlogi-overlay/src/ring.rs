//! The ring itself: the GPUI view, and the window it is drawn in.
//!
//! Placement is the interesting part — the panel is centred on the cursor and
//! clamped to the display it came up on, so a ring raised near a screen edge
//! stays whole instead of being cut off.

#[cfg(any(not(target_os = "windows"), test))]
use gpui::{Bounds, Pixels, Point, Size, point};
use gpui::{
    Context, Div, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Task, Window, WindowAppearance,
    WindowBackgroundAppearance, WindowKind, WindowOptions, div, hsla, img,
    prelude::FluentBuilder as _, px, rgb, svg,
};
use openlogi_core::binding::{Action, ActionRingSlot};
use openlogi_ipc::ActionRingInvocation;
use openlogi_ui::action_icons::RING_CANCEL_ICON;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
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
/// Pointer travel from the ring's opening position below which a release is a
/// tap, not a selection.
const RELEASE_TRAVEL: f32 = 12.0;
/// How long the trigger must stay down before the ring shows it as held, so
/// a quick tap never flashes the held state on its way to a click.
const HOLD_SHOW: Duration = Duration::from_millis(180);
/// The orange rule along the sheet's top edge while the trigger is held.
const HELD_RULE: f32 = 2.0;
const BRACKET_ARM: f32 = 8.0;
const BRACKET_STROKE: f32 = 1.5;
const MONO: &str = "JetBrains Mono";
const READOUT_TEXT: f32 = 10.5;

/// A user-supplied image for a slot with a custom label, drawn in full colour
/// in place of the built-in glyph: `<config dir>/ring-icons/<label>.svg` or
/// `.png`, drawn in light ink for a dark cell. On a light cell the ring looks
/// for `<label>.light.svg` or `.light.png` first, drawn in dark ink. Labels
/// that could escape that directory are ignored.
fn custom_icon_path(
    presentation: &openlogi_ipc::ActionRingPresentation,
    light_cell: bool,
) -> Option<PathBuf> {
    let label = presentation.label.trim();
    if !presentation.literal
        || label.is_empty()
        || label.starts_with('.')
        || label.contains(['/', '\\'])
    {
        return None;
    }
    let directory = openlogi_core::paths::config_dir().ok()?.join("ring-icons");
    let stems = if light_cell {
        vec![format!("{label}.light"), label.to_string()]
    } else {
        vec![label.to_string()]
    };
    stems
        .iter()
        .flat_map(|stem| {
            ["svg", "png"].map(|extension| directory.join(format!("{stem}.{extension}")))
        })
        .find(|candidate| candidate.is_file())
}

/// Datasheet Mono tokens: square solid surfaces, 1 px rules, one orange. The
/// sheet follows the OS appearance, dark by default and print on paper in
/// light; orange is the same in both.
#[derive(Clone, Copy)]
struct Palette {
    light: bool,
    surface: Hsla,
    chip: Hsla,
    /// The sheet's border and the readout rule.
    edge: Hsla,
    /// Outlines of the empty and centre cells, quieter than `edge`.
    cell_edge: Hsla,
    text: Hsla,
    text_2: Hsla,
    text_dim: Hsla,
    bracket_ink: Hsla,
    /// A pressed cell: the slot a release would run while the trigger is held.
    inv_bg: Hsla,
    inv_fg: Hsla,
}

impl Palette {
    fn dark() -> Self {
        Self {
            light: false,
            surface: rgb(0x0011_1214).into(),
            chip: rgb(0x001A_1B1F).into(),
            edge: rgb(0x002C_2E33).into(),
            cell_edge: rgb(0x002C_2E33).into(),
            text: hsla(0.0, 0.0, 1.0, 0.92),
            text_2: hsla(0.0, 0.0, 1.0, 0.58),
            text_dim: hsla(0.0, 0.0, 1.0, 0.46),
            bracket_ink: hsla(0.0, 0.0, 1.0, 0.78),
            inv_bg: rgb(0x00FF_FFFF).into(),
            inv_fg: rgb(0x0011_1214).into(),
        }
    }

    fn light() -> Self {
        let ink: Hsla = rgb(0x0011_1214).into();
        Self {
            light: true,
            surface: rgb(0x00FF_FFFF).into(),
            chip: rgb(0x00F2_F2F4).into(),
            edge: ink,
            cell_edge: rgb(0x00D3_D4D8).into(),
            text: ink,
            text_2: ink,
            text_dim: ink.opacity(0.5),
            bracket_ink: ink.opacity(0.78),
            inv_bg: ink,
            inv_fg: rgb(0x00FF_FFFF).into(),
        }
    }

    fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
        }
    }
}

fn accent() -> Hsla {
    rgb(0x00FF_4F1F).into()
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
fn brackets(left: f32, top: f32, ink: Hsla) -> Vec<Div> {
    let near = BRACKET_GAP + BRACKET_STROKE;
    let (x0, y0) = (left - near, top - near);
    let (x1, y1) = (left + CELL + BRACKET_GAP, top + CELL + BRACKET_GAP);
    let (x1_arm, y1_arm) = (
        x1 + BRACKET_STROKE - BRACKET_ARM,
        y1 + BRACKET_STROKE - BRACKET_ARM,
    );
    let bar = |x: f32, y: f32, w: f32, h: f32| {
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .bg(ink)
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
    /// Global top-left of this window, for reading the live cursor on release.
    origin: Option<(f64, f64)>,
    /// Pointer position, in window coordinates, when the ring opened.
    opened_at: Option<(f32, f32)>,
    /// The press that opened this ring is still down.
    trigger_down: bool,
    /// The trigger has stayed down past [`HOLD_SHOW`], so the ring shows it
    /// as held.
    held: bool,
    /// The slot a release would run right now, drawn pressed. Only while held.
    armed: Option<ActionRingSlot>,
    /// Drawn in light or dark, read from the window on every render.
    palette: Palette,
    /// Preview builds only: an appearance that overrides the window's.
    #[cfg(feature = "ring-preview")]
    preview_appearance: Option<WindowAppearance>,
    hold_timer: Option<Task<()>>,
}

impl RingView {
    /// Open a view on `invocation`, reporting interactions through `commands`.
    pub(crate) fn new(
        invocation: ActionRingInvocation,
        commands: mpsc::UnboundedSender<OverlayCommand>,
        live: &Arc<ClickAwaySession>,
    ) -> Self {
        let showing = live.showing(invocation.session_id);
        let trigger_down = !invocation.trigger_released;
        Self {
            invocation,
            commands,
            hovered: None,
            cancel_hovered: false,
            _showing: showing,
            origin: None,
            opened_at: None,
            trigger_down,
            held: false,
            armed: None,
            palette: Palette::dark(),
            #[cfg(feature = "ring-preview")]
            preview_appearance: None,
            hold_timer: None,
        }
    }

    /// Show the ring as held once the trigger has stayed down past
    /// [`HOLD_SHOW`]. A release before then is a tap, and the ring opens
    /// straight into its click state.
    pub(crate) fn watch_hold(mut self, cx: &mut Context<Self>) -> Self {
        if self.trigger_down {
            self.hold_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(HOLD_SHOW).await;
                let _ = this.update(cx, |view, cx| {
                    if view.trigger_down {
                        view.held = true;
                        view.armed = view.armed_slot();
                        cx.notify();
                    }
                });
            }));
        }
        self
    }

    /// Anchor the view to its window's global origin and record where the
    /// pointer was when it opened, so a release can be resolved against the
    /// live cursor. Without an origin a release never selects.
    pub(crate) fn anchored(mut self, origin: Option<(f64, f64)>) -> Self {
        self.origin = origin;
        self.opened_at = self.local_cursor();
        self
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "cursor coordinates are screen-sized and exactly usable as GPUI f32 pixels"
    )]
    fn local_cursor(&self) -> Option<(f32, f32)> {
        let (left, top) = self.origin?;
        let cursor = openlogi_hook::cursor_position()?;
        Some(((cursor.x - left) as f32, (cursor.y - top) as f32))
    }

    /// The populated slot under the live cursor, provided the pointer has moved
    /// far enough from where the ring opened to count as deliberate. Read from
    /// the cursor, not hover state: hover can go stale when the pointer leaves
    /// the window between events, and a ring clamped at a screen edge can open
    /// with the pointer already over a slot.
    fn release_target(&self) -> Option<ActionRingSlot> {
        let start = self.opened_at?;
        let (x, y) = self.local_cursor()?;
        if (x - start.0).hypot(y - start.1) < RELEASE_TRAVEL {
            return None;
        }
        ActionRingSlot::ALL.into_iter().find(|slot| {
            let (left, top) = {
                let (column, row) = grid_position(*slot);
                cell_origin(column, row)
            };
            self.invocation.slots.contains_key(slot)
                && (left..=left + CELL).contains(&x)
                && (top..=top + CELL).contains(&y)
        })
    }

    /// The slot a release would run now: only while the ring shows as held.
    fn armed_slot(&self) -> Option<ActionRingSlot> {
        if !self.held {
            return None;
        }
        // The preview has no live cursor, so its hovered slot stands in.
        #[cfg(feature = "ring-preview")]
        if self.origin.is_none() {
            return self.hovered;
        }
        self.release_target()
    }

    /// Preview builds only: draw in `appearance` whatever the system uses.
    #[cfg(feature = "ring-preview")]
    pub(crate) const fn preview_appearance(mut self, appearance: WindowAppearance) -> Self {
        self.preview_appearance = Some(appearance);
        self
    }

    /// Preview builds only: draw the trigger as held.
    #[cfg(feature = "ring-preview")]
    pub(crate) fn preview_held(mut self) -> Self {
        self.trigger_down = true;
        self.held = true;
        self.armed = self.armed_slot();
        self
    }

    /// Preview builds only: draw `slot` as hovered without a pointer.
    #[cfg(feature = "ring-preview")]
    pub(crate) fn preview_hovered(mut self, slot: Option<ActionRingSlot>) -> Self {
        self.hovered = slot;
        self.armed = self.armed_slot();
        self
    }

    /// Preview builds only: draw the centre cell as hovered.
    #[cfg(feature = "ring-preview")]
    pub(crate) fn preview_cancel_hovered(mut self) -> Self {
        self.cancel_hovered = true;
        self
    }

    /// The press that opened this ring came back up. When `selects`, activate
    /// the slot under the pointer and close, exactly as a click on it would.
    /// Otherwise, or released over the centre, a gap or an empty cell, the
    /// ring drops its held state and stays open for a click.
    pub(crate) fn release_trigger(
        &mut self,
        selects: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.trigger_down = false;
        self.held = false;
        self.armed = None;
        self.hold_timer.take();
        cx.notify();
        let Some(slot) = selects.then(|| self.release_target()).flatten() else {
            return;
        };
        let _ = self.commands.send(OverlayCommand::Activate {
            session_id: self.invocation.session_id,
            slot,
        });
        window.remove_window();
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
        let palette = self.palette;
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
            return cell
                .border_1()
                .border_color(palette.cell_edge)
                .into_any_element();
        };
        let session_id = self.invocation.session_id;
        let activate = self.commands.clone();
        // Held over a slot, the cell inverts: pressed under the thumb, and the
        // one a release runs.
        let pressed = self.armed == Some(slot);
        let (fill, ink) = if pressed {
            (palette.inv_bg, palette.inv_fg)
        } else {
            (palette.chip, palette.text)
        };
        let light_cell = palette.light != pressed;
        cell.bg(fill)
            .text_color(ink)
            .cursor_pointer()
            .child(match custom_icon_path(presentation, light_cell) {
                Some(path) => img(path).size(px(CUSTOM_ICON_SIZE)).into_any_element(),
                None => svg()
                    .path(presentation.icon.asset_path())
                    .size(px(GLYPH_SIZE))
                    .text_color(ink)
                    .into_any_element(),
            })
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered && this.hovered != Some(slot) {
                    this.hovered = Some(slot);
                    this.armed = this.armed_slot();
                    let _ = this
                        .commands
                        .send(OverlayCommand::Hover { session_id, slot });
                    cx.notify();
                } else if !*hovered && this.hovered == Some(slot) {
                    this.hovered = None;
                    this.armed = this.armed_slot();
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

    /// The centre cell, where the cursor lands: a click there cancels.
    fn cancel_cell(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let session_id = self.invocation.session_id;
        let commands = self.commands.clone();
        let (left, top) = cell_origin(0.0, 0.0);
        div()
            .id("ring-cancel")
            .absolute()
            .left(px(left))
            .top(px(top))
            .size(px(CELL))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(palette.cell_edge)
            .cursor_pointer()
            // An svg paints with its own text colour; it does not inherit the
            // parent's, so the glyph sets it directly.
            .child(
                svg()
                    .path(RING_CANCEL_ICON)
                    .size(px(16.0))
                    .flex_none()
                    .text_color(if self.cancel_hovered {
                        palette.text
                    } else {
                        palette.text_2
                    }),
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if this.cancel_hovered != *hovered {
                    this.cancel_hovered = *hovered;
                    cx.notify();
                }
            }))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                let _ = commands.send(OverlayCommand::Cancel { session_id });
                window.remove_window();
            })
    }

    /// The ruled row under the keypad: a status square and the armed action's
    /// name on the left, its compass position on the right. Both boxes are
    /// fixed, so hovering only swaps text in place. While the trigger is held
    /// the square is orange throughout, and with no slot under the pointer the
    /// readout says so.
    fn readout(&self) -> Div {
        let palette = self.palette;
        let armed = self
            .hovered
            .and_then(|slot| Some((self.slot_label(slot)?, compass(slot))));
        let lit = armed.is_some() || (self.held && !self.cancel_hovered);
        let (square, label, label_color, code) = match armed {
            Some((label, code)) => (accent(), label, palette.text, code),
            None if self.cancel_hovered => (
                gpui::transparent_black(),
                "Cancel".into(),
                palette.text_2,
                "",
            ),
            None if self.held => (accent(), "Held".into(), palette.text, ""),
            None => (
                gpui::transparent_black(),
                "Actions".into(),
                palette.text_dim,
                "",
            ),
        };
        let square = div()
            .flex_none()
            .size(px(6.0))
            .bg(square)
            .when(!lit, |square| {
                square.border_1().border_color(palette.text_dim)
            });
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
                    .text_color(palette.text_2)
                    .child(SharedString::from(code)),
            )
    }
}

impl Render for RingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = window.appearance();
        #[cfg(feature = "ring-preview")]
        let appearance = self.preview_appearance.unwrap_or(appearance);
        self.palette = Palette::for_appearance(appearance);
        let palette = self.palette;
        let session_id = self.invocation.session_id;
        let root_commands = self.commands.clone();
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
                    .bg(palette.surface)
                    .border_1()
                    .border_color(palette.edge)
                    .shadow_lg(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(SHEET_LEFT))
                    .top(px(SHEET_TOP + GRID + 2.0 * PAD))
                    .w(px(SHEET_WIDTH))
                    .h(px(1.0))
                    .bg(palette.edge),
            )
            // Held: an orange rule printed along the sheet's top edge, over
            // the border, so nothing inside moves.
            .when(self.held, |ring| {
                ring.child(
                    div()
                        .absolute()
                        .left(px(SHEET_LEFT))
                        .top(px(SHEET_TOP))
                        .w(px(SHEET_WIDTH))
                        .h(px(HELD_RULE))
                        .bg(accent()),
                )
            })
            .children(cells)
            .child(self.cancel_cell(cx))
            .child(self.readout())
            .when_some(bracketed, |ring, (left, top)| {
                ring.children(brackets(left, top, palette.bracket_ink))
            })
            // Held, the pressed cell follows the live cursor, which can cross
            // the release threshold without entering or leaving a cell.
            .on_mouse_move(cx.listener(|this, _, _, cx| {
                if this.held {
                    let armed = this.armed_slot();
                    if armed != this.armed {
                        this.armed = armed;
                        cx.notify();
                    }
                }
            }))
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

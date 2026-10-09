//! Development-only static ring for screenshots (`--features ring-preview`).
//!
//! `OPENLOGI_RING_PREVIEW=default|c11` opens one sample ring at a fixed spot on
//! the primary display, with no agent and no IPC. `OPENLOGI_RING_PREVIEW_HOVER`
//! names a slot (or `Center`) to draw as hovered, `OPENLOGI_RING_PREVIEW_EMPTY`
//! a slot to leave empty, `OPENLOGI_RING_PREVIEW_HELD=1` draws the trigger as
//! held (a hovered slot then draws pressed), `OPENLOGI_RING_PREVIEW_APPEARANCE`
//! `light` or `dark` overrides the system appearance, and
//! `OPENLOGI_RING_PREVIEW_AT=x,y` moves the window.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use gpui::{AppContext as _, Bounds, Size, WindowBounds, WindowOptions, point, px};
use openlogi_core::binding::{Action, ActionRingIcon, ActionRingSlot};
use openlogi_ipc::{ActionRingInvocation, ActionRingPresentation};
use tokio::sync::mpsc;

use crate::ring::{RingView, WINDOW_SIZE, ring_window_options};
use crate::session::ClickAwaySession;

fn slot_named(name: &str) -> Option<ActionRingSlot> {
    ActionRingSlot::ALL
        .into_iter()
        .find(|slot| format!("{slot:?}").eq_ignore_ascii_case(name))
}

fn stock(action: &Action) -> ActionRingPresentation {
    ActionRingPresentation {
        label: action.label(),
        literal: false,
        icon: ActionRingIcon::for_action(action),
    }
}

fn custom(label: &str, icon: ActionRingIcon) -> ActionRingPresentation {
    ActionRingPresentation {
        label: label.to_string(),
        literal: true,
        icon,
    }
}

fn sample(which: &str) -> BTreeMap<ActionRingSlot, ActionRingPresentation> {
    use ActionRingSlot as S;
    let mut slots = BTreeMap::new();
    if which == "c11" {
        slots.insert(S::Top, custom("Workspace up", ActionRingIcon::ArrowUp));
        slots.insert(S::TopRight, custom("MouthKeys", ActionRingIcon::Keyboard));
        slots.insert(S::Right, custom("Tab right", ActionRingIcon::NextTab));
        slots.insert(S::BottomRight, custom("Return", ActionRingIcon::ArrowDown));
        slots.insert(
            S::Bottom,
            custom("Workspace down", ActionRingIcon::ArrowDown),
        );
        slots.insert(
            S::BottomLeft,
            custom("Go to waiting", ActionRingIcon::Search),
        );
        slots.insert(S::Left, custom("Tab left", ActionRingIcon::PreviousTab));
        slots.insert(S::TopLeft, custom("New agent", ActionRingIcon::Terminal));
    } else {
        slots.insert(S::Top, stock(&Action::NewTab));
        slots.insert(S::TopRight, custom("MouthKeys", ActionRingIcon::Keyboard));
        slots.insert(S::Right, stock(&Action::NextTrack));
        slots.insert(S::BottomRight, stock(&Action::BrowserForward));
        slots.insert(S::Bottom, stock(&Action::PlayPause));
        slots.insert(S::BottomLeft, stock(&Action::BrowserBack));
        slots.insert(S::Left, stock(&Action::ReopenTab));
        slots.insert(S::TopLeft, stock(&Action::CloseTab));
    }
    slots
}

pub(crate) fn run(which: String) -> Result<()> {
    let hover = std::env::var("OPENLOGI_RING_PREVIEW_HOVER").unwrap_or_default();
    let empty = std::env::var("OPENLOGI_RING_PREVIEW_EMPTY").unwrap_or_default();
    let at = std::env::var("OPENLOGI_RING_PREVIEW_AT").unwrap_or_default();
    let held = std::env::var("OPENLOGI_RING_PREVIEW_HELD").is_ok_and(|value| value == "1");
    let appearance = match std::env::var("OPENLOGI_RING_PREVIEW_APPEARANCE").as_deref() {
        Ok("light") => Some(gpui::WindowAppearance::Light),
        Ok("dark") => Some(gpui::WindowAppearance::Dark),
        _ => None,
    };
    let (x, y) = at
        .split_once(',')
        .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)))
        .unwrap_or((40.0_f32, 80.0_f32));

    let mut slots = sample(&which);
    if let Some(slot) = slot_named(&empty) {
        slots.remove(&slot);
    }
    let invocation = ActionRingInvocation {
        session_id: 1,
        slots,
        language: None,
        trigger_released: false,
        release_selects: false,
    };

    let app = gpui_platform::application().with_assets(openlogi_ui::action_icons::ActionIcons);
    app.run(move |cx| {
        crate::platform::configure_application();
        let (commands, receiver) = mpsc::unbounded_channel();
        std::mem::forget(receiver);
        let live = Arc::new(ClickAwaySession::new());
        let bounds = Bounds::new(
            point(px(x), px(y)),
            Size::new(px(WINDOW_SIZE), px(WINDOW_SIZE)),
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: cx.primary_display().map(|display| display.id()),
            ..ring_window_options()
        };
        let opened = cx.open_window(options, |_, cx| {
            cx.new(|_| {
                let mut view = RingView::new(invocation, commands, &live);
                if held {
                    view = view.preview_held();
                }
                if let Some(appearance) = appearance {
                    view = view.preview_appearance(appearance);
                }
                if hover.eq_ignore_ascii_case("center") {
                    view.preview_cancel_hovered()
                } else {
                    view.preview_hovered(slot_named(&hover))
                }
            })
        });
        if opened.is_ok() {
            crate::platform::configure_windows();
        }
    });
    Ok(())
}

//! Lightweight GPUI host for the cursor-centred Actions Ring.
//!
//! This process is a pure IPC client. The agent owns HID++, session validation,
//! haptic output, and action execution; the overlay only renders the
//! agent-snapshotted actions and reports hover/activate/cancel interactions.

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

// `t!` resolves against a backend the invoking crate must generate itself, so
// both binaries expand `i18n!` over the one catalog in `openlogi-ui` — the same
// crate this one already depends on for locale negotiation.
rust_i18n::i18n!("../openlogi-ui/locales", fallback = "en");

mod ipc;
mod platform;
#[cfg(feature = "ring-preview")]
mod preview;
mod ring;
mod session;

use std::sync::Arc;

use anyhow::Result;
use gpui::AppContext as _;
use tracing::warn;

use openlogi_core::action_ring::DISPLAY_LIFETIME;

use crate::ipc::OverlayCommand;
use crate::platform::RingPlacement;
use crate::ring::RingView;
use crate::session::{ClickAwaySession, claim_the_role, spawn_click_away_dismissal};

fn main() -> Result<()> {
    openlogi_core::logging::init_stderr();

    openlogi_core::locale::activate(None);
    #[cfg(feature = "ring-preview")]
    if let Ok(which) = std::env::var("OPENLOGI_RING_PREVIEW") {
        return preview::run(which);
    }
    // Held for the whole run: dropping it hands the role to the replacement.
    let _tenancy = claim_the_role()?;
    let ipc::Handle {
        mut invocations,
        commands,
    } = ipc::spawn();

    let mut app = gpui_platform::application().with_assets(openlogi_ui::action_icons::ActionIcons);
    app = app.with_quit_mode(gpui::QuitMode::Explicit);
    app.run(move |cx| {
        platform::configure_application();
        let live_session = Arc::new(ClickAwaySession::new());
        spawn_click_away_dismissal(cx, Arc::clone(&live_session));
        cx.spawn(async move |cx| {
            // The ring on screen, so a later observation of the same session
            // (its trigger released) reaches that view instead of reopening it,
            // and whether its release has been handled: a reconnect re-delivers
            // the released invocation, which must not commit a second time.
            let mut showing: Option<(u64, gpui::WindowHandle<RingView>, bool)> = None;
            while let Some(observed) = invocations.recv().await {
                // No ring is what a dismissal looks like: close whatever is
                // showing and open nothing. The agent has already forgotten the
                // session, so there is nothing to acknowledge either.
                let Some(invocation) = observed else {
                    showing = None;
                    cx.update(|cx| {
                        for handle in cx.windows() {
                            let _ = handle.update(cx, |_, window, _| window.remove_window());
                        }
                    });
                    continue;
                };
                if let Some((session_id, handle, released)) = showing.as_mut()
                    && *session_id == invocation.session_id
                {
                    if invocation.trigger_released && !*released {
                        *released = true;
                        release(*handle, invocation.release_selects, cx);
                    }
                    continue;
                }
                openlogi_core::locale::activate(invocation.language.as_deref());
                showing = cx.update(|cx| {
                    for handle in cx.windows() {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    let placement = match RingPlacement::capture(cx) {
                        Ok(placement) => placement,
                        Err(error) => {
                            warn!(%error, "could not locate Actions Ring display");
                            let _ = commands.send(OverlayCommand::Cancel {
                                session_id: invocation.session_id,
                            });
                            return None;
                        }
                    };
                    let commands = commands.clone();
                    let timeout_commands = commands.clone();
                    let session_id = invocation.session_id;
                    // Already released when first seen: a tap that finished
                    // before the window opened, with nothing left to commit.
                    let released = invocation.trigger_released;
                    let origin = placement.global_origin();
                    match cx.open_window(placement.window_options(), |_, cx| {
                        cx.new(|cx| {
                            RingView::new(invocation, commands, &live_session)
                                .anchored(origin)
                                .watch_hold(cx)
                        })
                    }) {
                        Ok(handle) => {
                            if let Err(error) = handle
                                .update(cx, |_, window, _| placement.show(window))
                                .and_then(std::convert::identity)
                            {
                                warn!(%error, "could not position Actions Ring window");
                                let _ = handle.update(cx, |_, window, _| window.remove_window());
                                let _ =
                                    timeout_commands.send(OverlayCommand::Cancel { session_id });
                                return None;
                            }
                            cx.spawn(async move |cx| {
                                cx.background_executor().timer(DISPLAY_LIFETIME).await;
                                if handle
                                    .update(cx, |_, window, _| window.remove_window())
                                    .is_ok()
                                {
                                    let _ = timeout_commands
                                        .send(OverlayCommand::Cancel { session_id });
                                }
                            })
                            .detach();
                            Some((session_id, handle, released))
                        }
                        Err(error) => {
                            warn!(%error, "could not open Actions Ring window");
                            None
                        }
                    }
                });
            }
        })
        .detach();
    });
    Ok(())
}

/// Hand the showing ring's trigger release to its view.
fn release(handle: gpui::WindowHandle<RingView>, selects: bool, cx: &mut gpui::AsyncApp) {
    let _ = cx.update(|cx| {
        handle.update(cx, |view, window, cx| {
            view.release_trigger(selects, window, cx);
        })
    });
}

#[cfg(test)]
mod tests;

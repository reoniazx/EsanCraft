//! Main-thread ownership for independently repainted native viewports.
//! Deferred callbacks capture only an id, never the audio stream or GUI handles.

use crate::{SoundApp, main_windows};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

thread_local! {
    static HOSTS: RefCell<HashMap<u64, Weak<RefCell<SoundApp>>>> = RefCell::new(HashMap::new());
}

static NEXT_HOST: AtomicU64 = AtomicU64::new(1);

pub struct NativeUiHost {
    pub state: Rc<RefCell<SoundApp>>,
    id: u64,
    view_state: RefCell<Option<(u64, crate::UiState, Option<&'static str>)>>,
}

impl NativeUiHost {
    pub fn new(mut app: SoundApp) -> Result<Self, String> {
        let id = NEXT_HOST.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1)).map_err(|_| "UI host ids exhausted")?;
        app.main_windows.deferred = true;
        let state = Rc::new(RefCell::new(app));
        HOSTS.with(|hosts| {
            hosts.try_borrow_mut().map_err(|_| "UI host registry busy")?.insert(id, Rc::downgrade(&state));
            Ok::<(), String>(())
        })?;
        Ok(Self { state, id, view_state: RefCell::new(None) })
    }

    pub fn ui(&self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let started = std::time::Instant::now();
        let spec = {
            let Ok(mut app) = self.state.try_borrow_mut() else {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
                return;
            };
            app.ui(ui);
            // Compare across frames, including commands drained by logic before
            // drawing. Clone preferences only when their visible state changes.
            let changed = if let Ok(mut cache) = self.view_state.try_borrow_mut() {
                if let Some((revision, previous, dialog)) = cache.as_mut() {
                    previous.window = app.ui.window;
                    let ui_changed = *previous != app.ui;
                    let changed = *revision != app.engine.revision || ui_changed || *dialog != app.dialogs.open_name();
                    *dialog = app.dialogs.open_name();
                    *revision = app.engine.revision;
                    if ui_changed {
                        *previous = app.ui.clone();
                    }
                    changed
                } else {
                    *cache = Some((app.engine.revision, app.ui.clone(), app.dialogs.open_name()));
                    true
                }
            } else {
                true
            };
            if app.ui.both_windows && !app.main_windows.secondary_hidden && changed {
                ctx.request_repaint_of(main_windows::secondary_id());
            }
            app.main_windows.root_ui_ms = started.elapsed().as_secs_f64() * 1000.0;
            main_windows::secondary_builder(&app, &ctx)
        }; // Release the borrow before registering a callback (including test backends).
        if let Some(builder) = spec {
            let id = self.id;
            ctx.show_viewport_deferred(main_windows::secondary_id(), builder, move |ui, _class| render_secondary(id, ui));
        }
        if let Ok(mut app) = self.state.try_borrow_mut() {
            main_windows::flush_focus(&mut app, &ctx, app_has_secondary(&ctx));
        }
    }

    pub fn raw_input_hook(&self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if raw.viewport_id != egui::ViewportId::ROOT && raw.viewport_id != main_windows::secondary_id() {
            return;
        }
        let Ok(mut app) = self.state.try_borrow_mut() else { return };
        if raw.viewport_id == egui::ViewportId::ROOT {
            app.raw_input_hook(raw);
        }
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        if raw.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. })) {
            let primary = app.main_windows.primary(&app);
            let window = if raw.viewport_id == egui::ViewportId::ROOT { primary } else { primary.other() };
            main_windows::activate(&mut app, window);
        }
        // Window switching is a global shortcut, including while a search or
        // comments field has focus. A modal document dialog still owns input.
        // eframe preserves UI input during logic-only passes. Dispatch fresh
        // focus keys here, and consume them so they cannot replay when an
        // occluded root later draws, or wait behind a secondary repaint.
        main_windows::consume_focus_keys(&mut app, &mut raw.events);
    }
}

fn app_has_secondary(ctx: &egui::Context) -> bool {
    ctx.viewport_for(main_windows::secondary_id(), |v| v.used)
}

fn render_secondary(id: u64, ui: &mut egui::Ui) {
    let state = HOSTS.with(|hosts| hosts.try_borrow().ok().and_then(|hosts| hosts.get(&id).and_then(Weak::upgrade)));
    let Some(state) = state else { return };
    let Ok(mut app) = state.try_borrow_mut() else {
        // Native file dialogs may pump events while the root view owns state.
        // Defer rather than blocking, panicking, or making the app Send/Sync.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        return;
    };
    let started = std::time::Instant::now();
    main_windows::show_secondary(&mut app, ui);
    app.main_windows.secondary_ui_ms = started.elapsed().as_secs_f64() * 1000.0;
}

impl Drop for NativeUiHost {
    fn drop(&mut self) {
        HOSTS.with(|hosts| {
            if let Ok(mut hosts) = hosts.try_borrow_mut() {
                hosts.remove(&self.id);
            }
        });
    }
}

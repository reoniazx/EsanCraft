//! Main-thread ownership for independently repainted native viewports.
//! Deferred callbacks capture only an id, never the audio stream or GUI handles.

use crate::{SoundApp, main_windows};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

thread_local! {
    static HOSTS: RefCell<HashMap<u64, Weak<RefCell<SoundApp>>>> = RefCell::new(HashMap::new());
    static PRESENTERS: RefCell<HashMap<u64, Rc<dyn crate::context_menu::Presenter>>> = RefCell::new(HashMap::new());
}

static NEXT_HOST: AtomicU64 = AtomicU64::new(1);

pub struct NativeUiHost {
    pub state: Rc<RefCell<SoundApp>>,
    id: u64,
    view_state: RefCell<Option<(u64, crate::UiState, Option<&'static str>)>>,
}

impl NativeUiHost {
    pub fn set_context_menu_presenter(&self, presenter: Rc<dyn crate::context_menu::Presenter>) -> Result<(), String> {
        let mut app = self.state.try_borrow_mut().map_err(|_| "UI host busy")?;
        PRESENTERS.with(|presenters| {
            presenters.try_borrow_mut().map_err(|_| "Context menu registry busy")?.insert(self.id, presenter);
            Ok::<(), String>(())
        })?;
        app.context_menus.enabled = true;
        Ok(())
    }

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
            crate::dropdown::prepare(&app, &ctx);
            app.guarded(|app| app.ui(ui));
            crate::dropdown::collect(&mut app, &ctx);
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
        present_pending(self.id, &ctx);
    }

    pub fn raw_input_hook(&self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let presenter = PRESENTERS.with(|p| p.try_borrow().ok().and_then(|p| p.get(&self.id).cloned()));
        if let Some(presenter) = presenter {
            presenter.capture_input(raw);
        }
        if raw.viewport_id != egui::ViewportId::ROOT && raw.viewport_id != main_windows::secondary_id() {
            return;
        }
        let Ok(mut app) = self.state.try_borrow_mut() else { return };
        if raw.viewport_id == egui::ViewportId::ROOT {
            app.raw_input_hook(raw);
        }
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        if app.ui.mix_separate {
            main_windows::consume_focus_keys(&mut app, &mut raw.events);
            return;
        }
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
    crate::dropdown::prepare(&app, ui.ctx());
    app.guarded(|app| main_windows::show_secondary(app, ui));
    crate::dropdown::collect(&mut app, ui.ctx());
    app.main_windows.secondary_ui_ms = started.elapsed().as_secs_f64() * 1000.0;
    drop(app);
    present_pending(id, ui.ctx());
}

fn present_pending(id: u64, ctx: &egui::Context) {
    let state = HOSTS.with(|hosts| hosts.try_borrow().ok().and_then(|hosts| hosts.get(&id).and_then(Weak::upgrade)));
    let presenter = PRESENTERS.with(|p| p.try_borrow().ok().and_then(|p| p.get(&id).cloned()));
    let (Some(state), Some(presenter)) = (state, presenter) else { return };
    let request = {
        let Ok(mut app) = state.try_borrow_mut() else { return };
        if app.context_menus.active {
            return;
        }
        let Some(request) = app.context_menus.pending.take() else { return };
        if request.epoch != app.context_menus.epoch || !crate::context_menu::valid(&app, &request.target) {
            return;
        }
        app.context_menus.active = true;
        request
    };
    // AppKit tracks the menu in a nested event loop. Neither the application nor the
    // registries may be borrowed here: another viewport may repaint or close.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| presenter.present(&request)))
        .unwrap_or_else(|_| Err("Native context menu failed; using in-window menus".into()));
    if let Ok(mut app) = state.try_borrow_mut() {
        app.context_menus.active = false;
        match result {
            Ok(Some(selection)) if request.epoch == app.context_menus.epoch && crate::context_menu::valid(&app, &request.target) => {
                if let crate::context_menu::Action::Choice { control, index, label } = selection.action {
                    let pass = ctx.cumulative_pass_nr();
                    ctx.data_mut(|data| data.insert_temp(control, crate::dropdown::Pick { index, label, epoch: request.epoch, pass }));
                } else {
                    app.guarded(|app| crate::context_menu::execute(app, selection.action, selection.modifiers, request.viewport));
                }
            }
            Err(error) => {
                log::warn!("Native context menu: {error}");
                app.context_menus.enabled = false;
                app.ui.status = error;
            }
            _ => {}
        }
    }
    ctx.request_repaint_of(request.viewport);
    ctx.request_repaint_of(egui::ViewportId::ROOT);
}

impl Drop for NativeUiHost {
    fn drop(&mut self) {
        HOSTS.with(|hosts| {
            if let Ok(mut hosts) = hosts.try_borrow_mut() {
                hosts.remove(&self.id);
            }
        });
        PRESENTERS.with(|p| {
            if let Ok(mut p) = p.try_borrow_mut() {
                p.remove(&self.id);
            }
        });
    }
}

#[cfg(test)]
mod context_menu_tests {
    use super::*;
    use crate::context_menu::{self, Action, Presenter, Request, Selection, Target};
    use egui::{Modifiers, ViewportId};
    use serde_json::json;

    struct Mock {
        state: Weak<RefCell<SoundApp>>,
        requests: RefCell<Vec<Request>>,
        result: RefCell<Result<Option<Selection>, String>>,
        replace_session: bool,
    }

    impl Presenter for Mock {
        fn present(&self, request: &Request) -> Result<Option<Selection>, String> {
            // Reentrant callbacks must be able to borrow the app while tracking.
            let state = self.state.upgrade().unwrap();
            let mut app = state.try_borrow_mut().expect("host must release the app before tracking");
            assert!(app.context_menus.active);
            if self.replace_session {
                app.run("session.new", json!({})).unwrap();
            }
            self.requests.borrow_mut().push(request.clone());
            self.result.borrow_mut().clone()
        }
    }

    fn setup(result: Result<Option<Selection>, String>, replace_session: bool) -> (NativeUiHost, Rc<Mock>, soundcraft_model::TrackId) {
        let mut engine = soundcraft_engine::Engine::default();
        engine.execute("track.new", &json!({"name": "Voice"})).unwrap();
        let track = engine.session().tracks[0].id;
        let host = NativeUiHost::new(SoundApp::new(engine, None, crate::Services::default())).unwrap();
        let presenter =
            Rc::new(Mock { state: Rc::downgrade(&host.state), requests: RefCell::new(vec![]), result: RefCell::new(result), replace_session });
        host.set_context_menu_presenter(presenter.clone()).unwrap();
        let mut app = host.state.borrow_mut();
        let target = Target::Track(track);
        app.context_menus.pending = Some(Request {
            entries: context_menu::describe(&app, &target),
            target,
            viewport: main_windows::secondary_id(),
            epoch: app.context_menus.epoch,
            anchor: crate::context_menu::Anchor::Pointer,
        });
        drop(app);
        (host, presenter, track)
    }

    #[test]
    fn dispatches_once_after_tracking_without_borrowing_the_app() {
        let (host, presenter, track) = setup(Ok(None), false);
        *presenter.result.borrow_mut() =
            Ok(Some(Selection { action: Action::Command("track.hide".into(), json!({"tracks": [track.0]})), modifiers: Modifiers::NONE }));
        let before = host.state.borrow().engine.undo_history().len();
        let ctx = egui::Context::default();
        present_pending(host.id, &ctx);
        present_pending(host.id, &ctx);
        let app = host.state.borrow();
        assert!(app.engine.session().track(track).unwrap().hidden);
        assert_eq!(app.engine.undo_history().len(), before + 1);
        assert!(!app.context_menus.active);
        assert_eq!(presenter.requests.borrow().len(), 1);
        assert_eq!(presenter.requests.borrow()[0].viewport, main_windows::secondary_id());
    }

    #[test]
    fn cancellation_does_not_execute_and_failure_enables_the_fallback() {
        for result in [Ok(None), Err("Test presenter unavailable".into())] {
            let failed = result.is_err();
            let (host, presenter, _) = setup(result, false);
            let before = host.state.borrow().engine.undo_history().len();
            present_pending(host.id, &egui::Context::default());
            let app = host.state.borrow();
            assert_eq!(app.engine.undo_history().len(), before);
            assert_eq!(app.context_menus.enabled, !failed);
            assert!(!app.context_menus.active);
            assert_eq!(presenter.requests.borrow().len(), 1);
        }
    }

    #[test]
    fn ignores_results_from_a_replaced_session_and_deleted_targets() {
        let (host, presenter, _) =
            setup(Ok(Some(Selection { action: Action::Command("track.new".into(), json!({"name": "Stale"})), modifiers: Modifiers::NONE })), true);
        present_pending(host.id, &egui::Context::default());
        assert!(host.state.borrow().engine.session().tracks.is_empty());
        assert_eq!(presenter.requests.borrow().len(), 1);

        let (host, presenter, track) = setup(Ok(None), false);
        host.state.borrow_mut().run("track.delete", json!({"tracks": [track.0]})).unwrap();
        present_pending(host.id, &egui::Context::default());
        assert!(presenter.requests.borrow().is_empty());
    }

    #[test]
    fn search_is_opened_on_the_requesting_viewport() {
        let (host, presenter, track) = setup(Ok(None), false);
        let viewport = ViewportId::from_hash_of("search origin");
        host.state.borrow_mut().context_menus.pending.as_mut().unwrap().viewport = viewport;
        *presenter.result.borrow_mut() = Ok(Some(Selection {
            action: Action::Command("window.plugin_search".into(), json!({"track": track.0, "slot": 0})),
            modifiers: Modifiers::NONE,
        }));
        present_pending(host.id, &egui::Context::default());
        assert_eq!(host.state.borrow().context_menus.search.as_ref().unwrap().viewport, viewport);
    }

    #[test]
    fn native_choices_are_delivered_without_reentering_the_context_lock() {
        let (host, presenter, _) = setup(Ok(None), false);
        let control = egui::Id::new("form-control");
        *presenter.result.borrow_mut() =
            Ok(Some(Selection { action: Action::Choice { control, index: 1, label: "Stereo".into() }, modifiers: Modifiers::NONE }));
        let ctx = egui::Context::default();
        present_pending(host.id, &ctx);
        let pick = ctx.data(|data| data.get_temp::<crate::dropdown::Pick>(control)).unwrap();
        assert_eq!(pick.index, 1);
        assert_eq!(pick.label, "Stereo");
        assert!(!host.state.borrow().context_menus.active);
    }
}

#![cfg(not(target_arch = "wasm32"))]

use egui::{Event, Modifiers, Pos2, RawInput, Rect, ViewportCommand, ViewportId, ViewportInfo, vec2};
use serde_json::json;
use soundcraft_ui_egui::{MainWindow, Services, SoundApp, main_windows, native_host::NativeUiHost};
use std::sync::Arc;

type Callback = Arc<dyn Fn(&mut egui::Ui) + Send + Sync>;

fn host() -> NativeUiHost {
    NativeUiHost::new(SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default())).unwrap()
}

fn raw(id: ViewportId, events: Vec<Event>, time: f64) -> RawInput {
    let mut input = RawInput {
        viewport_id: id,
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))),
        events,
        time: Some(time),
        ..Default::default()
    };
    for viewport in [ViewportId::ROOT, main_windows::secondary_id()] {
        input.viewports.insert(
            viewport,
            ViewportInfo {
                focused: Some(viewport == ViewportId::ROOT),
                minimized: Some(false),
                outer_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1028.0))),
                inner_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))),
                ..Default::default()
            },
        );
    }
    input
}

fn root(ctx: &egui::Context, host: &NativeUiHost, events: Vec<Event>, time: f64) -> egui::FullOutput {
    let mut input = raw(ViewportId::ROOT, events, time);
    host.raw_input_hook(ctx, &mut input);
    let mut output = ctx.run_ui(input, |ui| {
        host.state.borrow_mut().logic(ui.ctx());
        host.ui(ui);
    });
    output.textures_delta.clear();
    output
}

fn ready(ctx: &egui::Context, host: &NativeUiHost) {
    ctx.set_embed_viewports(false);
    for i in 0..6 {
        let _ = root(ctx, host, vec![], f64::from(i) / 60.0);
    }
}

fn open(ctx: &egui::Context, host: &NativeUiHost) -> Callback {
    host.state.borrow_mut().run("window.mix", json!({})).unwrap();
    let output = root(ctx, host, vec![], 0.2);
    assert!(output.viewport_output[&main_windows::secondary_id()].class == egui::ViewportClass::Deferred);
    output.viewport_output[&main_windows::secondary_id()].viewport_ui_cb.clone().unwrap()
}

fn child(ctx: &egui::Context, callback: &Callback, events: Vec<Event>, time: f64) {
    let mut output = ctx.run_ui(raw(main_windows::secondary_id(), events, time), |ui| callback(ui));
    output.textures_delta.clear();
}

fn frames(host: &NativeUiHost, ctx: &egui::Context) -> (u64, u64) {
    let app = host.state.borrow();
    let stats = app.inspect(ctx)["main_windows"]["perf"].clone();
    (stats["root_frames"].as_u64().unwrap(), stats["secondary_frames"].as_u64().unwrap())
}

#[test]
fn deferred_child_repaints_without_drawing_or_ticking_the_root() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let callback = open(&ctx, &host);
    let before = frames(&host, &ctx);
    let revision = host.state.borrow().engine.revision;
    for i in 0..20 {
        child(&ctx, &callback, vec![], 0.3 + f64::from(i) / 60.0);
    }
    let after = frames(&host, &ctx);
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1 + 20);
    assert_eq!(host.state.borrow().engine.revision, revision);
    let _ = root(&ctx, &host, vec![], 1.0);
    assert_eq!(frames(&host, &ctx).1, after.1, "root repaint must not paint the Mixer");
}

#[test]
fn rapid_shortcuts_keep_parity_coalesce_focus_and_never_restore_normal_windows() {
    for count in [1, 2, 99, 100] {
        let host = host();
        let ctx = egui::Context::default();
        ready(&ctx, &host);
        let _ = open(&ctx, &host);
        host.state.borrow_mut().run("window.edit", json!({})).unwrap();
        let _ = root(&ctx, &host, vec![], 0.5);
        let revision = host.state.borrow().engine.revision;
        let mut events = vec![];
        for _ in 0..count {
            for pressed in [true, false] {
                events.push(Event::Key { key: egui::Key::Equals, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND });
            }
        }
        let output = root(&ctx, &host, events, 1.0);
        let commands: Vec<_> = output.viewport_output.values().flat_map(|v| &v.commands).collect();
        assert!(commands.iter().filter(|c| ***c == ViewportCommand::Focus).count() <= 1);
        assert!(!commands.iter().any(|c| matches!(c, ViewportCommand::Minimized(false))));
        let app = host.state.borrow();
        assert_eq!(app.ui.window, if count % 2 == 0 { MainWindow::Edit } else { MainWindow::Mix });
        assert!(app.ui.both_windows);
        assert_eq!(app.engine.revision, revision);
        assert_eq!(ctx.zoom_factor(), 1.0);
    }
}

#[test]
fn stale_root_focus_notification_cannot_undo_the_requested_mix_focus() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let _ = open(&ctx, &host);
    let _ = root(&ctx, &host, vec![Event::WindowFocused(true)], 0.21);
    assert_eq!(host.state.borrow().ui.window, MainWindow::Mix);
}

#[test]
fn deferred_callbacks_handle_reentrant_borrows_and_retired_hosts_without_panicking() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let callback = open(&ctx, &host);
    let before = frames(&host, &ctx);
    {
        let _guard = host.state.borrow_mut();
        child(&ctx, &callback, vec![], 0.3);
    }
    assert_eq!(frames(&host, &ctx), before);
    child(&ctx, &callback, vec![], 0.4);
    let state = host.state.clone();
    drop(host);
    let revision = state.borrow().engine.revision;
    child(&ctx, &callback, vec![], 0.5);
    assert_eq!(state.borrow().engine.revision, revision);
}

#[test]
fn mixer_virtualizes_large_sessions_and_releases_read_snapshots() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let callback = open(&ctx, &host);
    host.state.borrow_mut().run("track.new", json!({"count":100,"kind":"audio","format":"stereo"})).unwrap();
    for i in 0..4 {
        child(&ctx, &callback, vec![], 0.3 + f64::from(i) / 60.0);
    }
    let app = host.state.borrow();
    let stats = app.inspect(&ctx)["main_windows"]["perf"].clone();
    let visible = stats["visible_mix_strips"].as_u64().unwrap();
    assert!(visible > 0 && visible < 20, "painted {visible} strips for a 100-track session");
    let snapshot = app.engine.session_arc();
    assert_eq!(Arc::strong_count(&snapshot), 2, "the renderer must not retain a session snapshot after the frame");
}

#[test]
fn control_edits_before_root_drawing_invalidate_the_deferred_mixer() {
    let (tx, rx) = std::sync::mpsc::channel();
    let app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default()).with_control(rx);
    let host = NativeUiHost::new(app).unwrap();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let callback = open(&ctx, &host);
    for i in 0..6 {
        child(&ctx, &callback, vec![], 0.3 + f64::from(i) / 60.0);
    }
    let (request, reply) =
        soundcraft_ui_egui::ControlRequest::new("engine.execute", json!({"command":"mix.volume","params":{"track":"Kick","db":-18}}));
    tx.send(request).unwrap();
    let output = root(&ctx, &host, vec![], 1.0);
    assert_eq!(reply.try_recv().unwrap()["ok"], true);
    assert_eq!(output.viewport_output[&main_windows::secondary_id()].repaint_delay, std::time::Duration::ZERO);
    child(&ctx, &callback, vec![], 1.01);
    assert_eq!(host.state.borrow().engine.session().track_by_name("Kick").unwrap().mixer.volume_db, -18.0);
}

#[test]
fn focus_keys_dispatch_once_even_when_root_only_runs_logic() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let events = [true, false]
        .map(|pressed| Event::Key { key: egui::Key::Equals, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND })
        .to_vec();
    let mut input = raw(ViewportId::ROOT, events, 1.0);
    host.raw_input_hook(&ctx, &mut input);
    assert_eq!(host.state.borrow().ui.window, MainWindow::Mix);
    assert!(input.events.is_empty(), "handled focus keys must not be retained by eframe");
    for _ in 0..5 {
        let _ = ctx.run_logic(&input, |ctx| host.state.borrow_mut().logic(ctx));
        host.raw_input_hook(&ctx, &mut input);
        assert_eq!(host.state.borrow().ui.window, MainWindow::Mix);
    }
    let _ = root(&ctx, &host, vec![], 1.1);
    assert_eq!(host.state.borrow().ui.window, MainWindow::Mix);
}

#[test]
fn secondary_focus_shortcut_is_global_and_ignores_key_repeat() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let _ = open(&ctx, &host);
    let mut text = String::new();
    let _ = ctx.run_ui(raw(main_windows::secondary_id(), vec![], 0.3), |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
    });
    assert!(ctx.egui_wants_keyboard_input());
    let mut input = raw(
        main_windows::secondary_id(),
        vec![Event::Key { key: egui::Key::Equals, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }],
        0.4,
    );
    host.raw_input_hook(&ctx, &mut input);
    assert_eq!(host.state.borrow().ui.window, MainWindow::Edit);
    assert!(input.events.is_empty());
    input.events.push(Event::Key { key: egui::Key::Equals, physical_key: None, pressed: true, repeat: true, modifiers: Modifiers::COMMAND });
    host.raw_input_hook(&ctx, &mut input);
    assert_eq!(host.state.borrow().ui.window, MainWindow::Edit);
    assert!(input.events.is_empty());
}

#[test]
fn control_focus_shortcut_does_not_wait_for_another_root_input_pass() {
    let (tx, rx) = std::sync::mpsc::channel();
    let host = NativeUiHost::new(SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default()).with_control(rx)).unwrap();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    for (i, target) in [MainWindow::Mix, MainWindow::Edit, MainWindow::Mix].into_iter().enumerate() {
        let (request, reply) = soundcraft_ui_egui::ControlRequest::new("ui.key", json!({"key":"=","cmd":true}));
        tx.send(request).unwrap();
        let _ = ctx.run_logic(&raw(ViewportId::ROOT, vec![], 0.5 + i as f64 * 0.1), |ctx| host.state.borrow_mut().logic(ctx));
        assert_eq!(reply.try_recv().unwrap()["ok"], true);
        assert_eq!(host.state.borrow().ui.window, target);
        let _ = root(&ctx, &host, vec![], 0.55 + i as f64 * 0.1);
        assert_eq!(host.state.borrow().ui.window, target, "queued keys must not replay in the next input pass");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn restoring_a_parked_window_orders_it_once_and_still_activates_background_apps() {
    for active in [true, false] {
        let host = host();
        let ctx = egui::Context::default();
        ready(&ctx, &host);
        let _ = open(&ctx, &host);
        host.state.borrow_mut().run("window.edit", json!({})).unwrap();
        let _ = root(&ctx, &host, vec![], 1.0);
        host.state.borrow_mut().run("window.mix", json!({})).unwrap();
        let mut input = raw(ViewportId::ROOT, vec![], 1.1);
        input.focused = active;
        input.viewports.get_mut(&ViewportId::ROOT).unwrap().focused = Some(active);
        let mut output = ctx.run_ui(input, |ui| {
            host.state.borrow_mut().logic(ui.ctx());
            host.ui(ui);
        });
        output.textures_delta.clear();
        let commands = &output.viewport_output[&main_windows::secondary_id()].commands;
        assert_eq!(commands.iter().filter(|c| **c == ViewportCommand::Visible(true)).count(), 1);
        assert_eq!(commands.contains(&ViewportCommand::Focus), !active);
        assert_eq!(host.state.borrow().ui.window, MainWindow::Mix);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn fully_covered_windows_are_parked_without_closing_and_tile_restores_them() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let _ = open(&ctx, &host);
    host.state.borrow_mut().run("window.edit", json!({})).unwrap();
    let output = root(&ctx, &host, vec![], 1.0);
    assert!(host.state.borrow().ui.both_windows);
    assert_eq!(host.state.borrow().inspect(&ctx)["main_windows"]["parked"]["secondary"], true);
    assert_eq!(
        output.viewport_output[&main_windows::secondary_id()].builder.visible,
        None,
        "visibility commands must not be duplicated by viewport builder patches"
    );
    assert!(output.viewport_output[&main_windows::secondary_id()].commands.contains(&ViewportCommand::Visible(false)));
    let mut input = raw(ViewportId::ROOT, vec![], 1.1);
    input.viewports.get_mut(&main_windows::secondary_id()).unwrap().outer_rect =
        Some(Rect::from_min_size(egui::pos2(1700.0, 0.0), vec2(800.0, 1000.0)));
    let mut output = ctx.run_ui(input, |ui| {
        host.state.borrow_mut().logic(ui.ctx());
        host.ui(ui);
    });
    output.textures_delta.clear();
    assert_eq!(host.state.borrow().inspect(&ctx)["main_windows"]["parked"]["secondary"], false);
    assert!(output.viewport_output[&main_windows::secondary_id()].commands.contains(&ViewportCommand::Visible(true)));
    assert!(host.state.borrow().ui.both_windows);
}

#[test]
fn a_save_prompt_opened_from_root_logic_invalidates_the_active_mixer() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let callback = open(&ctx, &host);
    child(&ctx, &callback, vec![], 0.3);
    let _ = root(&ctx, &host, vec![], 0.4);
    {
        let mut app = host.state.borrow_mut();
        app.run("mix.volume", json!({"track":"Kick","db":-18})).unwrap();
        assert!(soundcraft_ui_egui::dialogs::ask_to_save(&mut app, "app.quit", &json!({})));
    }
    let output = root(&ctx, &host, vec![], 0.5);
    assert_eq!(output.viewport_output[&main_windows::secondary_id()].repaint_delay, std::time::Duration::ZERO);
    child(&ctx, &callback, vec![], 0.6);
    assert_eq!(host.state.borrow().dialogs.open_name(), Some("save_changes"));
    assert!(!host.state.borrow().quit_requested);
    let _ = root(
        &ctx,
        &host,
        vec![Event::Key { key: egui::Key::Equals, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }],
        0.7,
    );
    assert_eq!(host.state.borrow().ui.window, MainWindow::Mix, "the modal save prompt owns input");
}

#[test]
fn a_visible_mixer_schedules_paced_shared_logic_when_the_root_is_occluded() {
    let host = host();
    let ctx = egui::Context::default();
    ready(&ctx, &host);
    let callback = open(&ctx, &host);
    host.state.borrow_mut().run("transport.play", json!({})).unwrap();
    let _ = root(&ctx, &host, vec![], 1.0);
    let mut input = raw(main_windows::secondary_id(), vec![], 1.1);
    input.viewports.get_mut(&ViewportId::ROOT).unwrap().occluded = Some(true);
    let mut output = ctx.run_ui(input, |ui| callback(ui));
    output.textures_delta.clear();
    let delay = output.viewport_output[&ViewportId::ROOT].repaint_delay;
    assert!(delay <= std::time::Duration::from_millis(50), "shared logic was scheduled after {delay:?}");
    assert!(host.state.borrow().is_playing());
}

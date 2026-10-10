use egui::{Event, Id, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp, midi_editor};

fn frame(ctx: &egui::Context, app: &mut SoundApp, events: Vec<Event>) {
    let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))), events, ..Default::default() };
    ctx.run_ui(raw, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    })
    .textures_delta
    .clear();
}

fn panel(ctx: &egui::Context) -> Rect {
    egui::containers::panel::PanelState::load(ctx, Id::new("midi_editor")).unwrap().outer_rect
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() }
}

fn check_resize(with_clip: bool) {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.ui.show_midi_editor = true;
    if with_clip {
        app.run("edit.select", json!({"tracks": ["Keys"]})).unwrap();
    }
    assert_eq!(midi_editor::target_clip(&app).is_some(), with_clip);
    let ctx = egui::Context::default();
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![]);
    }
    let initial = panel(&ctx);
    assert!((initial.height() - 320.0).abs() < 1.0, "initial panel: {initial:?}");
    let notes = app.engine.session().clone();

    // Drag the shared boundary up and down, then allow several idle frames
    // to catch a panel that looks resized but snaps back after release.
    for height in [440.0, 280.0, 200.0] {
        let before = panel(&ctx);
        let start = pos2(before.center().x, before.top() + 1.0);
        let end = pos2(start.x, before.bottom() - height);
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=6 {
            frame(&ctx, &mut app, vec![Event::PointerMoved(start.lerp(end, step as f32 / 6.0))]);
        }
        frame(&ctx, &mut app, vec![button(end, false)]);
        for _ in 0..4 {
            frame(&ctx, &mut app, vec![]);
        }
        let after = panel(&ctx);
        assert!((after.height() - height.max(260.0)).abs() < 1.0, "requested {height}, panel: {after:?}");
        assert!((after.bottom() - initial.bottom()).abs() < 1.0);
        assert!(app.edit_layout.timeline[3] <= after.top());
    }

    // Closing and reopening the dock keeps the chosen height.
    let resized = panel(&ctx);
    app.run("window.midi_editor", json!({})).unwrap();
    frame(&ctx, &mut app, vec![]);
    app.run("window.midi_editor", json!({})).unwrap();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![]);
    }
    assert_eq!(panel(&ctx), resized);
    assert_eq!(*app.engine.session(), notes);
}

#[test]
fn midi_panel_resizes_with_a_selected_clip() {
    check_resize(true);
}

#[test]
fn midi_panel_resizes_without_a_selected_clip() {
    check_resize(false);
}

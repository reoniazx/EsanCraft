use egui::{Event, Id, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp, midi_editor};

fn frame(ctx: &egui::Context, app: &mut SoundApp, events: Vec<Event>) {
    frame_at_size(ctx, app, events, vec2(1600.0, 1000.0));
}

fn frame_at_size(ctx: &egui::Context, app: &mut SoundApp, events: Vec<Event>, size: egui::Vec2) {
    let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), events, ..Default::default() };
    ctx.run_ui(raw, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    })
    .textures_delta
    .clear();
}

fn panel(ctx: &egui::Context) -> Rect {
    ctx.data(|data| data.get_temp::<Rect>(Id::new("soundcraft-midi-body"))).unwrap()
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
        let start = pos2(before.center().x, before.top() - 26.0 - 2.0);
        let end = pos2(start.x, before.bottom() - height - 26.0 - 2.0);
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
    let saved = serde_json::to_string(&app.ui).unwrap();
    app.ui = serde_json::from_str(&saved).unwrap();
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

#[test]
fn layout_reset_cancels_a_held_midi_divider_without_editing_the_session() {
    check_held_replacement("reset");
}

#[test]
fn arrange_cancels_a_held_midi_divider_without_editing_the_session() {
    check_held_replacement("arrange");
}

#[test]
fn legacy_visibility_sync_cancels_a_held_midi_divider_without_editing_the_session() {
    check_held_replacement("visibility");
}

fn check_held_replacement(operation: &str) {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.ui.show_midi_editor = true;
    app.run("edit.select", json!({"tracks":["Keys"]})).unwrap();
    app.ui.show_memory_locations = operation == "arrange";
    let session = app.engine.session().clone();
    let ctx = egui::Context::default();
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![]);
    }
    let initial = panel(&ctx);
    let start = pos2(initial.center().x, initial.top() - 28.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
    frame(&ctx, &mut app, vec![button(start, true)]);
    let first = start - vec2(0.0, 80.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(first)]);
    frame(&ctx, &mut app, vec![]);
    assert!((panel(&ctx).height() - 400.0).abs() < 1.0, "first drag must resize");
    let second = start - vec2(0.0, 120.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(second)]);
    frame(&ctx, &mut app, vec![]);
    assert!((panel(&ctx).height() - 440.0).abs() < 1.0, "a normal second held move must continue resizing");
    match operation {
        "reset" => {
            app.run("ui.dock.reset", json!({})).unwrap();
        }
        "arrange" => {
            app.run("window.arrange_cascade", json!({})).unwrap();
        }
        "visibility" => {
            app.run("window.midi_editor", json!({})).unwrap();
            frame(&ctx, &mut app, vec![]);
            app.run("window.midi_editor", json!({})).unwrap();
            frame(&ctx, &mut app, vec![]);
        }
        _ => panic!("unknown synthetic operation"),
    }
    // Arrange opens both main views, changing the available dock height. The
    // shared docking renderer stores a dragged split as a ratio, so compare
    // against its settled height in the arranged view rather than 440 points.
    let expected = if operation == "arrange" {
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        panel(&ctx).height()
    } else if operation == "reset" {
        320.0
    } else {
        440.0
    };
    let reset = serde_json::to_value(&app.ui.docking).unwrap();
    let after_reset = start - vec2(0.0, 160.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(after_reset)]);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(serde_json::to_value(&app.ui.docking).unwrap(), reset, "old held divider must not resize the replacement layout");
    assert!((panel(&ctx).height() - expected).abs() < 1.0, "{operation}: expected {expected}, panel: {:?}, layout: {reset}", panel(&ctx));
    frame(&ctx, &mut app, vec![button(after_reset, false)]);
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![]);
    }
    assert_eq!(serde_json::to_value(&app.ui.docking).unwrap(), reset);
    assert_eq!(*app.engine.session(), session);
}

fn max_scroll(app: &SoundApp) -> f32 {
    let layout = &app.edit_layout;
    (layout.content_h - (layout.timeline[3] - layout.timeline[1]) + 40.0).max(0.0)
}

fn assert_rows_match_scroll(app: &SoundApp) {
    let layout = &app.edit_layout;
    let first = layout.rows.first().unwrap().1;
    assert!((first[1] - (layout.timeline[1] - layout.scroll_y)).abs() < 0.1, "rows use a stale scroll offset");
}

#[test]
fn closing_midi_dock_clamps_timeline_before_drawing_without_wheel_input() {
    for size in [vec2(1600.0, 1000.0), vec2(1600.0, 1400.0)] {
        for with_clip in [false, true] {
            let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
            app.ui.show_midi_editor = true;
            if with_clip {
                app.run("edit.select", json!({"tracks": ["Keys"]})).unwrap();
            }
            let ctx = egui::Context::default();
            for _ in 0..6 {
                frame_at_size(&ctx, &mut app, vec![], size);
            }
            let dock_height = panel(&ctx).height();
            let before_bottom = app.edit_layout.timeline[3];
            let old_max = max_scroll(&app);
            assert!(old_max > 0.0);
            app.edit_layout.scroll_y = old_max;
            frame_at_size(&ctx, &mut app, vec![], size);
            let session = app.engine.session().clone();

            // Close with the actual dock tab, while the pointer is outside the
            // timeline. The release frame must already paint the corrected rows.
            let tab = pos2(50.0, size.y - 33.0);
            frame_at_size(&ctx, &mut app, vec![Event::PointerMoved(tab), button(tab, true)], size);
            frame_at_size(&ctx, &mut app, vec![button(tab, false)], size);
            assert!(!app.ui.show_midi_editor);
            // The shared dock also releases its tab/divider chrome. Closing
            // must free at least the complete MIDI body before drawing rows.
            assert!(app.edit_layout.timeline[3] - before_bottom >= dock_height - 1.0);
            assert!(max_scroll(&app) < old_max);
            assert!((app.edit_layout.scroll_y - max_scroll(&app)).abs() < 0.1);
            assert_rows_match_scroll(&app);
            for _ in 0..4 {
                frame_at_size(&ctx, &mut app, vec![], size);
                assert!((app.edit_layout.scroll_y - max_scroll(&app)).abs() < 0.1);
                assert_rows_match_scroll(&app);
            }
            assert_eq!(*app.engine.session(), session);
        }
    }
}

#[test]
fn closing_midi_dock_preserves_a_scroll_offset_that_still_fits() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.ui.show_midi_editor = true;
    let ctx = egui::Context::default();
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![]);
    }
    for scroll in [0.0, 80.0] {
        app.ui.show_midi_editor = true;
        app.edit_layout.scroll_y = scroll;
        frame(&ctx, &mut app, vec![]);
        app.run("window.midi_editor", json!({})).unwrap();
        frame(&ctx, &mut app, vec![]);
        assert!(max_scroll(&app) >= scroll);
        assert_eq!(app.edit_layout.scroll_y, scroll);
        assert_rows_match_scroll(&app);
    }
}

#[test]
fn growing_window_clamps_timeline_before_drawing_without_wheel_input() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    let ctx = egui::Context::default();
    for _ in 0..6 {
        frame_at_size(&ctx, &mut app, vec![], vec2(1600.0, 600.0));
    }
    app.edit_layout.scroll_y = max_scroll(&app);
    assert!(app.edit_layout.scroll_y > 0.0);
    frame_at_size(&ctx, &mut app, vec![], vec2(1600.0, 1400.0));
    assert_eq!(app.edit_layout.scroll_y, 0.0);
    assert_rows_match_scroll(&app);
}

#[test]
fn shrinking_track_content_clamps_timeline_before_drawing_without_wheel_input() {
    for command in ["track.hide", "track.height"] {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
        let ctx = egui::Context::default();
        for _ in 0..6 {
            frame(&ctx, &mut app, vec![]);
        }
        let old_content = app.edit_layout.content_h;
        app.edit_layout.scroll_y = max_scroll(&app);
        assert!(app.edit_layout.scroll_y > 0.0);
        let tracks: Vec<u64> = app.engine.session().tracks.iter().map(|t| t.id.0).collect();
        app.run(command, json!({"tracks": tracks, "height": "micro"})).unwrap();
        frame(&ctx, &mut app, vec![]);
        assert!(app.edit_layout.content_h < old_content);
        assert_eq!(app.edit_layout.scroll_y, 0.0);
        if command == "track.hide" {
            assert!(app.edit_layout.rows.is_empty());
        } else {
            assert_rows_match_scroll(&app);
        }
    }
}

#[test]
fn a_selected_video_clip_is_not_a_midi_target() {
    use soundcraft_model::{ChannelFormat, Clip, ClipContent, SourceId, Track, TrackKind};
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.run("edit.select", json!({"tracks": ["Keys"]})).unwrap();
    let midi = midi_editor::target_clip(&app).unwrap();
    let s = app.engine.session_mut();
    let track = s.add_track(TrackKind::Video, ChannelFormat::Mono, None);
    let video = s.new_clip_id();
    let source = SourceId(s.alloc());
    s.track_mut(track).and_then(Track::playlist_mut).unwrap().clips.push(Clip::video(video, "picture", source, 0, 0, 48_000));

    app.run("edit.select", json!({"clips": [video.0]})).unwrap();
    assert_eq!(midi_editor::target_clip(&app), None, "a video clip alone leaves the MIDI editors without a target");
    app.run("edit.select", json!({"clips": [video.0, midi.0]})).unwrap();
    assert_eq!(midi_editor::target_clip(&app), Some(midi), "the MIDI clip after a video clip is the target");
    let (_, clip) = app.engine.session().find_clip(midi).unwrap();
    assert!(matches!(clip.content, ClipContent::Midi { .. }));
}

#[test]
fn only_pencil_inserts_notes_in_the_midi_roll() {
    use soundcraft_model::{ClipContent, Tool};

    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    app.ui.show_midi_editor = true;
    let created = app.run("midi.new_clip", json!({"track": "Keys", "start": 0, "length": 96000})).unwrap();
    let clip = created["clip"].as_u64().unwrap();
    app.run("edit.select", json!({"clips": [clip]})).unwrap();
    let ctx = egui::Context::default();
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![]);
    }
    let body = panel(&ctx);
    let point = pos2(body.left() + 200.0, body.top() + 60.0);
    for tool in Tool::ALL.into_iter().filter(|tool| *tool != Tool::Pencil).chain([Tool::Pencil]) {
        app.run("edit.tool", json!({"tool": tool.id()})).unwrap();
        let before = app.engine.session().clone();
        frame(&ctx, &mut app, vec![Event::PointerMoved(point)]);
        frame(&ctx, &mut app, vec![button(point, true)]);
        frame(&ctx, &mut app, vec![button(point, false)]);
        let (_, edited) = app.engine.session().find_clip(soundcraft_model::ClipId(clip)).unwrap();
        let ClipContent::Midi { sequence } = &edited.content else { panic!("MIDI clip required") };
        if tool == Tool::Pencil {
            assert_eq!(sequence.notes.len(), 1);
            app.run("edit.undo", json!({})).unwrap();
        }
        assert!(*app.engine.session() == before, "{tool:?} click must not change the session");
    }
}

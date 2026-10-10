//! Thai names survive text entry in the real rename dialogs and session serialization.

use egui::{Event, Key, Modifiers};
use soundcraft_ui_egui::{Services, SoundApp};

fn frame(ctx: &egui::Context, app: &mut SoundApp, events: Vec<Event>) {
    ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    })
    .textures_delta
    .clear();
}

fn enter_name(ctx: &egui::Context, app: &mut SoundApp, name: &str) {
    for _ in 0..3 {
        frame(ctx, app, vec![]);
    }
    frame(ctx, app, vec![Event::Text(name.into())]);
    frame(ctx, app, vec![Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(app.dialogs.open.is_none());
}

#[test]
fn thai_track_and_clip_names_can_be_entered_and_saved() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    let track = app.engine.session().track_by_name("Kick").unwrap();
    let track_id = track.id;
    let clip_id = track.clips().first().unwrap().id;
    let ctx = egui::Context::default();

    app.dialogs.open_rename_track(track_id, "");
    enter_name(&ctx, &mut app, "ร้องนำ - น้ำเสียง ๑");
    assert_eq!(app.engine.session().track(track_id).unwrap().name, "ร้องนำ - น้ำเสียง ๑");

    app.dialogs.open_rename_clip(clip_id, "");
    enter_name(&ctx, &mut app, "บันทึกเสียงครั้งที่ ๒.wav");
    assert_eq!(app.engine.session().find_clip(clip_id).unwrap().1.name, "บันทึกเสียงครั้งที่ ๒.wav");

    let saved = app.engine.session().to_json().unwrap();
    let restored = soundcraft_model::Session::from_json(&saved).unwrap();
    assert_eq!(restored.track(track_id).unwrap().name, "ร้องนำ - น้ำเสียง ๑");
    assert_eq!(restored.find_clip(clip_id).unwrap().1.name, "บันทึกเสียงครั้งที่ ๒.wav");
}

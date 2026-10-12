use egui::{Color32, Event, FullOutput, Rect, Shape, pos2, vec2};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp, theme::ThemeMode};

#[derive(Debug)]
struct Slot {
    rect: Rect,
    fill: Color32,
    text: Color32,
    border: Color32,
}

// Read the rectangle and foreground emitted by the real Mix drawing path, rather than
// testing a palette token that a custom-painted control might never use.
fn slot(output: &FullOutput, label: &str) -> Slot {
    // Edit can show the same plugin label in its track header while Mix is
    // open alongside it. Select the label inside a real 15-point Mix slot.
    let (text, rect) = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) if text.galley.job.text == label => Some(text),
            _ => None,
        })
        .find_map(|text| {
            let center = text.visual_bounding_rect().center();
            output.shapes.iter().find_map(|shape| match &shape.shape {
                Shape::Rect(rect) if rect.rect.height() == 15.0 && rect.rect.contains(center) => Some((text, rect)),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("missing painted slot for {label}"));
    let foreground = text.override_text_color.unwrap_or(text.galley.job.sections[0].format.color);
    assert_eq!(text.opacity_factor, 1.0);
    assert_eq!(foreground.a(), 255);
    assert_eq!(rect.fill.a(), 255);
    Slot { rect: rect.rect, fill: rect.fill, text: foreground, border: rect.stroke.color }
}

fn contrast(a: Color32, b: Color32) -> f64 {
    let luminance = |color: Color32| {
        let linear = |channel: u8| {
            let s = f64::from(channel) / 255.0;
            if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn populated_mix_instrument_uses_readable_palette_in_real_frames() {
    let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
    assert_eq!(app.ui.theme, ThemeMode::Dark);
    // Keep demo audio loaded for the preservation check while isolating the new strips.
    for track in &mut app.engine.session_mut().tracks {
        track.hidden = true;
    }
    app.run("track.new", json!({"kind": "instrument", "name": "Built-in instrument"})).unwrap();
    let instrument = app.engine.session().edit.selected_tracks[0];
    assert_eq!(app.engine.session().track(instrument).unwrap().instrument.as_ref().unwrap().plugin, "subtractive_synth");
    app.run("mix.insert", json!({"track": instrument.0, "slot": 0, "plugin": "eq_7band"})).unwrap();
    app.run("track.new", json!({"kind": "instrument", "name": "Empty instrument"})).unwrap();
    // An unassigned instrument is a supported model state; track.new normally supplies a synth.
    let empty = app.engine.session().edit.selected_tracks[0];
    app.engine.session_mut().track_mut(empty).unwrap().instrument = None;
    app.run("window.mix", json!({})).unwrap();
    app.run("view.mix_section", json!({"section": "minimal"})).unwrap();
    for section in ["instruments", "inserts_ae"] {
        app.run("view.mix_section", json!({"section": section})).unwrap();
    }
    let session = app.engine.session().clone();
    assert!(!session.pool.is_empty(), "the fixture must include decoded audio");
    let proof_dir = std::env::var_os("SOUNDCRAFT_MIX_PROOF_DIR").map(std::path::PathBuf::from);
    let mut builder = egui_kittest::Harness::builder().with_size(vec2(1600.0, 1000.0)).with_pixels_per_point(2.0);
    if proof_dir.is_some() {
        builder = builder.wgpu();
    }
    let mut harness = builder.build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    harness.input_mut().max_texture_side = Some(8192);
    let mut failures = Vec::new();
    for (name, mode, system, light) in [
        ("light", ThemeMode::Light, Some(egui::Theme::Dark), true),
        ("dark", ThemeMode::Dark, Some(egui::Theme::Light), false),
        ("system-light", ThemeMode::System, Some(egui::Theme::Light), true),
        ("system-dark", ThemeMode::System, Some(egui::Theme::Dark), false),
    ] {
        harness.state_mut().run("ui.theme", json!({"mode": mode.id()})).unwrap();
        harness.input_mut().system_theme = system;
        harness.input_mut().events.push(Event::PointerMoved(pos2(1550.0, 950.0)));
        for _ in 0..4 {
            harness.step();
        }
        let ordinary = slot(harness.output(), "SubSynth");
        let insert = slot(harness.output(), "EQ7");
        let empty = slot(harness.output(), "no instrument");
        let palette = if light { soundcraft_ui_egui::theme::Tokens::LIGHT } else { soundcraft_ui_egui::theme::Tokens::DARK };
        assert_eq!(insert.fill, palette.insert_on);
        assert_eq!(empty.fill, palette.slot_bg);
        assert_eq!(ordinary.text, palette.text);
        if ordinary.fill != insert.fill || ordinary.border != insert.border {
            failures.push(format!("{name}: instrument {ordinary:?} differs from populated insert {insert:?}"));
        }
        for hovered in [false, true] {
            if hovered {
                harness.input_mut().events.push(Event::PointerMoved(ordinary.rect.center()));
                harness.step();
                harness.step();
            }
            let painted = slot(harness.output(), "SubSynth");
            let ratio = contrast(painted.text, painted.fill);
            println!(
                "{name} hover={hovered}: foreground={:?}, fill={:?}, contrast={ratio:.6}:1, rect={:?}",
                painted.text.to_array(),
                painted.fill.to_array(),
                painted.rect
            );
            if ratio < 4.5 {
                failures.push(format!("{name} hover={hovered}: contrast {ratio:.6}:1 is below 4.5:1"));
            }
            if !light {
                let original = Color32::from_rgb(52, 62, 80);
                assert_eq!(painted.fill, if hovered { original.gamma_multiply(1.3) } else { original });
                assert_eq!(painted.border, Color32::from_rgb(16, 16, 16));
            }
            if let Some(dir) = &proof_dir {
                harness.render().unwrap().save(dir.join(format!("{name}-{}.png", if hovered { "hover" } else { "ordinary" }))).unwrap();
            }
        }
        assert_eq!(harness.state().ui.theme, mode);
        assert_eq!(harness.state().engine.session(), &session, "appearance and hover must preserve the full session and decoded audio pool");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    // Exercise the same painted control's ordinary click/picker path after palette changes.
    let at = slot(harness.output(), "SubSynth").rect.center();
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        });
        harness.step();
    }
    harness.step(); // Let the popup finish its initial sizing pass.
    harness.get_by_label_contains("Built-in").hover();
    harness.step();
    harness.step();
    harness.get_by_label("Drum Synth").click();
    harness.step();
    harness.step();
    let app = harness.state();
    let replacement = app.engine.session().track(instrument).unwrap().instrument.as_ref().unwrap();
    assert_eq!(replacement.plugin, "drum_synth");
    for param in soundcraft_dsp::plugin_info("drum_synth").unwrap().params {
        assert_eq!(replacement.params.get(param.id), Some(&param.default));
    }
    assert_eq!(app.engine.session().edit.selected_tracks, session.edit.selected_tracks);
    assert_eq!(app.engine.session().pool, session.pool);
    assert!(app.ui.plugin_windows.is_empty(), "a built-in instrument opens its picker");
    harness.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(harness.state().engine.session(), &session);
}

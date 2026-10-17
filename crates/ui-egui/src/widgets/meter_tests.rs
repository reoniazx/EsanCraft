//! Check the painted geometry, including fractional origins and display scales.
use super::*;
use egui::{FullOutput, Pos2, RawInput, Shape};

fn input(scale: f32) -> RawInput {
    let mut input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 520.0))), ..Default::default() };
    input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(scale);
    input
}

fn frame(ctx: &egui::Context, scale: f32, draw: impl FnMut(&mut Ui)) -> FullOutput {
    let mut output = ctx.run_ui(input(scale), draw);
    output.textures_delta.clear();
    output
}

fn mesh(output: &FullOutput) -> &Mesh {
    output.shapes.iter().find_map(|s| if let Shape::Mesh(m) = &s.shape { Some(m.as_ref()) } else { None }).unwrap()
}

/// Only triangles with visible area count: an empty band must not masquerade as a lit one.
fn color_bounds(mesh: &Mesh, color: Color32) -> Rect {
    mesh.indices.as_chunks::<3>().0.iter().fold(Rect::NOTHING, |bounds, triangle| {
        let vertices = triangle.map(|i| &mesh.vertices[i as usize]);
        if !vertices.iter().all(|v| v.color == color) {
            return bounds;
        }
        let r = vertices.iter().fold(Rect::NOTHING, |r, v| r.union(Rect::from_min_max(v.pos, v.pos)));
        if r.is_positive() { bounds.union(r) } else { bounds }
    })
}

#[test]
fn color_boundaries_rules_ticks_and_labels_share_the_same_height() {
    let _palette = crate::theme::lock_test_palette();
    for light in [false, true] {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx, light);
        let t = Tokens::current();
        for scale in [1.0, 1.5, 2.0] {
            for height in [64.0, 120.0, 420.0] {
                let r = Rect::from_min_size(pos2(90.2, 20.3), vec2(28.0, height));
                let output = frame(&ctx, scale, |ui| {
                    meter(ui, r, 0.0, 0.0, false);
                    meter_scale(ui.painter(), r, r.min.x - 1.0);
                });
                assert_eq!(ctx.pixels_per_point(), scale);
                let mesh = mesh(&output);
                let dim = if t.meter_bg.intensity() > 0.5 { 0.08 } else { 0.18 };
                let green = color_bounds(mesh, t.meter_green.gamma_multiply(dim));
                let yellow = color_bounds(mesh, t.meter_yellow.gamma_multiply(dim));
                let red = color_bounds(mesh, t.meter_red.gamma_multiply(dim));
                assert_eq!(green.top(), yellow.bottom());
                assert_eq!(yellow.top(), red.bottom());
                for (text, boundary) in [("10", green.top()), ("5", yellow.top())] {
                    let label = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            Shape::Text(t) if t.galley.job.text == text => Some(t.galley.rect.translate(t.pos.to_vec2())),
                            _ => None,
                        })
                        .unwrap();
                    assert!((label.center().y - boundary).abs() < 0.001, "{text} at {height}pt, {scale}x");
                    // Both the rule through the bar and the tick beside the label meet this boundary.
                    let lines: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            Shape::LineSegment { points: [a, b], .. } if a.y == boundary && b.y == boundary => Some((a.x, b.x)),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(lines.len(), 2, "{text} at {height}pt, {scale}x: {lines:?}");
                    assert!(lines.iter().any(|(left, right)| *left == green.left() && *right == green.right()));
                    assert!(lines.iter().any(|(_, right)| *right < r.left()));
                }
            }
        }
    }
}

#[test]
fn full_scale_holds_and_clip_lights_stay_inside_the_well() {
    let _palette = crate::theme::lock_test_palette();
    for light in [false, true] {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx, light);
        let t = Tokens::current();
        for scale in [1.0, 1.5, 2.0] {
            for height in [64.0, 120.0, 420.0] {
                let r = Rect::from_min_size(pos2(90.2, 20.3), vec2(12.0, height));
                for (level, hold) in [(0.0, 0.0), (0.1, 0.1), (0.0, 1.0), (1.0, 0.1), (1.0, 1.0), (4.0, 4.0), (f32::NAN, f32::INFINITY)] {
                    for clip in [false, true] {
                        let output = frame(&ctx, scale, |ui| meter(ui, r, level, hold, clip));
                        let body = output.shapes.iter().find(|s| matches!(&s.shape, Shape::Mesh(_))).unwrap();
                        let bar = body.clip_rect;
                        assert!(r.contains_rect(bar) && bar.top() > r.top());
                        assert_eq!(bar.top(), r.top() + 5.0, "0 dB must retain its alignment with the fader");
                        let well = output
                            .shapes
                            .iter()
                            .find_map(|s| match &s.shape {
                                Shape::Rect(rect) if rect.fill == t.meter_bg && rect.rect == r => Some(rect.rect),
                                _ => None,
                            })
                            .unwrap();
                        let light = output
                            .shapes
                            .iter()
                            .find_map(|s| match &s.shape {
                                Shape::Rect(rect)
                                    if rect.rect.bottom() < bar.top() && rect.fill == if clip { t.meter_red } else { t.meter_clip_off } =>
                                {
                                    Some(rect.rect)
                                }
                                _ => None,
                            })
                            .unwrap();
                        assert!(well.shrink(1.0).contains_rect(light));
                        assert_eq!(light.height(), 3.0);
                        assert_eq!(bar.top() - light.bottom(), 1.0);
                        assert!(mesh(&output).vertices.iter().all(|v| v.pos.is_finite() && bar.contains(v.pos)));
                        for s in &output.shapes {
                            if let Shape::Rect(mark) = &s.shape
                                && s.clip_rect == bar
                            {
                                assert!(bar.contains_rect(mark.rect), "peak hold escapes at {hold}: {:?} vs {bar:?}", mark.rect);
                            }
                        }
                        if hold.is_finite() && hold >= 1.0 {
                            assert!(output.shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(mark) if mark.fill == t.meter_red && mark.rect.top() == bar.top() && mark.rect.height() > 0.0 && s.clip_rect == bar)));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn lit_bands_and_peak_holds_use_the_same_thresholds() {
    let _palette = crate::theme::lock_test_palette();
    let ctx = egui::Context::default();
    for light in [false, true] {
        crate::theme::apply(&ctx, light);
        let t = Tokens::current();
        for scale in [1.0, 1.5, 2.0] {
            for (db, expected) in [
                (-20.0, t.meter_green),
                (-10.1, t.meter_green),
                (-10.0, t.meter_yellow),
                (-9.9, t.meter_yellow),
                (-5.1, t.meter_yellow),
                (-5.0, t.meter_red),
                (-4.9, t.meter_red),
                (0.0, t.meter_red),
                (12.0, t.meter_red),
            ] {
                let level = 10.0_f32.powf(db / 20.0);
                let r = Rect::from_min_size(pos2(90.2, 20.3), vec2(28.0, 420.0));
                let output = frame(&ctx, scale, |ui| meter(ui, r, level, level, false));
                let hold = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        Shape::Rect(mark) if s.clip_rect == meter_bar(r) => Some(mark),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(hold.fill, expected, "hold at {db} dB, {scale}x");
                // At an exact threshold the bar ends at the boundary; its hold already names
                // the new zone. Elsewhere its topmost visible band and hold must agree.
                if db != -10.0 && db != -5.0 {
                    let mesh = mesh(&output);
                    let bright = [t.meter_green, t.meter_yellow, t.meter_red]
                        .into_iter()
                        .map(|color| (color, color_bounds(mesh, color)))
                        .filter(|(_, r)| r.is_positive())
                        .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
                        .unwrap();
                    assert_eq!(bright.0, expected, "lit band at {db} dB");
                    assert!((bright.1.top() - hold.rect.center().y).abs() <= 1.0);
                }
            }
        }
    }
}

#[test]
fn multichannel_meters_keep_each_bar_and_hold_inside_its_own_well() {
    let _palette = crate::theme::lock_test_palette();
    let ctx = egui::Context::default();
    for light in [false, true] {
        crate::theme::apply(&ctx, light);
        for scale in [1.0, 1.5, 2.0] {
            for width in [36.0, 120.0] {
                let levels = [0.0, 0.1, 0.32, 0.56, 1.0, 4.0];
                let labels = ["L", "R", "C", "LFE", "Ls", "Rs"].map(str::to_owned);
                let r = Rect::from_min_size(pos2(90.2, 20.3), vec2(width, 120.0));
                let output = frame(&ctx, scale, |ui| multi_meter(ui, r, &levels, &labels, true));
                let bars: Vec<_> = output.shapes.iter().filter(|s| matches!(&s.shape, Shape::Mesh(_))).collect();
                assert_eq!(bars.len(), levels.len());
                for bar in &bars {
                    assert!(r.contains_rect(bar.clip_rect));
                    if let Shape::Mesh(mesh) = &bar.shape {
                        assert!(mesh.vertices.iter().all(|v| bar.clip_rect.contains(v.pos)));
                    }
                }
                assert!(bars.windows(2).all(|b| b[0].clip_rect.right() < b[1].clip_rect.left()));
                for s in &output.shapes {
                    if let Shape::Rect(mark) = &s.shape
                        && let Some(bar) = bars.iter().find(|bar| bar.clip_rect == s.clip_rect)
                    {
                        assert!(bar.clip_rect.contains_rect(mark.rect));
                    }
                }
            }
        }
    }
}

#[test]
fn capture_meter_visual_fixtures() {
    let _palette = crate::theme::lock_test_palette();
    let Some(directory) = std::env::var_os("CRAFT_UI_METER_SHOTS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&directory).unwrap();
    for light in [false, true] {
        let name = if light { "light" } else { "dark" };
        for scale in [1.0, 1.5, 2.0] {
            let mut h = egui_kittest::Harness::builder().with_size(vec2(700.0, 520.0)).with_pixels_per_point(scale).wgpu().build_ui(|ui| {
                crate::theme::apply(ui.ctx(), light);
                let t = Tokens::current();
                ui.painter().rect_filled(ui.max_rect(), 0.0, t.strip_bg);
                for (index, db) in [-80.0, -20.0, -10.1, -10.0, -9.9, -5.1, -5.0, -4.9, 0.0, 12.0].into_iter().enumerate() {
                    let r = Rect::from_min_size(pos2(60.2 + index as f32 * 60.0, 50.3), vec2(28.0, 320.0));
                    let level = 10.0_f32.powf(db / 20.0);
                    meter(ui, r, level, level, db > 0.0);
                    meter_scale(ui.painter(), r, r.min.x - 1.0);
                    ui.painter().text(pos2(r.center().x, 30.0), Align2::CENTER_CENTER, format!("{db}"), regular(10.0), t.text);
                }
                let r = Rect::from_min_size(pos2(60.2, 400.3), vec2(120.0, 100.0));
                let levels = [0.0, 0.1, 0.32, 0.56, 1.0, 4.0];
                let labels = ["L", "R", "C", "LFE", "Ls", "Rs"].map(str::to_owned);
                multi_meter(ui, r, &levels, &labels, true);
                for (i, level) in levels.into_iter().enumerate() {
                    meter(ui, Rect::from_min_size(pos2(240.2 + i as f32 * 40.0, 400.3), vec2(12.0, 64.0)), level, level, false);
                }
            });
            h.run_steps(4);
            h.render().unwrap().save(directory.join(format!("meters-{name}-{scale}x.png"))).unwrap();
        }
        for narrow in [false, true] {
            let mut app = crate::SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Default::default());
            app.run("window.mix", serde_json::json!({})).unwrap();
            app.run("ui.theme", serde_json::json!({"mode": name})).unwrap();
            app.ui.both_windows = false;
            app.ui.narrow_mix = narrow;
            let mut h = egui_kittest::Harness::builder().with_size(vec2(1600.0, 1000.0)).with_pixels_per_point(2.0).wgpu().build_ui_state(
                |ui, app: &mut crate::SoundApp| {
                    app.logic(ui.ctx());
                    for (i, track) in app.engine.session().tracks.iter().enumerate() {
                        let level = [0.0, 0.1, 0.32, 0.56, 1.0, 4.0][i % 6];
                        app.meters.insert(
                            track.id,
                            crate::MeterDisplay { level: [level, level * 0.7], hold: [level, level], clip: level > 1.0, ..Default::default() },
                        );
                    }
                    app.ui(ui);
                },
                app,
            );
            h.input_mut().max_texture_side = Some(8192);
            h.run_steps(6);
            h.render().unwrap().save(directory.join(format!("mix-{name}-narrow-{narrow}.png"))).unwrap();
        }
    }
}

//! Native audio imports prepare samples and waveforms off the UI thread. The engine commits
//! the complete batch through `file.import_audio`, preserving atomic failure and one-step Undo.

use crate::SoundApp;
use crate::theme::bold;
use serde_json::Value;
use soundcraft_engine::io::PreparedImport;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::Receiver,
};

pub struct ImportJob {
    params: Value,
    names: Vec<String>,
    worker: Receiver<Result<Vec<PreparedImport>, String>>,
    progress: Arc<Mutex<Progress>>,
    cancelled: Arc<AtomicBool>,
    shown: bool,
    displayed_fraction: f32,
    completed: bool,
}

impl Drop for ImportJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

// The browser has no background import worker to construct these processing stages.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Debug, Clone, Copy)]
enum Phase {
    Reading,
    Decoding(f32),
    Converting,
    Waveform,
    Ready,
}

#[derive(Debug, Clone, Copy)]
struct Progress {
    file: usize,
    phase: Phase,
}

impl Progress {
    /// Four preparation stages per file plus the final atomic placement. This measures work
    /// stages, not elapsed time. Decoding contributes its measured fraction within its stage.
    fn fraction(self, files: usize) -> f32 {
        if files == 0 {
            return 0.0;
        }
        let stage = match self.phase {
            Phase::Reading => 0.0,
            Phase::Decoding(f) => 1.0 + if f.is_finite() { f.clamp(0.0, 1.0) } else { 0.0 },
            Phase::Converting => 2.0,
            Phase::Waveform => 3.0,
            Phase::Ready => 4.0,
        };
        let done = self.file.min(files.saturating_sub(1)) as f64 * 4.0 + f64::from(stage);
        // Only a successful engine commit may display 100%, including for huge file counts.
        (done / (files as f64 * 4.0 + 1.0)).clamp(0.0, 0.99) as f32
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self { file: 0, phase: Phase::Reading }
    }
}

/// Start a UI import. Programmatic `SoundApp::run` keeps its synchronous result contract.
/// Browser byte imports continue to use the browser shell's import path.
pub fn start(app: &mut SoundApp, params: &Value) -> Result<Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        app.run("file.import_audio", params.clone())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let result = start_native(app, params);
        if let Err(error) = &result {
            app.ui.status = error.clone();
        }
        result
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn start_native(app: &mut SoundApp, params: &Value) -> Result<Value, String> {
    if refuses_session_switch(app) {
        return Err("Finish or cancel the import first".into());
    }
    if app.bounce.is_some() {
        return Err("Finish or cancel the bounce first".into());
    }
    let params = soundcraft_engine::cmd::audio_import_params(&app.engine, params).map_err(|e| e.to_string())?;
    let paths: Vec<String> =
        params.get("paths").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let names = paths.iter().map(|path| file_name(path)).collect();
    let progress = Arc::new(Mutex::new(Progress::default()));
    let cancelled = Arc::new(AtomicBool::new(false));
    let rate = app.engine.session().sample_rate.hz();
    let roots = soundcraft_engine::file_access::current();
    let (tx, worker) = std::sync::mpsc::channel();
    let (worker_progress, worker_cancelled) = (Arc::clone(&progress), Arc::clone(&cancelled));
    std::thread::Builder::new()
        .name("soundcraft-import".into())
        .spawn(move || {
            let result = soundcraft_engine::file_access::confine(roots.as_ref(), || prepare_files(paths, rate, &worker_progress, &worker_cancelled));
            let _ = tx.send(result);
        })
        .map_err(|e| format!("Could not start audio import: {e}"))?;
    app.import = Some(ImportJob { params, names, worker, progress, cancelled, shown: false, displayed_fraction: 0.0, completed: false });
    app.ui.status = "Importing audio…".into();
    Ok(Value::Null)
}

#[cfg(not(target_arch = "wasm32"))]
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Audio".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn prepare_files(paths: Vec<String>, rate: u32, progress: &Mutex<Progress>, cancelled: &AtomicBool) -> Result<Vec<PreparedImport>, String> {
    let mut prepared = Vec::new();
    for (file, path) in paths.into_iter().enumerate() {
        let update = |phase| -> soundcraft_audio_io::Result<()> {
            if cancelled.load(Ordering::Relaxed) {
                return Err(soundcraft_audio_io::AudioError::Cancelled);
            }
            if let Ok(mut state) = progress.lock() {
                *state = Progress { file, phase };
            }
            Ok(())
        };
        let name = file_name(&path);
        let error = |e: soundcraft_audio_io::AudioError| format!("{name}: {e}");
        update(Phase::Reading).map_err(error)?;
        let bytes = soundcraft_engine::io::read_file(&path).map_err(|e| e.to_string())?;
        let ext = std::path::Path::new(&path).extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase);
        let (info, buf) = soundcraft_audio_io::decode_with(&bytes, ext.as_deref(), |f| update(Phase::Decoding(f))).map_err(error)?;
        drop(bytes);
        update(Phase::Converting).map_err(error)?;
        let buf = soundcraft_engine::io::prepare_import_buffer(&name, buf, rate).map_err(|e| e.to_string())?;
        update(Phase::Waveform).map_err(error)?;
        let audio = Arc::new(soundcraft_model::SourceAudio::new(buf));
        // Check again after stages that cannot be interrupted. Keep ownership of this worker
        // until it exits, so repeated Cancel/Import cannot accumulate background workers.
        update(Phase::Waveform).map_err(error)?;
        prepared.push(PreparedImport { path, name, info, audio });
    }
    if let Ok(mut state) = progress.lock() {
        state.phase = Phase::Ready;
    }
    Ok(prepared)
}

/// Poll without blocking; commit only after the loading window has had a chance to draw.
pub fn step(app: &mut SoundApp, ctx: &egui::Context) {
    let Some(job) = &mut app.import else { return };
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
    if job.completed {
        if job.shown {
            app.import = None;
        }
        return;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        job.cancelled.store(true, Ordering::Relaxed);
    }
    if !job.shown {
        return;
    }
    let result = match job.worker.try_recv() {
        Ok(result) => result,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("Audio import worker stopped unexpectedly; please try again".into()),
    };
    let status = if job.cancelled.load(Ordering::Relaxed) {
        "Import cancelled".into()
    } else {
        match result.and_then(|files| soundcraft_engine::io::execute_prepared_imports(&mut app.engine, &job.params, files).map_err(|e| e.to_string()))
        {
            Ok(_) => {
                // Paint the full bar once after the engine has committed, then close next frame.
                job.completed = true;
                job.shown = false;
                if job.names.len() == 1 {
                    format!("Imported {}", job.names.first().map(String::as_str).unwrap_or("audio"))
                } else {
                    format!("Imported {} files", job.names.len())
                }
            }
            Err(error) => format!("Import failed: {error}"),
        }
    };
    if !job.completed {
        app.import = None;
    }
    app.ui.status = status;
}

pub fn refuses_session_switch(app: &mut SoundApp) -> bool {
    // A hidden window may not have painted its final 100% frame. Completed work must not
    // prevent quitting, replacing the session, or starting another job in that case.
    if app.import.as_ref().is_some_and(|job| job.completed) {
        app.import = None;
    }
    if app.import.is_some() {
        app.ui.status = "Finish or cancel the import first".into();
    }
    app.import.is_some()
}

/// One accumulating bar for the whole batch, filled only by completed work and decoding
/// progress. Interpolation softens stage transitions without guessing progress from elapsed time.
pub fn show(app: &mut SoundApp, ctx: &egui::Context) {
    let Some(job) = &mut app.import else { return };
    job.shown = true;
    let progress = job.progress.lock().map(|p| *p).unwrap_or_default();
    let cancelling = job.cancelled.load(Ordering::Relaxed);
    if job.completed {
        job.displayed_fraction = 1.0;
    } else if !cancelling {
        let target = progress.fraction(job.names.len()).max(job.displayed_fraction);
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
        job.displayed_fraction = (job.displayed_fraction + dt * 1.5).min(target);
    }
    let name = job.names.get(progress.file).map(String::as_str).unwrap_or("Audio");
    let mut cancel = false;
    egui::Modal::new(egui::Id::new("import_progress")).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
        ui.set_width(360.0_f32.min((ctx.content_rect().width() - 48.0).max(160.0)));
        ui.label(
            egui::RichText::new(if job.completed {
                "Import complete"
            } else if cancelling {
                "Cancelling import…"
            } else {
                "Importing Audio"
            })
            .font(bold(15.0)),
        );
        ui.add(egui::Label::new(name).truncate()).on_hover_text(name);
        let stage: String = if job.completed {
            "Audio imported successfully".into()
        } else if cancelling {
            "Stopping audio processing…".into()
        } else {
            match progress.phase {
                Phase::Reading => "Reading audio…".into(),
                Phase::Decoding(_) => "Decoding audio…".into(),
                Phase::Converting => "Converting sample rate…".into(),
                Phase::Waveform => "Building waveform…".into(),
                Phase::Ready => "Adding audio to session…".into(),
            }
        };
        ui.add(egui::ProgressBar::new(job.displayed_fraction).desired_height(18.0).corner_radius(9.0));
        ui.horizontal(|ui| {
            ui.label(&stage);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{:.0}%", (job.displayed_fraction * 100.0).floor()));
            });
        });
        ui.horizontal(|ui| {
            ui.label(format!("File {} of {}", progress.file.saturating_add(1), job.names.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancel = ui.add_enabled(!cancelling && !job.completed, egui::Button::new("Cancel")).clicked();
            });
        });
    });
    if cancel {
        job.cancelled.store(true, Ordering::Relaxed);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::Services;
    use egui_kittest::{Harness, kittest::Queryable};
    use serde_json::json;
    use soundcraft_audio_io::{AudioBuffer, EncodeOptions};
    use soundcraft_engine::Engine;
    use std::sync::mpsc;

    struct Fixture(std::path::PathBuf);
    #[allow(clippy::unwrap_used)]
    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sc-import-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn wav(&self, name: &str, rate: u32) -> String {
            let buf = AudioBuffer { sample_rate: rate, channels: vec![vec![0.25; rate as usize / 10]; 2] };
            let bytes = soundcraft_audio_io::encode(&buf, &EncodeOptions::default()).unwrap();
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path.to_string_lossy().into_owned()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn app() -> SoundApp {
        SoundApp::new(Engine::default(), None, Services::default())
    }
    fn finish(app: &mut SoundApp) {
        if let Some(job) = &mut app.import {
            job.shown = true;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.import.is_some() {
            if let Some(job) = &mut app.import {
                job.shown = true;
            }
            step(app, &egui::Context::default());
            assert!(std::time::Instant::now() < deadline, "import must terminate");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn batch_import_resamples_and_builds_waveforms_with_one_undo() {
        let f = Fixture::new("batch");
        let (a, b) = (f.wav("first.wav", 44_100), f.wav("second.wav", 48_000));
        let mut app = app();
        // `path` is appended to `paths`, matching the engine command.
        start(&mut app, &json!({"paths": [a], "path": b, "at": 9600})).unwrap();
        assert!(app.engine.session().tracks.is_empty());
        assert!(start(&mut app, &json!({"path": "ignored.wav"})).is_err());
        assert!(app.run("session.new", json!({})).is_err());
        assert!(app.run("app.quit", json!({})).is_err());
        finish(&mut app);
        assert_eq!(app.ui.status, "Imported 2 files");
        let s = app.engine.session();
        assert_eq!(s.tracks.len(), 2);
        for track in &s.tracks {
            assert_eq!(track.clips()[0].start, 9600);
            assert_eq!(track.clips()[0].length, 4800);
        }
        for source in &s.sources {
            let audio = s.pool.get(source.id).unwrap();
            assert_eq!(audio.buffer.sample_rate, 48_000);
            assert_eq!(audio.peaks.len(), 2);
            assert!((audio.channel(0).unwrap()[100] - 0.25).abs() < 0.01);
        }
        assert_eq!(app.engine.undo_history().len(), 1);
        assert!(app.engine.undo());
        assert!(app.engine.session().tracks.is_empty());
        assert!(app.engine.redo());
        assert_eq!(app.engine.session().tracks.len(), 2);
    }

    #[test]
    fn a_later_file_failure_leaves_no_partial_import() {
        let f = Fixture::new("failure");
        let good = f.wav("good.wav", 48_000);
        let missing = f.0.join("missing.wav");
        let mut app = app();
        start(&mut app, &json!({"paths": [good, missing]})).unwrap();
        finish(&mut app);
        assert!(app.ui.status.contains("missing.wav"), "{}", app.ui.status);
        assert!(app.engine.session().tracks.is_empty());
        assert!(app.engine.session().sources.is_empty());
        assert!(!app.engine.can_undo());
    }

    #[test]
    fn worker_preserves_the_callers_read_authority() {
        let f = Fixture::new("authority");
        let outside = f.wav("outside.wav", 48_000);
        let allowed = f.0.join("allowed");
        std::fs::create_dir_all(&allowed).unwrap();
        let roots = soundcraft_engine::file_access::AutomationRoots::new(Some(&allowed), None).unwrap();
        let mut app = app();
        soundcraft_engine::file_access::confine(roots.as_ref(), || start(&mut app, &json!({"path": outside}))).unwrap();
        finish(&mut app);
        assert!(app.ui.status.contains("outside the read root"), "{}", app.ui.status);
        assert!(app.engine.session().tracks.is_empty());
    }

    #[test]
    fn invalid_destination_fails_before_starting_a_worker() {
        let mut app = app();
        for params in [json!({}), json!({"path": "anything.wav", "new_tracks": false}), json!({"path": "anything.wav", "track": "Missing"})] {
            assert!(start(&mut app, &params).is_err());
            assert!(app.import.is_none());
        }
    }

    #[test]
    fn selected_destination_is_frozen_when_import_starts() {
        let f = Fixture::new("destination");
        let path = f.wav("audio.wav", 48_000);
        let mut app = app();
        app.run("track.new", json!({"name": "Target"})).unwrap();
        let id = app.engine.session().tracks[0].id;
        app.engine.session_mut().edit.selected_tracks = vec![id];
        start(&mut app, &json!({"path": path, "new_tracks": false})).unwrap();
        app.engine.session_mut().edit.selected_tracks.clear();
        finish(&mut app);
        assert_eq!(app.engine.session().tracks.len(), 1);
        assert_eq!(app.engine.session().tracks[0].clips().len(), 1);
    }

    #[test]
    fn programmatic_import_remains_synchronous_and_does_not_open_a_modal() {
        let f = Fixture::new("command");
        let path = f.wav("audio.wav", 48_000);
        let mut app = app();
        let result = app.run("file.import_audio", json!({"path": path})).unwrap();
        assert_eq!(result["imported"][0]["frames"], 4800);
        assert!(app.import.is_none());
        assert_eq!(app.engine.session().tracks.len(), 1);
    }

    fn pending(app: &mut SoundApp) -> mpsc::Sender<Result<Vec<PreparedImport>, String>> {
        let (tx, worker) = mpsc::channel();
        app.import = Some(ImportJob {
            params: json!({"path": "pending.wav"}),
            names: vec!["pending.wav".into()],
            worker,
            progress: Arc::new(Mutex::new(Progress::default())),
            cancelled: Arc::new(AtomicBool::new(false)),
            shown: false,
            displayed_fraction: 0.0,
            completed: false,
        });
        tx
    }

    #[test]
    fn cancellation_discards_even_a_completed_worker_result_and_waits_for_exit() {
        let mut app = app();
        let tx = pending(&mut app);
        let token = Arc::clone(&app.import.as_ref().unwrap().cancelled);
        token.store(true, Ordering::Relaxed);
        app.import.as_mut().unwrap().shown = true;
        step(&mut app, &egui::Context::default());
        assert!(app.import.is_some(), "keep worker owned until it exits");
        tx.send(Ok(Vec::new())).unwrap();
        step(&mut app, &egui::Context::default());
        assert!(app.import.is_none());
        assert_eq!(app.ui.status, "Import cancelled");
        assert!(!app.engine.is_dirty());
    }

    #[test]
    fn overall_progress_never_resets_between_stages_or_files() {
        let mut previous = 0.0;
        for file in 0..3 {
            for phase in [Phase::Reading, Phase::Decoding(0.0), Phase::Decoding(0.25), Phase::Decoding(1.0), Phase::Converting, Phase::Waveform] {
                let fraction = Progress { file, phase }.fraction(3);
                assert!(fraction >= previous, "progress went backwards at {file}: {phase:?}");
                assert!(fraction < 1.0, "only successful placement may complete the bar");
                previous = fraction;
            }
        }
        let ready = Progress { file: 2, phase: Phase::Ready }.fraction(3);
        assert!(ready > previous && ready < 1.0);
        assert_eq!(Progress::default().fraction(3), 0.0);
        assert_eq!(Progress::default().fraction(0), 0.0);
        for f in [f32::NAN, f32::INFINITY, -1.0, 2.0] {
            let fraction = Progress { file: usize::MAX, phase: Phase::Decoding(f) }.fraction(usize::MAX);
            assert!(fraction.is_finite() && (0.0..1.0).contains(&fraction));
        }
    }

    #[test]
    fn a_completed_import_does_not_block_quit_while_the_window_is_hidden() {
        let mut app = app();
        let _tx = pending(&mut app);
        app.import.as_mut().unwrap().completed = true;
        app.run("app.quit", json!({})).unwrap();
        assert!(app.quit_requested);
        assert!(app.import.is_none());
    }

    #[test]
    fn full_bar_is_painted_after_success_before_the_popup_closes() {
        let mut app = app();
        let tx = pending(&mut app);
        let mut h = Harness::builder().with_size(egui::vec2(900.0, 600.0)).wgpu().build_ui_state(
            |ui, app: &mut SoundApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        h.input_mut().max_texture_side = Some(8192);
        for _ in 0..4 {
            h.step();
        }
        assert_eq!(h.state().import.as_ref().unwrap().displayed_fraction, 0.0);
        let buf = AudioBuffer { sample_rate: 48_000, channels: vec![vec![0.25; 480]] };
        let bytes = soundcraft_audio_io::encode(&buf, &EncodeOptions::default()).unwrap();
        let (info, buf) = soundcraft_audio_io::decode(&bytes, Some("wav")).unwrap();
        tx.send(Ok(vec![PreparedImport {
            path: "pending.wav".into(),
            name: "pending.wav".into(),
            info,
            audio: Arc::new(soundcraft_model::SourceAudio::new(buf)),
        }]))
        .unwrap();
        h.step();
        assert_eq!(h.state().engine.session().tracks.len(), 1);
        assert!(h.state().import.as_ref().unwrap().completed);
        assert_eq!(h.state().import.as_ref().unwrap().displayed_fraction, 1.0);
        assert!(h.query_by_label("Import complete").is_some());
        if let Some(dir) = std::env::var_os("SOUNDCRAFT_IMPORT_SHOTS") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            h.render().unwrap().save(dir.join("complete.png")).unwrap();
        }
        h.step();
        assert!(h.state().import.is_none());
        assert_eq!(h.state().ui.status, "Imported pending.wav");
    }

    #[test]
    fn disconnected_worker_closes_the_popup_with_an_error() {
        let mut app = app();
        drop(pending(&mut app));
        finish(&mut app);
        assert!(app.ui.status.contains("worker stopped unexpectedly"));
        assert!(!app.engine.is_dirty());
    }

    #[test]
    fn cancel_button_and_escape_work_and_long_names_fit_the_modal() {
        let mut app = app();
        let tx = pending(&mut app);
        let name = format!("เสียงร้อง_{}final.wav", "long-session-name_".repeat(20));
        app.import.as_mut().unwrap().names = vec![name, "second.wav".into()];
        let mut h = Harness::builder().with_size(egui::vec2(900.0, 600.0)).wgpu().build_ui_state(
            |ui, app: &mut SoundApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        h.input_mut().max_texture_side = Some(8192);
        for _ in 0..4 {
            h.step();
        }
        assert!(h.get_by_label("Importing Audio").rect().width() < 400.0);
        let shots = std::env::var_os("SOUNDCRAFT_IMPORT_SHOTS").map(std::path::PathBuf::from);
        if let Some(dir) = &shots {
            std::fs::create_dir_all(dir).unwrap();
        }
        for (name, phase) in
            [("reading", Phase::Reading), ("decoding", Phase::Decoding(0.63)), ("converting", Phase::Converting), ("waveform", Phase::Waveform)]
        {
            h.state().import.as_ref().unwrap().progress.lock().unwrap().phase = phase;
            for _ in 0..15 {
                h.step();
            }
            if let Some(dir) = &shots {
                h.render().unwrap().save(dir.join(format!("{name}.png"))).unwrap();
            }
        }
        h.state_mut().ui.theme = crate::theme::ThemeMode::Light;
        h.step();
        h.step();
        if let Some(dir) = &shots {
            h.render().unwrap().save(dir.join("light-waveform.png")).unwrap();
        }
        h.state_mut().ui.theme = crate::theme::ThemeMode::Dark;
        h.step();
        h.get_by_label("Cancel").click();
        h.step();
        assert!(h.state().import.as_ref().unwrap().cancelled.load(Ordering::Relaxed));
        h.step();
        assert!(h.query_by_label("Cancelling import…").is_some());
        if let Some(dir) = &shots {
            h.render().unwrap().save(dir.join("cancelling.png")).unwrap();
        }
        tx.send(Err("cancelled".into())).unwrap();
        h.step();
        assert!(h.state().import.is_none());
        assert_eq!(h.state().ui.status, "Import cancelled");

        let tx = pending(h.state_mut());
        h.step();
        h.key_press(egui::Key::Escape);
        h.step();
        assert!(h.state().import.as_ref().unwrap().cancelled.load(Ordering::Relaxed));
        drop(tx);
        h.step();
        assert!(h.state().import.is_none());
    }
}

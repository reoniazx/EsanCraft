//! Context menu descriptions and actions, independent of the window system.
//! Native hosts present requests after releasing the application borrow.

use crate::SoundApp;
use egui::{Modifiers, Response, Ui};
use serde_json::{Value, json};
use soundcraft_model::{ClipId, GroupId, SourceId, TrackHeight, TrackId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Control(egui::Id),
    TrackView(TrackId),
    Playlists(TrackId),
    Voice(TrackId),
    Automation(TrackId),
    Timebase(TrackId),
    Elastic(TrackId),
    EditColumns,
    Route { track: TrackId, input: bool },
    SendRoute { track: TrackId, slot: usize },
    Counter,
    Grid { nudge: bool },
    MidiScrolling,
    Presets { track: TrackId, slot: usize, plugin: String },
    Track(TrackId),
    Clip(Option<ClipId>),
    Group(Option<GroupId>),
    ClipList { id: u64, source: bool },
    Insert { track: TrackId, slot: usize },
    Instrument(TrackId),
    Send { track: TrackId, slot: usize },
}

#[derive(Clone, Debug)]
pub enum Action {
    Choice { control: egui::Id, index: usize, label: String },
    NewBus { track: TrackId, send: Option<usize>, input: bool },
    LoadPreset { track: TrackId, slot: usize, name: String },
    Command(String, Value),
    RenameTrack(TrackId),
    RenameClip(ClipId),
    NewGroup,
    ModifyGroup(GroupId),
    AutoRename(Vec<ClipId>),
    Clear { clips: Vec<ClipId>, sources: Vec<SourceId> },
    DeleteClip(ClipId),
    TrackHeight { own: Vec<TrackId>, height: String },
    Insert { track: TrackId, slot: usize, plugin: String },
    PluginEditor { track: TrackId, slot: usize, open: bool },
    PluginWindow { track: TrackId, slot: usize },
}

#[derive(Clone, Debug)]
pub enum Entry {
    Item { label: String, action: Action, enabled: bool, checked: Option<bool>, color: Option<[u8; 3]> },
    Submenu { label: String, entries: Vec<Entry> },
    Separator,
}

impl Entry {
    pub fn item(label: impl Into<String>, action: Action) -> Self {
        Self::Item { label: label.into(), action, enabled: true, checked: None, color: None }
    }

    pub fn command(label: impl Into<String>, id: &str, params: Value) -> Self {
        Self::item(label, Action::Command(id.into(), params))
    }

    pub fn checked(self, value: bool) -> Self {
        match self {
            Self::Item { label, action, enabled, color, .. } => Self::Item { label, action, enabled, checked: Some(value), color },
            other => other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub target: Target,
    pub entries: Vec<Entry>,
    pub viewport: egui::ViewportId,
    pub epoch: u64,
    pub anchor: Anchor,
}

#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    Pointer,
    Below { rect: egui::Rect, zoom: f32 },
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub action: Action,
    pub modifiers: Modifiers,
}

/// Main-thread callback. Implementations must not borrow SoundApp during menu tracking.
#[cfg(not(target_arch = "wasm32"))]
pub trait Presenter {
    fn capture_input(&self, _raw: &egui::RawInput) {}
    fn present(&self, request: &Request) -> Result<Option<Selection>, String>;
}

#[derive(Default)]
pub struct State {
    pub enabled: bool,
    pub active: bool,
    pub(crate) pending: Option<Request>,
    pub(crate) epoch: u64,
    pub(crate) search: Option<Search>,
    pub(crate) routing: Option<RoutingSearch>,
    pub(crate) preset: Option<(TrackId, usize, egui::ViewportId)>,
}

pub(crate) struct RoutingSearch {
    pub track: TrackId,
    pub send: Option<usize>,
    pub input: bool,
    pub viewport: egui::ViewportId,
    pub fresh: bool,
}

pub(crate) struct Search {
    pub track: TrackId,
    pub slot: usize,
    pub instruments: bool,
    pub viewport: egui::ViewportId,
    pub fresh: bool,
}

impl State {
    pub(crate) fn invalidate(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
        self.pending = None;
        self.search = None;
        self.routing = None;
        self.preset = None;
    }
}

pub fn valid(app: &SoundApp, target: &Target) -> bool {
    let s = app.engine.session();
    match target {
        Target::Presets { track, slot, plugin } => {
            s.track(*track).and_then(|t| t.mixer.inserts.get(*slot)).and_then(Option::as_ref).is_some_and(|insert| insert.plugin == *plugin)
        }
        Target::Control(_) | Target::EditColumns | Target::Counter | Target::Grid { .. } | Target::MidiScrolling => true,
        Target::TrackView(id)
        | Target::Playlists(id)
        | Target::Voice(id)
        | Target::Automation(id)
        | Target::Timebase(id)
        | Target::Elastic(id)
        | Target::Route { track: id, .. } => s.track(*id).is_some(),
        Target::SendRoute { track, slot } => s.track(*track).is_some_and(|t| t.mixer.sends.get(*slot).is_some()),
        Target::Track(id) => s.track(*id).is_some(),
        Target::Instrument(id) => s.track(*id).is_some_and(|t| t.kind == soundcraft_model::TrackKind::Instrument),
        Target::Clip(id) => id.is_none_or(|id| s.find_clip(id).is_some()),
        Target::Group(id) => id.is_none_or(|id| s.groups.iter().any(|g| g.id == id)),
        Target::ClipList { id, source: true } => s.sources.iter().any(|src| src.id.0 == *id),
        Target::ClipList { id, source: false } => s.find_clip(ClipId(*id)).is_some(),
        Target::Insert { track, slot } => s.track(*track).is_some_and(|t| t.mixer.inserts.get(*slot).is_some()),
        Target::Send { track, slot } => s.track(*track).is_some_and(|t| t.mixer.sends.get(*slot).is_some_and(Option::is_some)),
    }
}

/// Returns true when the native host owns this response's context menu, even on idle frames.
pub fn native(app: &mut SoundApp, ui: &Ui, response: &Response, target: Target) -> bool {
    if !app.context_menus.enabled {
        return false;
    }
    if response.secondary_clicked() && !app.context_menus.active && valid(app, &target) {
        let entries = describe(app, &target);
        app.context_menus.pending =
            Some(Request { target, entries, viewport: ui.ctx().viewport_id(), epoch: app.context_menus.epoch, anchor: Anchor::Pointer });
        ui.ctx().request_repaint();
    }
    true
}

/// Left-click selectors keep their existing closed controls and use the same native presenter.
pub fn dropdown(app: &mut SoundApp, response: &Response, target: Target) -> bool {
    if !app.context_menus.enabled {
        return false;
    }
    if response.clicked() && !app.context_menus.active && valid(app, &target) {
        app.context_menus.pending = Some(Request {
            entries: describe(app, &target),
            target,
            viewport: response.ctx.viewport_id(),
            epoch: app.context_menus.epoch,
            anchor: Anchor::Below { rect: response.rect, zoom: response.ctx.zoom_factor() },
        });
        response.ctx.request_repaint();
    }
    true
}

/// Simple context menus share their contents and dispatch with the native presentation.
pub fn attach(app: &mut SoundApp, ui: &Ui, response: &Response, target: Target) {
    if !native(app, ui, response, target.clone()) {
        response.context_menu(|ui| show(app, ui, &target));
    }
}

pub fn show(app: &mut SoundApp, ui: &mut Ui, target: &Target) {
    render(app, ui, &describe(app, target));
}

fn render(app: &mut SoundApp, ui: &mut Ui, entries: &[Entry]) {
    for entry in entries {
        match entry {
            Entry::Separator => {
                ui.separator();
            }
            Entry::Submenu { label, entries } => {
                ui.menu_button(label, |ui| render(app, ui, entries));
            }
            Entry::Item { label, action, enabled, checked, color } => {
                if let Some(color) = color {
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(120.0, 14.0), egui::Sense::click());
                    ui.painter().rect_filled(rect, 2.0, crate::theme::rgb(*color));
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, *enabled, label));
                    if *enabled && response.clicked() {
                        execute(app, action.clone(), ui.input(|i| i.modifiers), ui.ctx().viewport_id());
                        ui.close();
                    }
                    continue;
                }
                let mut button = egui::Button::new(label);
                if let Some(checked) = checked {
                    button = button.selected(*checked);
                }
                if ui.add_enabled(*enabled, button).clicked() {
                    execute(app, action.clone(), ui.input(|i| i.modifiers), ui.ctx().viewport_id());
                    ui.close();
                }
            }
        }
    }
}

pub fn execute(app: &mut SoundApp, action: Action, modifiers: Modifiers, viewport: egui::ViewportId) {
    let opening_search = matches!(&action, Action::Command(id, _) if id == "window.plugin_search");
    let opening_routing = matches!(&action, Action::Command(id, _) if id == "window.routing_search");
    let opening_preset = matches!(&action, Action::Command(id, _) if id == "window.save_preset");
    let result = match action {
        Action::Choice { .. } => Err("Dropdown selection requires a window host".into()),
        Action::LoadPreset { track, slot, name } => crate::panels::load_preset(app, track, slot, &name),
        Action::NewBus { track, send, input } => {
            let s = app.engine.session();
            let mut n = s.busses.len().saturating_add(1);
            while s.busses.iter().any(|bus| bus.name.eq_ignore_ascii_case(&format!("Bus {n}"))) || s.track_by_name(&format!("Bus {n}")).is_some() {
                let Some(next) = n.checked_add(1) else { return };
                n = next;
            }
            let name = format!("Bus {n}");
            if let Some(slot) = send {
                app.run("mix.new_bus", json!({"name": name}))
                    .and_then(|_| app.run("mix.send", json!({"track": track.0, "slot": slot, "bus": name})))
                    .map(|_| ())
            } else {
                let key = if input { "input" } else { "output" };
                app.run(if input { "track.input" } else { "track.output" }, json!({"track": track.0, key: name})).map(|_| ())
            }
        }
        Action::Command(id, params) => app.run(&id, params).map(|_| ()),
        Action::RenameTrack(id) => {
            if let Some(track) = app.engine.session().track(id) {
                app.dialogs.open_rename_track(id, &track.name);
            }
            Ok(())
        }
        Action::RenameClip(id) => {
            if let Some((_, clip)) = app.engine.session().find_clip(id) {
                app.dialogs.open_rename_clip(id, &clip.name);
            }
            Ok(())
        }
        Action::NewGroup => {
            app.dialogs.open_for_command(&app.engine, "track.group");
            Ok(())
        }
        Action::ModifyGroup(id) => {
            app.dialogs.open_modify_group(&app.engine, id);
            Ok(())
        }
        Action::AutoRename(clips) => {
            app.dialogs.open_auto_rename(&app.engine, clips);
            Ok(())
        }
        Action::Clear { clips, sources } => {
            app.dialogs.open_clear(&app.engine, clips, sources);
            Ok(())
        }
        Action::DeleteClip(id) => app.run("edit.select", json!({"clips": [id.0]})).and_then(|_| app.run("edit.clear", json!({}))).map(|_| ()),
        Action::TrackHeight { own, height } => {
            let tracks = crate::edit_window::height_targets(app.engine.session(), modifiers, &own);
            app.run("track.height", json!({"tracks": tracks, "height": height})).map(|_| ())
        }
        Action::Insert { track, slot, plugin } => app.run("mix.insert", json!({"track": track.0, "slot": slot, "plugin": plugin})).map(|_| {
            app.pending_open = Some((track, slot));
        }),
        Action::PluginEditor { track, slot, open } => {
            if let Some(player) = &app.player {
                if open {
                    player.open_editor(track, slot)
                } else {
                    player.close_editor(track, slot);
                    Ok(())
                }
            } else {
                Err("Playback engine is unavailable".into())
            }
        }
        Action::PluginWindow { track, slot } => {
            if !app.ui.plugin_windows.contains(&(track, slot)) {
                app.ui.plugin_windows.push((track, slot));
            }
            Ok(())
        }
    };
    if opening_search && let Some(search) = app.context_menus.search.as_mut() {
        search.viewport = viewport;
    }
    if opening_routing && let Some(routing) = app.context_menus.routing.as_mut() {
        routing.viewport = viewport;
    }
    if opening_preset && let Some(preset) = app.context_menus.preset.as_mut() {
        preset.2 = viewport;
    }
    if let Err(error) = result {
        app.ui.status = error;
    }
}

pub fn describe(app: &SoundApp, target: &Target) -> Vec<Entry> {
    if !valid(app, target) {
        return vec![];
    }
    let mut entries = match *target {
        Target::Control(_) => vec![],
        Target::Presets { track, slot, .. } => crate::panels::native_preset_menu(app, track, slot),
        Target::TrackView(id) => crate::edit_window::native_view_menu(app, id),
        Target::Playlists(id) => crate::edit_window::native_playlist_menu(app, id),
        Target::Voice(id) => app
            .engine
            .session()
            .track(id)
            .map(|track| {
                [("dyn", false), ("off", true)]
                    .into_iter()
                    .map(|(label, inactive)| {
                        Entry::command(label, "track.make_inactive", json!({"tracks": [id.0], "inactive": inactive}))
                            .checked(track.inactive == inactive)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Target::Automation(id) => app
            .engine
            .session()
            .track(id)
            .map(|track| {
                soundcraft_model::AutomationMode::ALL
                    .into_iter()
                    .map(|mode| {
                        Entry::command(mode.label(), "mix.automation_mode", json!({"track": id.0, "mode": mode.label()}))
                            .checked(track.mixer.automation_mode == mode)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Target::Timebase(id) => app
            .engine
            .session()
            .track(id)
            .map(|track| {
                [("Samples", false), ("Ticks", true)]
                    .into_iter()
                    .map(|(label, ticks)| {
                        Entry::command(label, "track.timebase", json!({"tracks": [id.0], "ticks": ticks})).checked(track.ticks_timebase == ticks)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Target::Elastic(id) => app
            .engine
            .session()
            .track(id)
            .map(|track| {
                ["none", "polyphonic", "rhythmic", "monophonic", "varispeed", "x-form"]
                    .into_iter()
                    .map(|algorithm| {
                        Entry::command(algorithm, "track.elastic", json!({"tracks": [id.0], "algorithm": algorithm}))
                            .checked(track.elastic.as_deref().unwrap_or("none") == algorithm)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Target::EditColumns => crate::edit_window::native_columns_menu(app),
        Target::Route { track, input } => crate::mix_window::native_route_menu(app, track, input, None),
        Target::SendRoute { track, slot } => crate::mix_window::native_route_menu(app, track, false, Some(slot)),
        Target::Counter => soundcraft_time::TimeFormat::ALL
            .into_iter()
            .map(|format| {
                Entry::command(format.label(), "view.main_counter", json!({"format": format.id()}))
                    .checked(app.engine.session().edit.main_counter == format)
            })
            .collect(),
        Target::Grid { nudge } => crate::toolbar::native_value_menu(app, nudge),
        Target::MidiScrolling => crate::midi_playback::MidiScrollingMode::ALL
            .into_iter()
            .map(|mode| Entry::command(mode.label(), "ui.midi_scrolling", json!({"mode": mode.id()})).checked(app.ui.midi_scrolling == mode))
            .collect(),
        Target::Track(id) => track_menu(app, id),
        Target::Clip(clip) => clip_menu(app, clip),
        Target::Group(id) => group_menu(app, id),
        Target::ClipList { id, source } => clip_list_menu(app, id, source),
        Target::Insert { track, slot } => crate::mix_window::native_plugin_menu(app, track, slot, false),
        Target::Instrument(track) => crate::mix_window::native_plugin_menu(app, track, soundcraft_mix::INSTRUMENT_SLOT, true),
        Target::Send { track, slot } => send_menu(app, track, slot),
    };
    refresh_enabled(app, &mut entries);
    entries
}

fn refresh_enabled(app: &SoundApp, entries: &mut [Entry]) {
    for entry in entries {
        match entry {
            Entry::Submenu { entries, .. } => refresh_enabled(app, entries),
            Entry::Item { action: Action::Command(id, params), enabled, .. } => {
                if let Some(command) = soundcraft_engine::find_command(id) {
                    // Match Engine::execute: explicit context targets do not require a selection.
                    let explicit = ["track", "tracks", "clip", "clips", "sources", "start", "end", "at"].iter().any(|key| params.get(key).is_some());
                    *enabled &= !app.engine.blocked_while_recording(id) && (explicit || (command.enabled)(&app.engine).is_ok());
                }
            }
            _ => {}
        }
    }
}

fn track_menu(app: &SoundApp, id: TrackId) -> Vec<Entry> {
    let s = app.engine.session();
    let tracks: Vec<TrackId> = if s.edit.selected_tracks.contains(&id) { s.edit.selected_tracks.clone() } else { vec![id] };
    let mut entries = vec![Entry::item("Rename...", Action::RenameTrack(id))];
    for (label, cmd) in [
        ("Duplicate...", "track.duplicate"),
        ("Make Inactive", "track.make_inactive"),
        ("Hide", "track.hide"),
        ("Delete", "track.delete"),
        ("Split into Mono", "track.split_into_mono"),
        ("Commit...", "track.commit"),
        ("Freeze", "track.freeze"),
        ("Move to New Folder...", "track.move_to_new_folder"),
        ("Group...", "track.group"),
    ] {
        let mut entry = Entry::command(label, cmd, json!({"tracks": tracks}));
        if cmd == "track.freeze" {
            entry = entry.checked(tracks.iter().all(|id| s.track(*id).is_some_and(|t| t.frozen)));
        }
        if cmd == "track.make_inactive" {
            entry = entry.checked(tracks.iter().all(|id| s.track(*id).is_some_and(|t| t.inactive)));
        }
        entries.push(entry);
    }
    let folders: Vec<_> = s
        .tracks
        .iter()
        .filter(|t| t.is_folder() && !tracks.contains(&t.id))
        .map(|t| Entry::command(&t.name, "track.move_to_folder", json!({"tracks": tracks, "folder": t.id.0})))
        .collect();
    if !folders.is_empty() {
        entries.push(Entry::Submenu { label: "Move to Folder".into(), entries: folders });
    }
    if s.track(id).is_some_and(|t| t.folder.is_some()) {
        entries.push(Entry::command("Remove from Folder", "track.remove_from_folder", json!({"tracks": tracks})));
    }
    entries.push(Entry::command("Scroll Into View", "view.scroll_into_view", json!({"track": id.0})));
    entries.push(Entry::Separator);
    entries.push(Entry::Submenu {
        label: "Track Height".into(),
        entries: TrackHeight::ALL
            .into_iter()
            .map(|height| {
                Entry::item(height.label(), Action::TrackHeight { own: tracks.clone(), height: height.label().into() })
                    .checked(tracks.iter().all(|id| s.track(*id).is_some_and(|t| t.height == height)))
            })
            .collect(),
    });
    entries.push(Entry::Submenu {
        label: "Track Color".into(),
        entries: soundcraft_model::TRACK_COLORS
            .iter()
            .enumerate()
            .map(|(i, color)| Entry::Item {
                label: format!("Color {}", i + 1),
                action: Action::Command("track.color".into(), json!({"tracks": tracks, "color": i})),
                enabled: true,
                checked: None,
                color: Some(*color),
            })
            .collect(),
    });
    entries
}

fn clip_menu(app: &SoundApp, clip: Option<ClipId>) -> Vec<Entry> {
    let Some(id) = clip else {
        return vec![Entry::command("Paste", "edit.paste", json!({}))];
    };
    let mut entries: Vec<_> = [
        ("Mute/Unmute", "edit.mute_clips"),
        ("Edit Lock/Unlock", "clip.edit_lock"),
        ("Time Lock/Unlock", "clip.time_lock"),
        ("Quantize to Grid", "clip.quantize_to_grid"),
        ("Render Clip Gain", "clip.gain_render"),
        ("Bring to Front", "clip.bring_to_front"),
        ("Send to Back", "clip.send_to_back"),
    ]
    .into_iter()
    .map(|(label, cmd)| {
        let entry = Entry::command(label, cmd, json!({"clips": [id.0]}));
        if let Some((_, clip)) = app.engine.session().find_clip(id) {
            match cmd {
                "edit.mute_clips" => entry.checked(clip.muted),
                "clip.edit_lock" => entry.checked(clip.edit_locked),
                "clip.time_lock" => entry.checked(clip.time_locked),
                _ => entry,
            }
        } else {
            entry
        }
    })
    .collect();
    entries.extend([Entry::Separator, Entry::item("Rename...", Action::RenameClip(id)), Entry::item("Delete", Action::DeleteClip(id))]);
    entries
}

fn group_menu(app: &SoundApp, id: Option<GroupId>) -> Vec<Entry> {
    let Some(group) = id.and_then(|id| app.engine.session().groups.iter().find(|g| g.id == id)) else {
        return vec![Entry::item("New Group…", Action::NewGroup)];
    };
    vec![
        Entry::command("Select Tracks in Group", "edit.select", json!({"tracks": group.members, "exact": true})),
        Entry::command(if group.active { "Disable" } else { "Enable" }, "track.group_toggle", json!({"group": group.id.0})),
        Entry::item("Modify Group…", Action::ModifyGroup(group.id)),
        Entry::command("Delete Group", "track.ungroup", json!({"group": group.id.0})),
    ]
}

fn clip_list_menu(app: &SoundApp, id: u64, source: bool) -> Vec<Entry> {
    let mut entries = Vec::new();
    if source {
        entries.push(Entry::command("Place on New Track", "clip.place_source", json!({"source": id})));
        if let Some(track) = app.engine.session().edit.selected_tracks.first() {
            entries.push(Entry::command("Place on Selected Track", "clip.place_source", json!({"source": id, "track": track.0})));
        }
        entries.push(Entry::Separator);
    } else {
        let selected = &app.engine.session().edit.selected_clips;
        let clips = if selected.contains(&ClipId(id)) { selected.clone() } else { vec![ClipId(id)] };
        entries.push(Entry::item("Auto Rename…", Action::AutoRename(clips)));
    }
    entries.push(Entry::item(
        "Clear…",
        Action::Clear { clips: if source { vec![] } else { vec![ClipId(id)] }, sources: if source { vec![SourceId(id)] } else { vec![] } },
    ));
    entries
}

fn send_menu(app: &SoundApp, track: TrackId, slot: usize) -> Vec<Entry> {
    let Some(send) = app.engine.session().track(track).and_then(|t| t.mixer.sends.get(slot)).and_then(Option::as_ref) else {
        return vec![];
    };
    let mut entries = vec![
        Entry::command("Open Send Controls…", "window.send_controls", json!({"track": track.0, "slot": slot})),
        Entry::Separator,
        Entry::command("Mute", "mix.send_level", json!({"track": track.0, "slot": slot, "mute": !send.mute})).checked(send.mute),
        Entry::command("Pre-fader", "mix.send_level", json!({"track": track.0, "slot": slot, "pre_fader": !send.pre_fader})).checked(send.pre_fader),
        Entry::Separator,
    ];
    for (label, delta) in [("−3 dB", -3.0), ("−1 dB", -1.0), ("+1 dB", 1.0), ("+3 dB", 3.0)] {
        entries.push(Entry::command(
            label,
            "mix.send_level",
            json!({"track": track.0, "slot": slot, "db": (send.level_db.max(-60.0) + delta).clamp(-60.0, 12.0)}),
        ));
    }
    for (label, db) in [("0 dB", 0.0), ("−∞", -144.0)] {
        entries.push(Entry::command(label, "mix.send_level", json!({"track": track.0, "slot": slot, "db": db})));
    }
    if let soundcraft_model::Route::Bus(bus) = send.target
        && let Some(aux) = app
            .engine
            .session()
            .tracks
            .iter()
            .find(|t| t.kind == soundcraft_model::TrackKind::Aux && t.mixer.input == soundcraft_model::Route::Bus(bus))
    {
        entries.push(Entry::item("Rename bus…", Action::RenameTrack(aux.id)));
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(entries: &[Entry], label: &str) -> Action {
        entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Item { label: text, action, .. } if text == label => Some(action.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn every_context_has_live_actions_and_missing_targets_have_none() {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, crate::Services::default());
        let track = app.engine.session().tracks[0].id;
        let clip = app.engine.session().tracks[0].clips()[0].id;
        let source = app.engine.session().sources[0].id;
        app.run("track.group", json!({"tracks": [track.0], "name": "Drums"})).unwrap();
        let group = app.engine.session().groups[0].id;
        app.run("mix.new_bus", json!({"name": "Test Verb"})).unwrap();
        app.run("mix.send", json!({"track": track.0, "slot": 0, "bus": "Test Verb"})).unwrap();
        app.run("track.new", json!({"kind": "instrument", "name": "Keys Test"})).unwrap();
        let instrument = app.engine.session().tracks.iter().find(|t| t.name == "Keys Test").unwrap().id;
        for target in [
            Target::Track(track),
            Target::Clip(Some(clip)),
            Target::Clip(None),
            Target::Group(None),
            Target::Group(Some(group)),
            Target::ClipList { id: source.0, source: true },
            Target::ClipList { id: clip.0, source: false },
            Target::Insert { track, slot: 0 },
            Target::Instrument(instrument),
            Target::Send { track, slot: 0 },
        ] {
            assert!(!describe(&app, &target).is_empty(), "{target:?}");
        }
        for target in [
            Target::Track(TrackId(u64::MAX)),
            Target::Instrument(track),
            Target::Clip(Some(ClipId(u64::MAX))),
            Target::Group(Some(GroupId(u64::MAX))),
            Target::Insert { track, slot: usize::MAX },
            Target::Send { track, slot: usize::MAX },
        ] {
            assert!(describe(&app, &target).is_empty(), "{target:?}");
        }
    }

    #[test]
    fn track_actions_keep_the_clicked_target_and_multi_selection() {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, crate::Services::default());
        let ids: Vec<_> = app.engine.session().tracks.iter().take(3).map(|t| t.id).collect();
        app.run("edit.select", json!({"tracks": [ids[0].0, ids[1].0], "exact": true})).unwrap();
        for (target, expected) in [(ids[0], vec![ids[0], ids[1]]), (ids[2], vec![ids[2]])] {
            let Action::Command(id, params) = action(&describe(&app, &Target::Track(target)), "Delete") else { panic!("command expected") };
            assert_eq!(id, "track.delete");
            assert_eq!(params["tracks"], json!(expected));
        }
    }

    #[test]
    fn track_height_uses_modifiers_at_selection_time_and_is_undoable() {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, crate::Services::default());
        let ids: Vec<_> = app.engine.session().tracks.iter().take(3).map(|t| t.id).collect();
        app.run("edit.select", json!({"tracks": [ids[0].0], "exact": true})).unwrap();
        let height = TrackHeight::ALL.last().copied().unwrap();
        let own = vec![ids[1]];
        execute(
            &mut app,
            Action::TrackHeight { own, height: height.label().into() },
            Modifiers { alt: true, shift: true, ..Modifiers::NONE },
            egui::ViewportId::ROOT,
        );
        assert_eq!(app.engine.session().track(ids[0]).unwrap().height, height);
        assert_eq!(app.engine.session().track(ids[1]).unwrap().height, height);
        assert_ne!(app.engine.session().track(ids[2]).unwrap().height, height);
        app.run("edit.undo", json!({})).unwrap();
        assert_ne!(app.engine.session().track(ids[1]).unwrap().height, height);
    }

    #[test]
    fn window_commands_reject_invalid_targets_and_hostile_slot_numbers() {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, crate::Services::default());
        let track = app.engine.session().tracks[0].id;
        for id in ["window.plugin_search", "window.send_controls"] {
            for params in [
                json!({}),
                json!({"track": u64::MAX, "slot": 0}),
                json!({"track": track.0, "slot": -1}),
                json!({"track": track.0, "slot": u64::MAX}),
                json!({"track": track.0, "slot": 0.5}),
            ] {
                assert!(app.run(id, params).is_err());
            }
        }
        assert!(app.run("window.plugin_search", json!({"track": track.0, "instruments": true})).is_err());
        assert!(app.context_menus.search.is_none());
        assert!(app.ui.send_window.is_none());
    }

    #[test]
    fn dropdown_track_view_and_routing_actions_target_the_requested_track() {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, crate::Services::default());
        let track = app.engine.session().tracks[0].id;
        let other = app.engine.session().tracks[1].id;
        let other_view = app.engine.session().track(other).unwrap().view.clone();
        let view = action(&describe(&app, &Target::TrackView(track)), "blocks");
        execute(&mut app, view, Modifiers::NONE, egui::ViewportId::ROOT);
        assert_eq!(app.engine.session().track(track).unwrap().view, "blocks");
        assert_eq!(app.engine.session().track(other).unwrap().view, other_view);
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(app.engine.session().track(track).unwrap().view, "waveform");

        app.run("mix.new_bus", json!({"name": "Dropdown Bus"})).unwrap();
        let bus = app.engine.session().busses.iter().find(|bus| bus.name == "Dropdown Bus").unwrap().id;
        let route = action(&describe(&app, &Target::Route { track, input: false }), "Dropdown Bus");
        execute(&mut app, route, Modifiers::NONE, egui::ViewportId::ROOT);
        assert_eq!(app.engine.session().track(track).unwrap().mixer.output, soundcraft_model::Route::Bus(bus));
        let send = action(&describe(&app, &Target::SendRoute { track, slot: 0 }), "Dropdown Bus");
        execute(&mut app, send, Modifiers::NONE, egui::ViewportId::ROOT);
        assert_eq!(app.engine.session().track(track).unwrap().mixer.sends[0].as_ref().unwrap().target, soundcraft_model::Route::Bus(bus));
    }

    #[test]
    fn dropdown_auxiliary_windows_validate_their_targets() {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, crate::Services::default());
        let track = app.engine.session().tracks[0].id;
        for params in [json!({}), json!({"track": u64::MAX}), json!({"track": track.0, "send": -1}), json!({"track": track.0, "send": u64::MAX})] {
            assert!(app.run("window.routing_search", params).is_err());
        }
        for params in [json!({}), json!({"track": u64::MAX, "slot": 0}), json!({"track": track.0, "slot": u64::MAX})] {
            assert!(app.run("window.save_preset", params).is_err());
        }
        app.run("window.routing_search", json!({"track": track.0, "send": 0})).unwrap();
        assert_eq!(app.context_menus.routing.as_ref().unwrap().send, Some(0));
        app.run("session.new", json!({})).unwrap();
        assert!(app.context_menus.routing.is_none());
    }
}

//! Edit and Mix are independent views of the same engine, not separate sessions.
//! Native hosts register independently repainted deferred viewports through the
//! UI-thread host. Browsers and offscreen hosts use resizable egui windows.

use crate::{MainWindow, SoundApp, edit_window, menus, mix_window, shortcuts};
use egui::{Rect, Ui, Vec2, ViewportCommand, ViewportId, pos2, vec2};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Arrangement {
    Tile,
    Horizontal,
    Vertical,
    Cascade,
}

#[derive(Debug, Default)]
pub struct WindowState {
    primary: Option<MainWindow>,
    focus: Option<MainWindow>,
    arrange: Option<Arrangement>,
    /// The workspace before tiling, in OS points on native hosts, so repeating
    /// Tile does not halve it again and UI zoom does not move it off screen.
    workspace: Option<Rect>,
    primary_rect: Option<Rect>,
    secondary_rect: Option<Rect>,
    secondary_initial_rect: Option<Rect>,
    embedded_arrangement: Option<Arrangement>,
    close_request: Option<MainWindow>,
    root_title: String,
    pub(crate) deferred: bool,
    sent_focus: Option<(MainWindow, f64)>,
    parent_woken: bool,
    pub(crate) rendered_strips: usize,
    pub(crate) root_frames: u64,
    pub(crate) secondary_frames: u64,
    pub(crate) root_ui_ms: f64,
    pub(crate) secondary_ui_ms: f64,
    focus_commands: u64,
    pub(crate) primary_hidden: bool,
    pub(crate) secondary_hidden: bool,
}

pub fn secondary_id() -> ViewportId {
    ViewportId::from_hash_of("soundcraft_second_main_window")
}

impl WindowState {
    pub fn primary(&self, app: &SoundApp) -> MainWindow {
        if app.ui.both_windows { self.primary.unwrap_or(app.ui.window) } else { app.ui.window }
    }

    pub fn is_open(&self, app: &SoundApp, window: MainWindow) -> bool {
        app.ui.both_windows || window == self.primary(app)
    }

    pub fn viewport(&self, app: &SoundApp, window: MainWindow) -> ViewportId {
        if window == self.primary(app) { ViewportId::ROOT } else { secondary_id() }
    }

    pub fn inspect(&self, app: &SoundApp) -> Value {
        let primary = self.primary(app);
        let rect = |window| {
            let r = if window == primary { self.primary_rect } else { self.secondary_rect };
            r.map(|r| [r.min.x, r.min.y, r.max.x, r.max.y])
        };
        json!({
            "primary": primary, "active": app.ui.window,
            "rendering": if self.deferred { "deferred" } else { "embedded_or_immediate" },
            "parked": {"primary": self.primary_hidden, "secondary": self.secondary_hidden},
            "perf": {"focus_commands": self.focus_commands, "root_frames": self.root_frames,
                "secondary_frames": self.secondary_frames, "root_ui_ms": self.root_ui_ms,
                "secondary_ui_ms": self.secondary_ui_ms, "visible_mix_strips": self.rendered_strips},
            "edit": {"open": self.is_open(app, MainWindow::Edit), "rect": rect(MainWindow::Edit)},
            "mix": {"open": self.is_open(app, MainWindow::Mix), "rect": rect(MainWindow::Mix)},
        })
    }
}

pub fn activate(app: &mut SoundApp, window: MainWindow) {
    let primary = app.main_windows.primary(app);
    app.main_windows.primary = Some(primary);
    if window != primary {
        app.ui.both_windows = true;
    }
    app.ui.window = window;
    app.main_windows.focus = Some(window);
    app.main_windows.parent_woken = false;
}

/// Consume global native focus shortcuts before egui can retain them across
/// logic-only passes. Both physical input and control-channel input use this.
pub(crate) fn consume_focus_keys(app: &mut SoundApp, events: &mut Vec<egui::Event>) {
    if app.dialogs.open.is_some() {
        return;
    }
    events.retain(|event| {
        if let egui::Event::Key { key: egui::Key::Equals, pressed, repeat, modifiers, .. } = event
            && modifiers.command
        {
            if *pressed && !*repeat {
                let _ = app.run("window.toggle_mix_edit", json!({}));
            }
            false
        } else {
            true
        }
    });
}

pub fn arrange(app: &mut SoundApp, arrangement: Arrangement) {
    app.main_windows.primary = Some(app.main_windows.primary(app));
    app.ui.both_windows = true;
    app.main_windows.arrange = Some(arrangement);
}

pub fn request_close(app: &mut SoundApp) {
    app.main_windows.close_request = Some(app.ui.window);
}

/// Closing either main view keeps the other view and the session alive. If
/// the root closes, move the remaining view into it before retiring the child.
pub fn close(app: &mut SoundApp, window: MainWindow, ctx: &egui::Context) {
    if !app.ui.both_windows {
        return;
    }
    let primary = app.main_windows.primary(app);
    if window == primary {
        app.main_windows.primary = Some(primary.other());
        if !ctx.embed_viewports()
            && let Some(rect) = app.main_windows.secondary_rect
        {
            place(ctx, ViewportId::ROOT, rect, decoration(ctx, secondary_id()));
        }
    }
    app.ui.both_windows = false;
    app.ui.window = window.other();
    app.main_windows.focus = Some(app.ui.window);
    app.main_windows.secondary_rect = None;
    app.main_windows.secondary_hidden = false;
    if app.main_windows.primary_hidden {
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(true));
        app.main_windows.primary_hidden = false;
    }
    ctx.request_repaint_of(ViewportId::ROOT);
}

pub fn handle_root_close(app: &mut SoundApp, ctx: &egui::Context) {
    if app.ui.both_windows && !app.quit_requested && ctx.input(|i| i.viewport().close_requested()) {
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        close(app, app.main_windows.primary(app), ctx);
    }
}

/// Native integrations keep logic ticking when every window is minimized or
/// occluded. Window commands must still close views and bring them back then.
pub fn process_requests(app: &mut SoundApp, ctx: &egui::Context) {
    if let Some(window) = app.main_windows.close_request.take() {
        close(app, window, ctx);
    }
    if ctx.embed_viewports() {
        return;
    }
    let (root_rect, child_rect, child_exists) = ctx.input(|i| {
        (
            i.raw.viewports.get(&ViewportId::ROOT).and_then(|v| v.outer_rect),
            i.raw.viewports.get(&secondary_id()).and_then(|v| v.outer_rect),
            i.raw.viewports.contains_key(&secondary_id()),
        )
    });
    app.main_windows.primary_rect = root_rect;
    app.main_windows.secondary_rect = child_rect;
    reconcile_focus(app, ctx);
    flush_focus(app, ctx, child_exists);
    update_occlusion(app, ctx);
}

fn update_occlusion(app: &mut SoundApp, ctx: &egui::Context) {
    if !cfg!(target_os = "macos") || !app.main_windows.deferred {
        return;
    }
    let primary = app.main_windows.primary(app);
    let (hide_root, hide_child) = if app.ui.both_windows {
        match (app.main_windows.primary_rect, app.main_windows.secondary_rect) {
            (Some(root), Some(child)) if root.is_positive() && child.is_positive() => {
                (app.ui.window != primary && child.contains_rect(root), app.ui.window == primary && root.contains_rect(child))
            }
            _ => (false, false),
        }
    } else {
        (false, false)
    };
    // AppKit can report the old occlusion state during focus transitions,
    // letting Metal block in nextDrawable for a fully covered window. Park
    // only windows with no visible pixels, retaining their viewport/surface.
    // Tile/Cascade layouts with visible portions keep both windows displayed.
    for (id, hidden, old) in
        [(ViewportId::ROOT, hide_root, &mut app.main_windows.primary_hidden), (secondary_id(), hide_child, &mut app.main_windows.secondary_hidden)]
    {
        if *old != hidden {
            ctx.send_viewport_cmd_to(id, ViewportCommand::Visible(!hidden));
            *old = hidden;
        }
    }
}

/// Tile Horizontal stacks rows; Tile Vertical places columns. The automatic
/// variant chooses the split with the larger usable dimension. All rectangles
/// stay inside the original workspace, including negative monitor coordinates.
pub fn arranged_rects(area: Rect, arrangement: Arrangement) -> (Rect, Rect) {
    let horizontal = match arrangement {
        Arrangement::Horizontal => true,
        Arrangement::Vertical => false,
        _ => area.height() > area.width(),
    };
    if arrangement == Arrangement::Cascade {
        let inset = 36.0_f32.min(area.width() * 0.1).min(area.height() * 0.1);
        let size = (area.size() - vec2(inset, inset)).max(vec2(1.0, 1.0));
        return (Rect::from_min_size(area.min, size), Rect::from_min_size(area.min + vec2(inset, inset), size));
    }
    if horizontal {
        let middle = area.center().y;
        (Rect::from_min_max(area.min, pos2(area.max.x, middle)), Rect::from_min_max(pos2(area.min.x, middle), area.max))
    } else {
        let middle = area.center().x;
        (Rect::from_min_max(area.min, pos2(middle, area.max.y)), Rect::from_min_max(pos2(middle, area.min.y), area.max))
    }
}

fn decoration(ctx: &egui::Context, id: ViewportId) -> Vec2 {
    ctx.input(|i| i.raw.viewports.get(&id).and_then(|v| Some((v.outer_rect?.size() - v.inner_rect?.size()).max(Vec2::ZERO))))
        .unwrap_or(vec2(0.0, 28.0))
}

fn place(ctx: &egui::Context, id: ViewportId, rect: Rect, chrome: Vec2) {
    ctx.send_viewport_cmd_to(id, ViewportCommand::Fullscreen(false));
    ctx.send_viewport_cmd_to(id, ViewportCommand::Maximized(false));
    ctx.send_viewport_cmd_to(id, ViewportCommand::MinInnerSize(vec2(320.0, 220.0)));
    ctx.send_viewport_cmd_to(id, ViewportCommand::OuterPosition(rect.min));
    ctx.send_viewport_cmd_to(id, ViewportCommand::InnerSize((rect.size() - chrome).max(vec2(1.0, 1.0))));
}

fn scaled_rect(rect: Rect, scale: f32) -> Rect {
    Rect::from_min_max((rect.min.to_vec2() * scale).to_pos2(), (rect.max.to_vec2() * scale).to_pos2())
}

pub fn title(app: &SoundApp, window: MainWindow) -> String {
    format!("{}{} — SoundCraft — {}", app.engine.session().name, if app.engine.is_dirty() { " *" } else { "" }, window.label())
}

fn surface(app: &mut SoundApp, ui: &mut Ui, window: MainWindow) {
    if window == app.main_windows.primary(app) {
        app.main_windows.root_frames = app.main_windows.root_frames.saturating_add(1);
    } else {
        app.main_windows.secondary_frames = app.main_windows.secondary_frames.saturating_add(1);
    }
    // Independent panel/scroll IDs also matter when both views are embedded in
    // one browser canvas: scrolling a track list must not scroll the other one.
    ui.push_id(window.label(), |ui| match window {
        MainWindow::Edit => edit_window::show(app, ui),
        MainWindow::Mix => mix_window::show(app, ui),
    });
}

fn received_input(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        i.raw.focused
            && i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::WindowFocused(true) | egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: true, .. }
                )
            })
    })
}

fn focus_pending(app: &SoundApp) -> bool {
    app.main_windows.focus.is_some() || app.main_windows.sent_focus.is_some()
}

fn reconcile_focus(app: &mut SoundApp, ctx: &egui::Context) {
    if let Some((window, sent_at)) = app.main_windows.sent_focus {
        let id = app.main_windows.viewport(app, window);
        let acknowledged = ctx.input(|i| i.raw.viewports.get(&id).is_some_and(|v| v.focused == Some(true)));
        // Some window managers reject activation. Do not let a missing ack
        // prevent later physical input from identifying the focused view.
        if acknowledged || ctx.input(|i| i.time) - sent_at >= 0.25 {
            app.main_windows.sent_focus = None;
        }
    }
}

/// The only place that emits focus commands. Input bursts retain their final
/// destination; logic and drawing can both flush without sending twice.
pub(crate) fn flush_focus(app: &mut SoundApp, ctx: &egui::Context, child_registered: bool) {
    if ctx.embed_viewports() {
        return;
    }
    let Some(window) = app.main_windows.focus else { return };
    let id = app.main_windows.viewport(app, window);
    if id == secondary_id() && !child_registered {
        if !app.main_windows.parent_woken {
            restore_and_focus(app, ctx, ViewportId::ROOT, false);
            app.main_windows.parent_woken = true;
        }
        return;
    }
    app.main_windows.focus = None;
    app.main_windows.parent_woken = false;
    let focused = ctx.input(|i| i.raw.viewports.get(&id).is_some_and(|v| v.focused == Some(true)));
    let reversing_pending = app.main_windows.sent_focus.is_some_and(|(target, _)| target != window);
    if focused && !reversing_pending {
        app.main_windows.sent_focus = None;
    } else if app.main_windows.sent_focus.map(|(target, _)| target) != Some(window) {
        restore_and_focus(app, ctx, id, reversing_pending);
        app.main_windows.sent_focus = Some((window, ctx.input(|i| i.time)));
    }
}

fn restore_and_focus(app: &mut SoundApp, ctx: &egui::Context, id: ViewportId, reversing_pending: bool) {
    let restored = if id == ViewportId::ROOT && app.main_windows.primary_hidden {
        ctx.send_viewport_cmd_to(id, ViewportCommand::Visible(true));
        app.main_windows.primary_hidden = false;
        true
    } else if id == secondary_id() && app.main_windows.secondary_hidden {
        ctx.send_viewport_cmd_to(id, ViewportCommand::Visible(true));
        app.main_windows.secondary_hidden = false;
        true
    } else {
        false
    };
    let (minimized, focused) =
        ctx.input(|i| i.raw.viewports.get(&id).map_or((false, false), |v| (v.minimized == Some(true), v.focused == Some(true))));
    if minimized {
        ctx.send_viewport_cmd_to(id, ViewportCommand::Minimized(false));
    }
    // winit's macOS Visible(true) already makes the window key and frontmost.
    // Avoid ordering it a second time while our app has keyboard focus. When
    // another application is active, Focus is still needed to activate ours.
    let restored_in_active_app = restored && cfg!(target_os = "macos") && ctx.input(|i| i.raw.viewports.values().any(|v| v.focused == Some(true)));
    if (!focused || reversing_pending) && !restored_in_active_app {
        ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
        app.main_windows.focus_commands = app.main_windows.focus_commands.saturating_add(1);
    }
}

pub(crate) fn animating(app: &SoundApp) -> bool {
    app.is_playing() || app.meters.values().any(|m| m.level.iter().any(|v| *v > 0.0001)) || app.main_meter.level.iter().any(|v| *v > 0.0001)
}

pub(crate) fn request_views_repaint(ctx: &egui::Context) {
    ctx.request_repaint_of(ViewportId::ROOT);
    if ctx.viewport_for(secondary_id(), |v| v.used) {
        ctx.request_repaint_of(secondary_id());
    }
}

pub(crate) fn secondary_builder(app: &SoundApp, ctx: &egui::Context) -> Option<egui::ViewportBuilder> {
    if !app.ui.both_windows || !app.fonts_ready {
        return None;
    }
    let secondary = app.main_windows.primary(app).other();
    let mut builder = egui::ViewportBuilder::default()
        .with_title(title(app, secondary))
        .with_inner_size([1100.0, 780.0])
        .with_min_inner_size([320.0, 220.0])
        .with_drag_and_drop(true);
    if let Some(rect) = app.main_windows.secondary_initial_rect {
        let rect = scaled_rect(rect, ctx.zoom_factor().recip());
        builder = builder.with_position(rect.min).with_inner_size((rect.size() - decoration(ctx, secondary_id())).max(vec2(1.0, 1.0)));
    }
    Some(builder)
}

pub(crate) fn show_secondary(app: &mut SoundApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if !app.ui.both_windows {
        ctx.request_repaint_of(ViewportId::ROOT);
        return;
    }
    ui.set_style(ctx.global_style());
    let secondary = app.main_windows.primary(app).other();
    if ctx.input(|i| i.viewport().close_requested()) {
        close(app, secondary, &ctx);
        return;
    }
    reconcile_focus(app, &ctx);
    app.main_windows.secondary_rect = ctx.input(|i| i.viewport().outer_rect);
    if ctx.input(|i| i.pointer.any_pressed()) {
        activate(app, secondary);
    } else if !focus_pending(app) && received_input(&ctx) {
        app.ui.window = secondary;
    }
    let revision = app.engine.revision;
    let interaction = ctx.input(|i| {
        i.pointer.any_down()
            || i.events.iter().any(|e| {
                matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: true, .. } | egui::Event::Text(_))
            })
    });
    let before = interaction.then(|| app.ui.clone());
    shortcuts::handle(app, &ctx);
    if let Some(window) = app.main_windows.close_request.take() {
        close(app, window, &ctx);
    }
    if !app.native_menu_bar {
        menus::menu_bar(app, ui);
    }
    surface(app, ui, secondary);
    if app.ui.window == secondary {
        app.floating_ui(&ctx);
    }
    // Focus/hover/keyup alone does not invalidate the parent's document view.
    if app.engine.revision != revision || before.as_ref().is_some_and(|state| *state != app.ui) {
        ctx.request_repaint_of(ViewportId::ROOT);
        ctx.request_repaint();
    }
    if app.main_windows.deferred {
        flush_focus(app, &ctx, true);
    }
    if animating(app) {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

pub fn show(app: &mut SoundApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if !app.ui.both_windows {
        app.main_windows.primary = Some(app.ui.window);
    }
    let primary = app.main_windows.primary(app);
    app.main_windows.primary = Some(primary);
    let embedded = ctx.embed_viewports();
    let area = if embedded { ui.max_rect() } else { ctx.input(|i| i.viewport().outer_rect).unwrap_or(ui.max_rect()) };
    if !app.ui.both_windows || app.main_windows.workspace.is_none() {
        app.main_windows.workspace = Some(if embedded { area } else { scaled_rect(area, ctx.zoom_factor()) });
    }
    if !embedded && !focus_pending(app) && received_input(&ctx) {
        app.ui.window = primary;
    }
    // The native input hook consumes focus shortcuts before deferred drawing.
    // Other shortcuts and embedded hosts keep their normal UI dispatch.
    shortcuts::handle(app, &ctx);
    if !app.native_menu_bar {
        menus::menu_bar(app, ui);
    }
    if let Some(window) = app.main_windows.close_request.take() {
        close(app, window, &ctx);
    }
    let primary = app.main_windows.primary(app);
    if let Some(arrangement) = app.main_windows.arrange.take() {
        let workspace = app.main_windows.workspace.map_or(area, |rect| if embedded { rect } else { scaled_rect(rect, ctx.zoom_factor().recip()) });
        if embedded {
            app.main_windows.embedded_arrangement = Some(arrangement);
        } else {
            let (first, second) = arranged_rects(workspace, arrangement);
            place(&ctx, ViewportId::ROOT, first, decoration(&ctx, ViewportId::ROOT));
            place(&ctx, secondary_id(), second, decoration(&ctx, secondary_id()));
            app.main_windows.secondary_initial_rect = Some(scaled_rect(second, ctx.zoom_factor()));
        }
    }
    if embedded && app.ui.both_windows {
        embedded_windows(app, ui, primary);
        app.floating_ui(&ctx);
        return;
    }
    app.main_windows.primary_rect = Some(area);
    let root_title = title(app, primary);
    if root_title != app.main_windows.root_title {
        ctx.send_viewport_cmd(ViewportCommand::Title(root_title.clone()));
        app.main_windows.root_title = root_title;
    }
    if !app.main_windows.primary_hidden {
        surface(app, ui, primary);
    }
    if app.ui.window == primary || !app.ui.both_windows {
        app.floating_ui(&ctx);
    }
    if !app.main_windows.deferred {
        if let Some(builder) = secondary_builder(app, &ctx) {
            ctx.show_viewport_immediate(secondary_id(), builder, |ui, _class| show_secondary(app, ui));
        }
        flush_focus(app, &ctx, app.ui.both_windows);
    }
}

fn embedded_windows(app: &mut SoundApp, ui: &mut Ui, primary: MainWindow) {
    let ctx = ui.ctx().clone();
    let area = ui.available_rect_before_wrap();
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::theme::Tokens::current().window_bg)).show(ui, |_| {});
    let requested = app.main_windows.embedded_arrangement.take();
    let (first, second) = arranged_rects(area.shrink(4.0), requested.unwrap_or(Arrangement::Cascade));
    for (window, rect) in [(primary, first), (primary.other(), second)] {
        if !app.ui.both_windows {
            break;
        }
        let mut open = true;
        let mut pane = egui::Window::new(title(app, window))
            .id(egui::Id::new(("main_window", window.label())))
            .open(&mut open)
            .collapsible(false)
            .min_size([320.0, 220.0])
            .default_pos(rect.min)
            .default_size(rect.size());
        if requested.is_some() {
            // Area constrains positions using the previous frame's window
            // size. Do not push a newly tiled pane left based on its old width.
            pane = pane.fixed_rect(rect).constrain(false);
        }
        let response = pane.show(&ctx, |ui| {
            if ui.rect_contains_pointer(ui.max_rect()) && ui.input(|i| i.pointer.any_pressed()) {
                app.ui.window = window;
            }
            surface(app, ui, window);
            ui.take_available_space();
        });
        if let Some(response) = response {
            if window == primary {
                app.main_windows.primary_rect = Some(response.response.rect);
            } else {
                app.main_windows.secondary_rect = Some(response.response.rect);
            }
        }
        if !open {
            close(app, window, &ctx);
        }
    }
    if let Some(window) = app.main_windows.focus {
        ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, egui::Id::new(("main_window", window.label()))));
    }
    app.main_windows.focus = None;
}

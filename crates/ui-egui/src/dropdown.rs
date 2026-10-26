//! Native presentation for ordinary form ComboBoxes. Values remain owned by their forms:
//! only a row identity crosses the native tracking loop, and the next frame applies it.
use crate::context_menu::{Action, Anchor, Entry, Request, Target};
#[cfg(any(test, not(target_arch = "wasm32")))]
use egui::Context;
use egui::{Id, InnerResponse, Ui, WidgetText, vec2};

#[derive(Clone, Copy)]
struct Frame {
    enabled: bool,
    active: bool,
    epoch: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Pick {
    pub index: usize,
    pub label: String,
    pub epoch: u64,
    pub pass: u64,
}

#[cfg(any(test, not(target_arch = "wasm32")))]
pub(crate) fn prepare(app: &crate::SoundApp, ctx: &Context) {
    ctx.data_mut(|data| {
        data.insert_temp(
            Id::unique("native_dropdown_frame"),
            Frame { enabled: app.context_menus.enabled, active: app.context_menus.active, epoch: app.context_menus.epoch },
        )
    });
}

#[cfg(any(test, not(target_arch = "wasm32")))]
pub(crate) fn collect(app: &mut crate::SoundApp, ctx: &Context) {
    let request = ctx.data_mut(|data| {
        let id = Id::unique("native_dropdown_request");
        let request = data.get_temp::<Request>(id);
        data.remove::<Request>(id);
        request
    });
    if let Some(request) = request
        && request.epoch == app.context_menus.epoch
        && !app.context_menus.active
    {
        app.context_menus.pending = Some(request);
    }
}

/// Same small ComboBox API used by SoundCraft's forms, with an unchanged egui fallback.
pub struct ComboBox {
    fallback: egui::ComboBox<'static>,
    salt: Id,
    label: Option<WidgetText>,
    selected: WidgetText,
    width: Option<f32>,
}

impl ComboBox {
    pub fn from_id_salt(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        let salt = Id::unique(salt);
        Self { fallback: egui::ComboBox::from_id_salt(salt), salt, label: None, selected: "".into(), width: None }
    }
    pub fn from_label(label: impl Into<WidgetText>) -> Self {
        let label = label.into();
        Self {
            fallback: egui::ComboBox::from_label(label.clone()),
            salt: Id::unique(label.text()),
            label: Some(label),
            selected: "".into(),
            width: None,
        }
    }
    pub fn selected_text(mut self, text: impl Into<WidgetText>) -> Self {
        self.selected = text.into();
        self.fallback = self.fallback.selected_text(self.selected.clone());
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self.fallback = self.fallback.width(width);
        self
    }
    pub fn show_ui<R>(self, ui: &mut Ui, contents: impl FnOnce(&mut Choices<'_>) -> R) -> InnerResponse<Option<R>> {
        let frame = ui.ctx().data(|data| data.get_temp::<Frame>(Id::unique("native_dropdown_frame")));
        let Some(frame) = frame.filter(|frame| frame.enabled) else {
            return self
                .fallback
                .show_ui(ui, |ui| contents(&mut Choices { fallback: Some(ui), entries: vec![], control: Id::NULL, pick: None, changed: false }));
        };
        let control = ui.make_persistent_id((self.salt, ui.ctx().viewport_id()));
        let pick = ui
            .ctx()
            .data_mut(|data| data.remove_temp::<Pick>(control))
            .filter(|pick| pick.epoch == frame.epoch && ui.ctx().cumulative_pass_nr().saturating_sub(pick.pass) <= 4);
        let mut choices = Choices { fallback: None, entries: vec![], control, pick, changed: false };
        let inner = contents(&mut choices);
        let draw = |ui: &mut Ui| {
            let width = self.width.unwrap_or(ui.spacing().combo_width);
            let mut response = ui.add(egui::Button::new(self.selected.clone()).right_text("  ").min_size(vec2(width, ui.spacing().interact_size.y)));
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, ui.is_enabled(), self.selected.text()));
            crate::icons::draw(
                ui.painter(),
                egui::Rect::from_center_size(egui::pos2(response.rect.max.x - 9.0, response.rect.center().y), vec2(9.0, 9.0)),
                "triangle_down",
                ui.visuals().text_color(),
            );
            if choices.changed {
                response.mark_changed();
                ui.ctx().request_repaint();
            }
            response
        };
        let response = if let Some(label) = self.label {
            ui.horizontal(|ui| {
                let response = draw(ui);
                ui.label(label);
                response
            })
            .inner
        } else {
            draw(ui)
        };
        if response.clicked() && !frame.active && !choices.entries.is_empty() {
            let request = Request {
                target: Target::Control(control),
                entries: choices.entries,
                viewport: ui.ctx().viewport_id(),
                epoch: frame.epoch,
                anchor: Anchor::Below { rect: response.rect, zoom: ui.ctx().zoom_factor() },
            };
            ui.ctx().data_mut(|data| data.insert_temp(Id::unique("native_dropdown_request"), request));
        }
        InnerResponse { inner: Some(inner), response }
    }
}

pub struct ChoiceResponse {
    clicked: bool,
    changed: bool,
}
impl ChoiceResponse {
    pub fn clicked(&self) -> bool {
        self.clicked
    }
    pub fn changed(&self) -> bool {
        self.changed
    }
}

pub struct Choices<'a> {
    fallback: Option<&'a mut Ui>,
    entries: Vec<Entry>,
    control: Id,
    pick: Option<Pick>,
    changed: bool,
}

impl Choices<'_> {
    pub fn selectable_label(&mut self, selected: bool, text: impl Into<WidgetText>) -> ChoiceResponse {
        let text = text.into();
        if let Some(ui) = self.fallback.as_mut() {
            let response = ui.selectable_label(selected, text);
            return ChoiceResponse { clicked: response.clicked(), changed: response.changed() };
        }
        let label = text.text().to_string();
        let index = self.entries.len();
        let clicked = self.pick.as_ref().is_some_and(|pick| pick.index == index && pick.label == label);
        self.entries.push(Entry::item(label.clone(), Action::Choice { control: self.control, index, label }).checked(selected));
        self.changed |= clicked && !selected;
        ChoiceResponse { clicked, changed: clicked && !selected }
    }
    pub fn selectable_value<T: PartialEq>(&mut self, current: &mut T, value: T, text: impl Into<WidgetText>) -> ChoiceResponse {
        if let Some(ui) = self.fallback.as_mut() {
            let response = ui.selectable_value(current, value, text);
            return ChoiceResponse { clicked: response.clicked(), changed: response.changed() };
        }
        let response = self.selectable_label(*current == value, text);
        if response.clicked {
            *current = value;
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, Rect, Response};

    fn frame(ctx: &Context, app: &mut crate::SoundApp, value: &mut u32, events: Vec<Event>) -> Response {
        let mut response = None;
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(600.0, 400.0))), events, ..Default::default() },
            |ui| {
                prepare(app, ui.ctx());
                let text = format!("Value {value}");
                response = Some(
                    ComboBox::from_id_salt("test-choice")
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(value, 1, "One");
                            ui.selectable_value(value, 2, "Two");
                        })
                        .response,
                );
                collect(app, ui.ctx());
            },
        );
        output.textures_delta.clear();
        response.unwrap()
    }

    #[test]
    fn native_choices_apply_once_on_the_next_frame_without_capturing_a_form_borrow() {
        let ctx = Context::default();
        let mut app = crate::SoundApp::new(soundcraft_engine::Engine::default(), None, crate::Services::default());
        app.context_menus.enabled = true;
        let mut value = 1;
        let response = frame(&ctx, &mut app, &mut value, vec![]);
        let position = response.rect.center();
        frame(
            &ctx,
            &mut app,
            &mut value,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
                Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
            ],
        );
        let request = app.context_menus.pending.take().unwrap();
        assert!(matches!(request.anchor, Anchor::Below { .. }));
        assert_eq!(value, 1, "opening the menu does not change the field");
        let Entry::Item { action: Action::Choice { control, index, label }, .. } = &request.entries[1] else { panic!("choice expected") };
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|data| data.insert_temp(*control, Pick { index: *index, label: label.clone(), epoch: request.epoch, pass }));
        assert!(frame(&ctx, &mut app, &mut value, vec![]).changed());
        assert_eq!(value, 2);
        assert!(!frame(&ctx, &mut app, &mut value, vec![]).changed());
    }

    #[test]
    fn stale_labels_epochs_and_expired_choices_do_not_change_form_values() {
        for (label, epoch, expired) in [("Replaced option", 0, false), ("Two", 1, false), ("Two", 0, true)] {
            let ctx = Context::default();
            let mut app = crate::SoundApp::new(soundcraft_engine::Engine::default(), None, crate::Services::default());
            app.context_menus.enabled = true;
            let mut value = 1;
            for _ in 0..6 {
                frame(&ctx, &mut app, &mut value, vec![]);
            }
            // Obtain exactly the control identity published by the selector.
            let response = frame(&ctx, &mut app, &mut value, vec![]);
            let position = response.rect.center();
            frame(
                &ctx,
                &mut app,
                &mut value,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
                    Event::PointerButton { pos: position, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
                ],
            );
            let request = app.context_menus.pending.take().unwrap();
            let Target::Control(control) = request.target else { panic!("control expected") };
            let pass = if expired { 0 } else { ctx.cumulative_pass_nr() };
            ctx.data_mut(|data| data.insert_temp(control, Pick { index: 1, label: label.into(), epoch, pass }));
            assert!(!frame(&ctx, &mut app, &mut value, vec![]).changed());
            assert_eq!(value, 1);
        }
    }
}

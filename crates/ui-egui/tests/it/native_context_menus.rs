//! Native presentation owns context menus and left-click dropdowns without an egui popup.
#![cfg(not(target_arch = "wasm32"))]

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use soundcraft_ui_egui::{
    MainWindow, Services, SoundApp,
    context_menu::{Presenter, Request, Selection, Target},
    native_host::NativeUiHost,
};
use std::{cell::RefCell, rc::Rc};

struct Recorder(Rc<RefCell<Vec<Request>>>);
impl Presenter for Recorder {
    fn present(&self, request: &Request) -> Result<Option<Selection>, String> {
        self.0.borrow_mut().push(request.clone());
        Ok(None)
    }
}

fn harness() -> (Harness<'static, NativeUiHost>, Rc<RefCell<Vec<Request>>>) {
    let mut engine = soundcraft_engine::Engine::default();
    engine.execute("track.new", &json!({"name": "Voice"})).unwrap();
    engine.execute("mix.new_bus", &json!({"name": "Verb"})).unwrap();
    engine.execute("mix.send", &json!({"track": "Voice", "slot": 0, "bus": "Verb"})).unwrap();
    let host = NativeUiHost::new(SoundApp::new(engine, None, Services::default())).unwrap();
    let requests = Rc::new(RefCell::new(vec![]));
    host.set_context_menu_presenter(Rc::new(Recorder(requests.clone()))).unwrap();
    let mut h = Harness::builder().with_size(vec2(1400.0, 900.0)).build_ui_state(
        |ui, host: &mut NativeUiHost| {
            host.state.borrow_mut().logic(ui.ctx());
            host.ui(ui);
        },
        host,
    );
    h.run_steps(5);
    (h, requests)
}

fn right(h: &mut Harness<NativeUiHost>, at: Pos2) {
    h.hover_at(at);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(3);
}

#[test]
fn edit_and_mix_track_names_open_one_native_menu_without_an_egui_popup() {
    let (mut h, requests) = harness();
    let row = h.state().state.borrow().edit_layout.rows[0].1;
    right(&mut h, pos2(row[0] + 60.0, row[1] + 10.0));
    assert_eq!(requests.borrow().len(), 1);
    assert!(matches!(requests.borrow()[0].target, Target::Track(_)));
    assert!(h.query_by_label("Make Inactive").is_none());
    h.state().state.borrow_mut().ui.window = MainWindow::Mix;
    h.run_steps(4);
    let strip = h.get_by_label("Record enable for Voice").rect();
    right(&mut h, pos2(strip.center().x, 900.0 - 17.0));
    assert_eq!(requests.borrow().len(), 2);
    assert!(matches!(requests.borrow()[1].target, Target::Track(_)));
    assert!(h.query_by_label("Make Inactive").is_none());
}

#[test]
fn inserts_and_sends_use_native_context_menus_and_left_click_dropdowns() {
    let (mut h, requests) = harness();
    h.state().state.borrow_mut().ui.window = MainWindow::Mix;
    h.run_steps(4);
    let insert = h.get_by_label("Insert A for Voice").rect().center();
    right(&mut h, insert);
    assert!(matches!(requests.borrow().last().unwrap().target, Target::Insert { slot: 0, .. }));
    assert!(h.query_by_label("Search plugins…").is_none());
    let send = h.get_by_label("Send A level for Voice").rect().center();
    right(&mut h, send);
    assert!(matches!(requests.borrow().last().unwrap().target, Target::Send { slot: 0, .. }));
    assert!(h.query_by_label("Pre-fader").is_none());
    let n = requests.borrow().len();
    h.get_by_label("Insert A for Voice").click();
    h.run_steps(3);
    assert_eq!(requests.borrow().len(), n + 1);
    assert!(matches!(requests.borrow().last().unwrap().target, Target::Insert { slot: 0, .. }));
    assert!(matches!(requests.borrow().last().unwrap().anchor, soundcraft_ui_egui::context_menu::Anchor::Below { .. }));
    assert!(h.query_by_label("Search plugins…").is_none());
}

#[test]
fn track_view_dropdown_requests_a_native_menu() {
    let (mut h, requests) = harness();
    let row = h.state().state.borrow().edit_layout.rows[0].1;
    let point = pos2(row[0] + 70.0, row[1] + 47.0);
    h.hover_at(point);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: point, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(3);
    let requests = requests.borrow();
    assert!(matches!(requests.last().unwrap().target, Target::TrackView(_)));
    assert!(matches!(requests.last().unwrap().anchor, soundcraft_ui_egui::context_menu::Anchor::Below { .. }));
    assert!(h.query_by_label("blocks").is_none(), "the egui menu must not open behind the native menu");
}

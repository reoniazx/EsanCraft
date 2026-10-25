//! Native presentation owns right clicks while the inline pickers still own left clicks.
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
    let strip = h.get_by_label("Record enable").rect();
    right(&mut h, pos2(strip.center().x, 900.0 - 17.0));
    assert_eq!(requests.borrow().len(), 2);
    assert!(matches!(requests.borrow()[1].target, Target::Track(_)));
    assert!(h.query_by_label("Make Inactive").is_none());
}

#[test]
fn inserts_and_sends_use_native_context_menus_and_keep_left_click_search() {
    let (mut h, requests) = harness();
    h.state().state.borrow_mut().ui.window = MainWindow::Mix;
    h.run_steps(4);
    let insert = h.get_by_label("Insert A").rect().center();
    right(&mut h, insert);
    assert!(matches!(requests.borrow().last().unwrap().target, Target::Insert { slot: 0, .. }));
    assert!(h.query_by_label("Search plugins…").is_none());
    let send = h.get_by_label("Send A level").rect().center();
    right(&mut h, send);
    assert!(matches!(requests.borrow().last().unwrap().target, Target::Send { slot: 0, .. }));
    assert!(h.query_by_label("Pre-fader").is_none());
    let n = requests.borrow().len();
    h.get_by_label("Insert A").click();
    h.run_steps(3);
    h.event(Event::Text("reverb".into()));
    h.run_steps(2);
    let reverb = soundcraft_dsp::plugins().iter().find(|p| !p.is_instrument && p.name.to_lowercase().contains("reverb")).unwrap();
    assert!(h.query_by_label(reverb.name).is_some());
    assert_eq!(requests.borrow().len(), n);
}

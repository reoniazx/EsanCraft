//! AppKit context menus, sharing muda's single action/event handler with the menu bar.
//! A hidden menu-bar host gives us a retained, typed NSMenu through safe AppKit APIs.
use egui::{Modifiers, ViewportId};
use muda::{CheckMenuItem, Icon, IconMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSMenu, NSWindow};
use objc2_foundation::NSPoint;
use soundcraft_ui_egui::context_menu::{Action, Anchor, Entry, Presenter, Request, Selection};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

pub const EVENT_PREFIX: &str = "soundcraft.context.";

pub struct ContextEvent {
    pub id: MenuId,
    pub modifiers: Modifiers,
}

pub fn modifiers() -> Modifiers {
    let flags = NSEvent::modifierFlags_class();
    let command = flags.contains(NSEventModifierFlags::Command);
    Modifiers {
        alt: flags.contains(NSEventModifierFlags::Option),
        ctrl: flags.contains(NSEventModifierFlags::Control),
        shift: flags.contains(NSEventModifierFlags::Shift),
        mac_cmd: command,
        command,
    }
}

pub struct NativeContextMenu {
    root: Submenu,
    menu: Retained<NSMenu>,
    events: RefCell<Receiver<ContextEvent>>,
    next: Cell<u64>,
    origins: RefCell<HashMap<ViewportId, Origin>>,
}

struct Origin {
    pointer: NSPoint,
    window: Option<Retained<NSWindow>>,
}

impl NativeContextMenu {
    /// Install only after the app's real menu bar. The host stays hidden for its lifetime,
    /// so muda keeps its native submenu instance and all action targets alive.
    pub fn install(bar: &Menu, events: Receiver<ContextEvent>) -> Result<Rc<Self>, String> {
        let mtm = MainThreadMarker::new().ok_or("Context menus require the main thread")?;
        let root = Submenu::new("SoundCraft Context Menu Host", true);
        let index = isize::try_from(bar.items().len()).map_err(|_| "Menu bar is too large")?;
        bar.append(&root).map_err(|e| e.to_string())?;
        let nsapp = NSApplication::sharedApplication(mtm);
        let item = nsapp.mainMenu().and_then(|m| m.itemAtIndex(index));
        if let Some(item) = &item {
            item.setHidden(true);
        }
        let menu = item.as_ref().and_then(|item| item.submenu());
        let Some(menu) = menu else {
            let _ = bar.remove(&root);
            return Err("Native context menu host is unavailable".into());
        };
        // Keep popup tracking independent of the hidden menu-bar item's visibility.
        // Detach through AppKit while retaining both the typed menu and muda's host
        // instance, which still updates this retained instance when rows change.
        if let Some(item) = item {
            item.setSubmenu(None);
        }
        Ok(Rc::new(Self { root, menu, events: RefCell::new(events), next: Cell::new(0), origins: RefCell::new(HashMap::new()) }))
    }

    fn append(parent: &Submenu, entries: &[Entry], sequence: u64, actions: &mut Vec<(MenuId, Action)>) -> Result<(), String> {
        for entry in entries {
            match entry {
                Entry::Separator => parent.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?,
                Entry::Submenu { label, entries } => {
                    let child = Submenu::new(label.replace('&', "&&"), true);
                    Self::append(&child, entries, sequence, actions)?;
                    parent.append(&child).map_err(|e| e.to_string())?;
                }
                Entry::Item { label, action, enabled, checked, color } => {
                    let id = MenuId::new(format!("{EVENT_PREFIX}{sequence}.{}", actions.len()));
                    let label = label.replace('&', "&&");
                    if let Some([r, g, b]) = color {
                        // Original swatches are generated in code; there is no bundled artwork.
                        let pixels = [*r, *g, *b, 255].repeat(14 * 14);
                        let icon = Icon::from_rgba(pixels, 14, 14).map_err(|e| e.to_string())?;
                        parent.append(&IconMenuItem::with_id(id.clone(), label, *enabled, Some(icon), None)).map_err(|e| e.to_string())?;
                    } else if let Some(checked) = checked {
                        parent.append(&CheckMenuItem::with_id(id.clone(), label, *enabled, *checked, None)).map_err(|e| e.to_string())?;
                    } else {
                        parent.append(&MenuItem::with_id(id.clone(), label, *enabled, None)).map_err(|e| e.to_string())?;
                    }
                    actions.push((id, action.clone()));
                }
            }
        }
        Ok(())
    }
}

impl Presenter for NativeContextMenu {
    fn capture_input(&self, raw: &egui::RawInput) {
        if raw.events.iter().any(|event| matches!(event, egui::Event::PointerButton { pressed: false, .. } | egui::Event::Key { pressed: true, .. }))
            && let Ok(mut origins) = self.origins.try_borrow_mut()
        {
            // AppKit desktop points already account for Retina and every display's origin.
            // Capture before rendering so moving into the menu does not move its anchor.
            let window = MainThreadMarker::new().and_then(|mtm| {
                let app = NSApplication::sharedApplication(mtm);
                app.currentEvent().and_then(|event| event.window(mtm)).or_else(|| app.keyWindow())
            });
            origins.insert(raw.viewport_id, Origin { pointer: NSEvent::mouseLocation(), window });
        }
    }

    fn present(&self, request: &Request) -> Result<Option<Selection>, String> {
        let _mtm = MainThreadMarker::new().ok_or("Context menus require the main thread")?;
        let sequence = self.next.get().checked_add(1).ok_or("Context menu ids exhausted")?;
        self.next.set(sequence);
        while self.root.remove_at(0).is_some() {}
        let mut actions = Vec::new();
        Self::append(&self.root, &request.entries, sequence, &mut actions)?;
        {
            let events = self.events.try_borrow().map_err(|_| "Context menu event queue busy")?;
            while events.try_recv().is_ok() {}
        }
        let origin = self.origins.try_borrow_mut().ok().and_then(|mut origins| origins.remove(&request.viewport));
        let mut point = origin.as_ref().map_or_else(NSEvent::mouseLocation, |origin| origin.pointer);
        let view = if let Anchor::Below { .. } = request.anchor {
            origin.as_ref().and_then(|origin| origin.window.as_ref()).and_then(|window| window.contentView())
        } else {
            None
        };
        if let (Anchor::Below { rect, zoom }, Some(view)) = (request.anchor, view.as_ref()) {
            let bounds = view.bounds();
            let x = f64::from(rect.min.x * zoom);
            let y = f64::from(rect.max.y * zoom + 2.0);
            if x.is_finite() && y.is_finite() {
                point =
                    NSPoint::new(bounds.origin.x + x, if view.isFlipped() { bounds.origin.y + y } else { bounds.origin.y + bounds.size.height - y });
            }
        }
        // `None` means desktop coordinates. No raw window handles, casts, or unsafe FFI.
        // Do not hold any RefCell borrow across AppKit's nested tracking loop.
        log::debug!("Context menu {:?}: {} native rows at {}, {}", request.target, self.menu.numberOfItems(), point.x, point.y);
        let picked = self.menu.popUpMenuPositioningItem_atLocation_inView(None, point, view.as_deref());
        log::debug!("Context menu tracking finished (picked={picked})");
        let events = self.events.try_borrow().map_err(|_| "Context menu event queue busy")?;
        while let Ok(event) = events.try_recv() {
            if let Some((_, action)) = actions.iter().find(|(id, _)| *id == event.id) {
                return Ok(Some(Selection { action: action.clone(), modifiers: event.modifiers }));
            }
        }
        Ok(None)
    }
}

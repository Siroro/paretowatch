//! Small shared toolbar primitives: popover buttons that keep interactive
//! widgets open while they are used, and the option rows and single-choice
//! lists that go inside them.
//!
//! The popovers deliberately differ from `menu_button` menus: they close only
//! on an outside click (or Escape), so drag values, checkboxes, search fields,
//! and chip grids can be edited in place without the panel vanishing mid-drag.

use eframe::egui;

/// A toolbar button that toggles a small popover panel below itself.
///
/// Returns the body's return value while the popover is open, `None` while it
/// is closed — the body still runs every frame the popover is visible, so it
/// can mutate app state directly.
pub(super) fn popover_button<R>(
    ui: &mut egui::Ui,
    id: &str,
    label: impl Into<egui::WidgetText>,
    hover: Option<&str>,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    ui.push_id(id, |ui| {
        let response = ui.add(egui::Button::new(label));
        let response = match hover {
            Some(text) => response.on_hover_text(text),
            None => response,
        };
        // The popup lays widgets out left-to-right by default; wrap in a
        // vertical so stacked rows behave inside it.
        egui::Popup::from_toggle_button_response(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| ui.vertical(body))
            .map(|inner| inner.inner.inner)
    })
    .inner
}

/// Mutually exclusive option buttons laid out in one row — a segmented
/// control. Used inside popovers instead of `ComboBox` on purpose: a combo's
/// dropdown lives in a layer above the popover, so clicking an option in it
/// registers as a click outside the popover and would close the whole panel.
///
/// Returns `true` when the selection changed this frame.
pub(super) fn option_row<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: &mut T,
    options: &[(T, &str)],
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (value, label) in options {
            changed |= ui.selectable_value(current, *value, *label).changed();
        }
    });
    changed
}

/// Single-choice list for popovers (scaffold, creator pickers): one
/// full-width selectable row per option, so long names stay readable inside
/// a bounded scroll area.
///
/// Returns `true` when the selection changed this frame.
pub(super) fn radio_list<T: PartialEq + Clone>(
    ui: &mut egui::Ui,
    current: &mut T,
    options: &[(T, String)],
) -> bool {
    let mut changed = false;
    for (value, label) in options {
        changed |= ui
            .selectable_value(current, value.clone(), label.as_str())
            .changed();
    }
    changed
}

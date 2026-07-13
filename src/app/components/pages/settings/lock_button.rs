//! Shared lock-button wiring for settings widgets (EQ, pan, pitch).
//!
//! Each DSP widget has an identical lock toggle: while active, the associated
//! control is insensitive; while inactive, the control is editable. The button
//! icon switches between padlock-open and padlock-closed.

use gtk::prelude::*;

/// Icon shown on the lock button when the control is editable (unlocked).
const ICON_UNLOCKED: &str = "changes-allow-symbolic";
/// Icon shown on the lock button when the control is locked.
const ICON_LOCKED: &str = "changes-prevent-symbolic";

/// Wire a [`gtk::ToggleButton`] as a lock that controls the sensitivity of
/// `target`. While the button is active (locked), `target` is insensitive.
///
/// Also updates the button's icon to reflect the lock state, and sets the
/// initial state to locked.
pub fn setup_lock(lock: &gtk::ToggleButton, target: &impl IsA<gtk::Widget>) {
    // The target widget is only editable while unlocked (active = false).
    lock.bind_property("active", target, "sensitive")
        .invert_boolean()
        .sync_create()
        .build();

    // Reflect the lock state in the button icon.
    lock.connect_active_notify(|lock| {
        lock.set_icon_name(if lock.is_active() {
            ICON_LOCKED
        } else {
            ICON_UNLOCKED
        });
    });

    // Start locked.
    lock.set_active(true);
}

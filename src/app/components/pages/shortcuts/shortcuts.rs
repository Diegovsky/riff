pub fn make_shortcuts_dialog() -> libadwaita::ShortcutsDialog {
    gtk::Builder::from_resource("/dev/diegovsky/Riff/components/shortcuts.ui")
        .object("shortcuts_dialog")
        .unwrap()
}

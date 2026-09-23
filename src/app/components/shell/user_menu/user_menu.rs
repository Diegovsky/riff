use gio::{prelude::ActionMapExt, SimpleAction, SimpleActionGroup};
use gtk::prelude::*;
use libadwaita::prelude::AdwDialogExt;
use std::rc::Rc;

use super::UserMenuModel;
use crate::app::components::{EventListener, Settings};
use crate::app::{state::LoginEvent, AppEvent};

pub struct UserMenu {
    model: Rc<UserMenuModel>,
}

impl UserMenu {
    pub fn new(
        user_button: gtk::MenuButton,
        main_menu: gio::Menu,
        settings: Settings,
        about: libadwaita::AboutDialog,
        shortcuts_dialog: libadwaita::ShortcutsDialog,
        parent: libadwaita::ApplicationWindow,
        model: UserMenuModel,
    ) -> Self {
        let model = Rc::new(model);

        let action_group = SimpleActionGroup::new();

        action_group.add_action(&{
            let logout = SimpleAction::new("logout", None);
            logout.connect_activate(clone!(
                #[weak]
                model,
                move |_, _| {
                    model.logout();
                }
            ));
            logout
        });

        parent.add_action(&{
            let preferences_action = SimpleAction::new("preferences", None);
            preferences_action.connect_activate(move |_, _| {
                settings.show_self();
            });
            preferences_action
        });

        parent.add_action(&{
            let show_shortcuts_action = SimpleAction::new("show-shortcuts", None);
            show_shortcuts_action.connect_activate(clone!(
                #[weak]
                shortcuts_dialog,
                #[weak]
                parent,
                move |_, _| {
                    shortcuts_dialog.present(Some(&parent));
                }
            ));
            show_shortcuts_action
        });

        action_group.add_action(&{
            let about_action = SimpleAction::new("about", None);
            about_action.connect_activate(clone!(
                #[weak]
                about,
                #[weak]
                parent,
                move |_, _| {
                    about.present(Some(&parent));
                }
            ));
            about_action
        });

        action_group.add_action(&{
            let report_issue_action = SimpleAction::new("report-issue", None);
            report_issue_action.connect_activate(move |_, _| {
                crate::app::components::report_issue();
            });
            report_issue_action
        });

        user_button.insert_action_group("menu", Some(&action_group));
        user_button.set_menu_model(Some(&main_menu));

        Self { model }
    }
}

impl EventListener for UserMenu {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::LoginEvent(LoginEvent::LoginCompleted) | AppEvent::Started = event {
            self.model.fetch_user_playlists();
        }
    }
}

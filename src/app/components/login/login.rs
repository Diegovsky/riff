use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::rc::Rc;

use crate::app::components::EventListener;
use crate::app::credentials::Credentials;
use crate::app::state::{LoginCompletedEvent, LoginEvent};
use crate::app::AppEvent;

use super::LoginModel;
mod imp {

    use libadwaita::subclass::prelude::AdwWindowImpl;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/alextren/Spot/components/login.ui")]
    pub struct LoginWindow {
        #[template_child]
        pub login_with_spotify_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub auth_error_container: TemplateChild<gtk::Revealer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginWindow {
        const NAME: &'static str = "LoginWindow";
        type Type = super::LoginWindow;
        type ParentType = libadwaita::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginWindow {}
    impl WidgetImpl for LoginWindow {}
    impl AdwWindowImpl for LoginWindow {}
    impl WindowImpl for LoginWindow {}
}

glib::wrapper! {
    pub struct LoginWindow(ObjectSubclass<imp::LoginWindow>) @extends gtk::Widget, libadwaita::Window;
}

impl Default for LoginWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginWindow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn connect_close<F>(&self, on_close: F)
    where
        F: Fn() + 'static,
    {
        let window = self.upcast_ref::<libadwaita::Window>();
        window.connect_close_request(move |_| {
            on_close();
            gtk::Inhibit(false)
        });
    }

    fn connect_login_oauth_spotify<F>(&self, on_login_with_spotify_button: F)
    where
        F: Fn() + 'static,
    {
        self.imp().login_with_spotify_button.connect_clicked(
            clone!(@weak self as _self => move |_| {
                on_login_with_spotify_button()
            }),
        );
    }

    fn show_auth_error(&self, shown: bool) {
        let widget = self.imp();
        widget.auth_error_container.set_reveal_child(shown);
    }
}

pub struct Login {
    parent: gtk::Window,
    login_window: LoginWindow,
    model: Rc<LoginModel>,
}

impl Login {
    pub fn new(parent: gtk::Window, model: LoginModel) -> Self {
        let model = Rc::new(model);

        let login_window = LoginWindow::new();

        login_window.connect_close(clone!(@weak parent => move || {
            if let Some(app) = parent.application().as_ref() {
                app.quit();
            }
        }));

        login_window.connect_login_oauth_spotify(clone!(@weak model => move || {
            model.login_with_spotify();
        }));

        Self {
            parent,
            login_window,
            model,
        }
    }

    fn window(&self) -> &libadwaita::Window {
        self.login_window.upcast_ref::<libadwaita::Window>()
    }

    fn show_self(&self) {
        self.window().set_transient_for(Some(&self.parent));
        self.window().set_modal(true);
        self.window().set_visible(true);
    }

    fn hide_and_save_creds(&self, credentials: Credentials) {
        self.window().set_visible(false);
        self.model.save_for_autologin(credentials);
    }

    fn reveal_error(&self) {
        self.login_window.show_auth_error(true);
    }
}

impl EventListener for Login {
    fn on_event(&mut self, event: &AppEvent) {
        info!("received login event {:?}", event);
        match event {
            AppEvent::LoginEvent(LoginEvent::LoginCompleted(LoginCompletedEvent::Password(
                creds,
            ))) => {
                self.hide_and_save_creds(creds.clone());
            }
            AppEvent::LoginEvent(LoginEvent::LoginCompleted(LoginCompletedEvent::Token(token))) => {
                self.hide_and_save_creds(token.clone());
            }
            AppEvent::LoginEvent(LoginEvent::LoginFailed) => {
                self.model.clear_saved_credentials();
                self.reveal_error();
            }
            AppEvent::Started => {
                self.model.try_autologin();
            }
            AppEvent::LoginEvent(LoginEvent::LogoutCompleted | LoginEvent::LoginShown) => {
                self.show_self();
            }
            AppEvent::LoginEvent(LoginEvent::RefreshTokenCompleted {
                token,
                token_expiry_time,
            }) => {
                self.model.save_token(token.clone(), *token_expiry_time);
            }
            _ => {}
        }
    }
}

use std::time::SystemTime;

use gettextrs::*;

use crate::app::credentials::Credentials;
use crate::app::state::{LoginAction, TryLoginAction};
use crate::app::{ActionDispatcher, AppAction, Worker};

pub struct LoginModel {
    dispatcher: Box<dyn ActionDispatcher>,
    worker: Worker,
}

impl LoginModel {
    pub fn new(dispatcher: Box<dyn ActionDispatcher>, worker: Worker) -> Self {
        Self { dispatcher, worker }
    }

    pub fn try_autologin(&self) {
        self.dispatcher.dispatch_async(Box::pin(async {
            let action = match Credentials::retrieve().await {
                Ok(creds) => LoginAction::TryLogin(if creds.token_expired() {
                    TryLoginAction::Password {
                        username: creds.username,
                        password: creds.password,
                    }
                } else {
                    TryLoginAction::Token {
                        username: creds.username,
                        token: creds.token,
                    }
                }),
                Err(err) => {
                    warn!("Could not retrieve credentials: {}", err);
                    LoginAction::ShowLogin
                }
            };
            Some(action.into())
        }));
    }

    pub fn clear_saved_credentials(&self) {
        self.worker.send_task(async {
            let _ = Credentials::logout().await;
        });
    }

    pub fn save_token(&self, token: String, token_expiry_time: SystemTime) {
        self.worker.send_task(async move {
            if let Ok(mut credentials) = Credentials::retrieve().await {
                credentials.token = token;
                credentials.token_expiry_time = Some(token_expiry_time);
                if let Err(err) = credentials.save().await {
                    warn!("Could not save credentials: {}", err);
                }
            }
        });
    }

    pub fn save_for_autologin(&self, credentials: Credentials) {
        self.dispatcher.dispatch_async(Box::pin(async move {
            let Err(err) = credentials.save().await else {
                return None;
            };
            warn!("Could not save credentials: {}", err);
            Some(AppAction::ShowNotification(gettext(
                // translators: This notification shows up right after login if the password could not be stored in the keyring (that is, GNOME's keyring aka seahorse, or any other libsecret compliant secret store).
                "Could not save password. Make sure the session keyring is unlocked.",
            )))
        }));
    }

    pub fn login(&self, username: String, password: String) {
        self.dispatcher
            .dispatch(LoginAction::TryLogin(TryLoginAction::Password { username, password }).into())
    }

    pub fn login_with_spotify(&self) {
        self.dispatcher
            .dispatch(LoginAction::TryLogin(TryLoginAction::OAuthSpotify {}).into())
    }
}

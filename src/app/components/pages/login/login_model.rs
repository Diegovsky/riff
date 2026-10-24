use crate::app::state::{LoginAction, TryLoginAction};
use crate::app::Dispatcher;

pub struct LoginModel {
    dispatcher: Dispatcher,
}

impl LoginModel {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self { dispatcher }
    }

    pub fn try_autologin(&self) {
        self.dispatcher
            .dispatch(LoginAction::TryLogin(TryLoginAction::Restore).into());
    }

    pub fn login_with_spotify(&self) {
        self.dispatcher
            .dispatch(LoginAction::TryLogin(TryLoginAction::InitLogin).into())
    }
}

//! The action/event loop.
//!
//! Only responsible for getting [`AppAction`]s to the one place that applies
//! them. Concurrency, priority, and mutation ordering belong to the api layer.

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;

use super::AppAction;

/// A thin, cloneable wrapper around the action channel.
#[derive(Clone)]
pub struct Dispatcher {
    sender: UnboundedSender<AppAction>,
}

impl Dispatcher {
    pub fn new(sender: UnboundedSender<AppAction>) -> Self {
        Self { sender }
    }

    pub fn dispatch(&self, action: AppAction) {
        self.send(action);
    }

    pub fn dispatch_many(&self, actions: Vec<AppAction>) {
        for action in actions.into_iter() {
            self.send(action);
        }
    }

    /// A closed channel means the app is tearing down.
    fn send(&self, action: AppAction) {
        if self.sender.unbounded_send(action).is_err() {
            debug!("dispatch: loop is gone, dropping action");
        }
    }
}

// Funky name for a mere wrapper around an MPSC send/recv pair
pub struct DispatchLoop {
    receiver: UnboundedReceiver<AppAction>,
    sender: UnboundedSender<AppAction>,
}

impl DispatchLoop {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded::<AppAction>();
        Self { receiver, sender }
    }

    pub fn make_dispatcher(&self) -> UnboundedSender<AppAction> {
        self.sender.clone()
    }

    pub async fn attach(self, mut handler: impl FnMut(AppAction)) {
        self.receiver
            .for_each(|action| {
                handler(action);
                async {}
            })
            .await;
    }
}

use crate::app::state::*;
use ref_filter_map::*;
use riff_api::ApiService;
use std::cell::{Ref, RefCell};
use std::sync::Arc;

// Two purposes: give access to some services to users of the AppModel (shared)
// and give a read only view of the state
pub struct AppModel {
    state: RefCell<AppState>,
    api_service: Arc<ApiService>,
}

impl AppModel {
    pub fn new(state: AppState, api_service: Arc<ApiService>) -> Self {
        let state = RefCell::new(state);
        Self { state, api_service }
    }

    pub fn api(&self) -> Arc<ApiService> {
        Arc::clone(&self.api_service)
    }

    // Read only access to the state!
    pub fn get_state(&self) -> Ref<'_, AppState> {
        self.state.borrow()
    }

    // Convenience...
    pub fn map_state<T: 'static, F: FnOnce(&AppState) -> &T>(&self, map: F) -> Ref<'_, T> {
        Ref::map(self.state.borrow(), map)
    }

    // Convenience...
    pub fn map_state_opt<T: 'static, F: FnOnce(&AppState) -> Option<&T>>(
        &self,
        map: F,
    ) -> Option<Ref<'_, T>> {
        ref_filter_map(self.state.borrow(), map)
    }

    pub fn update_state(&self, action: AppAction) -> Vec<AppEvent> {
        // And this is the only mutable borrow of our state!
        let mut state = self.state.borrow_mut();
        state.update_state(action)
    }
}

pub trait ProvidesApi {
    fn api_service(&self) -> Arc<ApiService>;
}

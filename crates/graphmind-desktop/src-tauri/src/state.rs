use std::collections::HashMap;

#[derive(Default)]
pub struct AppState {
    pub watchers: HashMap<String, WatcherHandle>,
}

pub struct WatcherHandle {
    pub _stop_tx: std::sync::mpsc::Sender<()>,
}

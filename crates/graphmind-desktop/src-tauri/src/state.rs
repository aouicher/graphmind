use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[derive(Default)]
pub struct AppState {
    pub watchers: HashMap<String, WatcherHandle>,
    pub cancel_flags: HashMap<String, Arc<AtomicBool>>,
}

pub struct WatcherHandle {
    pub _stop_tx: std::sync::mpsc::Sender<()>,
}

//! Save storage (native only): filesystem-backed.

use std::sync::Mutex;
use std::sync::OnceLock;

pub struct FsSaveBackend;

impl FsSaveBackend {
    const PATH: &'static str = "save/player.ron";

    pub fn read(&self) -> Option<String> {
        std::fs::read_to_string(Self::PATH).ok()
    }

    pub fn write(&self, data: &str) {
        if let Some(parent) = std::path::Path::new(Self::PATH).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(Self::PATH, data) {
            log::warn!("save: could not write {}: {e}", Self::PATH);
        }
    }
}

pub fn create_save_backend() -> FsSaveBackend {
    FsSaveBackend
}

static SAVE_BACKEND: OnceLock<Mutex<Option<FsSaveBackend>>> = OnceLock::new();

pub fn init_save_backend() {
    let backend = create_save_backend();
    let mutex = Mutex::new(Some(backend));
    SAVE_BACKEND.set(mutex).ok();
}

pub fn read_save() -> Option<String> {
    let guard = SAVE_BACKEND.get()?;
    let lock = guard.lock().ok()?;
    lock.as_ref().and_then(|b| b.read())
}

pub fn write_save(data: &str) {
    if let Some(guard) = SAVE_BACKEND.get() {
        if let Ok(lock) = guard.lock() {
            if let Some(b) = lock.as_ref() {
                b.write(data);
            }
        }
    }
}

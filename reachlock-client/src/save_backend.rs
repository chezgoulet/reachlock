//! Save storage backends (S24): filesystem for native, localStorage for WASM.
//! The trait wraps the save path so the web build compiles without `std::fs`.

use std::sync::Mutex;
use std::sync::OnceLock;

/// Platform-independent save storage. Native uses filesystem; WASM uses
/// localStorage. Both backends store the same RON format — only the
/// transport differs.
pub trait SaveBackend: Send + Sync {
    fn read(&self) -> Option<String>;
    fn write(&self, data: &str);
}

// -----------------------------------------------------------------------
// Native: filesystem-backed
// -----------------------------------------------------------------------

pub struct FsSaveBackend;

impl FsSaveBackend {
    const PATH: &'static str = "save/player.ron";
}

impl SaveBackend for FsSaveBackend {
    fn read(&self) -> Option<String> {
        std::fs::read_to_string(Self::PATH).ok()
    }

    fn write(&self, data: &str) {
        if let Some(parent) = std::path::Path::new(Self::PATH).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(Self::PATH, data) {
            log::warn!("save: could not write {}: {e}", Self::PATH);
        }
    }
}

// -----------------------------------------------------------------------
// WASM: localStorage-backed
// -----------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
#[allow(unused_imports)]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = localStorage)]
    fn getItem(key: &str) -> Option<String>;
    #[wasm_bindgen(js_namespace = localStorage)]
    fn setItem(key: &str, value: &str);
}

#[cfg(target_arch = "wasm32")]
pub struct LocalStorageSaveBackend;

#[cfg(target_arch = "wasm32")]
impl SaveBackend for LocalStorageSaveBackend {
    fn read(&self) -> Option<String> {
        getItem("reachlock_save")
    }

    fn write(&self, data: &str) {
        setItem("reachlock_save", data);
    }
}

// -----------------------------------------------------------------------
// Factory
// -----------------------------------------------------------------------

pub fn create_save_backend() -> Box<dyn SaveBackend> {
    #[cfg(target_arch = "wasm32")]
    {
        Box::new(LocalStorageSaveBackend)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(FsSaveBackend)
    }
}

/// Global save backend, initialized once at startup. Used by inventory.rs's
/// load/save systems so they don't need to carry the backend as a parameter.
static SAVE_BACKEND: OnceLock<Mutex<Option<Box<dyn SaveBackend>>>> = OnceLock::new();

/// Initialize the global save backend. Must be called once during startup.
pub fn init_save_backend() {
    let backend = create_save_backend();
    let mutex = Mutex::new(Some(backend));
    SAVE_BACKEND.set(mutex).ok();
}

/// Read from the global save backend.
pub fn read_save() -> Option<String> {
    let guard = SAVE_BACKEND.get()?;
    let lock = guard.lock().ok()?;
    lock.as_ref().and_then(|b| b.read())
}

/// Write to the global save backend.
pub fn write_save(data: &str) {
    if let Some(guard) = SAVE_BACKEND.get() {
        if let Ok(lock) = guard.lock() {
            if let Some(b) = lock.as_ref() {
                b.write(data);
            }
        }
    }
}

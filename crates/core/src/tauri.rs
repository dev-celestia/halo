//! In-process stand-in for Tauri's `AppHandle` / `State` / `Emitter` / `async_runtime`.
//! Lets reference proxy + command code compile without a webview.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;

type EmitFn = std::sync::Arc<dyn Fn(&str, &serde_json::Value) + Send + Sync>;

#[derive(Clone)]
pub struct AppHandle {
    inner: Arc<AppInner>,
}

/// Non-owning callbacks must not keep managed subprocess handles alive forever.
#[derive(Clone)]
pub struct WeakAppHandle {
    inner: std::sync::Weak<AppInner>,
}

impl WeakAppHandle {
    pub fn upgrade(&self) -> Option<AppHandle> {
        self.inner.upgrade().map(|inner| AppHandle { inner })
    }
}

struct AppInner {
    state: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    emitters: Mutex<Vec<EmitFn>>,
}

impl AppHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AppInner {
                state: RwLock::new(HashMap::new()),
                emitters: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn downgrade(&self) -> WeakAppHandle {
        WeakAppHandle {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Register managed state (Tauri `app.manage(…)`).
    pub fn manage<T: Send + Sync + 'static>(&self, value: T) {
        self.inner
            .state
            .write()
            .expect("state lock")
            .insert(TypeId::of::<T>(), Arc::new(value));
    }

    /// Subscribe to backend events (replaces frontend `listen`).
    pub fn on_event<F>(&self, f: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        self.inner
            .emitters
            .lock()
            .expect("emitters lock")
            .push(std::sync::Arc::new(f));
    }
}

impl Default for AppHandle {
    fn default() -> Self {
        Self::new()
    }
}

pub trait Manager {
    fn state<T: Send + Sync + 'static>(&self) -> State<'_, T>;
    fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'_, T>>;
}

impl Manager for AppHandle {
    fn state<T: Send + Sync + 'static>(&self) -> State<'_, T> {
        self.try_state::<T>()
            .unwrap_or_else(|| panic!("missing managed state {}", std::any::type_name::<T>()))
    }

    fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'_, T>> {
        let map = self.inner.state.read().ok()?;
        let arc = map.get(&TypeId::of::<T>())?.clone();
        let typed = arc.downcast::<T>().ok()?;
        Some(State {
            inner: typed,
            _phantom: std::marker::PhantomData,
        })
    }
}

pub struct State<'r, T: Send + Sync + 'static> {
    inner: Arc<T>,
    _phantom: std::marker::PhantomData<&'r ()>,
}

impl<T: Send + Sync + 'static> Clone for State<'_, T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static> State<'_, T> {
    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn arc(&self) -> Arc<T> {
        Arc::clone(&self.inner)
    }
}

impl<T: Send + Sync + 'static> Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub trait Emitter<S: Serialize> {
    fn emit<E: AsRef<str>>(&self, event: E, payload: S) -> Result<(), String>;
}

impl<S: Serialize> Emitter<S> for AppHandle {
    fn emit<E: AsRef<str>>(&self, event: E, payload: S) -> Result<(), String> {
        let value = serde_json::to_value(&payload).map_err(|e| e.to_string())?;
        let event = event.as_ref();
        // Drop the subscriber list before callbacks. Agent ingress can block
        // on backpressure, and that must not stall other emitters or the UI.
        let emitters = self
            .inner
            .emitters
            .lock()
            .map(|emitters| emitters.clone())
            .unwrap_or_default();
        for emit in emitters {
            emit(event, &value);
        }
        Ok(())
    }
}

pub mod async_runtime {
    use std::future::Future;

    pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(future)
    }

    pub async fn spawn_blocking<F, R>(f: F) -> Result<R, tokio::task::JoinError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        tokio::task::spawn_blocking(f).await
    }
}

/// No-op attribute stand-in for `#[crate::tauri::command]`.
#[macro_export]
macro_rules! __noop_attr {
    ($($tt:tt)*) => {};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn weak_callback_does_not_retain_managed_resources() {
        struct Resource(Arc<AtomicBool>);
        impl Drop for Resource {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let app = AppHandle::new();
        app.manage(Resource(dropped.clone()));
        let weak = app.downgrade();
        assert!(weak.upgrade().is_some());
        drop(app);
        assert!(weak.upgrade().is_none());
        assert!(dropped.load(Ordering::SeqCst));
    }
}

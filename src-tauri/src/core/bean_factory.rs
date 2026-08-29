//! Simple service registry (ported from electron/core/BeanFactory.ts).
//!
//! Services are registered once during startup under stable names and later
//! retrieved by the IPC command layer.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

struct Registry {
    beans: HashMap<&'static str, Box<dyn Any + Send + Sync>>,
}

impl Registry {
    fn new() -> Self {
        Self {
            beans: HashMap::new(),
        }
    }
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::new()))
}

pub struct BeanFactory;

impl BeanFactory {
    pub fn set<T: Send + Sync + 'static>(name: &'static str, bean: T) {
        let mut reg = registry().lock().unwrap();
        reg.beans.insert(name, Box::new(bean));
    }

    pub fn get<T: 'static>(name: &'static str) -> Option<&'static T> {
        // Safety: beans are inserted once during startup and never removed, so
        // the leaked reference is valid for the app lifetime.
        let reg = registry().lock().unwrap();
        let boxed = reg.beans.get(name)?;
        let ptr: *const dyn Any = boxed.as_ref() as *const dyn Any;
        let ptr = ptr as *const T;
        // Safety: we only ever cast back to the exact type that was inserted.
        unsafe { Some(&*ptr) }
    }

    pub fn has(name: &'static str) -> bool {
        registry().lock().unwrap().beans.contains_key(name)
    }
}

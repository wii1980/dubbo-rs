use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

type Factory<T> = Box<dyn Fn() -> Box<T> + Send + Sync>;

pub struct ExtensionRegistry<T: 'static + ?Sized> {
    factories: RwLock<HashMap<String, Factory<T>>>,
    instances: RwLock<HashMap<String, Box<T>>>,
    default_name: RwLock<Option<String>>,
}

impl<T: 'static + ?Sized> ExtensionRegistry<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            factories: RwLock::new(HashMap::new()),
            instances: RwLock::new(HashMap::new()),
            default_name: RwLock::new(None),
        }
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn register_factory(
        &self,
        name: impl Into<String>,
        factory: impl Fn() -> Box<T> + Send + Sync + 'static,
    ) {
        let name = name.into();
        self.factories
            .write()
            .unwrap()
            .insert(name, Box::new(factory));
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn get_or_create_extension(&self, name: &str) -> Option<()> {
        // Check if already instantiated
        {
            let instances = self.instances.read().unwrap();
            if instances.contains_key(name) {
                return Some(());
            }
        }

        // Create new instance via factory
        let instance = {
            let factories = self.factories.read().unwrap();
            factories.get(name).map(|f| f())?
        };

        self.instances
            .write()
            .unwrap()
            .insert(name.to_string(), instance);
        Some(())
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn get_default_extension(&self) -> Option<()> {
        let name = self.default_name.read().unwrap().clone();
        name.as_deref()
            .and_then(|name| self.get_or_create_extension(name))
            .or_else(|| {
                let next = self.factories.read().unwrap().keys().next().cloned();
                next.and_then(|name| self.get_or_create_extension(&name))
            })
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn set_default(&self, name: impl Into<String>) {
        *self.default_name.write().unwrap() = Some(name.into());
    }

    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn has_extension(&self, name: &str) -> bool {
        self.factories.read().unwrap().contains_key(name)
    }

    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn available_extensions(&self) -> Vec<String> {
        self.factories.read().unwrap().keys().cloned().collect()
    }
}

impl<T: 'static + ?Sized> Default for ExtensionRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static + ?Sized> fmt::Debug for ExtensionRegistry<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionRegistry")
            .field("available", &self.available_extensions())
            .field("default", &self.default_name.read().unwrap())
            .finish_non_exhaustive()
    }
}

#[macro_export]
macro_rules! extension_register {
    ($registry:expr, $name:expr, $ty:ty, $factory:expr) => {
        inventory::submit! {
            $crate::extension::ExtensionRegistration {
                registry: $registry,
                name: $name,
                factory: $factory,
            }
        }
    };
}

pub struct ExtensionRegistration<F> {
    pub registry: &'static str,
    pub name: &'static str,
    pub factory: F,
}

#[cfg(test)]
mod tests {
    use super::*;

    trait Greeter: Send + Sync {
        #[allow(dead_code)]
        fn greet(&self) -> String;
    }

    struct HelloGreeter;
    impl Greeter for HelloGreeter {
        fn greet(&self) -> String {
            "Hello".into()
        }
    }

    #[test]
    fn test_register_and_has_extension() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("hello", || Box::new(HelloGreeter));
        assert!(registry.has_extension("hello"));
        assert!(!registry.has_extension("nonexistent"));
        assert_eq!(registry.available_extensions(), vec!["hello"]);
    }

    #[test]
    fn test_get_or_create_extension() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("hello", || Box::new(HelloGreeter));
        assert!(registry.get_or_create_extension("hello").is_some());
        assert!(registry.get_or_create_extension("nonexistent").is_none());
    }

    #[test]
    fn test_default_extension_set_and_get() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("hello", || Box::new(HelloGreeter));
        registry.register_factory("hi", || Box::new(HelloGreeter));
        registry.set_default("hello");

        let available = registry.available_extensions();
        assert_eq!(available.len(), 2);
        assert!(available.contains(&"hello".to_string()));
        assert!(available.contains(&"hi".to_string()));
        assert!(registry.get_default_extension().is_some());
    }

    #[test]
    fn test_empty_registry() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        assert!(registry.available_extensions().is_empty());
        assert!(registry.get_default_extension().is_none());
    }

    #[test]
    fn test_get_or_create_extension_idempotent() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("hello", || Box::new(HelloGreeter));

        let first = registry.get_or_create_extension("hello");
        let second = registry.get_or_create_extension("hello");
        assert!(first.is_some());
        assert!(second.is_some());
    }

    #[test]
    fn test_set_default_nonexistent_name() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("hello", || Box::new(HelloGreeter));
        registry.set_default("nonexistent");

        assert!(registry.get_default_extension().is_some());

        let empty_registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        empty_registry.set_default("ghost");
        assert!(empty_registry.get_default_extension().is_none());
    }

    #[test]
    fn test_default_extension_fallback() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("alpha", || Box::new(HelloGreeter));
        registry.register_factory("beta", || Box::new(HelloGreeter));
        assert!(registry.get_default_extension().is_some());
    }

    #[test]
    fn test_debug_format() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("hello", || Box::new(HelloGreeter));
        registry.register_factory("hi", || Box::new(HelloGreeter));
        registry.set_default("hello");

        let debug = format!("{registry:?}");
        assert!(debug.contains("ExtensionRegistry"));
        assert!(debug.contains("hello"));
        assert!(debug.contains("hi"));
    }

    #[test]
    fn test_available_extensions_after_registration() {
        let registry: ExtensionRegistry<dyn Greeter> = ExtensionRegistry::new();
        registry.register_factory("alpha", || Box::new(HelloGreeter));
        registry.register_factory("bravo", || Box::new(HelloGreeter));
        registry.register_factory("charlie", || Box::new(HelloGreeter));

        let mut names = registry.available_extensions();
        names.sort();
        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
    }
}

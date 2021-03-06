use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;

use crate::context::{Build, CloneSafe};
use crate::error::{Result, TensorNodeError};
use crate::nodes::NodeRoot;

pub struct NodeCache<T: Build> {
    paths: RefCell<HashMap<String, String>>,
    caches_source: RefCell<HashMap<String, String>>,
    caches: RefCell<HashMap<String, T::Output>>,
}

impl<T: Build> NodeCache<T> {
    pub fn new(caches: HashMap<String, String>) -> Self {
        Self {
            paths: RefCell::default(),
            caches_source: RefCell::new(caches),
            caches: RefCell::default(),
        }
    }

    pub fn add_source(&self, name: String, source: String) {
        self.caches_source.borrow_mut().insert(name, source);
    }

    pub fn add_path(&self, name: String, path: String) {
        self.paths.borrow_mut().insert(name, path);
    }

    pub fn get(&self, name: &str, root: &NodeRoot) -> Result<T::Output> {
        if let Some(cache) = self.caches.borrow().get(name) {
            let mut variables = vec![];
            return Ok(cache.clone_safe(&root.seed, &mut variables));
        }

        let path = self.paths.borrow_mut().remove(name);
        if let Some(path) = path {
            let source = fs::read_to_string(path)?;
            return self.build_and_store(name, root, source);
        }

        let source = self.caches_source.borrow_mut().remove(name);
        if let Some(source) = source {
            return self.build_and_store(name, root, source);
        }

        TensorNodeError::NoSuchNode {
            name: name.to_string(),
        }
        .into()
    }

    fn build_and_store(&self, name: &str, root: &NodeRoot, source: String) -> Result<T::Output> {
        // TODO: detect cycling
        let result = T::build(root, name, source)?;

        let mut variables = vec![];
        let cloned = result.clone_safe(&root.seed, &mut variables);

        self.caches.borrow_mut().insert(name.to_string(), result);
        Ok(cloned)
    }
}

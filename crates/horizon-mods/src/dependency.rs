//! Mod dependency resolution

use crate::container::{ModContainer, ModDependency};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub enum DependencyError {
    Missing(String),
    Circular(String),
    VersionMismatch { mod_id: String, required: String, found: String },
}

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyError::Missing(id) => write!(f, "Missing required dependency: {}", id),
            DependencyError::Circular(id) => write!(f, "Circular dependency detected: {}", id),
            DependencyError::VersionMismatch { mod_id, required, found } => {
                write!(f, "Version mismatch for {}: requires {}, found {}", mod_id, required, found)
            }
        }
    }
}

impl std::error::Error for DependencyError {}

pub struct DependencyResolver {
    containers: HashMap<String, ModContainer>,
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self {
            containers: HashMap::new(),
        }
    }

    pub fn add_container(&mut self, container: ModContainer) {
        self.containers.insert(container.id.clone(), container);
    }

    /// Resolve dependencies and return load order (topological sort)
    pub fn resolve(&self) -> Result<Vec<String>, DependencyError> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        for id in self.containers.keys() {
            if !visited.contains(id) {
                self.visit(id, &mut visited, &mut visiting, &mut order)?;
            }
        }

        Ok(order)
    }

    fn visit(
        &self,
        id: &str,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), DependencyError> {
        if visited.contains(id) {
            return Ok(());
        }
        if visiting.contains(id) {
            return Err(DependencyError::Circular(id.to_string()));
        }

        visiting.insert(id.to_string());

        if let Some(container) = self.containers.get(id) {
            for dep in &container.dependencies {
                if dep.required {
                    if !self.containers.contains_key(&dep.mod_id) {
                        return Err(DependencyError::Missing(dep.mod_id.clone()));
                    }
                    self.visit(&dep.mod_id, visited, visiting, order)?;
                }
            }
        } else {
            return Err(DependencyError::Missing(id.to_string()));
        }

        visiting.remove(id);
        visited.insert(id.to_string());
        order.push(id.to_string());

        Ok(())
    }

    /// Check if all dependencies are satisfied
    pub fn check_dependencies(&self, mod_id: &str) -> Result<(), DependencyError> {
        if let Some(container) = self.containers.get(mod_id) {
            for dep in &container.dependencies {
                if dep.required && !self.containers.contains_key(&dep.mod_id) {
                    return Err(DependencyError::Missing(dep.mod_id.clone()));
                }
            }
        }
        Ok(())
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

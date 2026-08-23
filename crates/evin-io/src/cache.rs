// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use crate::error::IoError;
use crate::source::SourceFile;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct SourceCache {
    inner: Arc<RwLock<HashMap<String, Arc<SourceFile>>>>,
}

impl SourceCache {
    pub fn load(&self, path: &str) -> Result<Arc<SourceFile>, IoError> {
        if let Some(cached) = self.get(path) {
            return Ok(cached);
        }

        let content = std::fs::read_to_string(path)?;
        Ok(self.insert(path, content))
    }

    pub fn insert(&self, name: impl Into<String>, text: impl Into<String>) -> Arc<SourceFile> {
        let name = name.into();
        let source = Arc::new(SourceFile::new(name.clone(), text.into()));
        self.inner.write().insert(name, source.clone());
        source
    }

    pub fn get(&self, name: &str) -> Option<Arc<SourceFile>> {
        self.inner.read().get(name).cloned()
    }

    pub fn evict(&self, name: &str) {
        self.inner.write().remove(name);
    }
}

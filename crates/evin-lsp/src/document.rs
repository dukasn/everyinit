// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use crate::SourcePositionIndex;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub struct LspDocument {
    pub uri: String,
    pub version: i32,
    pub text: String,
    pub index: SourcePositionIndex,
}
impl LspDocument {
    pub fn new(uri: String, version: i32, text: String) -> Self {
        let index = SourcePositionIndex::new(&text);
        Self {
            uri,
            version,
            text,
            index,
        }
    }
    pub fn update(&mut self, version: i32, text: String) {
        self.version = version;
        self.index = SourcePositionIndex::new(&text);
        self.text = text;
    }
}
#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: FxHashMap<String, LspDocument>,
}
impl DocumentStore {
    pub fn open(&mut self, document: LspDocument) {
        self.documents.insert(document.uri.clone(), document);
    }
    pub fn get(&self, uri: &str) -> Option<&LspDocument> {
        self.documents.get(uri)
    }
    pub fn get_mut(&mut self, uri: &str) -> Option<&mut LspDocument> {
        self.documents.get_mut(uri)
    }
    pub fn close(&mut self, uri: &str) -> Option<LspDocument> {
        self.documents.remove(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentStore, LspDocument};

    #[test]
    fn open_documents_shadow_and_close_cleanly() {
        let mut documents = DocumentStore::default();
        documents.open(LspDocument::new("file:///a.yaml".into(), 1, "one".into()));
        documents
            .get_mut("file:///a.yaml")
            .unwrap()
            .update(2, "two".into());
        assert_eq!(documents.get("file:///a.yaml").unwrap().text, "two");
        assert!(documents.close("file:///a.yaml").is_some());
        assert!(documents.get("file:///a.yaml").is_none());
    }
}

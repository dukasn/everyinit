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

use evin_release_sync::{ChangelogSlicer, NoteSlice, NotesExtractor, ProtocolError};
use std::path::Path;

pub struct FsNotesExtractor {
    slicer: ChangelogSlicer,
}

impl FsNotesExtractor {
    pub fn new(workspace_root: &Path, changelog_path: &str) -> Self {
        let full_path = workspace_root.join(changelog_path);
        let slicer = if full_path.exists() {
            let content = std::fs::read_to_string(full_path).unwrap_or_default();
            ChangelogSlicer::parse(&content)
        } else {
            ChangelogSlicer::default()
        };
        Self { slicer }
    }
}

impl NotesExtractor for FsNotesExtractor {
    fn extract_notes(&self, tag: &str) -> Result<Option<NoteSlice>, ProtocolError> {
        Ok(self.slicer.get_slice(tag).cloned())
    }
}

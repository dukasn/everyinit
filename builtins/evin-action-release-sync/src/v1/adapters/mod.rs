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

mod artifacts;
mod git;
mod notes;
mod plan;
mod policy;
mod remote_state;

pub use artifacts::FsArtifactInspector;
pub use git::CommandGitStateProvider;
pub use notes::FsNotesExtractor;
pub use plan::FsPlanSink;
pub use policy::ContextPolicyEvaluator;
pub use remote_state::FsRemoteStateSource;

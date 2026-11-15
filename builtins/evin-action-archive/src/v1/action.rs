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

#![allow(clippy::result_large_err)]
use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    RunnerContext,
};

const ARCHIVE_FIELDS: &[ActionFieldDescriptor] = &[
    ActionFieldDescriptor {
        path: "format",
        kind: ActionFieldKind::Enum(&["tar.gz", "deb", "apk", "arch"]),
        required: false,
        description: "Archive format.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "path",
        kind: ActionFieldKind::String,
        required: true,
        description: "Destination path for tar.gz archives.",
        condition: Some("format=tar.gz"),
    },
    ActionFieldDescriptor {
        path: "output_dir",
        kind: ActionFieldKind::String,
        required: true,
        description: "Distro package output directory.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "architecture",
        kind: ActionFieldKind::String,
        required: true,
        description: "Target package architecture.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "release",
        kind: ActionFieldKind::String,
        required: true,
        description: "Release JSON output from a prerequisite task.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package",
        kind: ActionFieldKind::Object,
        required: true,
        description: "Inline distro package metadata.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.name",
        kind: ActionFieldKind::String,
        required: true,
        description: "Canonical package and release-policy identity.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.summary",
        kind: ActionFieldKind::String,
        required: true,
        description: "Short package summary.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.description",
        kind: ActionFieldKind::String,
        required: false,
        description: "Long package description.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.maintainer",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package maintainer.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.license",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package license.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.homepage",
        kind: ActionFieldKind::String,
        required: false,
        description: "Package homepage.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.payload",
        kind: ActionFieldKind::List,
        required: true,
        description: "Payload mappings resolved from task cwd.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.payload.source",
        kind: ActionFieldKind::String,
        required: true,
        description: "Payload source path.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.payload.strip_prefix",
        kind: ActionFieldKind::String,
        required: true,
        description: "Source prefix removed from payload paths.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.payload.destination",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package destination path.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.payload.mode",
        kind: ActionFieldKind::Integer,
        required: false,
        description: "Normalized Unix file mode.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets",
        kind: ActionFieldKind::Object,
        required: true,
        description: "Format-specific metadata.",
        condition: Some("format=deb|apk|arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.deb",
        kind: ActionFieldKind::Object,
        required: false,
        description: "DEB-specific metadata.",
        condition: Some("format=deb"),
    },
    ActionFieldDescriptor {
        path: "package.targets.deb.depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "DEB runtime dependencies.",
        condition: Some("format=deb"),
    },
    ActionFieldDescriptor {
        path: "package.targets.deb.scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "DEB maintainer scripts.",
        condition: Some("format=deb"),
    },
    ActionFieldDescriptor {
        path: "package.targets.deb.scripts.preinst",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB pre-install script path.",
        condition: Some("format=deb"),
    },
    ActionFieldDescriptor {
        path: "package.targets.deb.scripts.postinst",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB post-install script path.",
        condition: Some("format=deb"),
    },
    ActionFieldDescriptor {
        path: "package.targets.deb.scripts.prerm",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB pre-remove script path.",
        condition: Some("format=deb"),
    },
    ActionFieldDescriptor {
        path: "package.targets.deb.scripts.postrm",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB post-remove script path.",
        condition: Some("format=deb"),
    },
    ActionFieldDescriptor {
        path: "package.targets.apk",
        kind: ActionFieldKind::Object,
        required: false,
        description: "APK-specific metadata.",
        condition: Some("format=apk"),
    },
    ActionFieldDescriptor {
        path: "package.targets.apk.depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "APK runtime dependencies.",
        condition: Some("format=apk"),
    },
    ActionFieldDescriptor {
        path: "package.targets.apk.scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "APK lifecycle scripts.",
        condition: Some("format=apk"),
    },
    ActionFieldDescriptor {
        path: "package.targets.apk.scripts.pre_install",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK pre-install script path.",
        condition: Some("format=apk"),
    },
    ActionFieldDescriptor {
        path: "package.targets.apk.scripts.post_install",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK post-install script path.",
        condition: Some("format=apk"),
    },
    ActionFieldDescriptor {
        path: "package.targets.apk.scripts.pre_deinstall",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK pre-deinstall script path.",
        condition: Some("format=apk"),
    },
    ActionFieldDescriptor {
        path: "package.targets.apk.scripts.post_deinstall",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK post-deinstall script path.",
        condition: Some("format=apk"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch",
        kind: ActionFieldKind::Object,
        required: false,
        description: "Arch-specific metadata.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch runtime dependencies.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.optdepends",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch optional dependencies.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.provides",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch provided capabilities.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.conflicts",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch conflicting packages.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.replaces",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch replaced packages.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.backup",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch backup paths.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "Arch install script metadata.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "package.targets.arch.scripts.install",
        kind: ActionFieldKind::String,
        required: false,
        description: "Arch .INSTALL script path.",
        condition: Some("format=arch"),
    },
    ActionFieldDescriptor {
        path: "include",
        kind: ActionFieldKind::List,
        required: true,
        description: "Input patterns for tar.gz archives.",
        condition: Some("format=tar.gz"),
    },
    ActionFieldDescriptor {
        path: "exclude",
        kind: ActionFieldKind::List,
        required: false,
        description: "Excluded tar.gz patterns.",
        condition: Some("format=tar.gz"),
    },
    ActionFieldDescriptor {
        path: "prefix",
        kind: ActionFieldKind::String,
        required: false,
        description: "Tar.gz path prefix.",
        condition: Some("format=tar.gz"),
    },
    ActionFieldDescriptor {
        path: "layout",
        kind: ActionFieldKind::Enum(&["flat", "relative"]),
        required: false,
        description: "Tar.gz payload layout.",
        condition: Some("format=tar.gz"),
    },
    ActionFieldDescriptor {
        path: "compression_level",
        kind: ActionFieldKind::Integer,
        required: false,
        description: "Gzip compression level.",
        condition: Some("format=tar.gz"),
    },
];
const ARCHIVE_OUTPUTS: &[ActionOutputDescriptor] = &[
    ActionOutputDescriptor {
        name: "path",
        description: "Published archive path.",
    },
    ActionOutputDescriptor {
        name: "size",
        description: "Published archive byte size.",
    },
];
const ARCHIVE_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create deterministic archives and distro packages.",
    fields: ARCHIVE_FIELDS,
    outputs: ARCHIVE_OUTPUTS,
};

pub struct ArchiveAction;

#[async_trait::async_trait]
impl Action for ArchiveAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&ARCHIVE_DESCRIPTOR)
    }
    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        super::validate_config(with)
    }

    fn validate_task_config(
        &self,
        ctx: &evin_action_sdk::ActionValidationContext<'_>,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        super::validate_task_config(ctx, with)
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        super::execute(ctx, with).await
    }
}

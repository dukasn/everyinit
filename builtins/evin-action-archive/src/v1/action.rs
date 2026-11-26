// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License, version 3.

#![allow(clippy::result_large_err)]
use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    PackageReleaseContract, RunnerContext,
};

const ARCHIVE_FIELDS: &[ActionFieldDescriptor] = &[
    ActionFieldDescriptor {
        path: "path",
        kind: ActionFieldKind::String,
        required: true,
        description: "Destination path for the tar.gz archive.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "include",
        kind: ActionFieldKind::List,
        required: true,
        description: "Input patterns for the archive.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "exclude",
        kind: ActionFieldKind::List,
        required: false,
        description: "Excluded input patterns.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "prefix",
        kind: ActionFieldKind::String,
        required: false,
        description: "Stored path prefix.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "layout",
        kind: ActionFieldKind::Enum(&["flat", "relative"]),
        required: false,
        description: "Stored path layout.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "compression_level",
        kind: ActionFieldKind::Integer,
        required: false,
        description: "Gzip compression level.",
        condition: None,
    },
];

macro_rules! package_fields {
    ($($specific:expr),* $(,)?) => {
        &[
            ActionFieldDescriptor { path: "output_dir", kind: ActionFieldKind::String, required: true, description: "Package output directory.", condition: None },
            ActionFieldDescriptor { path: "architecture", kind: ActionFieldKind::String, required: true, description: "Target package architecture.", condition: None },
            ActionFieldDescriptor { path: "release", kind: ActionFieldKind::String, required: true, description: "Release JSON output from a prerequisite task.", condition: None },
            ActionFieldDescriptor { path: "package", kind: ActionFieldKind::Object, required: true, description: "Package identity and payload.", condition: None },
            ActionFieldDescriptor { path: "package.name", kind: ActionFieldKind::String, required: true, description: "Canonical package and release-policy identity.", condition: None },
            ActionFieldDescriptor { path: "package.summary", kind: ActionFieldKind::String, required: true, description: "Short package summary.", condition: None },
            ActionFieldDescriptor { path: "package.maintainer", kind: ActionFieldKind::String, required: true, description: "Package maintainer.", condition: None },
            ActionFieldDescriptor { path: "package.homepage", kind: ActionFieldKind::String, required: false, description: "Package homepage.", condition: None },
            ActionFieldDescriptor { path: "package.payload", kind: ActionFieldKind::List, required: true, description: "Payload mappings resolved from task cwd.", condition: None },
            ActionFieldDescriptor { path: "package.payload.source", kind: ActionFieldKind::String, required: true, description: "Payload source path or glob.", condition: None },
            ActionFieldDescriptor { path: "package.payload.strip_prefix", kind: ActionFieldKind::String, required: true, description: "Source prefix removed from payload paths.", condition: None },
            ActionFieldDescriptor { path: "package.payload.destination", kind: ActionFieldKind::String, required: true, description: "Package installation directory.", condition: None },
            ActionFieldDescriptor { path: "package.payload.mode", kind: ActionFieldKind::Integer, required: false, description: "Normalized Unix file mode.", condition: None },
            $($specific,)*
        ]
    };
}

const DEB_FIELDS: &[ActionFieldDescriptor] = package_fields![
    ActionFieldDescriptor {
        path: "description",
        kind: ActionFieldKind::String,
        required: false,
        description: "Long package description.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "DEB runtime dependencies.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "DEB maintainer scripts.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.preinst",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB pre-install script path.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.postinst",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB post-install script path.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.prerm",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB pre-remove script path.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.postrm",
        kind: ActionFieldKind::String,
        required: false,
        description: "DEB post-remove script path.",
        condition: None
    },
];

const APK_FIELDS: &[ActionFieldDescriptor] = package_fields![
    ActionFieldDescriptor {
        path: "license",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package license identifier.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "APK runtime dependencies.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "APK lifecycle scripts.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.pre_install",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK pre-install script path.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.post_install",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK post-install script path.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.pre_deinstall",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK pre-deinstall script path.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.post_deinstall",
        kind: ActionFieldKind::String,
        required: false,
        description: "APK post-deinstall script path.",
        condition: None
    },
];

const ARCH_FIELDS: &[ActionFieldDescriptor] = package_fields![
    ActionFieldDescriptor {
        path: "license",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package license identifier.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch runtime dependencies.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "optdepends",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch optional dependencies.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "provides",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch provided capabilities.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "conflicts",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch conflicting packages.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "replaces",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch replaced packages.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "backup",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch backup paths.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "Arch install script metadata.",
        condition: None
    },
    ActionFieldDescriptor {
        path: "scripts.install",
        kind: ActionFieldKind::String,
        required: false,
        description: "Arch .INSTALL script path.",
        condition: None
    },
];

const OUTPUTS: &[ActionOutputDescriptor] = &[
    ActionOutputDescriptor {
        name: "path",
        description: "Published artifact path.",
    },
    ActionOutputDescriptor {
        name: "size",
        description: "Published artifact byte size.",
    },
];
const ARCHIVE_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create a deterministic tar.gz archive.",
    fields: ARCHIVE_FIELDS,
    outputs: OUTPUTS,
};
const DEB_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create a reproducible Debian package.",
    fields: DEB_FIELDS,
    outputs: OUTPUTS,
};
const APK_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create a reproducible Alpine package.",
    fields: APK_FIELDS,
    outputs: OUTPUTS,
};
const ARCH_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create a reproducible Arch package.",
    fields: ARCH_FIELDS,
    outputs: OUTPUTS,
};
const DEB_RELEASE_CONTRACT: PackageReleaseContract = PackageReleaseContract {
    package_name_path: "package.name",
    target: "deb",
};
const APK_RELEASE_CONTRACT: PackageReleaseContract = PackageReleaseContract {
    package_name_path: "package.name",
    target: "apk",
};
const ARCH_RELEASE_CONTRACT: PackageReleaseContract = PackageReleaseContract {
    package_name_path: "package.name",
    target: "arch",
};

pub struct ArchiveAction;
pub struct DebPackageAction;
pub struct ApkPackageAction;
pub struct ArchPackageAction;

macro_rules! implement_action {
    ($action:ty, $descriptor:ident, $contract:expr, $validate:ident, $validate_task:ident, $execute:ident) => {
        #[async_trait::async_trait]
        impl Action for $action {
            fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
                Some(&$descriptor)
            }

            fn package_release_contract(&self) -> Option<&'static PackageReleaseContract> {
                $contract
            }

            fn validate_config(
                &self,
                with: Option<&evin_flow::SpannedValue>,
            ) -> Result<(), evin_flow::ConfigError> {
                super::$validate(with)
            }

            fn validate_task_config(
                &self,
                ctx: &evin_action_sdk::ActionValidationContext<'_>,
                with: Option<&evin_flow::SpannedValue>,
            ) -> Result<(), evin_flow::ConfigError> {
                super::$validate_task(ctx, with)
            }

            async fn execute(
                &self,
                ctx: &RunnerContext,
                with: Option<evin_flow::SpannedValue>,
            ) -> Result<(), evin_diagnostic::Diagnostic> {
                super::$execute(ctx, with).await
            }
        }
    };
}

implement_action!(
    ArchiveAction,
    ARCHIVE_DESCRIPTOR,
    None,
    validate_archive_config,
    validate_archive_task_config,
    execute_archive
);
implement_action!(
    DebPackageAction,
    DEB_DESCRIPTOR,
    Some(&DEB_RELEASE_CONTRACT),
    validate_deb_config,
    validate_deb_task_config,
    execute_deb
);
implement_action!(
    ApkPackageAction,
    APK_DESCRIPTOR,
    Some(&APK_RELEASE_CONTRACT),
    validate_apk_config,
    validate_apk_task_config,
    execute_apk
);
implement_action!(
    ArchPackageAction,
    ARCH_DESCRIPTOR,
    Some(&ARCH_RELEASE_CONTRACT),
    validate_arch_config,
    validate_arch_task_config,
    execute_arch
);

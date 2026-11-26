// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

#![allow(clippy::result_large_err)]
use super::{config::Config, writer};
use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    PackageReleaseContract, RunnerContext, package_action,
    packaging::{self, LinuxArch},
};

const OUTPUTS: &[ActionOutputDescriptor] = &[
    ActionOutputDescriptor {
        name: "path",
        description: "Created package path.",
    },
    ActionOutputDescriptor {
        name: "size",
        description: "Created package size in bytes.",
    },
];
const FIELDS: &[ActionFieldDescriptor] = &[
    ActionFieldDescriptor {
        path: "output_dir",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package output directory.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "architecture",
        kind: ActionFieldKind::String,
        required: true,
        description: "Target Arch package architecture.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "release",
        kind: ActionFieldKind::String,
        required: true,
        description: "Release JSON output from a prerequisite task.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package",
        kind: ActionFieldKind::Object,
        required: true,
        description: "Package identity and payload.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.name",
        kind: ActionFieldKind::String,
        required: true,
        description: "Canonical package and release-policy identity.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.summary",
        kind: ActionFieldKind::String,
        required: true,
        description: "Short package summary.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.maintainer",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package maintainer.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.homepage",
        kind: ActionFieldKind::String,
        required: false,
        description: "Package homepage.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.payload",
        kind: ActionFieldKind::List,
        required: true,
        description: "Payload mappings resolved from task cwd.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.payload.source",
        kind: ActionFieldKind::String,
        required: true,
        description: "Payload source path or glob.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.payload.strip_prefix",
        kind: ActionFieldKind::String,
        required: true,
        description: "Source prefix removed from payload paths.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.payload.destination",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package installation directory.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "package.payload.mode",
        kind: ActionFieldKind::Integer,
        required: false,
        description: "Normalized Unix file mode.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "license",
        kind: ActionFieldKind::String,
        required: true,
        description: "Package license identifier.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch runtime dependencies.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "optdepends",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch optional dependencies.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "provides",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch provided capabilities.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "conflicts",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch conflicting packages.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "replaces",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch replaced packages.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "backup",
        kind: ActionFieldKind::List,
        required: false,
        description: "Arch backup paths.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "Arch install script metadata.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "scripts.install",
        kind: ActionFieldKind::String,
        required: false,
        description: "Arch install script path.",
        condition: None,
    },
];
const DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create a reproducible Arch package.",
    fields: FIELDS,
    outputs: OUTPUTS,
};
const RELEASE_CONTRACT: PackageReleaseContract = PackageReleaseContract {
    package_name_path: "package.name",
    target: "arch",
};

pub struct ArchPackageAction;

#[async_trait::async_trait]
impl Action for ArchPackageAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&DESCRIPTOR)
    }
    fn package_release_contract(&self) -> Option<&'static PackageReleaseContract> {
        Some(&RELEASE_CONTRACT)
    }
    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        let config: Config = package_action::parse_config(with)?;
        validate(&config).map_err(package_action::diagnostic_to_config_error)
    }
    fn validate_task_config(
        &self,
        ctx: &evin_action_sdk::ActionValidationContext<'_>,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        let config: Config = package_action::parse_config(with)?;
        validate(&config).map_err(package_action::diagnostic_to_config_error)?;
        package_action::validate_release_reference_config(&config.release, ctx.needs)
    }
    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        let config: Config = ctx.load_config(with.as_ref()).map_err(|error| {
            error.into_diagnostic(evin_locales::t!("package-invalid-config-title"), None)
        })?;
        validate(&config)?;
        let (version, revision) =
            package_action::release_state(&config.release, &config.package.name, "arch")?;
        let architecture = arch_architecture(config.architecture);
        let filename = format!(
            "{}-{}-{}-{}.pkg.tar.zst",
            config.package.name, version, revision, architecture
        );
        let path = package_action::output_path(&config.output_dir, &filename);
        let sources = package_action::resolve_payloads(ctx, &config.package)?;
        let unresolved_inputs = config
            .package
            .payload
            .iter()
            .map(|mapping| mapping.source.clone())
            .collect();
        let script_defs = config
            .scripts
            .install
            .as_deref()
            .map(|path| vec![(".INSTALL", path)])
            .unwrap_or_default();
        let scripts = package_action::load_scripts(ctx, &script_defs)?;
        let install = scripts
            .into_iter()
            .find(|(name, _)| name == ".INSTALL")
            .map(|(_, data)| data);
        let name = config.package.name;
        let summary = config.package.summary;
        let maintainer = config.package.maintainer;
        let homepage = config.package.homepage;
        let license = config.license;
        let depends = config.depends;
        let optdepends = config.optdepends;
        let provides = config.provides;
        let conflicts = config.conflicts;
        let replaces = config.replaces;
        let backup = config.backup;
        packaging::publish_artifact(
            ctx,
            path,
            "arch".into(),
            unresolved_inputs,
            sources,
            None,
            move |output, context| {
                writer::write(
                    output,
                    writer::ArchPackage {
                        context,
                        name: &name,
                        version: &version,
                        revision,
                        architecture,
                        summary: &summary,
                        homepage: homepage.as_deref(),
                        maintainer: &maintainer,
                        license: &license,
                        depends: &depends,
                        optdepends: &optdepends,
                        provides: &provides,
                        conflicts: &conflicts,
                        replaces: &replaces,
                        backup: &backup,
                        install: install.as_deref(),
                    },
                )
            },
        )
        .await?;
        Ok(())
    }
}

fn validate(config: &Config) -> Result<(), evin_diagnostic::Diagnostic> {
    package_action::validate_output_and_release(&config.output_dir, &config.release)?;
    package_action::validate_identity(&config.package)?;
    package_action::validate_metadata_string("license", &config.license)?;
    for (field, values) in [
        ("depends", &config.depends),
        ("optdepends", &config.optdepends),
        ("provides", &config.provides),
        ("conflicts", &config.conflicts),
        ("replaces", &config.replaces),
        ("backup", &config.backup),
    ] {
        package_action::validate_relations(field, values)?;
    }
    for path in &config.backup {
        packaging::normalize_archive_path(path).map_err(evin_diagnostic::Diagnostic::from)?;
    }
    if let Some(path) = config.scripts.install.as_deref() {
        package_action::validate_script_path(path)?;
    }
    Ok(())
}
pub(super) fn arch_architecture(architecture: LinuxArch) -> &'static str {
    match architecture {
        LinuxArch::X86_64 => "x86_64",
        LinuxArch::Aarch64 => "aarch64",
        LinuxArch::X86 => "i686",
        LinuxArch::Armv7 => "armv7h",
        LinuxArch::Riscv64 => "riscv64",
        LinuxArch::Ppc64le => "ppc64le",
        LinuxArch::S390x => "s390x",
        LinuxArch::Noarch => "any",
    }
}

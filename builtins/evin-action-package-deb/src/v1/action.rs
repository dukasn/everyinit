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
        description: "Target Debian package architecture.",
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
        path: "description",
        kind: ActionFieldKind::String,
        required: false,
        description: "Long package description.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "depends",
        kind: ActionFieldKind::List,
        required: false,
        description: "Debian runtime dependencies.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "scripts",
        kind: ActionFieldKind::Object,
        required: false,
        description: "Debian maintainer scripts.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "scripts.preinst",
        kind: ActionFieldKind::String,
        required: false,
        description: "Pre-install script path.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "scripts.postinst",
        kind: ActionFieldKind::String,
        required: false,
        description: "Post-install script path.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "scripts.prerm",
        kind: ActionFieldKind::String,
        required: false,
        description: "Pre-remove script path.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "scripts.postrm",
        kind: ActionFieldKind::String,
        required: false,
        description: "Post-remove script path.",
        condition: None,
    },
];

const DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create a reproducible Debian package.",
    fields: FIELDS,
    outputs: OUTPUTS,
};
const RELEASE_CONTRACT: PackageReleaseContract = PackageReleaseContract {
    package_name_path: "package.name",
    target: "deb",
};

pub struct DebPackageAction;

#[async_trait::async_trait]
impl Action for DebPackageAction {
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
            package_action::release_state(&config.release, &config.package.name, "deb")?;
        let architecture = deb_architecture(config.architecture);
        let filename = format!(
            "{}_{}-{}_{}.deb",
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
        let script_defs = [
            ("preinst", config.scripts.preinst.as_deref()),
            ("postinst", config.scripts.postinst.as_deref()),
            ("prerm", config.scripts.prerm.as_deref()),
            ("postrm", config.scripts.postrm.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, path)| path.map(|path| (name, path)))
        .collect::<Vec<_>>();
        let scripts = package_action::load_scripts(ctx, &script_defs)?;
        let name = config.package.name;
        let summary = config.package.summary;
        let maintainer = config.package.maintainer;
        let homepage = config.package.homepage;
        let description = config.description;
        let depends = config.depends;
        packaging::publish_artifact(
            ctx,
            path,
            "deb".into(),
            unresolved_inputs,
            sources,
            None,
            move |output, context| {
                writer::write(
                    output,
                    writer::DebPackage {
                        context,
                        name: &name,
                        version: &version,
                        revision,
                        architecture,
                        summary: &summary,
                        description: description.as_deref(),
                        maintainer: &maintainer,
                        homepage: homepage.as_deref(),
                        depends,
                        scripts,
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
    if let Some(description) = config.description.as_deref() {
        package_action::validate_metadata_string("description", description)?;
    }
    package_action::validate_relations("depends", &config.depends)?;
    for path in [
        config.scripts.preinst.as_deref(),
        config.scripts.postinst.as_deref(),
        config.scripts.prerm.as_deref(),
        config.scripts.postrm.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        package_action::validate_script_path(path)?;
    }
    Ok(())
}

pub(super) fn deb_architecture(architecture: LinuxArch) -> &'static str {
    match architecture {
        LinuxArch::X86_64 => "amd64",
        LinuxArch::Aarch64 => "arm64",
        LinuxArch::X86 => "i386",
        LinuxArch::Armv7 => "armhf",
        LinuxArch::Riscv64 => "riscv64",
        LinuxArch::Ppc64le => "ppc64el",
        LinuxArch::S390x => "s390x",
        LinuxArch::Noarch => "all",
    }
}

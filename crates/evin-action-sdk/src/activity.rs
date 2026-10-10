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

//! Localized, categorized action activity.

use std::time::SystemTime;

mod private {
    pub trait LocalizedMessage {}
    pub trait ActivityEvent {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivityCategory {
    Scan,
    Audit,
    Build,
    Write,
    Change,
    Rebuild,
    Reload,
    Ready,
    Retry,
    Skip,
    Clean,
    Quality,
    Package,
    Publish,
    Release,
    Version,
    Serve,
    Sync,
}

impl ActivityCategory {
    pub const ALL: &[Self] = &[
        Self::Scan,
        Self::Audit,
        Self::Build,
        Self::Write,
        Self::Change,
        Self::Rebuild,
        Self::Reload,
        Self::Ready,
        Self::Retry,
        Self::Skip,
        Self::Clean,
        Self::Quality,
        Self::Package,
        Self::Publish,
        Self::Release,
        Self::Version,
        Self::Serve,
        Self::Sync,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Audit => "audit",
            Self::Build => "build",
            Self::Write => "write",
            Self::Change => "change",
            Self::Rebuild => "rebuild",
            Self::Reload => "reload",
            Self::Ready => "ready",
            Self::Retry => "retry",
            Self::Skip => "skip",
            Self::Clean => "clean",
            Self::Quality => "quality",
            Self::Package => "package",
            Self::Publish => "publish",
            Self::Release => "release",
            Self::Version => "version",
            Self::Serve => "serve",
            Self::Sync => "sync",
        }
    }
}

#[derive(Clone)]
pub struct LocalizedText {
    pub key: &'static str,
    pub args: Vec<(&'static str, String)>,
    redactor: Option<std::sync::Arc<crate::secrets::SecretsProvider>>,
}

impl std::fmt::Debug for LocalizedText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalizedText")
            .field("key", &self.key)
            .field("args", &self.args)
            .finish_non_exhaustive()
    }
}

impl LocalizedText {
    pub(crate) fn with_redactor(
        mut self,
        redactor: std::sync::Arc<crate::secrets::SecretsProvider>,
    ) -> Self {
        self.redactor = Some(redactor);
        self
    }

    /// Redacts secrets after the localized message has been interpolated.
    pub fn redact_rendered(&self, rendered: &str) -> String {
        self.redactor.as_ref().map_or_else(
            || rendered.to_string(),
            |redactor| redactor.redact(rendered),
        )
    }
}

pub trait LocalizedMessage: private::LocalizedMessage + Send {
    fn localized_text(self) -> LocalizedText;
}

#[derive(Debug, Clone)]
pub struct ActivityRecord {
    pub at: SystemTime,
    pub level: ActivityLevel,
    pub category: ActivityCategory,
    pub text: LocalizedText,
}

pub trait ActivityEvent: private::ActivityEvent + LocalizedMessage {
    const LEVEL: ActivityLevel;
    const CATEGORY: ActivityCategory;

    fn into_record(self) -> ActivityRecord
    where
        Self: Sized,
    {
        ActivityRecord {
            at: SystemTime::now(),
            level: Self::LEVEL,
            category: Self::CATEGORY,
            text: self.localized_text(),
        }
    }
}

macro_rules! define_localized_messages {
    ($( $(#[$meta:meta])* pub struct $name:ident { $($field:ident : $ty:ty),* $(,)? } => $key:literal; )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone)]
            pub struct $name { $(pub $field: $ty),* }

            impl private::LocalizedMessage for $name {}

            impl LocalizedMessage for $name {
                fn localized_text(self) -> LocalizedText {
                    LocalizedText {
                        key: $key,
                        args: vec![$((stringify!($field), self.$field.to_string())),*],
                        redactor: None,
                    }
                }
            }
        )*
    };
}

macro_rules! define_activity_events {
    ($( $(#[$meta:meta])* pub struct $name:ident { $($field:ident : $ty:ty),* $(,)? } => ($level:ident, $category:ident, $key:literal); )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone)]
            pub struct $name { $(pub $field: $ty),* }

            impl private::LocalizedMessage for $name {}
            impl private::ActivityEvent for $name {}

            impl LocalizedMessage for $name {
                fn localized_text(self) -> LocalizedText {
                    LocalizedText {
                        key: $key,
                        args: vec![$((stringify!($field), self.$field.to_string())),*],
                        redactor: None,
                    }
                }
            }

            impl ActivityEvent for $name {
                const LEVEL: ActivityLevel = ActivityLevel::$level;
                const CATEGORY: ActivityCategory = ActivityCategory::$category;
            }
        )*
    };
}

define_activity_events! {
    pub struct DocsScanStarted { source: String } => (Info, Scan, "activity-docs-scan-started");
    pub struct DocsSourceMissing { source: String } => (Warn, Scan, "activity-docs-source-missing");
    pub struct DocsReadFailed { source: String, error: String } => (Warn, Scan, "activity-docs-read-failed");
    pub struct DocsAuditStarted {} => (Info, Audit, "activity-docs-audit-started");
    pub struct DocsLinkIssue { issue: String } => (Warn, Audit, "activity-docs-link-issue");
    pub struct DocsCssFallback { error: String } => (Warn, Build, "activity-docs-css-fallback");
    pub struct DocsGenerated { output: String } => (Info, Ready, "activity-docs-generated");
    pub struct DocsReady {} => (Info, Ready, "activity-docs-ready");
    pub struct DocsChanged { path: String } => (Info, Change, "activity-docs-changed");
    pub struct DocsRebuildSucceeded { duration_ms: u128 } => (Info, Rebuild, "activity-docs-rebuild-succeeded");
    pub struct DocsReloaded {} => (Info, Reload, "activity-docs-reloaded");
    pub struct DocsRebuildFailed { reason: String } => (Error, Rebuild, "activity-docs-rebuild-failed");
    pub struct DocsServeReady { url: String } => (Info, Ready, "activity-docs-serve-ready");
    pub struct ArchiveCompressing { files: usize, path: String } => (Info, Package, "activity-archive-compressing");
    pub struct ArchiveCreated { format: String, path: String, input_files: usize, input_size: String, output_size: String } => (Info, Package, "activity-archive-created");
}

define_localized_messages! {
    pub struct DocsServerRunning { url: String } => "status-docs-server-running";
    pub struct DocsWatching {} => "status-docs-watching";
    pub struct DocsRebuilding {} => "status-docs-rebuilding";
    pub struct DocsWatchingSucceeded {} => "status-docs-watching-succeeded";
    pub struct DocsWatchingFailed {} => "status-docs-watching-failed";
    pub struct DocsPlanBuild { output: String } => "plan-docs-build";
    pub struct DocsPlanServer {} => "plan-docs-server";
    pub struct ArchivePlanUnresolved { path: String } => "plan-archive-unresolved";
    pub struct ArchivePlan { files: usize, path: String } => "plan-archive";
    pub struct ChangelogPlan { path: String, sections: String } => "plan-changelog";
    pub struct VersionUpdatePlan { version: String, path: String } => "plan-version-update";
    pub struct VersionCommitPlan { message: String } => "plan-version-commit";
    pub struct VersionTagPlan { tag: String } => "plan-version-tag";
    pub struct ShellPlan { script: String } => "plan-shell-command";
    pub struct StageFilesPlan { files: String } => "plan-stage-files";
    pub struct GitCommitPlan { message: String, signed: bool } => "plan-git-commit";
    pub struct GitTagPlan { tag: String } => "plan-git-tag";
    pub struct WriteFilePlan { path: String } => "plan-write-file";
}

pub const ACTIVITY_LOCALE_KEYS: &[(&str, &[&str])] = &[
    ("activity-docs-scan-started", &["source"]),
    ("activity-docs-source-missing", &["source"]),
    ("activity-docs-read-failed", &["source", "error"]),
    ("activity-docs-audit-started", &[]),
    ("activity-docs-link-issue", &["issue"]),
    ("activity-docs-css-fallback", &["error"]),
    ("activity-docs-generated", &["output"]),
    ("activity-docs-ready", &[]),
    ("activity-docs-changed", &["path"]),
    ("activity-docs-rebuild-succeeded", &["duration_ms"]),
    ("activity-docs-reloaded", &[]),
    ("activity-docs-rebuild-failed", &["reason"]),
    ("activity-docs-serve-ready", &["url"]),
    ("activity-archive-compressing", &["files", "path"]),
    (
        "activity-archive-created",
        &["format", "path", "input_files", "input_size", "output_size"],
    ),
    ("status-docs-server-running", &["url"]),
    ("status-docs-watching", &[]),
    ("status-docs-rebuilding", &[]),
    ("status-docs-watching-succeeded", &[]),
    ("status-docs-watching-failed", &[]),
    ("plan-docs-server", &[]),
    ("plan-archive-unresolved", &["path"]),
    ("plan-archive", &["files", "path"]),
    ("plan-changelog", &["path", "sections"]),
    ("plan-version-update", &["version", "path"]),
    ("plan-version-commit", &["message"]),
    ("plan-version-tag", &["tag"]),
    ("plan-shell-command", &["script"]),
    ("plan-stage-files", &["files"]),
    ("plan-git-commit", &["message", "signed"]),
    ("plan-git-tag", &["tag"]),
    ("plan-write-file", &["path"]),
];

#[macro_export]
macro_rules! action_event {
    ($logger:expr, $event:expr $(,)?) => {{ $logger.activity($event) }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluent_bundle::FluentResource;
    use regex::Regex;
    use std::collections::{BTreeMap, BTreeSet};

    fn english_entries() -> BTreeMap<String, String> {
        let resources = evin_io::load_ftl_resources("en");
        let mut entries = BTreeMap::new();
        for resource in resources {
            FluentResource::try_new(resource.content.clone())
                .unwrap_or_else(|(_, errors)| panic!("malformed {}: {errors:?}", resource.path));
            let mut current: Option<(String, String)> = None;
            for line in resource.content.lines() {
                let is_entry = !line.starts_with(char::is_whitespace)
                    && !line.starts_with('#')
                    && line.contains(" = ");
                if is_entry {
                    if let Some((key, value)) = current.take() {
                        entries.insert(key, value);
                    }
                    let (key, value) = line.split_once(" = ").unwrap();
                    current = Some((key.to_string(), value.to_string()));
                } else if let Some((_, value)) = current.as_mut()
                    && line.starts_with(char::is_whitespace)
                {
                    value.push('\n');
                    value.push_str(line);
                }
            }
            if let Some((key, value)) = current {
                entries.insert(key, value);
            }
        }
        entries
    }

    #[test]
    fn categories_are_stable_protocol_tokens() {
        let expected = [
            "scan", "audit", "build", "write", "change", "rebuild", "reload", "ready", "retry",
            "skip", "clean", "quality", "package", "publish", "release", "version", "serve",
            "sync",
        ];
        assert_eq!(
            ActivityCategory::ALL
                .iter()
                .copied()
                .map(ActivityCategory::as_str)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn english_catalog_defines_every_typed_message_with_exact_variables() {
        let entries = english_entries();
        let variable = Regex::new(r"\$([a-z][a-z0-9_]*)").unwrap();
        for (key, expected) in ACTIVITY_LOCALE_KEYS {
            let value = entries
                .get(*key)
                .unwrap_or_else(|| panic!("English fallback is missing '{key}'"));
            let actual = variable
                .captures_iter(value)
                .map(|capture| capture[1].to_string())
                .collect::<BTreeSet<_>>();
            let expected = expected
                .iter()
                .map(|name| name.to_string())
                .collect::<BTreeSet<_>>();
            assert_eq!(actual, expected, "Fluent argument mismatch for '{key}'");
        }
    }

    #[test]
    fn partial_locale_uses_english_fallback() {
        let resources = evin_io::load_ftl_resources("non-existent-locale");
        assert!(resources.iter().any(|resource| {
            resource
                .content
                .contains("activity-docs-ready = Documentation site generated")
        }));
    }
}

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

use crate::models::GitRulesRoot;
use rayon::prelude::*;
use regex::Regex;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct StagedFile {
    pub path: String,
    pub content: String,
    pub is_binary: bool,
    pub size_bytes: usize,
    pub is_lfs_tracked: bool,
    pub status: String,
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("File size {size} bytes exceeds limit of {limit}")]
    FileSizeExceeded {
        file: String,
        size: usize,
        limit: String,
    },
    #[error("File {file} violates content block: {message}")]
    ContentBlockViolated { file: String, message: String },
    #[error("File {file} missing required pattern: {message}")]
    MissingRequiredPattern { file: String, message: String },
    #[error("File {file} contains forbidden pattern: {message}")]
    ForbiddenPatternFound { file: String, message: String },
    // Branches (§5)
    #[error("Branch name '{branch}' violates required format")]
    InvalidBranchName { branch: String },
    #[error("Pushing directly to main is blocked by policy")]
    PushToMainBlocked,
    // Tags (§6)
    #[error("Lightweight tags are not allowed by policy. Use annotated tags instead.")]
    LightweightTagBlocked,
    #[error("Tag name '{tag}' violates required format")]
    InvalidTagName { tag: String },
    #[error("Tag deletion is blocked by policy")]
    TagDeletionBlocked,
    #[error("Regex compilation error: {0}")]
    RegexError(String),
    #[error("LFS is required but not installed")]
    LfsNotInstalled,
    #[error("File {file} requires LFS tracking (.gitattributes)")]
    LfsTrackingRequired { file: String },
    #[error("Submodule creation is blocked by policy")]
    SubmoduleCreationBlocked,
    #[error("Submodule URL '{url}' is not allowed")]
    SubmoduleHostNotAllowed { url: String },
    #[error("Pushing to remote '{remote}' is not allowed")]
    RemoteNotAllowed { remote: String },
    #[error("Force pushing to branch '{branch}' is blocked by policy")]
    ForcePushBlocked { branch: String },
    // Commit message validation (§6)
    #[error("Commit message does not match required format '{expected_format}': {subject}")]
    InvalidCommitMessage {
        subject: String,
        expected_format: String,
    },
    #[error("Commit field '{field}' value '{value}' is not allowed: {reason}")]
    InvalidFieldValue {
        field: String,
        value: String,
        reason: String,
    },
    #[error("Commit body is required by policy but was not found")]
    MissingCommitBody,
    #[error("Commit message must contain a ticket reference with prefix '{prefix}'")]
    MissingTicketReference { prefix: String },
    #[error("Commit body line exceeds max length of {max_length}: {line}")]
    CommitBodyLineTooLong { max_length: usize, line: String },
    #[error("File {file} has an invalid name format. Required convention: {convention}")]
    InvalidFileName { file: String, convention: String },
    #[error("File {file} path is blocked by policy")]
    PathBlocked { file: String },
    #[error("Branch '{branch}' is not up to date with remote '{remote}'")]
    BranchNotUpToDate { branch: String, remote: String },
    // Authorship (§9)
    #[error("Email {email} is not allowed by policy")]
    EmailNotAllowed { email: String },
    #[error("Identity forging detected: author email does not match committer email")]
    IdentityForgingDetected,
    #[error("Unsigned commits are not allowed by policy")]
    UnsignedCommitBlocked,
    // History (§10)
    #[error("Merge commits are not allowed by policy (linear history required)")]
    MergeCommitBlocked,
}

/// Helper to parse a size string like "50MB" or "500KB" into bytes.
pub fn parse_size_limit(limit_str: &str) -> Result<usize, String> {
    let limit_str = limit_str.trim().to_uppercase();
    if limit_str.ends_with("MB") {
        let val: f64 = limit_str
            .trim_end_matches("MB")
            .trim()
            .parse()
            .map_err(|_| "Invalid number")?;
        Ok((val * 1024.0 * 1024.0) as usize)
    } else if limit_str.ends_with("KB") {
        let val: f64 = limit_str
            .trim_end_matches("KB")
            .trim()
            .parse()
            .map_err(|_| "Invalid number")?;
        Ok((val * 1024.0) as usize)
    } else if limit_str.ends_with("GB") {
        let val: f64 = limit_str
            .trim_end_matches("GB")
            .trim()
            .parse()
            .map_err(|_| "Invalid number")?;
        Ok((val * 1024.0 * 1024.0 * 1024.0) as usize)
    } else if limit_str.ends_with("B") {
        let val: f64 = limit_str
            .trim_end_matches("B")
            .trim()
            .parse()
            .map_err(|_| "Invalid number")?;
        Ok(val as usize)
    } else {
        let val: f64 = limit_str.parse().map_err(|_| "Invalid number")?;
        Ok(val as usize) // Default to bytes if no unit
    }
}

pub struct RulesValidator<'a> {
    rules: &'a GitRulesRoot,
}

impl<'a> RulesValidator<'a> {
    pub fn new(rules: &'a GitRulesRoot) -> Self {
        Self { rules }
    }

    pub fn validate_branch_name(&self, branch_name: &str) -> Result<(), ValidationError> {
        if let Some(branches) = &self.rules.branches {
            let is_base_branch = branch_name == "main" || branch_name == "master";

            if let Some(policies) = &branches.policies
                && policies.block_push_to_main.unwrap_or(false)
                && is_base_branch
            {
                return Err(ValidationError::PushToMainBlocked);
            }

            // Base branches are inherently exempt from conventional branch naming formats
            if !is_base_branch && let Some(format_list) = &branches.name_format {
                let mut matched = false;
                for format in format_list.iter() {
                    let compiled = evin_yaml::template::regex::compile_template(format)
                        .map_err(|e| ValidationError::RegexError(e.to_string()))?;

                    if compiled.is_match(branch_name) {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Err(ValidationError::InvalidBranchName {
                        branch: branch_name.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn validate_tag_name(
        &self,
        tag_name: &str,
        is_annotated: bool,
    ) -> Result<(), ValidationError> {
        if let Some(tags) = &self.rules.tags {
            if let Some(policies) = &tags.policies
                && policies.require_annotated.unwrap_or(false)
                && !is_annotated
            {
                return Err(ValidationError::LightweightTagBlocked);
            }

            if let Some(format_list) = &tags.name_format {
                let mut matched = false;
                for format in format_list.iter() {
                    let compiled = evin_yaml::template::regex::compile_template(format)
                        .map_err(|e| ValidationError::RegexError(e.to_string()))?;

                    if compiled.is_match(tag_name) {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Err(ValidationError::InvalidTagName {
                        tag: tag_name.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn validate_tag_deletion(&self, _remote_ref: &str) -> Result<(), ValidationError> {
        if let Some(tags) = &self.rules.tags
            && let Some(policies) = &tags.policies
            && policies.block_deletion.unwrap_or(false)
        {
            return Err(ValidationError::TagDeletionBlocked);
        }
        Ok(())
    }

    pub fn validate_staged_files(&self, files: &[StagedFile]) -> Result<(), Vec<ValidationError>> {
        let mut global_errors = Vec::new();

        // 1. Hard Size Limit (synchronous, very fast)
        if let Some(history) = &self.rules.history
            && let Some(policies) = &history.policies
            && let Some(max_size_str) = &policies.max_staged_file_size
            && let Ok(limit_bytes) = parse_size_limit(max_size_str)
        {
            for file in files {
                if file.size_bytes > limit_bytes {
                    global_errors.push(ValidationError::FileSizeExceeded {
                        file: file.path.clone(),
                        size: file.size_bytes,
                        limit: max_size_str.clone(),
                    });
                }
            }
        }

        if !global_errors.is_empty() {
            return Err(global_errors);
        }

        // Pre-compile Regexes
        let mut content_blocks = Vec::new();
        if let Some(history) = &self.rules.history {
            for block in &history.content_blocks {
                if let Ok(re) = Regex::new(&block.pattern) {
                    content_blocks.push((re, block.error_message.clone()));
                }
            }
        }

        let mut content_rules = Vec::new();
        if let Some(files_ns) = &self.rules.files {
            for rule in &files_ns.content_rules {
                let require = rule
                    .require_pattern
                    .as_ref()
                    .and_then(|p| Regex::new(p).ok());
                let forbid = rule
                    .forbid_pattern
                    .as_ref()
                    .and_then(|p| Regex::new(p).ok());
                content_rules.push((rule.clone(), require, forbid));
            }
        }

        // Shared Arc to avoid cloning heavy structures into the thread pool
        let content_blocks = Arc::new(content_blocks);
        let content_rules = Arc::new(content_rules);

        // Pre-compile files.ignore patterns
        let ignore_patterns: Vec<glob::Pattern> = self
            .rules
            .files
            .as_ref()
            .map(|f| {
                f.ignore
                    .iter()
                    .filter_map(|p| glob::Pattern::new(p).ok())
                    .collect()
            })
            .unwrap_or_default();

        // Pre-compile files.policies.block_paths
        let block_paths: Vec<glob::Pattern> = self
            .rules
            .files
            .as_ref()
            .and_then(|f| f.policies.as_ref())
            .map(|p| {
                p.block_paths
                    .iter()
                    .filter_map(|p| glob::Pattern::new(p).ok())
                    .collect()
            })
            .unwrap_or_default();

        let naming_convention = self
            .rules
            .files
            .as_ref()
            .and_then(|f| f.naming_convention.clone());

        // 2. Parallel Processing for Regex Rules
        // Filter out megafiles (e.g. > 1MB) and binaries. The user bypass rule says > 1MB skip regex.
        let validation_results: Vec<ValidationError> = files
            .par_iter()
            .filter(|f| !f.is_binary && f.size_bytes <= 1_000_000)
            .filter(|f| !ignore_patterns.iter().any(|p| p.matches(&f.path)))
            .filter(|f| !f.path.ends_with(".evin/gitrules.flow"))
            .flat_map(|file| {
                let mut errors = Vec::new();

                // Check block_paths
                if block_paths.iter().any(|p| p.matches(&file.path)) {
                    errors.push(ValidationError::PathBlocked {
                        file: file.path.clone(),
                    });
                }

                // Check naming_convention
                if let Some(convention) = &naming_convention {
                    let file_name = std::path::Path::new(&file.path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");

                    if !check_naming_convention(file_name, convention) {
                        errors.push(ValidationError::InvalidFileName {
                            file: file.path.clone(),
                            convention: convention.clone(),
                        });
                    }
                }

                let blocks = Arc::clone(&content_blocks);
                let rules = Arc::clone(&content_rules);

                // Check History Content Blocks (AWS Keys, Private Keys)
                for (re, err_msg) in blocks.iter() {
                    if re.is_match(&file.content) {
                        errors.push(ValidationError::ContentBlockViolated {
                            file: file.path.clone(),
                            message: err_msg.clone(),
                        });
                    }
                }

                // Check Files Content Rules
                for (rule, require_re, forbid_re) in rules.iter() {
                    // Very rudimentary glob matching just for demo/extensibility
                    // A proper implementation would use `glob` or `ignore` crate
                    let matches_include = rule.include.is_empty()
                        || rule.include.iter().any(|p| glob_match(p, &file.path));
                    let matches_exclude = rule.exclude.iter().any(|p| glob_match(p, &file.path));

                    if matches_include && !matches_exclude {
                        if let Some(req) = require_re
                            && !req.is_match(&file.content)
                        {
                            errors.push(ValidationError::MissingRequiredPattern {
                                file: file.path.clone(),
                                message: rule.error_message.clone(),
                            });
                        }
                        if let Some(forbid) = forbid_re
                            && forbid.is_match(&file.content)
                        {
                            errors.push(ValidationError::ForbiddenPatternFound {
                                file: file.path.clone(),
                                message: rule.error_message.clone(),
                            });
                        }
                    }
                }

                errors
            })
            .collect();

        if !validation_results.is_empty() {
            return Err(validation_results);
        }

        Ok(())
    }

    pub fn validate_lfs(
        &self,
        files: &[StagedFile],
        is_lfs_installed: bool,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if let Some(lfs) = &self.rules.lfs
            && let Some(policies) = &lfs.policies
        {
            if policies.require_lfs_installed.unwrap_or(false) && !is_lfs_installed {
                errors.push(ValidationError::LfsNotInstalled);
            }

            for file in files {
                for ext in &policies.require_tracking_for {
                    if file.path.ends_with(ext) && !file.is_lfs_tracked {
                        errors.push(ValidationError::LfsTrackingRequired {
                            file: file.path.clone(),
                        });
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn validate_submodules(
        &self,
        files: &[StagedFile],
        submodule_urls: &[String],
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if let Some(submodules) = &self.rules.submodules
            && let Some(policies) = &submodules.policies
        {
            if !policies.allow_creation.unwrap_or(true) {
                for file in files {
                    if file.path == ".gitmodules" && file.status.starts_with('A') {
                        errors.push(ValidationError::SubmoduleCreationBlocked);
                    }
                }
            }

            if !policies.allowed_hosts.is_empty() {
                for url in submodule_urls {
                    let mut allowed = false;
                    for host in &policies.allowed_hosts {
                        if url.contains(host) {
                            allowed = true;
                            break;
                        }
                    }
                    if !allowed {
                        errors.push(ValidationError::SubmoduleHostNotAllowed { url: url.clone() });
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn validate_remotes(
        &self,
        remote_name: &str,
        branch: &str,
        is_force_push: bool,
        is_up_to_date: bool,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if let Some(remotes) = &self.rules.remotes
            && let Some(policies) = &remotes.policies
        {
            if !policies.allowed_push_remotes.is_empty()
                && !policies
                    .allowed_push_remotes
                    .contains(&remote_name.to_string())
            {
                errors.push(ValidationError::RemoteNotAllowed {
                    remote: remote_name.to_string(),
                });
            }

            if is_force_push && policies.block_force_push.contains(&branch.to_string()) {
                errors.push(ValidationError::ForcePushBlocked {
                    branch: branch.to_string(),
                });
            }

            if policies.require_up_to_date.unwrap_or(false) && !is_up_to_date {
                errors.push(ValidationError::BranchNotUpToDate {
                    branch: branch.to_string(),
                    remote: remote_name.to_string(),
                });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn validate_history_integrity(
        &self,
        repo_root: &std::path::Path,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if let Some(history) = &self.rules.history
            && let Some(policies) = &history.policies
            && policies.require_linear_history.unwrap_or(false)
        {
            let merge_head = repo_root.join(".git").join("MERGE_HEAD");
            if merge_head.exists() {
                // For now, reuse an existing error or define a new one! Wait, I didn't define a LinearHistoryValidationError!
                errors.push(ValidationError::MergeCommitBlocked);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validates the authorship environment against the `authorship` namespace (§9).
    pub fn validate_authorship(
        &self,
        author_email: &str,
        committer_email: &str,
        is_signed: bool,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if let Some(authorship) = &self.rules.authorship {
            // 1. Allowed emails
            if !authorship.allowed_emails.is_empty() {
                let mut allowed = false;
                for pattern in &authorship.allowed_emails {
                    if let Ok(glob) = glob::Pattern::new(pattern) {
                        if glob.matches(author_email) {
                            allowed = true;
                            break;
                        }
                    } else if pattern == author_email {
                        allowed = true;
                        break;
                    }
                }
                if !allowed {
                    errors.push(ValidationError::EmailNotAllowed {
                        email: author_email.to_string(),
                    });
                }
            }

            if let Some(policies) = &authorship.policies {
                // 2. Identity Forging
                if policies.block_identity_forging.unwrap_or(false)
                    && author_email != committer_email
                {
                    errors.push(ValidationError::IdentityForgingDetected);
                }

                // 3. Required Signatures
                if policies.require_signatures.unwrap_or(false) && !is_signed {
                    errors.push(ValidationError::UnsignedCommitBlocked);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validates a raw commit message string against the `commits` namespace (§6).
    /// Returns all violations found, or `Ok(())` if the message is valid or matches an ignore pattern.
    pub fn validate_commit_message(&self, source: &str) -> Result<(), Vec<ValidationError>> {
        let commits_ns = match &self.rules.commits {
            Some(c) => c,
            None => return Ok(()), // namespace not configured, nothing to enforce
        };

        // 1. Ignore patterns bypass
        for pattern in &commits_ns.ignore_patterns {
            if let Ok(re) = Regex::new(pattern)
                && re.is_match(source)
            {
                return Ok(());
            }
        }

        // 2. Extract first non-empty, non-comment subject line
        let subject = source
            .lines()
            .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .unwrap_or("");

        // 3. Compile and match message_format template
        let mut errors = Vec::new();
        if let Some(format_list) = &commits_ns.message_format {
            let mut matched = false;
            let mut compiled_captures = None;
            let mut last_format = String::new();

            for format in format_list.iter() {
                last_format = format.clone();
                match evin_yaml::template::regex::compile_template(format) {
                    Err(e) => errors.push(ValidationError::RegexError(e.to_string())),
                    Ok(compiled) => {
                        if let Some(captures) = compiled.captures(subject) {
                            matched = true;
                            compiled_captures = Some((compiled, captures));
                            break;
                        }
                    }
                }
            }

            if !matched {
                let expected_format = if format_list.iter().count() > 1 {
                    format!(
                        "one of: [{}]",
                        format_list.iter().cloned().collect::<Vec<_>>().join(", ")
                    )
                } else {
                    last_format
                };
                errors.push(ValidationError::InvalidCommitMessage {
                    subject: subject.to_string(),
                    expected_format,
                });
            } else if let Some((compiled, captures)) = compiled_captures {
                // 4. Field constraint checks
                for field in compiled.capture_names().flatten() {
                    if let Some(value) = captures.name(field)
                        && let Some(rule) = commits_ns.fields.get(field)
                    {
                        // allowed_values
                        if let Some(allowed) = &rule.allowed_values
                            && !allowed.iter().any(|v| v == value.as_str())
                        {
                            errors.push(ValidationError::InvalidFieldValue {
                                field: field.to_string(),
                                value: value.as_str().to_string(),
                                reason: format!("expected one of: {}", allowed.join(", ")),
                            });
                        }
                        // must_be
                        if let Some(must_be) = &rule.must_be {
                            let valid = match must_be.as_str() {
                                "lowercase-with-hyphens" => value.as_str().bytes().all(|b| {
                                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
                                }),
                                "alphanumeric-with-hyphens" => value
                                    .as_str()
                                    .bytes()
                                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                                _ => true,
                            };
                            if !valid {
                                errors.push(ValidationError::InvalidFieldValue {
                                    field: field.to_string(),
                                    value: value.as_str().to_string(),
                                    reason: format!("must satisfy '{must_be}' format"),
                                });
                            }
                        }
                        // max_length
                        if let Some(max) = rule.max_length {
                            let actual = value.as_str().chars().count();
                            if actual > max {
                                errors.push(ValidationError::InvalidFieldValue {
                                    field: field.to_string(),
                                    value: value.as_str().to_string(),
                                    reason: format!("max length is {max}, got {actual}"),
                                });
                            }
                        }
                        // pattern
                        if let Some(pattern) = &rule.pattern {
                            if let Ok(re) = Regex::new(pattern) {
                                if !re.is_match(value.as_str()) {
                                    errors.push(ValidationError::InvalidFieldValue {
                                        field: field.to_string(),
                                        value: value.as_str().to_string(),
                                        reason: format!("must match regex '{}'", pattern),
                                    });
                                }
                            } else {
                                errors.push(ValidationError::RegexError(format!(
                                    "Invalid field pattern for {}: {}",
                                    field, pattern
                                )));
                            }
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        // 5. Policy checks (only if format matched — errors would have been collected above)
        if let Some(policies) = &commits_ns.policies {
            // require_body
            if policies.require_body.unwrap_or(false) {
                let has_body = source
                    .lines()
                    .skip_while(|l| l.trim().is_empty() || l.trim_start().starts_with('#'))
                    .skip(1) // skip the subject
                    .skip_while(|l| l.trim().is_empty()) // skip the blank separator
                    .any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
                if !has_body {
                    errors.push(ValidationError::MissingCommitBody);
                }
            }
            // max_body_line_length
            if let Some(max_len) = policies.max_body_line_length {
                let body_lines = source
                    .lines()
                    .skip_while(|l| l.trim().is_empty() || l.trim_start().starts_with('#'))
                    .skip(1) // skip the subject
                    .skip_while(|l| l.trim().is_empty()) // skip the blank separator
                    .filter(|l| !l.trim_start().starts_with('#'));
                for line in body_lines {
                    if line.chars().count() > max_len {
                        errors.push(ValidationError::CommitBodyLineTooLong {
                            max_length: max_len,
                            line: line.to_string(),
                        });
                    }
                }
            }
            // require_ticket_reference
            if policies.require_ticket_reference.unwrap_or(false) {
                let prefix = self
                    .rules
                    .releases
                    .as_ref()
                    .and_then(|r| r.changelog.as_ref())
                    .and_then(|c| c.ticket_linking.as_ref())
                    .and_then(|t| t.prefix.as_ref());
                if let Some(p) = prefix
                    && !source.contains(p.as_str())
                {
                    errors.push(ValidationError::MissingTicketReference { prefix: p.clone() });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// Glob pattern matcher using the `glob` crate — supports `*.ext`, `src/**/*.rs`, `tests/` etc.
fn glob_match(pattern: &str, path: &str) -> bool {
    glob::Pattern::new(pattern)
        .map(|p| p.matches(path))
        .unwrap_or(false)
}

fn check_naming_convention(name: &str, convention: &str) -> bool {
    if name.is_empty() {
        return true;
    }

    // Strip extension for casing checks, if applicable, unless it's a dotfile.
    let base_name = if name.starts_with('.') {
        name
    } else {
        name.split('.').next().unwrap_or(name)
    };

    match convention {
        "snake_case" => base_name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
        "kebab-case" => base_name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "PascalCase" => {
            let mut chars = base_name.chars();
            if let Some(first) = chars.next() {
                if !first.is_ascii_uppercase() {
                    return false;
                }
                chars.all(|c| c.is_ascii_alphanumeric())
            } else {
                true
            }
        }
        "camelCase" => {
            let mut chars = base_name.chars();
            if let Some(first) = chars.next() {
                if !first.is_ascii_lowercase() {
                    return false;
                }
                chars.all(|c| c.is_ascii_alphanumeric())
            } else {
                true
            }
        }
        _ => true, // Unknown convention, pass by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FilesNamespace, FilesPolicies};

    #[test]
    fn test_case_convention_kebab() {
        assert!(check_naming_convention("my-file-name", "kebab-case"));
        assert!(!check_naming_convention("my_file_name", "kebab-case"));
    }

    #[test]
    fn test_case_convention_snake() {
        assert!(check_naming_convention("my_file_name", "snake_case"));
        assert!(!check_naming_convention("my-file-name", "snake_case"));
    }

    #[test]
    fn test_parse_size_limit() {
        assert_eq!(parse_size_limit("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size_limit("500KB").unwrap(), 500 * 1024);
        assert_eq!(parse_size_limit("100B").unwrap(), 100);
        assert!(parse_size_limit("invalid").is_err());
    }

    #[test]
    fn test_validate_branch_name() {
        let rules = GitRulesRoot::default();
        let validator = RulesValidator::new(&rules);

        assert!(validator.validate_branch_name("feature/my-branch").is_ok());
        assert!(validator.validate_branch_name("fix/bug-fix").is_ok());
        assert!(validator.validate_branch_name("main").is_ok());

        assert!(
            validator
                .validate_branch_name("INVALID_BRANCH_NAME!")
                .is_err()
        );
    }

    #[test]
    fn test_validate_tag_name() {
        let rules = GitRulesRoot::default();
        let validator = RulesValidator::new(&rules);

        assert!(validator.validate_tag_name("v1.0.0", true).is_ok());
        assert!(validator.validate_tag_name("v2.1.0-alpha.1", true).is_ok());
        assert!(
            validator
                .validate_tag_name("invalid-tag-format", true)
                .is_err()
        );
    }

    #[test]
    fn test_validate_commit_message() {
        let rules = GitRulesRoot::default();
        let validator = RulesValidator::new(&rules);

        assert!(
            validator
                .validate_commit_message("feat: valid commit message")
                .is_ok()
        );
        assert!(
            validator
                .validate_commit_message("fix(scope): another valid commit")
                .is_ok()
        );
        assert!(
            validator
                .validate_commit_message("invalid non-conventional commit")
                .is_err()
        );
    }

    #[test]
    fn test_validate_staged_files() {
        let mut rules = GitRulesRoot::default();
        rules
            .history
            .as_mut()
            .unwrap()
            .policies
            .as_mut()
            .unwrap()
            .max_staged_file_size = Some("1MB".to_string());
        rules.files = Some(FilesNamespace {
            policies: Some(FilesPolicies {
                block_paths: vec!["*.exe".to_string(), "secret.key".to_string()],
            }),
            ..Default::default()
        });
        let validator = RulesValidator::new(&rules);

        let valid_files = vec![StagedFile {
            path: "src/main.rs".to_string(),
            content: String::new(),
            is_binary: false,
            size_bytes: 100,
            is_lfs_tracked: false,
            status: "added".to_string(),
        }];
        assert!(validator.validate_staged_files(&valid_files).is_ok());

        let invalid_extension = vec![StagedFile {
            path: "bin/tool.exe".to_string(),
            content: String::new(),
            is_binary: false,
            size_bytes: 100,
            is_lfs_tracked: false,
            status: "added".to_string(),
        }];
        assert!(validator.validate_staged_files(&invalid_extension).is_err());

        let oversized_file = vec![StagedFile {
            path: "large_data.bin".to_string(),
            content: String::new(),
            is_binary: false,
            size_bytes: 2 * 1024 * 1024,
            is_lfs_tracked: false,
            status: "added".to_string(),
        }];
        assert!(validator.validate_staged_files(&oversized_file).is_err());
    }
}

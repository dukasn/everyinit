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

use evin_action_sdk::Logger;

#[derive(Debug, Default)]
pub(crate) struct DocsDebugStats {
    pub scans: usize,
    pub pages: usize,
    pub rst: usize,
    pub markdown: usize,
    pub references: usize,
    pub read_failures: usize,
    pub link_audits: usize,
    pub links: usize,
    pub anchors: usize,
    pub link_issues: usize,
    pub seo_audits: usize,
    pub indexable: usize,
    pub excluded: usize,
    pub sitemap_files: usize,
    pub missing_descriptions: usize,
    pub long_titles: usize,
    pub description_length: usize,
    pub duplicate_titles: usize,
    pub duplicate_descriptions: usize,
    pub deployment_warnings: usize,
    pub renders: usize,
    pub pages_written: usize,
    pub assets_written: usize,
    pub write_failures: usize,
    pub bundle: String,
    pub changes: usize,
    pub rebuilds: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub reloads: usize,
    pub last_duration_ms: u128,
    pub port: u16,
    pub network_url: Option<String>,
}

impl DocsDebugStats {
    pub fn emit_scan(&self, logger: &Logger) {
        evin_action_sdk::debug_event!(
            logger,
            evin_action_sdk::debug::DocsScan {
                scans: self.scans,
                pages: self.pages,
                rst: self.rst,
                markdown: self.markdown,
                references: self.references,
                read_failures: self.read_failures,
            }
        );
    }

    pub fn emit_links(&self, logger: &Logger, enabled: bool) {
        evin_action_sdk::debug_event!(
            logger,
            evin_action_sdk::debug::DocsLinkAudit {
                enabled,
                audits: self.link_audits,
                links: self.links,
                anchors: self.anchors,
                issues: self.link_issues,
            }
        );
    }

    pub fn emit_discovery(&self, logger: &Logger) {
        let quality_warnings = self.missing_descriptions
            + self.long_titles
            + self.description_length
            + self.duplicate_titles
            + self.duplicate_descriptions
            + self.deployment_warnings;
        evin_action_sdk::debug_event!(
            logger,
            evin_action_sdk::debug::DocsDiscoveryAudit {
                audits: self.seo_audits,
                indexable: self.indexable,
                excluded: self.excluded,
                sitemap_files: self.sitemap_files,
                quality_warnings,
                missing_descriptions: self.missing_descriptions,
                long_titles: self.long_titles,
                description_length: self.description_length,
                duplicate_titles: self.duplicate_titles,
                duplicate_descriptions: self.duplicate_descriptions,
                deployment_warnings: self.deployment_warnings,
            }
        );
    }

    pub fn emit_output(&self, logger: &Logger) {
        evin_action_sdk::debug_event!(
            logger,
            evin_action_sdk::debug::DocsOutput {
                renders: self.renders,
                pages_written: self.pages_written,
                assets_written: self.assets_written,
                write_failures: self.write_failures,
                bundle: self.bundle.clone(),
            }
        );
    }

    pub fn emit_serve(&self, logger: &Logger) {
        evin_action_sdk::debug_event!(
            logger,
            evin_action_sdk::debug::DocsServe {
                changes: self.changes,
                rebuilds: self.rebuilds,
                succeeded: self.succeeded,
                failed: self.failed,
                reloads: self.reloads,
                last_duration_ms: self.last_duration_ms,
                port: self.port,
                network_url: self.network_url.clone(),
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_action_sdk::{
        LogMessage,
        debug::{DebugAggregation, DebugValue},
        secrets::SecretsProvider,
    };
    use std::sync::Arc;
    use tokio::sync::mpsc;

    #[test]
    fn docs_metrics_are_coded_snapshots_and_rebuild_counts_are_cumulative() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let logger = Logger::new(
            "serve-site".to_string(),
            tx,
            Arc::new(SecretsProvider::default()),
        );
        let mut stats = DocsDebugStats {
            scans: 1,
            pages: 19,
            rst: 18,
            markdown: 1,
            references: 11,
            link_audits: 1,
            links: 37,
            anchors: 64,
            seo_audits: 1,
            excluded: 19,
            missing_descriptions: 1,
            renders: 1,
            pages_written: 19,
            assets_written: 6,
            bundle: "separated".to_string(),
            ..Default::default()
        };
        stats.emit_scan(&logger);
        stats.emit_links(&logger, true);
        stats.emit_discovery(&logger);
        stats.emit_output(&logger);
        stats.emit_serve(&logger);
        stats.changes = 3;
        stats.rebuilds = 3;
        stats.succeeded = 2;
        stats.failed = 1;
        stats.reloads = 2;
        stats.last_duration_ms = 184;
        stats.emit_serve(&logger);

        let messages: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(messages.len(), 6);
        assert!(messages.iter().all(|message| matches!(
            message,
            LogMessage::DebugRecord { record, .. }
                if matches!(record.aggregation, DebugAggregation::Snapshot { .. })
        )));
        let LogMessage::DebugRecord { record, .. } = &messages[5] else {
            unreachable!()
        };
        assert_eq!(record.code, "E_555");
        assert!(
            record
                .payload
                .contains(&("succeeded", DebugValue::Unsigned(2)))
        );
        assert!(
            record
                .payload
                .contains(&("failed", DebugValue::Unsigned(1)))
        );
        assert!(
            record
                .payload
                .contains(&("reloads", DebugValue::Unsigned(2)))
        );
    }

    #[test]
    fn disabled_link_validation_reports_zero_counts() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let logger = Logger::new(
            "build-site".to_string(),
            tx,
            Arc::new(SecretsProvider::default()),
        );
        DocsDebugStats::default().emit_links(&logger, false);
        let LogMessage::DebugRecord { record, .. } = rx.try_recv().unwrap() else {
            unreachable!()
        };
        assert_eq!(record.payload[0], ("enabled", DebugValue::Bool(false)));
        assert!(
            record.payload[1..]
                .iter()
                .all(|(_, value)| *value == DebugValue::Unsigned(0))
        );
    }
}

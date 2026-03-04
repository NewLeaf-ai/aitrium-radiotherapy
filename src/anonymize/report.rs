use crate::anonymize::policy::{RuleAction, RuleSource};
use crate::types::{
    AnonymizeActionCounts, AnonymizeDecisionTrace, AnonymizeOutputSummary, AnonymizeRuleCounts,
    AnonymizeSafetyChecks, AnonymizeSourceSummary, RtAnonymizeMetadataResponse, SCHEMA_VERSION,
};
use std::time::Instant;

pub struct ReportState {
    started_at: Instant,
    pub response: RtAnonymizeMetadataResponse,
}

impl ReportState {
    pub fn new(mode: &str, source_path: &str, output_path: Option<String>) -> Self {
        Self {
            started_at: Instant::now(),
            response: RtAnonymizeMetadataResponse {
                schema_version: SCHEMA_VERSION.to_string(),
                mode: mode.to_string(),
                source_summary: AnonymizeSourceSummary {
                    source_path: source_path.to_string(),
                    total_files: 0,
                    dicom_files: 0,
                    non_dicom_files: 0,
                },
                output_summary: AnonymizeOutputSummary {
                    output_path,
                    files_written: 0,
                    dicom_written: 0,
                    non_dicom_copied: 0,
                },
                action_counts: AnonymizeActionCounts::default(),
                rule_counts: AnonymizeRuleCounts::default(),
                warnings: Vec::new(),
                errors: Vec::new(),
                safety_checks: AnonymizeSafetyChecks::default(),
                duration_ms: 0,
                decision_trace: Vec::new(),
            },
        }
    }

    pub fn record_action(&mut self, action: &RuleAction) {
        match action {
            RuleAction::Keep => self.response.action_counts.keep += 1,
            RuleAction::Remove => self.response.action_counts.remove += 1,
            RuleAction::Empty => self.response.action_counts.empty += 1,
            RuleAction::Replace => self.response.action_counts.replace += 1,
        }
    }

    pub fn record_rule_source(&mut self, source: RuleSource) {
        match source {
            RuleSource::TagExact | RuleSource::TagKeyword => self.response.rule_counts.tag += 1,
            RuleSource::VrFallback => self.response.rule_counts.vr += 1,
            RuleSource::DefaultPrivate => self.response.rule_counts.default_private += 1,
            RuleSource::DefaultUnknownPublic => {
                self.response.rule_counts.default_unknown_public += 1
            }
            RuleSource::DefaultKeep => self.response.rule_counts.default_keep += 1,
        }
    }

    pub fn push_warning(&mut self, value: impl Into<String>) {
        self.response.warnings.push(value.into());
    }

    pub fn push_error(&mut self, value: impl Into<String>) {
        self.response.errors.push(value.into());
    }

    pub fn push_trace(&mut self, trace: AnonymizeDecisionTrace) {
        self.response.decision_trace.push(trace);
    }

    pub fn finish(mut self) -> RtAnonymizeMetadataResponse {
        self.response.duration_ms = self.started_at.elapsed().as_millis() as u64;
        self.response
    }
}

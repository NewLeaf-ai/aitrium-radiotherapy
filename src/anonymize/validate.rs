use crate::anonymize::policy::{AnonymizationPolicy, DateStrategy, ReplaceMode, RuleAction};
use crate::types::{ApiError, ErrorCode};

pub fn validate_policy(policy: &AnonymizationPolicy) -> Result<(), ApiError> {
    for (selector, rule) in &policy.tag_rules {
        validate_rule(&format!("tag_rules.{selector}"), rule)?;
    }

    for (vr, rule) in &policy.vr_rules {
        validate_rule(&format!("vr_rules.{vr}"), rule)?;
    }

    Ok(())
}

fn validate_rule(path: &str, rule: &crate::anonymize::policy::RuleSpec) -> Result<(), ApiError> {
    match rule.action {
        RuleAction::Keep | RuleAction::Remove | RuleAction::Empty => Ok(()),
        RuleAction::Replace => {
            let Some(replace) = &rule.replace else {
                return Err(ApiError::new(
                    ErrorCode::InvalidInput,
                    format!("{path}: replace action requires replace configuration"),
                ));
            };

            let mode = replace.mode.clone().unwrap_or_else(|| {
                if replace.uid_root.is_some() {
                    ReplaceMode::UidMap
                } else if replace.date_strategy.is_some() {
                    ReplaceMode::DateTransform
                } else if replace.token_prefix.is_some() || replace.token_length.is_some() {
                    ReplaceMode::TokenMap
                } else {
                    ReplaceMode::Literal
                }
            });

            match mode {
                ReplaceMode::Literal => {
                    if replace.literal.is_none() {
                        return Err(ApiError::new(
                            ErrorCode::InvalidInput,
                            format!("{path}: literal replace mode requires replace.literal"),
                        ));
                    }
                }
                ReplaceMode::UidMap => {
                    if let Some(root) = &replace.uid_root {
                        if !is_valid_uid_root(root) {
                            return Err(ApiError::new(
                                ErrorCode::InvalidInput,
                                format!("{path}: invalid uid_root '{root}'"),
                            ));
                        }
                    }
                }
                ReplaceMode::DateTransform => {
                    let strategy = replace
                        .date_strategy
                        .clone()
                        .unwrap_or(DateStrategy::KeepYearOnly);
                    match strategy {
                        DateStrategy::KeepYearOnly => {}
                        DateStrategy::FixedShiftDataset => {
                            if replace.days_shift.is_none() {
                                return Err(ApiError::new(
                                    ErrorCode::InvalidInput,
                                    format!(
                                        "{path}: fixed_shift_dataset requires replace.days_shift"
                                    ),
                                ));
                            }
                        }
                        DateStrategy::FixedValue => {
                            if replace.fixed_value.is_none() {
                                return Err(ApiError::new(
                                    ErrorCode::InvalidInput,
                                    format!("{path}: fixed_value requires replace.fixed_value"),
                                ));
                            }
                        }
                    }
                }
                ReplaceMode::TokenMap => {
                    if let Some(prefix) = &replace.token_prefix {
                        if prefix.is_empty() {
                            return Err(ApiError::new(
                                ErrorCode::InvalidInput,
                                format!("{path}: token_prefix must not be empty"),
                            ));
                        }
                        if !prefix
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                        {
                            return Err(ApiError::new(
                                ErrorCode::InvalidInput,
                                format!(
                                    "{path}: token_prefix must contain only [A-Za-z0-9_-] characters"
                                ),
                            ));
                        }
                    }

                    if let Some(length) = replace.token_length {
                        if !(4..=64).contains(&length) {
                            return Err(ApiError::new(
                                ErrorCode::InvalidInput,
                                format!("{path}: token_length must be between 4 and 64"),
                            ));
                        }
                    }
                }
            }

            Ok(())
        }
    }
}

fn is_valid_uid_root(input: &str) -> bool {
    if input.is_empty() || input.len() > 62 {
        return false;
    }
    if input.starts_with('.') || input.ends_with('.') || input.contains("..") {
        return false;
    }
    input.chars().all(|c| c.is_ascii_digit() || c == '.')
}

use crate::types::{ApiError, ErrorCode};
use dicom_core::dictionary::{DataDictionary, DataDictionaryEntry};
use dicom_core::{Tag, VR};
use dicom_dictionary_std::StandardDataDictionary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Keep,
    Remove,
    Empty,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplaceMode {
    Literal,
    UidMap,
    DateTransform,
    TokenMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateStrategy {
    KeepYearOnly,
    FixedShiftDataset,
    FixedValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplaceConfig {
    #[serde(default)]
    pub mode: Option<ReplaceMode>,
    #[serde(default)]
    pub literal: Option<String>,
    #[serde(default)]
    pub uid_root: Option<String>,
    #[serde(default)]
    pub date_strategy: Option<DateStrategy>,
    #[serde(default)]
    pub fixed_value: Option<String>,
    #[serde(default)]
    pub days_shift: Option<i32>,
    #[serde(default)]
    pub token_prefix: Option<String>,
    #[serde(default)]
    pub token_length: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSpec {
    pub action: RuleAction,
    #[serde(default)]
    pub replace: Option<ReplaceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDefaults {
    #[serde(default = "default_private_action")]
    pub private_tag_default: RuleAction,
    #[serde(default = "default_unknown_public_action")]
    pub unknown_public_default: RuleAction,
}

fn default_private_action() -> RuleAction {
    RuleAction::Remove
}

fn default_unknown_public_action() -> RuleAction {
    RuleAction::Keep
}

impl Default for PolicyDefaults {
    fn default() -> Self {
        Self {
            private_tag_default: default_private_action(),
            unknown_public_default: default_unknown_public_action(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnonymizationPolicy {
    #[serde(default)]
    pub tag_rules: BTreeMap<String, RuleSpec>,
    #[serde(default)]
    pub vr_rules: BTreeMap<String, RuleSpec>,
    #[serde(default)]
    pub defaults: PolicyDefaults,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSource {
    TagExact,
    TagKeyword,
    VrFallback,
    DefaultPrivate,
    DefaultUnknownPublic,
    DefaultKeep,
}

#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    exact_tag_rules: BTreeMap<Tag, RuleSpec>,
    keyword_rules: BTreeMap<String, RuleSpec>,
    vr_rules: BTreeMap<String, RuleSpec>,
    pub defaults: PolicyDefaults,
}

#[derive(Debug, Clone)]
pub struct RuleResolution {
    pub source: RuleSource,
    pub rule: RuleSpec,
}

impl CompiledPolicy {
    pub fn compile(policy: AnonymizationPolicy) -> Result<Self, ApiError> {
        let mut exact_tag_rules = BTreeMap::new();
        let mut keyword_rules = BTreeMap::new();
        let mut vr_rules = BTreeMap::new();

        for (selector, rule) in policy.tag_rules {
            if looks_like_tag_expr(&selector) {
                let Some(tag) = StandardDataDictionary.parse_tag(&selector) else {
                    return Err(ApiError::new(
                        ErrorCode::InvalidInput,
                        format!("Invalid tag selector in policy: '{selector}'"),
                    ));
                };
                exact_tag_rules.insert(tag, rule);
            } else {
                let Some(entry) = StandardDataDictionary.by_name(selector.trim()) else {
                    return Err(ApiError::new(
                        ErrorCode::InvalidInput,
                        format!("Unknown DICOM keyword in policy: '{selector}'"),
                    ));
                };
                keyword_rules.insert(entry.alias().to_ascii_lowercase(), rule);
            }
        }

        for (vr, rule) in policy.vr_rules {
            let key = vr.trim().to_ascii_uppercase();
            if parse_vr(&key).is_none() {
                return Err(ApiError::new(
                    ErrorCode::InvalidInput,
                    format!("Invalid VR selector in policy: '{vr}'"),
                ));
            }
            vr_rules.insert(key, rule);
        }

        Ok(Self {
            exact_tag_rules,
            keyword_rules,
            vr_rules,
            defaults: policy.defaults,
        })
    }

    pub fn resolve(
        &self,
        tag: Tag,
        vr: VR,
        keyword: Option<&str>,
        is_private: bool,
        is_known_public: bool,
    ) -> RuleResolution {
        if let Some(rule) = self.exact_tag_rules.get(&tag) {
            return RuleResolution {
                source: RuleSource::TagExact,
                rule: rule.clone(),
            };
        }

        if let Some(keyword) = keyword {
            let key = keyword.to_ascii_lowercase();
            if let Some(rule) = self.keyword_rules.get(&key) {
                return RuleResolution {
                    source: RuleSource::TagKeyword,
                    rule: rule.clone(),
                };
            }
        }

        let vr_key = vr.to_string();
        if let Some(rule) = self.vr_rules.get(vr_key) {
            return RuleResolution {
                source: RuleSource::VrFallback,
                rule: rule.clone(),
            };
        }

        if is_private {
            return RuleResolution {
                source: RuleSource::DefaultPrivate,
                rule: RuleSpec {
                    action: self.defaults.private_tag_default.clone(),
                    replace: None,
                },
            };
        }

        if !is_known_public {
            return RuleResolution {
                source: RuleSource::DefaultUnknownPublic,
                rule: RuleSpec {
                    action: self.defaults.unknown_public_default.clone(),
                    replace: None,
                },
            };
        }

        RuleResolution {
            source: RuleSource::DefaultKeep,
            rule: RuleSpec {
                action: RuleAction::Keep,
                replace: None,
            },
        }
    }
}

fn looks_like_tag_expr(selector: &str) -> bool {
    let s = selector.trim();
    if s.starts_with('(') && s.ends_with(')') {
        return true;
    }
    s.contains(',') || (s.len() == 8 && s.chars().all(|c| c.is_ascii_hexdigit()))
}

fn parse_vr(input: &str) -> Option<VR> {
    use std::str::FromStr;
    VR::from_str(input).ok()
}

pub fn is_private_tag(tag: Tag) -> bool {
    tag.0 % 2 == 1
}

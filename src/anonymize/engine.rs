use crate::anonymize::date::{derive_shift, transform_date_like};
use crate::anonymize::policy::{
    CompiledPolicy, DateStrategy, ReplaceMode, RuleAction, RuleResolution,
};
use crate::anonymize::report::ReportState;
use crate::anonymize::uid::UidMapper;
use crate::types::{AnonymizeDecisionTrace, ApiError, ErrorCode};
use dicom_core::dictionary::{DataDictionary, DataDictionaryEntry};
use dicom_core::header::Header;
use dicom_core::value::{DataSetSequence, InMemFragment, PrimitiveValue, Value};
use dicom_core::{DataElement, Length, Tag, VR};
use dicom_dictionary_std::tags;
use dicom_dictionary_std::StandardDataDictionary;
use dicom_object::DefaultDicomObject;
use dicom_object::InMemDicomObject;

pub struct EngineContext {
    pub policy: CompiledPolicy,
    pub include_trace: bool,
    pub uid_mapper: UidMapper,
    pub default_days_shift: i32,
}

impl EngineContext {
    pub fn new(
        policy: CompiledPolicy,
        include_trace: bool,
        deterministic_secret: Option<&str>,
    ) -> Result<Self, ApiError> {
        let uid_mapper = UidMapper::new(None, deterministic_secret)?;
        let default_days_shift = derive_shift(uid_mapper.secret_bytes());
        Ok(Self {
            policy,
            include_trace,
            uid_mapper,
            default_days_shift,
        })
    }

    pub fn transform_file(
        &mut self,
        file: DefaultDicomObject,
        report: &mut ReportState,
        file_label: &str,
    ) -> Result<DefaultDicomObject, ApiError> {
        let meta = file.meta().clone();
        let data = file.into_inner();
        let transformed = self.transform_dataset(data, report, file_label, "".to_string())?;

        let mut rebuilt = transformed.with_exact_meta(meta);
        sync_meta_uid_from_dataset(&mut rebuilt);
        Ok(rebuilt)
    }

    fn transform_dataset(
        &mut self,
        dataset: InMemDicomObject,
        report: &mut ReportState,
        file_label: &str,
        prefix: String,
    ) -> Result<InMemDicomObject, ApiError> {
        let mut output = InMemDicomObject::new_empty();

        for element in dataset {
            let (header, value) = element.into_parts();
            let tag = header.tag();
            let vr = header.vr();
            let entry = StandardDataDictionary.by_tag(tag);
            let keyword = entry.map(|e| e.alias().to_string());
            let is_private = crate::anonymize::policy::is_private_tag(tag);
            let is_known_public = entry.is_some();

            let resolution =
                self.policy
                    .resolve(tag, vr, keyword.as_deref(), is_private, is_known_public);

            report.record_rule_source(resolution.source);
            report.record_action(&resolution.rule.action);

            let selector = selector_text(&prefix, tag);
            if self.include_trace {
                report.push_trace(AnonymizeDecisionTrace {
                    file: file_label.to_string(),
                    selector: selector.clone(),
                    keyword,
                    vr: vr.to_string().to_string(),
                    action: format!("{:?}", resolution.rule.action).to_ascii_lowercase(),
                    rule_source: format!("{:?}", resolution.source).to_ascii_lowercase(),
                });
            }

            match resolution.rule.action {
                RuleAction::Remove => {
                    continue;
                }
                RuleAction::Empty => {
                    output.put(DataElement::empty(tag, vr));
                }
                RuleAction::Keep => {
                    let transformed_value =
                        self.transform_value(value, report, file_label, selector, &resolution)?;
                    output.put(DataElement::new(tag, vr, transformed_value));
                }
                RuleAction::Replace => {
                    let transformed_value = self.transform_replace_value(
                        value,
                        vr,
                        report,
                        &resolution,
                        file_label,
                        selector,
                    )?;
                    output.put(DataElement::new(tag, vr, transformed_value));
                }
            }
        }

        Ok(output)
    }

    fn transform_value(
        &mut self,
        value: Value<InMemDicomObject, InMemFragment>,
        report: &mut ReportState,
        file_label: &str,
        selector: String,
        _resolution: &RuleResolution,
    ) -> Result<Value<InMemDicomObject, InMemFragment>, ApiError> {
        match value {
            Value::Sequence(sequence) => {
                let mut out_items = Vec::new();
                for (index, item) in sequence.into_items().into_iter().enumerate() {
                    let nested_prefix = format!("{}[{}]", selector, index);
                    let transformed =
                        self.transform_dataset(item, report, file_label, nested_prefix)?;
                    out_items.push(transformed);
                }
                Ok(Value::from(DataSetSequence::new(
                    out_items,
                    Length::UNDEFINED,
                )))
            }
            other => Ok(other),
        }
    }

    fn transform_replace_value(
        &mut self,
        value: Value<InMemDicomObject, InMemFragment>,
        vr: VR,
        report: &mut ReportState,
        resolution: &RuleResolution,
        file_label: &str,
        selector: String,
    ) -> Result<Value<InMemDicomObject, InMemFragment>, ApiError> {
        let Some(replace) = resolution.rule.replace.as_ref() else {
            return Err(ApiError::new(
                ErrorCode::InvalidInput,
                "replace action requires replace configuration",
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

        match value {
            Value::Primitive(primitive) => {
                let replaced = match mode {
                    ReplaceMode::Literal => {
                        let literal = replace.literal.clone().unwrap_or_default();
                        PrimitiveValue::from(literal)
                    }
                    ReplaceMode::UidMap => {
                        let mapped = remap_uid_string(&primitive.to_str(), &mut self.uid_mapper)?;
                        PrimitiveValue::from(mapped)
                    }
                    ReplaceMode::DateTransform => {
                        let strategy = replace
                            .date_strategy
                            .clone()
                            .unwrap_or(DateStrategy::KeepYearOnly);
                        let shift = replace.days_shift.unwrap_or(self.default_days_shift);
                        let transformed = transform_date_like(
                            &primitive.to_str(),
                            vr.to_string(),
                            strategy,
                            shift,
                            replace.fixed_value.as_deref(),
                        )
                        .ok_or_else(|| {
                            ApiError::new(
                                ErrorCode::InvalidInput,
                                format!(
                                    "Unable to apply date transformation for VR={} using configured strategy",
                                    vr.to_string()
                                ),
                            )
                        })?;
                        PrimitiveValue::from(transformed)
                    }
                    ReplaceMode::TokenMap => {
                        let prefix = replace.token_prefix.as_deref().unwrap_or("ID");
                        let token_length = replace.token_length.unwrap_or(12) as usize;
                        let mapped = remap_token_string(
                            &primitive.to_str(),
                            &self.uid_mapper,
                            prefix,
                            token_length,
                        )?;
                        PrimitiveValue::from(mapped)
                    }
                };

                Ok(Value::from(replaced))
            }
            Value::Sequence(sequence) => {
                report.push_warning(
                    "Replace action targeted sequence value; applying keep+recurse behavior for sequence items",
                );
                let mut out_items = Vec::new();
                for (index, item) in sequence.into_items().into_iter().enumerate() {
                    let nested_prefix = format!("{}[{}]", selector, index);
                    let transformed =
                        self.transform_dataset(item, report, file_label, nested_prefix)?;
                    out_items.push(transformed);
                }
                Ok(Value::from(DataSetSequence::new(
                    out_items,
                    Length::UNDEFINED,
                )))
            }
            Value::PixelSequence(pixels) => Ok(Value::PixelSequence(pixels)),
        }
    }
}

fn selector_text(prefix: &str, tag: Tag) -> String {
    let own = format!("({:04X},{:04X})", tag.0, tag.1);
    if prefix.is_empty() {
        own
    } else {
        format!("{prefix}.{own}")
    }
}

fn remap_uid_string(value: &str, mapper: &mut UidMapper) -> Result<String, ApiError> {
    let mut mapped = Vec::new();
    for part in value.trim().split('\\') {
        if part.trim().is_empty() {
            continue;
        }
        mapped.push(mapper.map_uid(part.trim())?);
    }

    if mapped.is_empty() {
        Ok(value.to_string())
    } else {
        Ok(mapped.join("\\"))
    }
}

fn remap_token_string(
    value: &str,
    mapper: &UidMapper,
    prefix: &str,
    token_length: usize,
) -> Result<String, ApiError> {
    let mut mapped = Vec::new();
    for part in value.trim().split('\\') {
        if part.trim().is_empty() {
            continue;
        }
        mapped.push(mapper.map_token(part.trim(), prefix, token_length)?);
    }

    if mapped.is_empty() {
        Ok(value.to_string())
    } else {
        Ok(mapped.join("\\"))
    }
}

fn sync_meta_uid_from_dataset(file: &mut DefaultDicomObject) {
    if let Ok(sop_instance_uid) = file.element(tags::SOP_INSTANCE_UID) {
        if let Ok(uid) = sop_instance_uid.value().to_str() {
            file.meta_mut().media_storage_sop_instance_uid = uid.to_string();
        }
    }
    if let Ok(sop_class_uid) = file.element(tags::SOP_CLASS_UID) {
        if let Ok(uid) = sop_class_uid.value().to_str() {
            file.meta_mut().media_storage_sop_class_uid = uid.to_string();
        }
    }
}

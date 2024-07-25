use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::coerce;
use crate::{context::Context, DatamodelError};

use indexmap::IndexMap;
use internal_baml_diagnostics::{DatamodelWarning, Span};
use internal_baml_prompt_parser::ast::{ChatBlock, PrinterBlock, Variable};
use internal_baml_schema_ast::ast::{
    self, AdapterId, ClassId, ConfigurationId, EnumId, EnumValueId, Expression, FieldId, FieldType,
    RawString, SerializerFieldId, VariantConfigId, VariantSerializerId, WithIdentifier, WithName,
    WithSpan,
};

mod configurations;
mod prompt;
mod to_string_attributes;
mod types;

use prompt::validate_prompt;

pub use to_string_attributes::{
    DynamicStringAttributes, StaticStringAttributes, ToStringAttributes,
};
pub(crate) use types::EnumAttributes;
pub(crate) use types::*;

pub(super) fn resolve_types(ctx: &mut Context<'_>) {
    for (top_id, top) in ctx.ast.iter_tops() {
        match (top_id, top) {
            (_, ast::Top::Enum(enm)) => visit_enum(enm, ctx),
            (ast::TopId::Class(idx), ast::Top::Class(model)) => visit_class(idx, model, ctx),
            (_, ast::Top::Class(_)) => unreachable!("Class misconfigured"),
            (ast::TopId::TemplateString(idx), ast::Top::TemplateString(template_string)) => {
                visit_template_string(idx, template_string, ctx)
            }
            (_, ast::Top::TemplateString(_)) => unreachable!("TemplateString misconfigured"),
            (ast::TopId::Variant(idx), ast::Top::Variant(variant)) => {
                visit_variant(idx, variant, ctx)
            }
            (_, ast::Top::Variant(_)) => unreachable!("Variant misconfigured"),
            (ast::TopId::Config((idx, _)), ast::Top::Config(cfg)) => {
                visit_config(idx, cfg, ctx);
            }
            (_, ast::Top::Config(_)) => unreachable!("Config misconfigured"),
        }
    }
}

#[derive(Debug, Clone)]
/// Variables used inside of raw strings.
pub enum PromptVariable {
    /// Input variable.
    Input(Variable),
    /// Output variable.
    Enum(PrinterBlock),
    /// Output variable.
    Type(PrinterBlock),
    /// Chat
    Chat(ChatBlock),
}

impl Hash for PromptVariable {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            PromptVariable::Chat(blk) => {
                "chat".hash(state);
                blk.role.0.hash(state);
            }
            PromptVariable::Input(var) => {
                "input".hash(state);
                var.text.hash(state);
            }
            PromptVariable::Enum(blk) => {
                "enum".hash(state);
                blk.hash(state);
            }
            PromptVariable::Type(blk) => {
                "type".hash(state);
                blk.hash(state);
            }
        }
    }
}

impl<'a> PromptVariable {
    /// Unique Key
    pub fn key(&self) -> String {
        match self {
            PromptVariable::Input(var) => var.key(),
            PromptVariable::Enum(blk) => blk.key(),
            PromptVariable::Type(blk) => blk.key(),
            PromptVariable::Chat(blk) => blk.key(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StringValue {
    pub value: String,
    pub span: Span,
    pub key_span: Span,
}

#[derive(Debug)]
pub struct VariantProperties {
    pub client: StringValue,
    pub prompt: StringValue,
    pub prompt_replacements: Vec<PromptVariable>,
    pub replacers: (
        HashMap<Variable, String>,
        HashMap<PrinterBlock, String>,
        Vec<ChatBlock>,
    ),
    pub output_adapter: Option<(AdapterId, Vec<RawString>)>,
}

/// The representation of a prompt.
pub enum PromptAst<'a> {
    /// For single string prompts
    /// Prompt + Any used input replacers (key, val)
    String(String, Vec<(String, String)>),

    /// For prompts with multiple parts
    /// ChatBlock + Prompt + Any used input replacers (key, val)
    Chat(Vec<(Option<&'a ChatBlock>, String)>, Vec<(String, String)>),
}

impl VariantProperties {
    pub fn output_adapter_for_language(&self, language: &str) -> Option<&str> {
        self.output_adapter.as_ref().and_then(|f| {
            f.1.iter()
                .find(|r| r.language.as_ref().map(|(l, _)| l.as_str()) == Some(language))
                .map(|r| r.value())
        })
    }

    pub fn to_prompt(&self) -> PromptAst<'_> {
        let (input, output, chats) = &self.replacers;

        // Replace all the inputs with the input replacers
        let mut used_inputs = vec![];
        let prompt = input
            .iter()
            .fold(self.prompt.value.clone(), |prompt, (k, val)| {
                // Only add the input if it's used in the prompt
                let key = k.key();
                if prompt.contains(&key) {
                    used_inputs.push((key.clone(), val.clone()));
                    prompt
                } else {
                    prompt
                }
            });
        // Replace all the outputs with the output replacers
        let prompt = output.iter().fold(prompt, |prompt, (k, val)| {
            prompt.replace(&k.key(), &val.to_string())
        });

        used_inputs.sort();

        if chats.is_empty() {
            PromptAst::String(prompt, used_inputs)
        } else {
            // Split the prompt into parts based on the chat blocks.
            let mut last_idx = 0;
            let mut parts = vec![];
            for chat in chats {
                let splitter = chat.key();
                let idx = prompt[last_idx..].find(&splitter);
                if let Some(idx) = idx {
                    parts.push((
                        Some(chat),
                        (idx + last_idx, idx + last_idx + splitter.len()),
                    ));
                    last_idx += idx + splitter.len();
                }
            }

            match parts.first() {
                // If the first chat block is not at the start of the prompt, add the first part.
                Some(&(Some(_), (start, _))) if start > 0 => {
                    parts.insert(0, (None, (0, 0)));
                }
                Some(_) => {}
                _ => unreachable!("At least one chat block should exist"),
            }

            // Each chat block owns a part of the prompt. until the next chat block.
            PromptAst::Chat(
                parts
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, &(chat, (_, start)))| {
                        let end = if idx + 1 < parts.len() {
                            parts[idx + 1].1 .0
                        } else {
                            prompt.len()
                        };

                        let p = prompt[start..end].trim();
                        if p.is_empty() {
                            // info!("Skipping empty prompt part: {} {} {}", idx, start, end);
                            None
                        } else {
                            Some((chat, p.to_string()))
                        }
                    })
                    .collect(),
                used_inputs,
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub functions: Vec<(String, Span)>,
    // The span is the span of the argument (the expression has its own span)
    pub args: IndexMap<String, (Span, Expression)>,
    pub args_field_span: Span,
}

#[derive(Debug, Clone)]
pub struct Printer {
    pub template: (String, Span),
}

#[derive(Debug, Clone)]
/// The type of printer.
pub enum PrinterType {
    /// For types
    Type(Printer),
    /// For enums
    Enum(Printer),
}

impl PrinterType {
    /// The code template.
    pub fn template(&self) -> &str {
        match self {
            PrinterType::Type(printer) => &printer.template.0,
            PrinterType::Enum(printer) => &printer.template.0,
        }
    }
}

/// How to retry a request.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// The maximum number of retries.
    pub max_retries: u32,
    /// The strategy to use.
    pub strategy: RetryPolicyStrategy,
    /// Any additional options.
    pub options: Option<Vec<((String, Span), Expression)>>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
/// The strategy to use for retrying a request.
pub enum RetryPolicyStrategy {
    /// Constant delay.
    ConstantDelay(ContantDelayStrategy),
    /// Exponential backoff.
    ExponentialBackoff(ExponentialBackoffStrategy),
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
/// The strategy to use for retrying a request.
pub struct ContantDelayStrategy {
    /// The delay in milliseconds.
    pub delay_ms: u32,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
/// The strategy to use for retrying a request.
pub struct ExponentialBackoffStrategy {
    /// The delay in milliseconds.
    pub delay_ms: u32,
    /// The multiplier.
    pub multiplier: f32,
    /// The maximum delay in milliseconds.
    pub max_delay_ms: u32,
}

#[derive(Debug, Clone)]
pub struct TemplateStringProperties {
    /// This is dedented and trimmed.
    pub template: String,
}

#[derive(Debug, Default)]
pub(super) struct Types {
    pub(super) enum_attributes: HashMap<ast::EnumId, EnumAttributes>,
    pub(super) class_attributes: HashMap<ast::ClassId, ClassAttributes>,
    pub(super) class_dependencies: HashMap<ast::ClassId, HashSet<String>>,
    pub(super) variant_attributes: HashMap<ast::VariantConfigId, VariantAttributes>,
    pub(super) variant_properties: HashMap<ast::VariantConfigId, VariantProperties>,
    pub(super) retry_policies: HashMap<ast::ConfigurationId, RetryPolicy>,
    pub(super) printers: HashMap<ast::ConfigurationId, PrinterType>,
    pub(super) test_cases: HashMap<ast::ConfigurationId, TestCase>,
    pub(super) template_strings: HashMap<ast::TemplateStringId, TemplateStringProperties>,
}

impl Types {
    pub(super) fn refine_class_field(
        &self,
        (class_id, field_id): (ClassId, FieldId),
    ) -> either::Either<StaticFieldId, DynamicFieldId> {
        match self.class_attributes.get(&class_id) {
            Some(attrs) => match attrs.field_serilizers.get(&field_id) {
                Some(ToStringAttributes::Dynamic(_attrs)) => either::Either::Right(field_id.into()),
                _ => either::Either::Left(field_id.into()),
            },
            None => either::Either::Left(field_id.into()),
        }
    }

    pub(super) fn refine_enum_value(
        &self,
        (enum_id, value_id): (EnumId, EnumValueId),
    ) -> either::Either<StaticFieldId, DynamicFieldId> {
        match self.enum_attributes.get(&enum_id) {
            Some(attrs) => match attrs.value_serilizers.get(&value_id) {
                Some(ToStringAttributes::Dynamic(_attrs)) => either::Either::Right(value_id.into()),
                _ => either::Either::Left(value_id.into()),
            },
            None => either::Either::Left(value_id.into()),
        }
    }

    #[allow(dead_code)]
    pub(super) fn refine_serializer_field(
        &self,
        (variant_id, serializer_id, value_id): (
            VariantConfigId,
            VariantSerializerId,
            SerializerFieldId,
        ),
    ) -> either::Either<StaticFieldId, DynamicFieldId> {
        match self
            .variant_attributes
            .get(&variant_id)
            .and_then(|r| r.serializers.get(&serializer_id))
        {
            Some(attrs) => match attrs.field_serilizers.get(&value_id) {
                Some(ToStringAttributes::Dynamic(_)) => either::Either::Right(value_id.into()),
                _ => either::Either::Left(value_id.into()),
            },
            None => either::Either::Left(value_id.into()),
        }
    }
}

fn visit_template_string<'db>(
    idx: ast::TemplateStringId,
    template_string: &'db ast::TemplateString,
    ctx: &mut Context<'db>,
) {
    ctx.types.template_strings.insert(
        idx,
        TemplateStringProperties {
            template: template_string
                .value()
                .as_raw_string_value()
                .map(|v| v.value().to_string())
                .unwrap(),
        },
    );
}

fn visit_enum<'db>(_enm: &'db ast::Enum, _ctx: &mut Context<'db>) {}

fn visit_class<'db>(class_id: ast::ClassId, class: &'db ast::Class, ctx: &mut Context<'db>) {
    let used_types = class
        .iter_fields()
        .flat_map(|(_, f)| f.field_type.flat_idns())
        .filter(|id| {
            id.is_valid_type()
                && match id {
                    ast::Identifier::Primitive(..) => false,
                    _ => true,
                }
        })
        .map(|f| f.name().to_string())
        .collect::<HashSet<_>>();
    ctx.types.class_dependencies.insert(class_id, used_types);
}

fn visit_variant<'db>(idx: VariantConfigId, variant: &'db ast::Variant, ctx: &mut Context<'db>) {
    if !variant.is_llm() {
        ctx.push_error(DatamodelError::new_validation_error(
            "Only LLM variants are supported. Use: impl<llm>",
            variant.span().clone(),
        ));
        return;
    }

    let mut client = None;
    let mut prompt = None;

    variant
        .iter_fields()
        .for_each(|(_idx, field)| match field.name() {
            "client" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `client` instead of `client<...>`?",
                        field.span().clone(),
                    ));
                }
                match field.value.as_ref() {
                    Some(item) => client = Some((item, field.identifier().span().clone())),
                    _ => {}
                }
            }
            "prompt" => {
                if field.template_args.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Did you mean `prompt` instead of `prompt<...>`?",
                        field.span().clone(),
                    ));
                }
                match field.value.as_ref() {
                    Some(item) => prompt = Some((item, field.identifier().span().clone())),
                    _ => {}
                }
            }
            config => ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in impl<llm>", config),
                field.span().clone(),
            )),
        });

    let client = if let Some((client, client_key_span)) = client {
        coerce::string_with_span(client, ctx.diagnostics).map(|client| (client, client_key_span))
    } else {
        ctx.push_error(DatamodelError::new_validation_error(
            "Missing `client` field in impl<llm>",
            variant.identifier().span().clone(),
        ));
        None
    };

    let prompt = if let Some((prompt, prompt_key_span)) = prompt {
        if let Some(prompt) = prompt.as_raw_string_value() {
            validate_prompt(ctx, prompt).map(|(cleaned_prompt, replacer)| {
                ((cleaned_prompt, prompt.span(), replacer), prompt_key_span)
            })
        } else if let Some((prompt, span)) = coerce::string_with_span(prompt, ctx.diagnostics) {
            // warn the user that we are using this without validation.
            ctx.push_warning(DatamodelWarning::new(
                "To use comments and {#vars} use a block string. #\"...\"# instead.".into(),
                span.clone(),
            ));
            Some((
                (prompt.to_string(), span, Default::default()),
                prompt_key_span,
            ))
        } else {
            // Errors are handled by coerce.
            None
        }
    } else {
        ctx.push_error(DatamodelError::new_validation_error(
            "Missing `prompt` field in impl<llm>",
            variant.identifier().span().clone(),
        ));
        None
    };

    // Ensure that the adapters are valid.
    let (_input_adapter, output_adapter) =
        variant
            .iter_adapters()
            .fold((None, None), |prev, (idx, adapter)| {
                let is_input = match &adapter.from {
                    FieldType::Identifier(arity, idn) if idn.name() == "input" => {
                        if arity.is_optional() {
                            ctx.push_error(DatamodelError::new_validation_error(
                                "The `input` adapter cannot be optional.",
                                idn.span().clone(),
                            ));
                            false
                        } else {
                            true
                        }
                    }
                    _ => false,
                };

                let is_output = match &adapter.to {
                    FieldType::Identifier(arity, idn) if idn.name() == "output" => {
                        if arity.is_optional() {
                            ctx.push_error(DatamodelError::new_validation_error(
                                "The `output` adapter cannot be optional.",
                                idn.span().clone(),
                            ));
                            false
                        } else {
                            true
                        }
                    }
                    _ => false,
                };

                if is_input && is_output {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "The `input` and `output` adapters cannot be used together.",
                        adapter.span().clone(),
                    ));
                } else if is_input {
                    if prev.0.is_some() {
                        ctx.push_error(DatamodelError::new_validation_error(
                            "The `input` adapter can only be used once.",
                            adapter.span().clone(),
                        ));
                    } else {
                        // Ensure the expr is either a string of array of strings.
                        let impls = if let Some((arr, _)) = adapter.converter.as_array() {
                            Some(
                                arr.iter()
                                    .filter_map(|item| coerce::raw_string(item, ctx.diagnostics))
                                    .collect::<Vec<_>>(),
                            )
                        } else {
                            coerce::raw_string(&adapter.converter, ctx.diagnostics)
                                .map(|raw| vec![raw])
                        };

                        if let Some(impls) = impls {
                            ctx.push_warning(DatamodelWarning::new(
                                "The `input` adapter is note yet supported.".into(),
                                adapter.span().clone(),
                            ));
                            return (Some((idx, impls)), prev.1);
                        }
                    }
                } else if is_output {
                    if prev.1.is_some() {
                        ctx.push_error(DatamodelError::new_validation_error(
                            "The `output` adapter can only be used once.",
                            adapter.span().clone(),
                        ));
                    } else {
                        let impls = if let Some((arr, _)) = adapter.converter.as_array() {
                            Some(
                                arr.iter()
                                    .filter_map(|item| coerce::raw_string(item, ctx.diagnostics))
                                    .cloned()
                                    .collect::<Vec<_>>(),
                            )
                        } else {
                            coerce::raw_string(&adapter.converter, ctx.diagnostics)
                                .map(|raw| vec![raw.clone()])
                        };

                        if let Some(impls) = impls {
                            return (prev.0, Some((idx, impls)));
                        }
                    }
                } else {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "The `input` or `output` adapter must be used.",
                        adapter.span().clone(),
                    ));
                }
                prev
            });

    match (client, prompt) {
        (
            Some(((client, client_span), client_key_span)),
            Some(((prompt, prompt_span, replacers), prompt_key_span)),
        ) => {
            ctx.types.variant_properties.insert(
                idx,
                VariantProperties {
                    client: StringValue {
                        value: client.to_string(),
                        span: client_span.clone(),
                        key_span: client_key_span,
                    },
                    prompt: StringValue {
                        value: prompt.to_string(),
                        span: prompt_span.clone(),
                        key_span: prompt_key_span,
                    },
                    prompt_replacements: replacers,
                    replacers: Default::default(),
                    output_adapter,
                },
            );
        }
        _ => {}
    }
}

fn visit_config<'db>(
    idx: ConfigurationId,
    config: &'db ast::Configuration,
    ctx: &mut Context<'db>,
) {
    match config {
        ast::Configuration::RetryPolicy(retry) => {
            configurations::visit_retry_policy(idx, retry, ctx);
        }
        ast::Configuration::Printer(printer) => {
            configurations::visit_printer(idx, printer, ctx);
        }
        ast::Configuration::TestCase(test_case) => {
            configurations::visit_test_case(idx, test_case, ctx);
        }
    }
}

/// Prisma's builtin scalar types.
#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
#[allow(missing_docs)]
pub enum StaticType {
    Int,
    BigInt,
    Float,
    Boolean,
    String,
    Json,
    Bytes,
}

impl StaticType {
    /// The string representation of the scalar type in the schema.
    pub fn as_str(&self) -> &'static str {
        match self {
            StaticType::Int => "Int",
            StaticType::BigInt => "BigInt",
            StaticType::Float => "Float",
            StaticType::Boolean => "Boolean",
            StaticType::String => "String",
            StaticType::Json => "Json",
            StaticType::Bytes => "Bytes",
        }
    }

    /// True if the type is bytes.
    pub fn is_bytes(&self) -> bool {
        matches!(self, StaticType::Bytes)
    }

    pub(crate) fn try_from_str(s: &str) -> Option<StaticType> {
        match s {
            "Int" => Some(StaticType::Int),
            "BigInt" => Some(StaticType::BigInt),
            "Float" => Some(StaticType::Float),
            "Boolean" => Some(StaticType::Boolean),
            "String" => Some(StaticType::String),
            "Json" => Some(StaticType::Json),
            "Bytes" => Some(StaticType::Bytes),
            _ => None,
        }
    }
}

/// An opaque identifier for a class field in a schema that is dynamic.
#[derive(Copy, Clone, PartialEq, Debug, Hash, Eq, PartialOrd, Ord)]
pub struct DynamicFieldId(u32);

impl From<SerializerFieldId> for DynamicFieldId {
    fn from(id: SerializerFieldId) -> Self {
        DynamicFieldId(id.0)
    }
}

impl From<FieldId> for DynamicFieldId {
    fn from(id: FieldId) -> Self {
        DynamicFieldId(id.0)
    }
}

impl From<EnumValueId> for DynamicFieldId {
    fn from(id: EnumValueId) -> Self {
        DynamicFieldId(id.0)
    }
}

/// An opaque identifier for a class field.
#[derive(Copy, Clone, PartialEq, Debug, Eq, Hash)]
pub struct StaticFieldId(u32);

impl From<SerializerFieldId> for StaticFieldId {
    fn from(id: SerializerFieldId) -> Self {
        StaticFieldId(id.0)
    }
}

impl From<FieldId> for StaticFieldId {
    fn from(id: FieldId) -> Self {
        StaticFieldId(id.0)
    }
}

impl From<EnumValueId> for StaticFieldId {
    fn from(id: EnumValueId) -> Self {
        StaticFieldId(id.0)
    }
}

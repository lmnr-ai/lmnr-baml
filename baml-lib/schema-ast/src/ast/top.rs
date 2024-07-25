use super::{
    traits::WithSpan, Class, Configuration, Enum, Identifier, Span, TemplateString, Variant,
    WithIdentifier,
};

/// Enum for distinguishing between top-level entries
#[derive(Debug, Clone)]
pub enum Top {
    /// An enum declaration
    Enum(Enum),
    // A class declaration
    Class(Class),

    // Variant to run
    Variant(Variant),

    TemplateString(TemplateString),

    // Abritrary config (things with names and key-value pairs where keys are known)
    Config(Configuration),
}

impl Top {
    /// A string saying what kind of item this is.
    pub fn get_type(&self) -> &str {
        match self {
            // Top::CompositeType(_) => "composite type",
            Top::Enum(_) => "enum",
            Top::Class(_) => "class",
            Top::TemplateString(_) => "template_string",
            Top::Variant(v) if v.is_llm() => "impl<llm>",
            Top::Variant(_) => "impl<?>",
            Top::Config(c) => c.get_type(),
        }
    }

    /// Try to interpret the item as an enum declaration.
    pub fn as_enum(&self) -> Option<&Enum> {
        match self {
            Top::Enum(r#enum) => Some(r#enum),
            _ => None,
        }
    }

    pub fn as_class(&self) -> Option<&Class> {
        match self {
            Top::Class(class) => Some(class),
            _ => None,
        }
    }

    pub fn as_variant(&self) -> Option<&Variant> {
        match self {
            Top::Variant(variant) => Some(variant),
            _ => None,
        }
    }

    pub fn as_template_string(&self) -> Option<&TemplateString> {
        match self {
            Top::TemplateString(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_configurations(&self) -> Option<&Configuration> {
        match self {
            Top::Config(config) => Some(config),
            _ => None,
        }
    }
}

impl WithIdentifier for Top {
    /// The name of the item.
    fn identifier(&self) -> &Identifier {
        match self {
            // Top::CompositeType(ct) => &ct.name,
            Top::Enum(x) => x.identifier(),
            Top::Class(x) => x.identifier(),
            Top::TemplateString(x) => x.identifier(),
            Top::Variant(x) => x.identifier(),
            Top::Config(x) => x.identifier(),
        }
    }
}

impl WithSpan for Top {
    fn span(&self) -> &Span {
        match self {
            Top::Enum(en) => en.span(),
            Top::Class(class) => class.span(),
            Top::TemplateString(template) => template.span(),
            Top::Variant(variant) => variant.span(),
            Top::Config(config) => config.span(),
        }
    }
}

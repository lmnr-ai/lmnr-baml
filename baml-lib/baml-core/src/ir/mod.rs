mod ir_helpers;
mod json_schema;
pub mod repr;
mod walker;

pub use ir_helpers::{
    ClassFieldWalker, ClassWalker, EnumValueWalker, EnumWalker, IRHelper, TemplateStringWalker,
};

pub(super) use repr::IntermediateRepr;

// Add aliases for the IR types
pub type Enum = repr::Node<repr::Enum>;
pub type EnumValue = repr::Node<repr::EnumValue>;
pub type Class = repr::Node<repr::Class>;
pub type Field = repr::Node<repr::Field>;
pub type FieldType = baml_types::FieldType;
pub type Expression = repr::Expression;
pub type Identifier = repr::Identifier;
pub type TypeValue = baml_types::TypeValue;
pub type TemplateString = repr::Node<repr::TemplateString>;
pub(super) type Walker<'db, I> = repr::Walker<'db, I>;

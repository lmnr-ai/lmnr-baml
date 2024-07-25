//! Convenient access to a datamodel as understood by ParserDatabase.
//!
//! The walkers:
//! - Know about specific types and what kind they are (models, enums, etc.)
//! - Know about attributes and which ones are defined and allowed in a Prisma schema.
//! - Know about relations.
//! - Do not know anything about connectors, they are generic.

mod r#class;
mod configuration;
mod r#enum;
mod field;
mod template_string;
mod variants;

pub use configuration::*;
use either::Either;
pub use field::*;
use internal_baml_schema_ast::ast::{self, FieldType, Identifier, TopId, WithName};
pub use r#class::*;
pub use r#enum::*;
pub use variants::*;

pub use self::template_string::TemplateStringWalker;

/// A generic walker. Only walkers intantiated with a concrete ID type (`I`) are useful.
#[derive(Clone, Copy)]
pub struct Walker<'db, I> {
    /// The parser database being traversed.
    pub db: &'db crate::ParserDatabase,
    /// The identifier of the focused element.
    pub id: I,
}

impl<'db, I> Walker<'db, I> {
    /// Traverse something else in the same schema.
    pub fn walk<J>(self, other: J) -> Walker<'db, J> {
        self.db.walk(other)
    }
}

impl<'db, I> PartialEq for Walker<'db, I>
where
    I: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl<'db> crate::ParserDatabase {
    /// Find an enum by name.
    pub fn find_enum(&'db self, idn: &Identifier) -> Option<EnumWalker<'db>> {
        self.find_type(idn).and_then(|either| match either {
            Either::Right(class) => Some(class),
            _ => None,
        })
    }

    fn find_top_by_str(&'db self, name: &str) -> Option<&TopId> {
        self.interner
            .lookup(name)
            .and_then(|name_id| self.names.tops.get(&name_id))
    }

    /// Find a type by name.
    pub fn find_type_by_str(
        &'db self,
        name: &str,
    ) -> Option<Either<ClassWalker<'db>, EnumWalker<'db>>> {
        self.find_top_by_str(name).and_then(|top_id| match top_id {
            TopId::Class(class_id) => Some(Either::Left(self.walk(*class_id))),
            TopId::Enum(enum_id) => Some(Either::Right(self.walk(*enum_id))),
            _ => None,
        })
    }

    /// Find a type by name.
    pub fn find_type(
        &'db self,
        idn: &Identifier,
    ) -> Option<Either<ClassWalker<'db>, EnumWalker<'db>>> {
        match idn {
            Identifier::Local(local, _) => self.find_type_by_str(local),
            _ => None,
        }
    }

    /// Find a model by name.
    pub fn find_class(&'db self, idn: &Identifier) -> Option<ClassWalker<'db>> {
        self.find_type(idn).and_then(|either| match either {
            Either::Left(class) => Some(class),
            _ => None,
        })
    }

    /// Find a function by name.
    pub fn find_retry_policy(&'db self, name: &str) -> Option<ConfigurationWalker<'db>> {
        self.interner
            .lookup(name)
            .and_then(|name_id| self.names.tops.get(&name_id))
            .and_then(|top_id| top_id.as_retry_policy_id())
            .map(|model_id| self.walk((model_id, "retry_policy")))
    }

    /// Find printer by name.
    pub fn find_printer(&'db self, name: &str) -> Option<ConfigurationWalker<'db>> {
        self.interner
            .lookup(name)
            .and_then(|name_id| self.names.tops.get(&name_id))
            .and_then(|top_id| top_id.as_printer_id())
            .map(|model_id| self.walk((model_id, "printer")))
    }

    /// Traverse a schema element by id.
    pub fn walk<I>(&self, id: I) -> Walker<'_, I> {
        Walker { db: self, id }
    }

    /// Get all the types that are valid in the schema. (including primitives)
    pub fn valid_type_names(&'db self) -> Vec<String> {
        let mut names: Vec<String> = self.walk_classes().map(|c| c.name().to_string()).collect();
        names.extend(self.walk_enums().map(|e| e.name().to_string()));
        // Add primitive types
        names.extend(
            vec!["string", "int", "float", "bool"]
                .into_iter()
                .map(String::from),
        );
        names
    }

    /// Walk all enums in the schema.
    pub fn walk_enums(&self) -> impl Iterator<Item = EnumWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_enum_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all classes in the schema.
    pub fn walk_classes(&self) -> impl Iterator<Item = ClassWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_class_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all template strings in the schema.
    pub fn walk_templates(&self) -> impl Iterator<Item = TemplateStringWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_template_string_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all variants in the schema.
    pub fn walk_variants(&self) -> impl Iterator<Item = VariantWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_variant_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Convert a field type to a `Type`.
    pub fn to_jinja_type(&self, ft: &FieldType) -> internal_baml_jinja::Type {
        use internal_baml_jinja::Type;
        match ft {
            FieldType::Identifier(arity, idn) => {
                let t = match idn {
                    ast::Identifier::ENV(_, _) => Type::String,
                    ast::Identifier::Ref(x, _) => match self.find_type(idn) {
                        None => Type::Undefined,
                        Some(Either::Left(_)) => Type::ClassRef(x.full_name.clone()),
                        Some(Either::Right(_)) => Type::String,
                    },
                    ast::Identifier::Local(x, _) => match self.find_type(idn) {
                        None => Type::Undefined,
                        Some(Either::Left(_)) => Type::ClassRef(x.clone()),
                        Some(Either::Right(_)) => Type::String,
                    },
                    ast::Identifier::Primitive(idx, _) => match idx {
                        baml_types::TypeValue::String => Type::String,
                        baml_types::TypeValue::Int => Type::Int,
                        baml_types::TypeValue::Float => Type::Float,
                        baml_types::TypeValue::Bool => Type::Bool,
                        baml_types::TypeValue::Null => Type::None,
                        baml_types::TypeValue::Image => Type::Image,
                    },
                    ast::Identifier::String(_, _) => Type::String,
                    ast::Identifier::Invalid(_, _) => Type::Unknown,
                };
                if arity.is_optional() {
                    Type::None | t
                } else {
                    t
                }
            }
            FieldType::List(inner, dims, _) => {
                let mut t = self.to_jinja_type(inner);
                for _ in 0..*dims {
                    t = Type::List(Box::new(t));
                }
                t
            }
            FieldType::Tuple(arity, c, _) => {
                let mut t = Type::Tuple(c.iter().map(|e| self.to_jinja_type(e)).collect());
                if arity.is_optional() {
                    t = Type::None | t;
                }
                t
            }
            FieldType::Union(arity, options, _) => {
                let mut t = Type::Union(options.iter().map(|e| self.to_jinja_type(e)).collect());
                if arity.is_optional() {
                    t = Type::None | t;
                }
                t
            }
            FieldType::Dictionary(kv, _) => Type::Map(
                Box::new(self.to_jinja_type(&kv.0)),
                Box::new(self.to_jinja_type(&kv.1)),
            ),
        }
    }

    // added by LMNR team to convert walker `FieldType`s to actual `baml_types::FieldType`s
    /// Conver ast FieldType to raw FieldType
    pub fn to_raw_field_type(&self, ft: &FieldType) -> baml_types::FieldType {
        match ft {
            FieldType::Identifier(arity, identifier) => {
                let inner = match identifier {
                    Identifier::ENV(_, _) => {
                        baml_types::FieldType::Primitive(baml_types::TypeValue::String)
                    }
                    Identifier::Ref(x, _) => match self.find_type(identifier) {
                        None => baml_types::FieldType::Primitive(baml_types::TypeValue::Null),
                        Some(Either::Left(_)) => baml_types::FieldType::Class(x.full_name.clone()),
                        Some(Either::Right(_)) => {
                            baml_types::FieldType::Primitive(baml_types::TypeValue::String)
                        }
                    },
                    Identifier::Local(x, _) => match self.find_type(identifier) {
                        None => baml_types::FieldType::Primitive(baml_types::TypeValue::Null),
                        Some(Either::Left(_c)) => baml_types::FieldType::Class(x.clone()),
                        Some(Either::Right(_e)) => baml_types::FieldType::Enum(x.clone()),
                    },
                    Identifier::Primitive(idx, _) => baml_types::FieldType::Primitive(idx.clone()),
                    Identifier::String(_, _) => {
                        baml_types::FieldType::Primitive(baml_types::TypeValue::String)
                    }
                    Identifier::Invalid(_, _) => {
                        baml_types::FieldType::Primitive(baml_types::TypeValue::Null)
                    }
                };
                if arity.is_optional() {
                    baml_types::FieldType::Optional(Box::new(inner))
                } else {
                    inner
                }
            }
            FieldType::List(inner, dims, _) => {
                let mut t = self.to_raw_field_type(inner);
                for _ in 0..*dims {
                    t = baml_types::FieldType::List(Box::new(t));
                }
                t
            }
            FieldType::Tuple(arity, inner, _) => {
                let t = baml_types::FieldType::Tuple(
                    inner
                        .iter()
                        .map(|e| self.to_raw_field_type(e))
                        .collect::<Vec<_>>(),
                );
                if arity.is_optional() {
                    baml_types::FieldType::Optional(Box::new(t))
                } else {
                    t
                }
            }
            FieldType::Union(arity, inner, _) => {
                let t = baml_types::FieldType::Union(
                    inner
                        .iter()
                        .map(|e| self.to_raw_field_type(e))
                        .collect::<Vec<_>>(),
                );
                if arity.is_optional() {
                    baml_types::FieldType::Optional(Box::new(t))
                } else {
                    t
                }
            }
            FieldType::Dictionary(inner, _) => baml_types::FieldType::Map(
                Box::new(self.to_raw_field_type(&inner.0)),
                Box::new(self.to_raw_field_type(&inner.1)),
            ),
        }
    }
}

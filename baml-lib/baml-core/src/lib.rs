#![doc = include_str!("../README.md")]
#![deny(rust_2018_idioms, unsafe_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub use internal_baml_diagnostics;
pub use internal_baml_parser_database::{self};

pub use internal_baml_schema_ast::{self, ast};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::sync::Mutex;

use internal_baml_diagnostics::{Diagnostics, SourceFile, Span};

mod common;
pub mod configuration;
pub mod ir;
mod lockfile;
mod validate;

pub use lockfile::LockfileVersion;

pub use crate::{
    common::{PreviewFeature, PreviewFeatures, ALL_PREVIEW_FEATURES},
    configuration::Configuration,
};

pub use lockfile::LockFileWrapper;

pub struct ValidatedSchema {
    pub db: internal_baml_parser_database::ParserDatabase,
    pub diagnostics: Diagnostics,
}

impl std::fmt::Debug for ValidatedSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<Prisma schema>")
    }
}

fn validate_file(files: Vec<SourceFile>) -> ValidatedSchema {
    let mut diagnostics = Diagnostics::new();
    let mut db = internal_baml_parser_database::ParserDatabase::new();
    {
        let diagnostics = Mutex::new(&mut diagnostics);
        let db = Mutex::new(&mut db);
        files
            .par_iter()
            .for_each(|file| match internal_baml_schema_ast::parse_schema(file) {
                Ok((ast, err)) => {
                    let mut diagnostics = diagnostics.lock().unwrap();
                    let mut db = db.lock().unwrap();
                    diagnostics.push(err);
                    db.add_ast(ast);
                }
                Err(err) => {
                    let mut diagnostics = diagnostics.lock().unwrap();
                    diagnostics.push(err);
                }
            });
    }

    if diagnostics.has_errors() {
        return ValidatedSchema { db, diagnostics };
    }

    if let Err(d) = db.validate(&mut diagnostics) {
        return ValidatedSchema { db, diagnostics: d };
    }

    if diagnostics.has_errors() {
        return ValidatedSchema { db, diagnostics };
    }

    // actually run the validation pipeline
    validate::validate(&db, &mut diagnostics);

    if diagnostics.has_errors() {
        return ValidatedSchema { db, diagnostics };
    }

    // Some last linker stuff can only happen post validation.
    db.finalize(&mut diagnostics);

    ValidatedSchema { db, diagnostics }
}

/// The most general API for dealing with Prisma schemas. It accumulates what analysis and
/// validation information it can, and returns it along with any error and warning diagnostics.
pub fn validate(schema_string: &String) -> ValidatedSchema {
    let file = SourceFile::from(schema_string);
    validate_file(vec![file])
}

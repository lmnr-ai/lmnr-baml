mod context;
mod validations;

use crate::internal_baml_diagnostics::Diagnostics;
use internal_baml_parser_database::ParserDatabase;

/// Validate a Prisma schema.
pub(crate) fn validate(db: &ParserDatabase, mut diagnostics: &mut Diagnostics) {
    // Early return so that the validator does not have to deal with invalid schemas

    let mut context = context::Context {
        db: &db,
        diagnostics: &mut diagnostics,
    };

    validations::validate(&mut context);
}

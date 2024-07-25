mod classes;
mod common;
mod cycle;
mod enums;

use super::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    enums::validate(ctx);
    classes::validate(ctx);

    if !ctx.diagnostics.has_errors() {
        cycle::validate(ctx);
    }
}

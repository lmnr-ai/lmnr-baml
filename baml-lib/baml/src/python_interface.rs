use pyo3::{types::PyString, PyErr};

use crate::BamlContext;

fn pyerr_from_anyhow(e: anyhow::Error) -> PyErr {
    PyErr::new::<PyString, _>(e.to_string())
}

#[pyo3::pyfunction]
#[pyo3(signature = (schema_string, target_name=None))]
pub fn render_prompt(
    schema_string: String,
    target_name: Option<String>,
) -> pyo3::prelude::PyResult<String> {
    let baml_context =
        BamlContext::try_from_schema(&schema_string, target_name).map_err(pyerr_from_anyhow)?;
    baml_context.render_prompt().map_err(pyerr_from_anyhow)
}

#[pyo3::pyfunction]
#[pyo3(signature = (schema_string, result, target_name=None))]
pub fn validate_result(
    schema_string: String,
    result: String,
    target_name: Option<String>,
) -> pyo3::prelude::PyResult<String> {
    let baml_context =
        BamlContext::try_from_schema(&schema_string, target_name).map_err(pyerr_from_anyhow)?;
    baml_context
        .validate_result(&result)
        .map_err(pyerr_from_anyhow)
}

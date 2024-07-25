# Laminar BAML

This is a hard fork of BoundaryML's [BAML](https://github.com/BoundaryML/baml).

This is a very stripped down version that does only:
- Schema validation
- LLM prompting (for structured output)
- LLM output parsing (validation against schema)

The only required types from BAML AST for this are:
- Enum
- Class

and their dependencies.

Laminar's use cases are limited to that, so the rest of the library was removed.

However, we need to interface those functions with Python, so this library exposes
a few more interfaces through Pyo3.

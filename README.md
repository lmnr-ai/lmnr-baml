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

## Development

Until we properly isolate pyo3 into `#[cfg(test)]`, the way to update and build Python wheels is the following:

1. In `baml-lib/baml/Cargo.toml` uncomment: `pyo3 = { version = "0.22.2", features = ["extension-module"] }` and the build dependency `pyo3-build-config = "0.22.2"`
1. Uncomment the block of code in `baml-lib/baml/src/lib.rs` that declares `#[pyo3::prelude::pymodule]`
1. `cd baml-lib/baml`
1. `rustup target add aarch64-apple-darwin aarch64-unknown-linux-gnu i686-pc-windows-gnu i686-pc-windows-msvc i686-unknown-linux-gnu x86_64-apple-darwin x86_64-pc-windows-gnu x86_64-pc-windows-msvc x86_64-unknown-linux-gnu`
    - See more: https://doc.rust-lang.org/nightly/rustc/target-tier-policy.html#tier-1-with-host-tools
1. `maturin build -i python3.9 --target aarch64-apple-darwin`. And repeat for all targets above
1. Take the output wheel from `target/wheels` and place it in the library required
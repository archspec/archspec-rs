//! The Rust port of the archspec Python library.
//!
//! Both this and the corresponding [Python library](https://github.com/archspec/archspec)
//! are data-driven from a [JSON document](https://github.com/archspec/archspec-json) which
//! determines what architecture the current machine is a superset of, as well as provides
//! ways to compare architecture capabilities.
//!
//! Built documentation for the Python library can be found at
//! [archspec.readthedocs.org](https://archspec.readthedocs.org) for additional context.

mod builtin;
pub mod cpu;
mod cpuid;
pub mod schema;

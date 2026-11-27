//! Seams: the only modules that reach external tools. Each is a trait with
//! one real implementation and a fake (`fake`) for tests.

pub mod compose;
pub mod http;
pub mod mcp;
pub mod tailscale;

#[cfg(test)]
pub mod fake;

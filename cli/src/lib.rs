//! `sapphire-timer`: the timer's one binary.
//!
//! The framework's verbs (`serve`, `status`, `service`, `workspace`, `mcp`, …)
//! run the app server or talk to a running one. The modules are a library so
//! the integration tests can drive them without a process.

pub mod cli;
pub mod serve;

//! MCP for sapphire-timer, on the framework's MCP (`sapphire_framework::mcp`).
//!
//! Three parts:
//!
//! - [`methods`]: presets and logs, as `sapphire-timer.*` methods on the app
//!   server, working on the workspace it has selected.
//! - [`clients`]: the running timers, one per client, held wherever the MCP
//!   is served.
//! - [`tools`]: the MCP handler — tools for both, and the calling client's
//!   timer as a resource it can subscribe to. The framework serves it on
//!   stdio (`sapphire-timer mcp`) and at `/mcp` on the server's HTTP listener.

pub mod clients;
pub mod methods;
pub mod tools;

pub use clients::{Client, ClientKey, Clients};
pub use methods::{APP_API, StateSource, TimerMethods, register};
pub use tools::{ACTIVE_TIMER_URI, TimerTools, service};

//! Embedded web assets for the `ui` server.
//!
//! Kept in its own module so the server serves bytes that are compiled into the
//! binary; nothing is fetched from disk or the network.

/// Design tokens (colours, font stacks).
pub const TOKENS_CSS: &str = include_str!("ui/assets/tokens.css");
/// Workspace layout and components.
pub const APP_CSS: &str = include_str!("ui/assets/app.css");
/// Table filtering, bulk-selection state, and announcements.
pub const APP_JS: &str = include_str!("ui/assets/app.js");

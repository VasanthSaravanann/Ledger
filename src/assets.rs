//! Embedded web assets, shared by the generated site and the `ui` server.
//!
//! Kept in one module so the site generator does not need the `ui` feature to
//! publish the same bytes the server serves.

/// Design tokens (colours, font stacks).
pub const TOKENS_CSS: &str = include_str!("ui/assets/tokens.css");
/// Workspace layout and components.
pub const APP_CSS: &str = include_str!("ui/assets/app.css");
/// Table filtering, bulk-selection state, and announcements.
pub const APP_JS: &str = include_str!("ui/assets/app.js");

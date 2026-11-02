//! XMB-style menu for Deckard.
//!
//! - [`nav`]: menu model and navigation (pure state);
//! - [`quick`]: in-game quick menu (pure state);
//! - [`repeat`]: held buttons → navigation steps;
//! - [`layout`]: menu state → [`layout::Scene`] (positions, animation);
//! - [`i18n`]: interface strings;
//! - [`render`]: draws a scene with wgpu and glyphon.

pub mod i18n;
pub mod layout;
pub mod nav;
pub mod quick;
pub mod render;
pub mod repeat;

pub use render::UiRenderer;

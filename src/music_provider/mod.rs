use dioxus::core::Element;
use std::sync::Arc;

use crate::state::AppState;

pub mod local_file;

/// Type alias for a provider factory function.
pub type ProviderFactory = fn(Arc<AppState>) -> Box<dyn MusicProvider>;

/// A music source provider that renders its own UI for selecting music.
pub trait MusicProvider {
    /// The provider's display name
    fn name(&self) -> &'static str;

    /// Render the provider's selection UI
    fn render(&self) -> Element;
}

use dioxus::core::Element;
use std::sync::Arc;

pub mod apple_music;
pub mod local_file;

/// Narrow context provided to music providers for submitting audio data.
///
/// Providers call [`MusicProviderContext::play_now`] or
/// [`MusicProviderContext::queue`] when they have downloaded or prepared an
/// audio file. The context does not expose `AppState` or any playback/playlist
/// internals — providers only know that there are two possible actions.
#[derive(Clone)]
pub struct MusicProviderContext {
    play_now: Arc<dyn Fn(Vec<u8>, String) -> anyhow::Result<()> + Send + Sync>,
    queue: Arc<dyn Fn(Vec<u8>, String) -> anyhow::Result<()> + Send + Sync>,
}

impl MusicProviderContext {
    /// Create a new context from two closures.
    ///
    /// `play_now` starts immediate synchronized playback.
    /// `queue` appends the song to the shared playlist.
    pub fn new(
        play_now: impl Fn(Vec<u8>, String) -> anyhow::Result<()> + Send + Sync + 'static,
        queue: impl Fn(Vec<u8>, String) -> anyhow::Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            play_now: Arc::new(play_now),
            queue: Arc::new(queue),
        }
    }

    /// Start immediate playback of the given audio data.
    pub fn play_now(&self, data: Vec<u8>, title: String) -> anyhow::Result<()> {
        (self.play_now)(data, title)
    }

    /// Add the given audio data to the shared playlist queue.
    pub fn queue(&self, data: Vec<u8>, title: String) -> anyhow::Result<()> {
        (self.queue)(data, title)
    }
}

/// Type alias for a provider factory function.
pub type ProviderFactory = fn(MusicProviderContext) -> Box<dyn MusicProvider>;

/// A music source provider that renders its own UI for selecting music.
pub trait MusicProvider {
    /// The provider's display name
    fn name(&self) -> &'static str;

    /// Render the provider's selection UI
    fn render(&self) -> Element;
}

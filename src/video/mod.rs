//! Embedded video playback.
//!
//! With the `video` Cargo feature the real FFmpeg-based player is used. Without it, a placeholder
//! `Player` that can never be created is used, so the rest of the app compiles the same either way
//! and simply falls back to opening the system's default video player.

#[cfg(feature = "video")]
mod player;
#[cfg(feature = "video")]
pub use player::Player;

#[cfg(not(feature = "video"))]
mod stub;
#[cfg(not(feature = "video"))]
pub use stub::Player;

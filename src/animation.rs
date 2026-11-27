//! Animated GIF playback.
//!
//! A background thread decodes the GIF frame by frame and sends the frames over a small bounded
//! channel; the UI thread shows each one for its own delay and uploads it to a single texture.
//! When the GIF ends, the thread simply decodes it again from the start, so it loops forever while
//! only a few frames are ever held in memory. A GIF with a single frame is shown once and stops.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage, Frame};

use crate::config::{GIF_QUEUE_LEN, MAX_TEXTURE_SIDE};

/// Whether `path` has a `.gif` extension.
pub fn is_gif(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("gif"))
        .unwrap_or(false)
}

struct AnimFrame {
    image: egui::ColorImage,
    delay: Duration,
}

enum Msg {
    Frame(AnimFrame),
    Error(String),
}

pub struct AnimPlayer {
    path: PathBuf,
    /// View-only rotation (quarter turns) the frames were prepared with.
    rotation: u8,
    rx: Receiver<Msg>,
    stop: Arc<AtomicBool>,
    texture: Option<egui::TextureHandle>,
    /// When the frame on screen should be replaced by the next one.
    shown_until: Instant,
    paused: bool,
    /// Time left on the current frame when paused.
    remaining: Duration,
    /// The decoder has finished and sent everything (a one-frame GIF).
    finished: bool,
    error: Option<String>,
}

impl AnimPlayer {
    pub fn open(path: &Path, rotation: u8, ctx: &egui::Context) -> AnimPlayer {
        let (tx, rx) = mpsc::sync_channel::<Msg>(GIF_QUEUE_LEN);
        let stop = Arc::new(AtomicBool::new(false));

        {
            let path = path.to_path_buf();
            let stop = Arc::clone(&stop);
            let ctx = ctx.clone();
            thread::spawn(move || decode_thread(path, rotation, tx, stop, ctx));
        }

        AnimPlayer {
            path: path.to_path_buf(),
            rotation,
            rx,
            stop,
            texture: None,
            shown_until: Instant::now(),
            paused: false,
            remaining: Duration::ZERO,
            finished: false,
            error: None,
        }
    }

    /// Whether this player is already playing `path` with the given rotation.
    pub fn is_for(&self, path: &Path, rotation: u8) -> bool {
        self.path == path && self.rotation == rotation
    }

    /// The frame currently on screen, once the first one has been decoded.
    pub fn texture(&self) -> Option<&egui::TextureHandle> {
        self.texture.as_ref()
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            self.paused = false;
            self.shown_until = Instant::now() + self.remaining;
        } else {
            self.remaining = self.shown_until.saturating_duration_since(Instant::now());
            self.paused = true;
        }
    }

    /// Called every frame: shows the next GIF frame when the current one's time is up.
    pub fn pump(&mut self, ctx: &egui::Context) {
        if self.paused || self.finished || self.error.is_some() {
            return;
        }

        let now = Instant::now();
        if self.texture.is_none() || now >= self.shown_until {
            match self.rx.try_recv() {
                Ok(Msg::Frame(frame)) => {
                    // Keep the original rhythm, but if we fell far behind, restart from now.
                    let late = now.saturating_duration_since(self.shown_until);
                    let base = if self.texture.is_none() || late > frame.delay {
                        now
                    } else {
                        self.shown_until
                    };
                    self.shown_until = base + frame.delay;
                    match &mut self.texture {
                        Some(tex) => tex.set(frame.image, Default::default()),
                        None => {
                            self.texture =
                                Some(ctx.load_texture("gif_frame", frame.image, Default::default()))
                        }
                    }
                }
                Ok(Msg::Error(e)) => {
                    self.error = Some(e);
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.finished = true;
                    return;
                }
            }
        }

        // Wake up when the next frame is due (or soon, if the decoder hasn't delivered it yet).
        let wait = self.shown_until.saturating_duration_since(Instant::now());
        ctx.request_repaint_after(wait.max(Duration::from_millis(2)));
    }
}

impl Drop for AnimPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------------------------
// Decoder thread
// ---------------------------------------------------------------------------------------------

enum Pass {
    /// Reached the end of the GIF after this many frames.
    Finished { frames: usize },
    /// The player was closed.
    Stopped,
}

fn decode_thread(
    path: PathBuf,
    rotation: u8,
    tx: SyncSender<Msg>,
    stop: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    loop {
        match decode_pass(&path, rotation, &tx, &stop, &ctx) {
            // A one-frame GIF isn't animated: show it once and stop.
            Ok(Pass::Finished { frames }) if frames <= 1 => return,
            Ok(Pass::Finished { .. }) => {} // loop: decode again from the start
            Ok(Pass::Stopped) => return,
            Err(e) => {
                let _ = tx.send(Msg::Error(e));
                ctx.request_repaint();
                return;
            }
        }
    }
}

fn decode_pass(
    path: &Path,
    rotation: u8,
    tx: &SyncSender<Msg>,
    stop: &AtomicBool,
    ctx: &egui::Context,
) -> Result<Pass, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = GifDecoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;

    let mut frames = 0usize;
    for frame in decoder.into_frames() {
        if stop.load(Ordering::Relaxed) {
            return Ok(Pass::Stopped);
        }
        let frame = frame.map_err(|e| e.to_string())?;
        let delay = frame_delay(&frame);
        let image = prepare(frame, rotation);
        if tx.send(Msg::Frame(AnimFrame { image, delay })).is_err() {
            return Ok(Pass::Stopped); // the player was dropped
        }
        ctx.request_repaint();
        frames += 1;
    }
    Ok(Pass::Finished { frames })
}

/// How long to show a frame. Browsers treat very short delays (including 0) as 100 ms, so we do too.
fn frame_delay(frame: &Frame) -> Duration {
    let (numer, denom) = frame.delay().numer_denom_ms();
    let ms = numer.checked_div(denom).unwrap_or_default();
    Duration::from_millis(if ms <= 10 { 100 } else { ms as u64 })
}

/// Scales a huge frame down, applies the view-only rotation, and converts it for egui.
fn prepare(frame: Frame, rotation: u8) -> egui::ColorImage {
    let mut img = DynamicImage::ImageRgba8(frame.into_buffer());
    if img.width() > MAX_TEXTURE_SIDE || img.height() > MAX_TEXTURE_SIDE {
        img = img.thumbnail(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE);
    }
    img = match rotation % 4 {
        1 => img.rotate90(),
        2 => img.rotate180(),
        3 => img.rotate270(),
        _ => img,
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Rgba, RgbaImage};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "media_viewer_gif_test_{tag}_{}_{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn solid_frame(color: [u8; 4], delay_ms: u32) -> Frame {
        Frame::from_parts(
            RgbaImage::from_pixel(4, 4, Rgba(color)),
            0,
            0,
            Delay::from_numer_denom_ms(delay_ms, 1),
        )
    }

    fn write_gif(path: &Path, frames: Vec<Frame>) {
        let file = File::create(path).unwrap();
        let mut encoder = GifEncoder::new(file);
        encoder.encode_frames(frames).unwrap();
    }

    fn next_frame(player: &AnimPlayer) -> AnimFrame {
        match player.rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Msg::Frame(f)) => f,
            Ok(Msg::Error(e)) => panic!("decode error: {e}"),
            Err(e) => panic!("no frame arrived: {e}"),
        }
    }

    #[test]
    fn very_short_delays_are_stretched_to_100_ms() {
        assert_eq!(
            frame_delay(&solid_frame([0, 0, 0, 255], 0)),
            Duration::from_millis(100)
        );
        assert_eq!(
            frame_delay(&solid_frame([0, 0, 0, 255], 10)),
            Duration::from_millis(100)
        );
        assert_eq!(
            frame_delay(&solid_frame([0, 0, 0, 255], 40)),
            Duration::from_millis(40)
        );
    }

    #[test]
    fn detects_gif_files_by_extension() {
        assert!(is_gif(Path::new("a.gif")));
        assert!(is_gif(Path::new("A.GIF")));
        assert!(!is_gif(Path::new("a.png")));
        assert!(!is_gif(Path::new("gif")));
    }

    #[test]
    fn plays_all_frames_in_order_and_loops() {
        let dir = temp_dir("loop");
        let path = dir.join("t.gif");
        write_gif(
            &path,
            vec![
                solid_frame([255, 0, 0, 255], 100),
                solid_frame([0, 255, 0, 255], 100),
                solid_frame([0, 0, 255, 255], 100),
            ],
        );

        let player = AnimPlayer::open(&path, 0, &egui::Context::default());
        let frames: Vec<AnimFrame> = (0..4).map(|_| next_frame(&player)).collect();

        assert_eq!(frames[0].image.size, [4, 4]);
        assert_eq!(frames[0].delay, Duration::from_millis(100));
        assert_ne!(frames[0].image.pixels[0], frames[1].image.pixels[0]);
        assert_ne!(frames[1].image.pixels[0], frames[2].image.pixels[0]);
        // The fourth frame is the first one again: the GIF looped.
        assert_eq!(frames[3].image.pixels[0], frames[0].image.pixels[0]);

        drop(player);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_one_frame_gif_is_shown_once_and_stops() {
        let dir = temp_dir("single");
        let path = dir.join("one.gif");
        write_gif(&path, vec![solid_frame([255, 0, 0, 255], 100)]);

        let player = AnimPlayer::open(&path, 0, &egui::Context::default());
        let _first = next_frame(&player);

        // The decoder thread ends, so the channel closes instead of producing more frames.
        assert!(matches!(
            player.rx.recv_timeout(Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));

        drop(player);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rotation_is_applied_to_every_frame() {
        let dir = temp_dir("rotate");
        let path = dir.join("wide.gif");
        let wide = |color| {
            Frame::from_parts(
                RgbaImage::from_pixel(6, 2, Rgba(color)),
                0,
                0,
                Delay::from_numer_denom_ms(100, 1),
            )
        };
        write_gif(&path, vec![wide([255, 0, 0, 255]), wide([0, 255, 0, 255])]);

        let player = AnimPlayer::open(&path, 1, &egui::Context::default());
        let frame = next_frame(&player);

        assert_eq!(frame.image.size, [2, 6]); // 6x2 turned a quarter turn

        drop(player);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

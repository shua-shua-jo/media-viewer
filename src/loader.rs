//! Background image decoding, so the UI never waits on the disk or the decoder.

use std::collections::{HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use eframe::egui;

use crate::config::MAX_TEXTURE_SIDE;

/// Identifies one decoded image: the file plus its view-only rotation (quarter turns).
pub type Key = (PathBuf, u8);

/// Decodes an image, downscales it if huge, and applies the view-only rotation.
/// Runs on a background thread; the file on disk is never modified.
fn decode_image(path: &Path, quarter_turns: u8) -> Result<egui::ColorImage, String> {
    let mut img = image::open(path).map_err(|e| e.to_string())?;
    if img.width() > MAX_TEXTURE_SIDE || img.height() > MAX_TEXTURE_SIDE {
        img = img.thumbnail(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE);
    }
    img = match quarter_turns % 4 {
        1 => img.rotate90(),
        2 => img.rotate180(),
        3 => img.rotate270(),
        _ => img,
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

struct Shared {
    /// Images waiting to be decoded, most important first.
    queue: Mutex<VecDeque<Key>>,
    /// Images a worker is decoding right now.
    inflight: Mutex<HashSet<Key>>,
    wake: Condvar,
}

/// A finished (or failed) decode, sent back to the UI thread.
pub struct Loaded {
    pub key: Key,
    pub result: Result<egui::ColorImage, String>,
}

/// A small pool of background threads that decode images.
pub struct Loader {
    shared: Arc<Shared>,
    rx: Receiver<Loaded>,
}

impl Loader {
    pub fn new(ctx: egui::Context, workers: usize) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            inflight: Mutex::new(HashSet::new()),
            wake: Condvar::new(),
        });
        let (tx, rx) = mpsc::channel::<Loaded>();

        for _ in 0..workers {
            let shared = Arc::clone(&shared);
            let tx = tx.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                loop {
                    // Wait for a job; mark it in flight while still holding the queue lock.
                    let key = {
                        let mut queue = shared.queue.lock().unwrap();
                        loop {
                            if let Some(k) = queue.pop_front() {
                                shared.inflight.lock().unwrap().insert(k.clone());
                                break k;
                            }
                            queue = shared.wake.wait(queue).unwrap();
                        }
                    };

                    let result = catch_unwind(AssertUnwindSafe(|| decode_image(&key.0, key.1)))
                        .unwrap_or_else(|_| Err("decoder crashed on this file".to_string()));

                    let _ = tx.send(Loaded {
                        key: key.clone(),
                        result,
                    });
                    shared.inflight.lock().unwrap().remove(&key);
                    ctx.request_repaint(); // wake the UI so it picks up the result
                }
            });
        }

        Loader { shared, rx }
    }

    /// Replaces the waiting jobs with `wanted` (in priority order), skipping anything
    /// already finished (`skip`) or currently being decoded.
    pub fn set_wanted(&self, wanted: &[Key], skip: impl Fn(&Key) -> bool) {
        {
            let mut queue = self.shared.queue.lock().unwrap();
            let inflight = self.shared.inflight.lock().unwrap();
            queue.clear();
            for k in wanted {
                if !skip(k) && !inflight.contains(k) {
                    queue.push_back(k.clone());
                }
            }
        }
        self.shared.wake.notify_all();
    }

    /// Returns the next finished decode, if any, without waiting.
    pub fn try_recv(&self) -> Option<Loaded> {
        self.rx.try_recv().ok()
    }
}

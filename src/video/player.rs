//! FFmpeg-based video player (picture only; audio is not played yet).
//!
//! A background thread reads the file, decodes frames, converts them to RGBA and sends them over a
//! small bounded channel. The UI thread owns the playback clock: each frame it shows the newest
//! frame whose timestamp has been reached. Pausing simply stops the clock; the decoder then blocks
//! because the channel is full, so a paused video uses almost no CPU.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use ffmpeg_next as ffmpeg;

use crate::config::{VIDEO_AUTOPLAY, VIDEO_MAX_WIDTH, VIDEO_QUEUE_LEN};

// ---------------------------------------------------------------------------------------------
// Messages between the decoder thread and the UI
// ---------------------------------------------------------------------------------------------

struct Frame {
    image: egui::ColorImage,
    /// Presentation time in seconds.
    pts: f64,
    /// Which seek this frame belongs to; frames from before the latest seek are ignored.
    serial: u32,
}

enum Msg {
    Info { duration: f64 },
    Frame(Frame),
    Ended { serial: u32 },
    Error(String),
}

enum Cmd {
    Seek { target: f64, serial: u32 },
}

// ---------------------------------------------------------------------------------------------
// Playback clock
// ---------------------------------------------------------------------------------------------

struct Clock {
    playing: bool,
    /// Media time (seconds) at the moment `anchor` was taken.
    base: f64,
    anchor: Instant,
}

impl Clock {
    fn new(playing: bool) -> Self {
        Clock {
            playing,
            base: 0.0,
            anchor: Instant::now(),
        }
    }

    fn now(&self) -> f64 {
        if self.playing {
            self.base + self.anchor.elapsed().as_secs_f64()
        } else {
            self.base
        }
    }

    fn set(&mut self, media_time: f64) {
        self.base = media_time;
        self.anchor = Instant::now();
    }

    fn play(&mut self) {
        if !self.playing {
            self.anchor = Instant::now();
            self.playing = true;
        }
    }

    fn pause(&mut self) {
        if self.playing {
            self.base = self.now();
            self.playing = false;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The player (UI side)
// ---------------------------------------------------------------------------------------------

pub struct Player {
    path: PathBuf,
    rx: Receiver<Msg>,
    cmd_tx: Sender<Cmd>,
    stop: Arc<AtomicBool>,
    serial: u32,
    texture: Option<egui::TextureHandle>,
    /// A decoded frame waiting for its display time.
    pending: Option<Frame>,
    clock: Clock,
    /// True until the first frame after opening/seeking arrives; the clock is then synced to it.
    awaiting_anchor: bool,
    /// The decoder has delivered its last frame for the current serial.
    stream_ended: bool,
    /// The last frame has been shown.
    ended: bool,
    position: f64,
    duration: f64,
    error: Option<String>,
    /// Slider value while the user is dragging the seek bar.
    scrub: Option<f64>,
}

impl Player {
    pub fn open(path: &Path, ctx: &egui::Context) -> Result<Player, String> {
        let (tx, rx) = mpsc::sync_channel::<Msg>(VIDEO_QUEUE_LEN);
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let stop = Arc::new(AtomicBool::new(false));

        {
            let path = path.to_path_buf();
            let stop = Arc::clone(&stop);
            let ctx = ctx.clone();
            thread::spawn(move || decode_thread(path, tx, cmd_rx, stop, ctx));
        }

        Ok(Player {
            path: path.to_path_buf(),
            rx,
            cmd_tx,
            stop,
            serial: 0,
            texture: None,
            pending: None,
            clock: Clock::new(VIDEO_AUTOPLAY),
            awaiting_anchor: true,
            stream_ended: false,
            ended: false,
            position: 0.0,
            duration: 0.0,
            error: None,
            scrub: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Play / pause (or replay if the video has finished).
    pub fn toggle_pause(&mut self) {
        if self.ended {
            self.seek(0.0);
            self.clock.play();
        } else if self.clock.playing {
            self.clock.pause();
        } else {
            self.clock.play();
        }
    }

    fn seek(&mut self, target: f64) {
        let target = target.clamp(0.0, self.duration.max(0.0));
        self.serial = self.serial.wrapping_add(1);
        let _ = self.cmd_tx.send(Cmd::Seek {
            target,
            serial: self.serial,
        });
        // Throw away queued frames (this also unblocks a decoder that is waiting to send).
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Info { duration } => self.duration = duration,
                Msg::Error(e) => self.error = Some(e),
                _ => {}
            }
        }
        self.pending = None;
        self.awaiting_anchor = true;
        self.stream_ended = false;
        self.ended = false;
        self.position = target;
        self.clock.set(target);
    }

    /// Draws the video and its controls, and advances playback.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.pump(&ctx);

        if let Some(err) = self.error.clone() {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.heading("🎬 Video");
                ui.label(format!("Could not play this video: {err}"));
                if ui.button("Play in default player").clicked() {
                    let _ = open::that(&self.path);
                }
            });
            return;
        }

        ui.vertical(|ui| {
            let controls_height = 36.0;
            let image_height = (ui.available_height() - controls_height).max(40.0);
            ui.allocate_ui(egui::vec2(ui.available_width(), image_height), |ui| {
                ui.centered_and_justified(|ui| match &self.texture {
                    Some(tex) => {
                        let sized = egui::load::SizedTexture::from_handle(tex);
                        ui.add(egui::Image::new(sized).shrink_to_fit());
                    }
                    None => {
                        ui.label("Loading video…");
                    }
                });
            });
            self.controls(ui);
        });

        self.schedule_repaint(&ctx);
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let label = if self.ended {
                "Replay"
            } else if self.clock.playing {
                "Pause"
            } else {
                "Play"
            };
            if ui.button(label).clicked() {
                self.toggle_pause();
            }

            let mut t = self.scrub.unwrap_or(self.position);
            if self.duration > 0.0 {
                ui.spacing_mut().slider_width = (ui.available_width() - 120.0).max(60.0);
                let resp = ui.add(egui::Slider::new(&mut t, 0.0..=self.duration).show_value(false));
                if resp.changed() {
                    self.scrub = Some(t);
                }
                // Seek when the drag ends (or for a keyboard/click change that isn't a drag).
                if (resp.drag_stopped() || (resp.changed() && !resp.dragged()))
                    && let Some(target) = self.scrub.take()
                {
                    self.seek(target);
                }
            }
            ui.label(format!("{} / {}", fmt_time(t), fmt_time(self.duration)));
        });
    }

    /// Takes everything the decoder has produced and shows the newest frame that is due.
    fn pump(&mut self, ctx: &egui::Context) {
        let mut latest: Option<Frame> = None;

        loop {
            if self.pending.is_none() {
                match self.rx.try_recv() {
                    Ok(Msg::Info { duration }) => {
                        self.duration = duration;
                        continue;
                    }
                    Ok(Msg::Error(e)) => {
                        self.error = Some(e);
                        break;
                    }
                    Ok(Msg::Ended { serial }) => {
                        if serial == self.serial {
                            self.stream_ended = true;
                        }
                        continue;
                    }
                    Ok(Msg::Frame(f)) => {
                        if f.serial == self.serial {
                            self.pending = Some(f);
                        }
                        continue;
                    }
                    Err(_) => break, // nothing more right now
                }
            }

            let Some(f) = &self.pending else { break };
            if self.awaiting_anchor {
                self.clock.set(f.pts);
                self.awaiting_anchor = false;
            }
            if f.pts <= self.clock.now() + 0.002 {
                // Due. If several are due (we're behind), only the newest is drawn.
                latest = self.pending.take();
            } else {
                break;
            }
        }

        if let Some(f) = latest {
            self.position = f.pts;
            match &mut self.texture {
                Some(tex) => tex.set(f.image, Default::default()),
                None => {
                    self.texture =
                        Some(ctx.load_texture("video_frame", f.image, Default::default()))
                }
            }
        }

        if self.stream_ended && self.pending.is_none() && !self.ended {
            self.ended = true;
            self.clock.pause();
        }
    }

    fn schedule_repaint(&self, ctx: &egui::Context) {
        if self.clock.playing && !self.ended {
            let wait = match &self.pending {
                Some(f) => (f.pts - self.clock.now()).max(0.0),
                None => 0.005,
            };
            ctx.request_repaint_after(Duration::from_secs_f64(wait.clamp(0.001, 0.05)));
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // The decoder thread notices this (or the closed channel) and exits.
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

// ---------------------------------------------------------------------------------------------
// Decoder thread
// ---------------------------------------------------------------------------------------------

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

struct Pipeline {
    input: ffmpeg::format::context::Input,
    decoder: ffmpeg::decoder::Video,
    scaler: ffmpeg::software::scaling::Context,
    v_index: usize,
    /// Seconds per timestamp unit of the video stream.
    time_base: f64,
    serial: u32,
    /// After a seek, frames earlier than this are decoded but not shown.
    skip_until: Option<f64>,
    frame: ffmpeg::frame::Video,
    rgba: ffmpeg::frame::Video,
}

impl Pipeline {
    /// Sends every frame the decoder currently has. Returns false if the player was closed.
    fn drain(&mut self, tx: &SyncSender<Msg>, ctx: &egui::Context) -> Result<bool, String> {
        while self.decoder.receive_frame(&mut self.frame).is_ok() {
            let ts = self.frame.timestamp().or(self.frame.pts()).unwrap_or(0);
            let pts = ts as f64 * self.time_base;

            if let Some(target) = self.skip_until {
                if pts + 0.001 < target {
                    continue;
                }
                self.skip_until = None;
            }

            self.scaler.run(&self.frame, &mut self.rgba).map_err(err)?;
            let image = to_color_image(&self.rgba);
            let msg = Msg::Frame(Frame {
                image,
                pts,
                serial: self.serial,
            });
            if tx.send(msg).is_err() {
                return Ok(false);
            }
            ctx.request_repaint();
        }
        Ok(true)
    }
}

fn to_color_image(frame: &ffmpeg::frame::Video) -> egui::ColorImage {
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let stride = frame.stride(0);
    let data = frame.data(0);
    let mut buf = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        let start = row * stride;
        buf.extend_from_slice(&data[start..start + w * 4]);
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &buf)
}

fn decode_thread(
    path: PathBuf,
    tx: SyncSender<Msg>,
    cmd_rx: Receiver<Cmd>,
    stop: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    if let Err(e) = decode_loop(&path, &tx, &cmd_rx, &stop, &ctx) {
        let _ = tx.send(Msg::Error(e));
        ctx.request_repaint();
    }
}

fn decode_loop(
    path: &Path,
    tx: &SyncSender<Msg>,
    cmd_rx: &Receiver<Cmd>,
    stop: &AtomicBool,
    ctx: &egui::Context,
) -> Result<(), String> {
    ffmpeg::init().map_err(err)?;
    let input = ffmpeg::format::input(&path).map_err(err)?;
    let duration = (input.duration() as f64 / 1_000_000.0).max(0.0);

    let (v_index, time_base, decoder) = {
        let stream = input
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or("no video stream found")?;
        let tb = stream.time_base();
        let codec_ctx =
            ffmpeg::codec::context::Context::from_parameters(stream.parameters()).map_err(err)?;
        let decoder = codec_ctx.decoder().video().map_err(err)?;
        (
            stream.index(),
            tb.numerator() as f64 / tb.denominator().max(1) as f64,
            decoder,
        )
    };

    // Scale big videos down while converting, to save CPU and memory.
    let (sw, sh) = (decoder.width(), decoder.height());
    let (dw, dh) = if sw > VIDEO_MAX_WIDTH {
        let h = (sh as u64 * VIDEO_MAX_WIDTH as u64 / sw as u64) as u32;
        (VIDEO_MAX_WIDTH, h.max(2))
    } else {
        (sw, sh)
    };
    let scaler = ffmpeg::software::scaling::Context::get(
        decoder.format(),
        sw,
        sh,
        ffmpeg::format::Pixel::RGBA,
        dw,
        dh,
        ffmpeg::software::scaling::flag::Flags::BILINEAR,
    )
    .map_err(err)?;

    if tx.send(Msg::Info { duration }).is_err() {
        return Ok(());
    }

    let mut p = Pipeline {
        input,
        decoder,
        scaler,
        v_index,
        time_base,
        serial: 0,
        skip_until: None,
        frame: ffmpeg::frame::Video::empty(),
        rgba: ffmpeg::frame::Video::empty(),
    };

    let mut packet = ffmpeg::Packet::empty();
    let mut at_end = false;
    let mut next_cmd: Option<Cmd> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Newest command wins.
        let mut cmd = next_cmd.take();
        while let Ok(c) = cmd_rx.try_recv() {
            cmd = Some(c);
        }
        if let Some(Cmd::Seek { target, serial }) = cmd {
            let ts = (target * 1_000_000.0) as i64;
            let _ = p.input.seek(ts, ..ts); // if the file can't seek, just keep playing
            p.decoder.flush();
            p.serial = serial;
            p.skip_until = Some(target);
            at_end = false;
        }

        if at_end {
            // Finished: wait for a seek (replay) or for the player to close.
            match cmd_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(c) => next_cmd = Some(c),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
            continue;
        }

        match packet.read(&mut p.input) {
            Ok(()) => {
                if packet.stream() == p.v_index
                    && p.decoder.send_packet(&packet).is_ok()
                    && !p.drain(tx, ctx)?
                {
                    return Ok(());
                }
            }
            Err(_) => {
                // End of file (or unreadable): flush what the decoder still holds, then report the end.
                let _ = p.decoder.send_eof();
                if !p.drain(tx, ctx)? {
                    return Ok(());
                }
                if tx.send(Msg::Ended { serial: p.serial }).is_err() {
                    return Ok(());
                }
                ctx.request_repaint();
                at_end = true;
            }
        }
    }
}

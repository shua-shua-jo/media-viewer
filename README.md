# Media Viewer

A fast, lightweight desktop app for browsing the photos and videos in a folder. Built in Rust with [egui](https://github.com/emilk/egui) / eframe.

Open a folder, flip through your media, and mark favorites with a heart. Favorites are automatically copied into a `Favorites` subfolder, so your best shots are always collected in one place.

## Features

**Browsing**
- Opens any folder and shows the photos and videos directly inside it, sorted by file name.
- Photos: JPG, JPEG, PNG, GIF, WebP, BMP.
- Videos: MP4, MOV, MKV, AVI, WebM, M4V. Videos open in your system's default player.
- Previous / next buttons, arrow keys, and a number box to jump straight to any item.

**Favorites**
- Click the heart (or press `F`) to favorite. Hollow heart = not favorited, filled light red = favorited.
- A favorite is copied into a `Favorites` subfolder inside the folder you're browsing. Your original is never touched.
- Click the heart again to remove the favorite (the copy is deleted).
- Filter the view with **All**, **Favorites**, or **Not favorites**.

**Zoom**
- Mouse wheel (or trackpad pinch) zooms toward the cursor.
- Zoom slider and `+` / `-` buttons in the bottom bar.
- Drag to pan while zoomed in. Double-click the image, press `0`, or click **Fit** to reset.

**Rotate**
- Rotates the *view only*. The file on disk is never changed.
- Each photo remembers its rotation while the app is open.

**Delete**
- Sends the file to the Recycle Bin / Trash, so it can be recovered.
- A copy in `Favorites`, if there is one, is left alone.

**Performance**
- Images are decoded on background threads, and the next few photos are preloaded, so moving between photos feels instant.
- Only a small window of images is kept in memory at a time.

## Install

1. Download the file for your system from the [Releases](../../releases) page.
2. (Optional) Verify it: compare its SHA-256 with `SHA256SUMS.txt`, or run
    `gh attestation verify <file> --repo shua-shua-jo/media-viewer`.
3. Unzip it and run `media_viewer`.

**First-run warnings.** These downloads are not code-signed yet, so your system may warn you the first time you run the app.
- **Windows:** SmartScreen may say "Windows protected your PC". Click *More info*, then *Run anyway*.
- **macOS:** if the app is blocked, run `xattr -d com.apple.quarantine ./media_viewer` once, then open it.
- **Linux:** requires GTK 3, which most desktops include. If the file won't start, run `chmod +x media_viewer`.

You can check any download against `SHA256SUMS.txt`, or with `gh attestation verify`, to confirm it came from this repository's build.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `←` / `→` | Previous / next |
| `F` | Favorite / unfavorite |
| `R` / `L` | Rotate view right / left |
| `Delete` | Move to Recycle Bin / Trash |
| `+` / `-` | Zoom in / out |
| `0` | Reset zoom to fit |

Shortcuts are paused while you're typing in the number box.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (install with `rustup`).
- An `assets/icon.png` file (square, 256×256 or larger). It's embedded into the program as the window icon, so the build fails without it.
- **Linux only:** a few system libraries. On Ubuntu/Debian:
  ```bash
  sudo apt install libgtk-3-dev libxkbcommon-dev libssl-dev
  ```
  Windows and macOS need nothing extra.

## Run

From the project folder:

```bash
cargo run --release
```

You can also pass a folder to open right away:

```bash
cargo run --release -- "C:\Users\You\Pictures"
```

The first build takes a few minutes while dependencies compile. Later builds are much faster.

> Use `--release` for normal use. Debug builds are noticeably slower at decoding large photos.

## Build a standalone app

```bash
cargo build --release
```

The program is created at:

| OS | Path |
|---|---|
| Windows | `target\release\media_viewer.exe` |
| macOS / Linux | `target/release/media_viewer` |

It's a single file that runs on its own. You can copy it anywhere, pin it to the taskbar, or make a shortcut.

### Optional: `.exe` file icon on Windows

The window icon comes from `assets/icon.png`. To also change the icon of the `.exe` file itself (shown in File Explorer and the taskbar), add an `assets/icon.ico`, add `winresource = "0.1"` under `[build-dependencies]` in `Cargo.toml`, and create a `build.rs` next to `Cargo.toml`:

```rust
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().unwrap();
    }
}
```

## Project structure

```
media_viewer/
├── Cargo.toml
├── assets/
│   └── icon.png          window icon (required)
└── src/
    ├── main.rs           window and icon setup
    ├── config.rs         all tunable settings
    ├── media.rs          media types, folder scanning, favorites reading
    ├── loader.rs         background image decoding
    ├── widgets.rs        zoomable image area, heart and arrow buttons
    └── app/
        ├── mod.rs        app state, opening folders, filtering
        ├── preload.rs    texture cache and preloading
        ├── actions.rs    favorite, delete, rotate, zoom
        ├── shortcuts.rs  keyboard shortcuts
        ├── toolbar.rs    top toolbar
        └── view.rs       per-frame UI, status bar, image area
```

To add a feature, create a new file in `src/app/`, declare it in `app/mod.rs`, and give it its own `impl App { ... }` block.

## Configuration

Settings live in `src/config.rs`. Change a value and rebuild.

| Setting | Default | What it does |
|---|---|---|
| `FAV_DIR` | `"Favorites"` | Name of the favorites subfolder |
| `MAX_TEXTURE_SIDE` | `4096` | Photos larger than this are downscaled for display |
| `PRELOAD_AHEAD` / `PRELOAD_BEHIND` | `2` / `1` | How many neighbors are decoded in advance |
| `WORKERS` | `2` | Background decoder threads |
| `MIN_ZOOM` / `MAX_ZOOM` | `0.2` / `8.0` | Zoom range (relative to fit-to-window) |
| `SCROLL_ZOOM_SPEED` | `0.002` | Mouse wheel zoom speed |
| `HEART_RED` | light red | Color of a favorited heart |
| `MIN_WINDOW_WIDTH` / `MIN_WINDOW_HEIGHT` | `600` / `400` | Smallest window size |

Each preloaded photo uses roughly width × height × 4 bytes of memory (about 48 MB for a 12-megapixel photo). Lower the preload numbers if memory matters more than speed.

## Known limitations

- Videos aren't played inside the app; they open in your default player.
- Animated GIFs show only their first frame.
- Photos are decoded at up to `MAX_TEXTURE_SIDE` pixels, so very deep zoom on large photos can look soft.
- Favorites added or removed outside the app aren't noticed until you reopen the folder.
- Only files directly inside the opened folder are shown; subfolders are not scanned.

## Dependencies

[eframe / egui](https://github.com/emilk/egui) (GUI), [image](https://crates.io/crates/image) (decoding), [rfd](https://crates.io/crates/rfd) (folder picker), [trash](https://crates.io/crates/trash) (Recycle Bin), [open](https://crates.io/crates/open) (launch videos). See `Cargo.toml` for versions.

## License

Released under the [MIT License](LICENSE).

Third-party libraries used by this project are distributed under their own licenses; see `Cargo.toml` for the list of dependencies.
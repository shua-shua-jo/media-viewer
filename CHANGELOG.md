# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Sort options: by name, date modified, or size, in either direction. Name order now ignores case and compares numbers naturally (`img2.jpg` before `img10.jpg`).
- Remembers the last folder, filter, sort order, and window size between runs.
- Fullscreen mode (F11, exit with Esc) and a slideshow (S) with a selectable interval; the slideshow skips videos.
- Undo delete (Ctrl+Z or the Undo delete button) restores the most recently deleted files from the Recycle Bin. Windows and Linux only.
- Optional Subfolders mode shows photos and videos from subfolders too (name sort groups by folder). Favorites of files in subfolders keep their subfolder inside `Favorites`.

## [1.1.0] - 2026-11-23

### Added
- Open a single file: from the command line (`media-viewer photo.jpg`), with the new **Open file** button, or by dragging a file or folder onto the window. The file's folder opens with that file selected.
- Optional embedded video playback (build with `--features video`): play, pause, replay, and a seek bar. Picture only for now (no audio). The downloads on the Releases page are built without it and still open videos in your default player.
- Space bar pauses and resumes embedded video.

## [1.0.1] - 2026-11-22

### Fixed
- Photos taken in portrait mode (and other camera orientations) are now displayed upright.

### Security
- Updated `url` to 2.5.9 (RUSTSEC-2026-0592).

## [1.0.0] - 2026-11-06

First stable release.

### Added
- Browse the photos and videos in a folder, with previous/next buttons, arrow keys, and a jump-to-number box.
- Favorites: a heart button (or `F`) copies the file into a `Favorites` subfolder; click again to remove it.
- All / Favorites / Not favorites filter.
- Zoom with the mouse wheel, pinch, slider, and buttons; drag to pan; double-click or `0` to reset.
- View-only rotation (the file on disk is never changed).
- Delete to the Recycle Bin / Trash.
- Background decoding and preloading of neighboring photos for instant navigation.
- Videos open in the system's default player.
- Release downloads for Windows, Linux, and macOS (Apple Silicon and Intel), with SHA-256 checksums and build attestations.

### Security
- Dependencies are audited for known vulnerabilities and licenses on every change and weekly.
- The app contains no `unsafe` code and does not intentionally make network connections.

[Unreleased]: https://github.com/shua-shua-jo/media-viewer/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/shua-shua-jo/media-viewer/releases/tag/v1.0.0
[1.0.1]: https://github.com/shua-shua-jo/media-viewer/releases/tag/v1.0.1
[1.1.0]: https://github.com/shua-shua-jo/media-viewer/releases/tag/v1.1.0
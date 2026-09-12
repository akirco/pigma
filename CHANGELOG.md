## [0.2.14] - 2026-09-12

### 🚀 Features

- *(app)* Add login status request guard and improve song management in navigation (akirco)

### 🐛 Bug Fixes

- Restore terminal state and sync startup auth (caojialin)
- Synchronize liked songs pagination (Mars160)
- 修复帮助面板滚动溢出导致向上滚动失效的问题 (lorlike)

### 💼 Other

- *(deps)* Bump log from 0.4.33 to 0.4.34 (dependabot[bot])
- *(deps)* Bump stream-download from 0.24.3 to 0.24.4 (dependabot[bot])

### 📚 Documentation

- *(README)* Enhance completion script instructions for bash, zsh, fish, and powershell (akirco)

### ⚙️ Miscellaneous Tasks

- *(ci)* Fmt (akirco)
## [0.2.13] - 2026-08-22

### 🚀 Features


- *(cli/waybar)* Remove native waybar output (`--waybar`); waybar integration now uses a standalone bash script (`waybar/pigma`) that calls `pigma status --json` per invocation, with `ensure_daemon` for auto-start
- *(cli)* Add `pigma msg play <song-id>` to jump to a song in the queue by id, and `pigma msg toggle_play` play/pause toggle (akirco)
- *(cli)* Add `pigma msg search <keyword>`: the daemon searches NCM + sonar sources, returns songs tagged by source, and registers them so `pigma msg play <id>` can enqueue and play a search result across instances (sonar synthetic ids resolve in-process) (akirco)
- *(cli)* Move queue listing into `pigma msg list` (reusing the TUI queue-table rendering, `▶` marks the current song); drop the standalone `pigma list <endpoint>` command so the CLI only talks to a running instance (akirco)
- *(cli)* Add `pigma completions <shell>` to generate shell completion scripts (bash/zsh/fish/elvish/powershell); `msg` actions are now a clap `ValueEnum`, so `pigma msg <Tab>` completes them (with aliases) (akirco)
- *(cli)* Fold the global `--playlist` into `-d` via `ENDPOINT:N` (e.g. `pigma -d toplist:3`); the old global flag stays as a hidden backward-compatible alias. `msg switch-list --playlist N` is unchanged (akirco)


### 🐛 Bug Fixes

- *(playback)* Rebuild the audio device when the output stream dies (#61): Bluetooth disconnect/reconnect no longer leaves playback silent. The sink is now owned by the player task; fatal cpal stream errors (`DeviceNotAvailable`/`StreamInvalidated`/backend device loss on WASAPI/CoreAudio/ALSA) and a frozen-position watchdog (suppressed during recent network underruns) both trigger a rebuild that resumes at the last position. The player also follows system default-output changes (debounced, stable-id based), so after a Bluetooth headset reconnects playback moves back to it automatically — macOS, Windows and Linux alike


## [0.2.12] - 2026-08-16

### 🚀 Features

- *(path)* Add expand_tilde function to handle home directory paths (akirco)

### 🐛 Bug Fixes

- *(cli)* List download and local music panic (akirco)

### 🚜 Refactor

- *(cli)* Streamline CLI command handling and improve structure (akirco)
- *(bilivideo)* Improve cookie handling and enhance error logging for transient failures (akirco)
## [0.2.11] - 2026-08-16

### 🐛 Bug Fixes

- *(playback)* Bundle minimal ALSA config for musl static builds (akirco)
- *(playback)* Use default hw card in bundled musl ALSA config (akirco)

### 💼 Other

- Remove musl targets (static musl cannot use PipeWire/PA plugins) (akirco)

### 🎨 Styling

- *(playback)* Fmt and wrap unsafe set_var for edition 2024 (akirco)

### ⚙️ Miscellaneous Tasks

- Generate release notes with git-cliff and remove release-drafter (akirco)
- *(ci)* Remove 'dev' branch from CI trigger paths (akirco)
# Changelog

## [0.2.10] - 2026-08-16

### 🚀 Features

- *(ci)* Build linux aarch64 and musl targets with cross in CI and release (akirco)
- Add initial configuration for TOML format rules (akirco)

### 🐛 Bug Fixes

- *(manifest)* Flatten multi-line inline tables to single-line for strict TOML parsers (akirco)
- Update JSON structure for playback control commands in documentation (akirco)

### 🚜 Refactor

- Refactor cache management and improve performance (akirco)

### ⚙️ Miscellaneous Tasks

- Trigger CI on dev branch pushes (akirco)
## [0.2.9] - 2026-08-15

### 🚀 Features

- Add IPC support for pigma status and msg commands (akirco)
- *(waybar)* Add configuration and scripts for Pigma integration (akirco)
- Enhance IPC with queue management and headless mode support (akirco)
- *(ipc)* Add Windows named pipe support and update IPC documentation (akirco)
- *(ci)* Add aarch64 and aarch64-musl build jobs to CI workflow (akirco)
- *(ci)* Remove Linux audio dependencies installation and add pre-build scripts for aarch64 targets (akirco)
- *(ci)* Replace build script with inline pre-build steps for aarch64-musl target (akirco)
- *(ci)* Add x86_64-musl build job and update pre-build steps for ALSA (akirco)

### 🐛 Bug Fixes

- *(search)* Clamp search limit to prevent exceeding API constraints (akirco)
- *(ci)* Update ALSA_URL to point to the official ALSA project site (akirco)
- *(ci)* Update pre-build steps and environment variables for aarch64 and x86_64 targets (akirco)
- *(ci)* Update ALSA version and add environment variables for cross-compilation (akirco)
- *(ci)* Streamline environment variable setup and remove unnecessary build.env section (akirco)
- *(playback)* Update cfg attributes for Linux to include GNU environment (akirco)
- *(ci)* Add 'sed' to install dependencies for ALSA build (akirco)
- *(playback)* Refine Linux target configuration to include GNU environment (akirco)
- *(ci)* Add installation of musl-tools and ALSA build steps (akirco)
- *(ci)* Update musl-tools installation and setup for ALSA build (akirco)
- *(ci)* Enhance musl-tools installation for ALSA build with additional flags (akirco)
- *(ci)* Add symlink for asm-generic in musl-tools installation (akirco)

### 🚜 Refactor

- *(sonar)* Update dependencies and improve MD5 usage (akirco)

### ⚙️ Miscellaneous Tasks

- Add VSCode settings for CSS file association (akirco)
- *(ci)* Fmt (akirco)
## [0.2.8] - 2026-08-13

### 🚀 Features

- Enhance theme configuration and navigation features (akirco)
- Updated default theme colors for better visibility and aesthetics.
- Added navigation event handling for login functionality in input handling.
- Improved login key handling to navigate to the main page upon escape.
- Enhanced main input handling with volume adjustment and navigation position cycling.
- Refined splash screen input handling to streamline user experience.
- Modified layout structures to accommodate new logo rendering in the login screen.
- Implemented API service checks for login requirements on specific endpoints.
- Introduced command actions for cycling navigation positions.
- Enhanced splash state to track display duration.
- Updated UI components for better rendering and user feedback.
- Added help text for new login functionality and volume controls.
- Improved overall code structure and readability across multiple files.

### 🐛 Bug Fixes

- *(scan)* Prevent ID collision with sonar song-id by clearing top bit (akirco)
- *(splash)* Update version display to use dynamic package version (akirco)

### 🚜 Refactor

- *(ui)* Replace create_block with CornerBlock for improved block handling (akirco)
- *(README)* Update feature list and improve markup syntax explanation (akirco)
- Refactor navigation state and UI components for improved clarity and functionality
- Updated NavState to include methods for retrieving focused section, selected index, and selected item.
- Removed unused LoginState from NavigationState and adjusted related references.
- Enhanced navigation drawing logic to utilize new NavState methods for better readability.
- Added documentation comments to clarify the purpose of various functions and structures.
- Refined text input and UI rendering components with improved comments and organization.
- Implemented a new render_gauge function to streamline gauge rendering in the player bar.
- Updated utility functions for better clarity and consistency in naming conventions.
- Enhanced gradient and path utilities with improved documentation for better understanding.

### ⚙️ Miscellaneous Tasks

- *(ncm_client)* Fmt (akirco)

### 📚 Documentation

- *(README)* add splash screen and plan sections to improve documentation clarity
- *(config)* document splash duration setting in configuration example

## [0.2.7] - 2026-08-11

### 🚀 Features

- Add Homebrew tap update workflow for tag releases (akirco)
- Enhance audio playback error handling and buffering logic (akirco)

### 🚜 Refactor

- Remove ApiEndpoint enum from api.rs and integrate it into service.rs (akirco)
- *(app)* Implement event handling and navigation improvements (akirco)

### ⚙️ Miscellaneous Tasks

- Update dependencies in Cargo.lock and Cargo.toml for ncm-api and sonar (akirco)
## [0.2.6] - 2026-08-10

### 🚜 Refactor

- Update cookie file handling to improve permissions and writing logic (akirco)
- Refactor playback and UI components for improved performance and clarity
- Updated `handle_main_key` to remove unnecessary Arc wrapping for songs.
- Introduced `current_resolve` in `PlaybackEngine` to manage in-flight song resolve tasks.
- Modified `activate_by_id` to accept a `persist_previous` flag for better queue management.
- Changed song handling in `play_songs` and `append_songs_to_key` to use `Arc<SongInfo>` directly.
- Enhanced `ApiService` to return `Arc<SongInfo>` for shared ownership across components.
- Refactored UI rendering functions to streamline theme resolution and scrollbar rendering.
- Updated help text for clearing the playback queue to use 'w' instead of 'Ctrl+L'.
- Improved overall code readability and maintainability by reducing unnecessary clones and enhancing comments.

## [0.2.5] - 2026-08-09

### 🚀 Features

- Add keyboard shortcuts for manual refresh and help panel (akirco)
- Enhance song liking functionality with improved event handling and user feedback (akirco)
- Implement liked songs functionality with cloud sync and UI updates (akirco)

### 🐛 Bug Fixes

- *(ci)* Fmt (akirco)
## [0.2.4] - 2026-08-09

### 🚀 Features

- Add 'save_on_play' configuration and related functionality (akirco)
- Add Skeleton widget for loading state representation (akirco)

### 🚜 Refactor

- *(ncm-api)* Rewrite ncm-api (akirco)
- Update imports to use playback module for PlaybackState (akirco)
## [0.2.3] - 2026-08-08

### 🐛 Bug Fixes

- *(cover)* Wt img capability check (akirco)

### ⚙️ Miscellaneous Tasks

- Adjust file and code structure (akirco)
- Update config example (akirco)
- *(ci)* Fmt (akirco)
## [0.2.2] - 2026-08-07

### 🚀 Features

- *(sonar)* New fetch playlist api(get track_ids + lazy pagination) (akirco)
- *(mode)* Add toast for mode switch (akirco)

### 🐛 Bug Fixes

- *(sonar)* Examples and new test for bibili search (akirco)
- *(styled_text)* Styled_text is overrideded by default (akirco)

### 🚜 Refactor

- *(content)* Use built-in Row instead of manually (akirco)
- *(playlist)* New data loading logic (akirco)
- *(utils)* Remove unnecessary utils export (akirco)
- *(playerbar)* Simplify code logic (akirco)

### ⚙️ Miscellaneous Tasks

- *(ci)* Fmt (akirco)
## [0.2.1] - 2026-08-06

### 🐛 Bug Fixes

- *(theme)* Unknown color name(removed) (akirco)
## [0.2.0] - 2026-08-06

### 🚀 Features

- *(musicx)* Third-party multi-source search with lyrics/cover fallback (akirco)
- *(musicx)* Lyrics/cover loading and playback queue integration (akirco)
- *(input)* Search source switching and shortcut enhancements (Tab to switch source, g/G, S to like current playing) (akirco)
- *(ui)* Help popup and proxy config support for Normal/Reversed/Both (akirco)
- *(layout)* Hide sidebar and adapt cover size on narrow terminals (akirco)
- *(theme)* Add light theme and title style (akirco)
- *(playback)* Update progress bar color on cache completion and persist musicx registry (akirco)
- *(musicx)* Register utils::musicx module (akirco)

### 🐛 Bug Fixes

- *(playback)* Bilibili stream download 403 and proxy support for stream downloads (akirco)
- *(ncm-api)* Use device id and md5-hashed password in login (akirco)

### 🚜 Refactor

- *(playback)* Remove types.rs, merge types into playback.rs (akirco)
- [**breaking**] Rename musicx crate to sonar (akirco)
- *(core)* Migrate pigma to the sonar crate (akirco)

### 📚 Documentation

- Update README.md (akirco)

### 🧪 Testing

- *(musicx)* Testing (akirco)

### ⚙️ Miscellaneous Tasks

- *(ci)* Bump checkout and ssh-agent actions (akirco)
- *(config)* Update example config and linker flags (akirco)
## [0.1.9] - 2026-08-03

### 🚀 Features

- *(crates/musicx)* Unifield fallback sound source (akirco)

### 🚜 Refactor

- *(config)* Restructuring the config structure (akirco)
- *(fallback)* Using new sound source fallback (akirco)
- *(layout)* New navigation layout (akirco)

### 📚 Documentation

- *(README)* Add aur installtion desc (akirco)
- Update README.md (akirco)

### ⚙️ Miscellaneous Tasks

- Update deps (akirco)
## [0.1.8] - 2026-08-01

### 🚀 Features

- *(navigation)* New layout (top) (akirco)

### ⚙️ Miscellaneous Tasks

- *(release)* Add aur release (akirco)
- *(state)* Nav.rs renamed to navigation.rs (akirco)
## [0.1.7] - 2026-07-30

### 🐛 Bug Fixes

- *(events)* Seeking spinner (akirco)
- *(cover)* Ratatui-image image protocol check failed (akirco)
- *(navigation)* Need not cache failed responses (akirco)

### 🚜 Refactor

- *(playback)* Optimize memory by reusing player & manual memory recycling (akirco)

## [0.1.6] - 2026-07-29

### 💼 Other

- *(deps)* Bump actions/checkout from 4 to 7 (dependabot[bot])
- *(deps)* Bump softprops/action-gh-release from 2 to 3 (dependabot[bot])
- *(deps)* Bump actions/upload-artifact from 4 to 7 (dependabot[bot])

### 🚜 Refactor

- *(config)* Rewrite config file inline ArrayOfTables. (akirco)
- *(playerbar)* Fix layout issues (akirco)
- *(playerback)* Adjust cpal buffersize,reduce the frequency of thread wakes (akirco)
## [0.1.5] - 2026-07-27

### 🚀 Features

- *(navigation)* Add saved albums navigation tab (#18) (AshGrey🥕)
- Migrate API to service calls, improve uploads, local music cloud drive, and cover caching, update contribution guide (akirco)
- fixs: cache value `accessed_at` always 0, table content overwritten during fast navigation (akirco)

## [0.1.4] - 2026-07-26

### Added

- Daily recommendation "not interested": press `d` to mark a song as not interested, telling the algorithm not to recommend similar songs
- Daily recommendation "like": press `s` to add a song to My Liked Music (available on all song pages)
- Proxy target config `proxy_target`: supports `yt` (proxy YouTube, default), `ncm` (proxy NetEase Cloud Music), `both` (proxy both)

### Changed

- Perf: per-character gradient lyric rendering eliminates per-char String allocation (zero-allocation borrowing)
- Perf: gradient preset changed from string dispatch to enum match, eliminating multiple string comparisons per frame
- Perf: table field query returns `Cow` to avoid String clone
- Perf: player bar time display reuses `format_duration_into` buffer
- Perf: cache lookup merged into a single RwLock + iteration (was 4 locks + stat)
- Perf: cache total size tracked via `AtomicU64`, evict avoids O(n) stat syscalls
- Perf: evict sorting avoids filename clone
- Perf: `collect_cached_songs` removes redundant `path.exists()` check
- Perf: storage IO (playlist saving) offloaded to blocking thread via `spawn_blocking`
- Local music now loads on demand: released when switching navigation, reloaded from disk cache or re-scanned when returning
- My Liked Music: fixed missing cache write, now writes to disk cache after first load
- My Liked Music: most recently liked songs shown at top of list (IDs reversed)
- NCM proxy fix: corrected `like` endpoint params (endpoint `/api/radio/like`, params `trackId`/`alg`/`time`)
- Daily recommendation dislike endpoint corrected to `/api/v2/discovery/recommend/dislike` (params `resId`/`resType`/`sceneType`)

### Removed

- Playback reporting feature (`report_play` API call and `pending_report` mechanism)

### Fixed

- Fixed ratatui-image loading album covers using excessive memory (request 200x200 thumbnail from NCM CDN instead of original image)

## [0.1.3] - 2026-07-25

### Added

- Album cover display: terminal album cover rendering via `ratatui-image`, auto-cropped to circle
- Multiple player bar layouts: `default`, `modern`, `minimal`, configurable via `playerbar.layout`
- Player bar component visibility config: `playerbar.visible` independently controls cover, volume, play mode, and loading animation
- Border gradient animation: `border_gradient` and `border_gradient_speed` options with clockwise flowing gradient effect
- Config example: new `config.example.toml` with complete documentation of all options
- Centralized API service layer (`service.rs`): unified endpoint resolution, cache integration, and error mapping
- Recursive local music scan: automatically scans audio files in subdirectories
- Search result limit: new `search_limit` config option
- Automatic cache eviction: LRU-based auto cleanup of cache over 2GB, with stale entry cleanup

### Changed

- Refactored all API calls from `self.api` to `self.service.client()`, decoupling business layer from API layer
- Player bar split into multi-module structure (`widgets`, `build_layout`, `default_layout`, `modern_layout`, `minimal_layout`)
- Cache index lock upgraded from `Mutex` to `RwLock` for better concurrent read performance
- NCM network retries reduced from 3 to 2 for faster fallback to YouTube source
- buffer underrun/overrun errors silently ignored, rodio auto-recovers
- `PlaybackEngine::new` now takes `CacheManager` directly instead of scattered path/template params
- Removed commented-out dev-dependencies in `Cargo.toml`

### Fixed

- Fixed cache index potentially containing incomplete download entries (now written only after download completes)
- Fixed stale entries from deleted files not cleaned in cache index (auto-cleaned on exit)
- Fixed local music scan missing audio files in subdirectories

## [0.1.2] - 2026-07-23

### Added

- Gradient progress bar (GradientLineGauge) with colorgrad preset themes
- Border config `BorderConfig` with `rounded` and `follow_corner_color` options
- Player progress bar gradient config: `gradient_enabled` and `gradient_preset`
- Cache index stores song duration to avoid decoding audio on playlist load
- Async cache methods: `load_lyrics_cache_async`, `list_cached_songs_async`
- YouTube search helper module (`utils/youtube.rs`) with traditional/simplified Chinese normalization and improved match scoring

### Changed

- Refactored event system: `AppEvent` split into five domain sub-events `SplashEvent`, `AuthEvent`, `PlaybackEvent`, `NavigationEvent`, `CommandEvent`
- Unified playback strategy into a single `Strategy` enum, removed `Box<dyn PlayStrategy>` dynamic dispatch
- Player `player::run` returns a oneshot completion signal, ensuring previous track's decoder/sink/StreamDownload fully released before next starts
- YouTube search helpers extracted from `AudioSource` into a separate module
- Removed example files in examples dir and dev-dependencies
- Added `rustfmt.toml` for unified code formatting

### Fixed

- Fixed resource leak from old player resources (HTTP connections, buffers) not released promptly on track switch
- Fixed cache index deserialization compat with old format (plain string → new object format smooth migration)

## [0.1.1] - 2026-07-21

### Added

- YouTube fallback playback via y7dl submodule
- User-created playlist API (`user_created_playlist`)
- User-collected playlist API (`user_collected_playlist`)
- `SongList` model adds `subscribed` field
- Navigation adds "My Created Playlists" and "My Collected Playlists" endpoints
- Cache manager supports indexed cache and custom filename templates
- New cache config options: `cache_dir`, `quality`, `cache_template`
- History queue limited to max 200 songs
- Heartbeat mode limited to max 500 songs with auto queue trimming
- Auto-select current song in content list during playback
- Liked Music auto-sets playlist ID to support Heartbeat mode

### Changed

- Refactored cache manager to use `cache_index.json` indexed cache
- Split "My Playlists" into "My Created Playlists" and "My Collected Playlists"
- Improved audio quality selection with configurable `SongQuality`
- Enhanced Heartbeat mode logging and error handling
- Fixed local file playback, improved local music scan using path-based unique IDs

### Fixed

- Fixed downloaded music showing 00:00 duration (reads actual duration from audio file)

### Documentation

- Updated license info to Apache-2.0 and added usage notes
- Added Windows Scoop installation instructions

## [0.1.0] - 2026-07-20

### Added

- Pigma initial release - terminal music player
- Playback engine supporting multiple audio formats (MP3, FLAC, WAV, OGG, AAC, M4A, WMA)
- NetEase Cloud Music API integration for streaming playback
- Local music scanning and playback
- Playlist management with auto save/restore
- Multiple play modes: sequential, single loop, list loop, random, heartbeat
- Volume control and progress seeking
- Lyrics display and translation support
- UI styled text rendering
- UI gradient theme support
- Downloaded/cached song management
- Search functionality
- Keyboard shortcut navigation
- Playback queue management
- Artist and album browsing
- Charts browsing
- QR code login

### Changed

- Refactored UI and utility modules for better performance and organization
- Refactored playback module and UI components
- Improved code readability and module organization
- CI workflow adds Linux audio dependency installation
- Refactored log initialization and enhanced playback features

### Fixed

- Simplified release targets for stable builds
- Fixed release workflow dependencies and artifact upload
- Ran cargo fmt to unify code style

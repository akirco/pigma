# pigma Review TODO

> Review 基准：`289bceb`（dev 分支），约 3 万行 Rust。
> 验证：`cargo test --all-targets`（131 + 8 IPC）、`cargo clippy --all-targets -- -D warnings`、
> `cargo +nightly fmt --check` 全部通过；`crates/ncm-api`（49 个）与 `crates/sonar`（24 个）单测通过。
> **P1、P2 全部处理完毕**（2026-10-29；P2-3 设计如此不修改）；P3 待处理。

## P1 — 尽快修复

- [x] **1. 「边听边存」在下载完成前就写入了缓存索引**
  - 位置：`src/playback/source.rs:132-153`
  - 现象：`tracked_settings` 的 `mark_cache` 分支没有判断 `complete`，`stream-download` 在
    Prefetching 阶段每个 chunk 都会回调（crate `src/source/mod.rs:327`），首次 ~256KB 回调就调用
    `cache.mark_cached`，与注释「下载完成后才记录」相反。
  - 后果：中途切歌/退出会留下被索引的截断文件；`cached_total_bytes` 偏小导致 2GB LRU 淘汰失效；
    `PlaybackEvent::Cached` 过早点亮；重复缓存同一首歌会重复累加体积。
  - 修复：`mark_cached` + `Cached` 事件只在 `state.phase == StreamPhase::Complete` 时触发；
    `CacheManager::mark_cached` 替换旧条目时减去旧文件体积。
  - 回归测试：`playback::source::tests::cache_entry_is_finalized_only_once_the_download_completes`
    与 `temporary_streams_never_finalize`。

- [x] **2. 启动页网络检测忽略代理且无超时，代理用户会被误判离线**
  - 位置：`src/app/splash.rs:20-23`，消费方 `src/app.rs:370-371`
  - 现象：`TcpStream::connect("music.163.com:443")` 直连、无超时、忽略 `config.proxy`。
  - 后果：`proxy_target = "reversed"` 的海外用户直连失败 → `state.offline = true` →
    启动后直接进入本地音乐；网络不通时启动被 OS TCP 超时（可达 1–2 分钟）卡住。
  - 修复：改为通过已配置代理的 `NcmClient` 调用 `search_hot()` 探测，并包
    `tokio::time::timeout`（`NETWORK_CHECK_TIMEOUT = 3s`）。

## P2 — 逻辑/一致性问题

- [x] **3. 事件循环没有 tick：暂停或空闲时界面完全不刷新** — **设计如此，不修改**
  - 位置：`src/app/event.rs:141-155`
  - 暂停时 `player.rs:466-468` 不再发 `Progress`，主循环阻塞在 `events.next().await`：
    toast 不消失、边框渐变/spinner 冻结，只有按键等外部事件才重绘。
  - 结论：按用户确认，保持"事件驱动重绘"的设计，不引入周期 tick。

- [x] **4. 队列内搜索过滤会破坏 `current_index` 和 shuffle 顺序**
  - 位置：`src/playback/engine/queueing.rs:41-54`、`src/playback/queue.rs:91-95`、
    `src/playback/mode.rs:43-49`、`src/playback/engine/queueing.rs:183-192, 200-215`
  - `set_queue_songs` / `set_queue_indices` 只换列表，不修正 `current_index`、不重建 strategy；
    `Shuffle.order` 保存绝对下标，可能越界；`advance_to` / `prev` 不校验范围。
    结果：过滤后越界索引被持久化，重启后队列失效。
  - 修复：新增 `replace_queue_songs`，按正在播放的歌曲 id 重定位 `current_index` 并重建 strategy
    （`PlaylistQueue::set_songs(songs, keep_current)`）；`next` / `prev` 对 strategy 返回的下标
    做边界校验。
  - 回归测试：`playback::queue::tests::set_songs_follows_the_current_song_by_id`。

- [x] **5. 心动模式向队列追加歌曲时不递增 `queue.version()`**
  - 位置：`src/playback/engine/heartbeat.rs:73-94`
  - `play_heartbeat_song` 直接改 `songs` / `current_index`，缺 `self.queue.bump()`；
    `IpcState::update`（`src/ipc/state.rs:74-86`）因此不刷新 `pigma status -L` 快照。
  - 修复：追加/裁剪后补 `self.queue.bump()`。

- [x] **6. sonar 兜底循环里的 `?` 让「兜底」只尝试第一个源**
  - 位置：`crates/sonar/src/search.rs:448-463, 468-483, 493-505`
  - `search_first` 中 `find(...).cloned()?`、`get_lyrics_fallback` / `get_cover_fallback`
    中 `search_first(...).await?`：第一个源未配置或失败就整体返回，后续源不会尝试。
  - 修复：换成 `continue` / `let Some(..) = .. else { continue }`。
  - 回归测试：`search::tests::{search_first_skips_sources_that_are_not_configured,
    cover_fallback_keeps_trying_after_a_missing_source,
    lyrics_fallback_keeps_trying_after_a_missing_source}`。

- [ ] **7. 云盘上传把同步全量文件读取放在 async 线程上**
  - 位置：`crates/ncm-api/src/client/cloud.rs:91, 103-124`，调用方 `src/app/navigation.rs:298-308`
  - `stream_file_digest` + `parse_audio_meta` 在 async fn 中同步执行，调用方是 `tokio::spawn`
    而非 `spawn_blocking`；大文件会占住 tokio worker。
  - 修复：把「算 MD5 + 读元数据」放进 `spawn_blocking`。

- [x] **8. `config.toml` 和队列文件都是非原子写**
  - 位置：`src/config.rs:236-251`、`src/playback/storage.rs:298-313`
  - `fs::write` 截断后写，崩溃/被杀会留下空文件或半截 JSON：配置丢、会话恢复失败。
  - 修复：`Config::save` 与队列文件/`active_queue.txt` 统一改走 `cache::write_atomic`
    （temp + fsync + rename）；写入失败改为 `log::warn!` 而不是静默丢弃。
  - 回归测试：`playback::storage::tests::save_queue_sync_round_trips_without_temp_files`。
  - 遗留：`save_queue` 的 `spawn_blocking` 与退出时 `save_queue_sync` 仍存在完成顺序竞争
    （原子写只保证不产生半截文件，不保证最后落盘的是最新快照）。

## P3 — 工程/健壮性

- [ ] **9. CI 不跑子 crate 测试**：root 不是 workspace，`cargo test` 只跑 pigma 包；
  `crates/ncm-api`（49 个）、`crates/sonar` 的测试在 CI 中是盲区。
  建议加 `[workspace] members` 或在 CI 中分别 `cd` 进去跑。
- [ ] **10. CI 不覆盖 `dev` 分支**：`.github/workflows/ci.yml` 的 push 分支是
  `main/master/develop`（仓库实际用 `dev`）；test job 只跑 macOS/Windows，Linux 只跑 clippy。
- [ ] **11. 发布二进制要求 x86-64-v3（AVX2）**：`.cargo/config.toml` 写死
  `-C target-cpu=x86-64-v3`，老 CPU 会 `Illegal instruction`；`config.toml.bak` 被 git 跟踪，可删。
- [ ] **12. 日志默认 Debug 且无上限**：`src/logger.rs:31-36, 74-83` 会把
  reqwest/rustls/symphonia 的 debug（含请求体/账号信息）写入 `~/.config/pigma/debug.log` 且无限增长。
  建议默认 `Info` + 按大小滚动（>5MB）。
- [ ] **13. IPC socket 权限过宽**：`src/ipc/transport.rs:59` 默认 0755（umask 022），
  同机其他用户可控制播放、读 `status`。bind 后 `set_permissions(0o600)`，缓存目录 0700。
- [ ] **14. `panic = "abort"` 下鼠标捕获不恢复**：`src/main.rs:21-48` 的 `TerminalGuard::drop`
  在 release 不执行，ratatui panic hook 只恢复屏幕/raw mode。建议自定义 panic hook 补
  `DisableMouseCapture`。
- [ ] **15. PKGBUILD 版本过期**：`PKGBUILD:4` 为 `pkgver=0.1.0`（Cargo 0.2.16），`sha256sums=SKIP`。
- [ ] **16. 索引删除不落盘**：`src/cache/index.rs:209-218`、`src/cache/audio.rs:270-282`
  只改内存；退出时 `cleanup_index + flush_index` 会自愈，崩溃时可能指向已删文件。
- [ ] **17. `Stretch::try_seek` 方向可疑**：`src/playback/stretch/source.rs:89-93` 在
  `PreservePitch` 下 `pos * factor` 会过冲。当前 seek 走「重建 decoder」路径，属死代码；
  若接 `Player::try_seek` 需改成 `pos`。
- [ ] **18. 均衡器多字段快照用 Relaxed**：`src/playback/equalizer.rs:49-68, 149-155`，
  aarch64 上可能读到新 generation + 旧 band 且不自愈；`generation` 改用 `Release`/`Acquire`。
- [ ] **19. 死代码内存风险**：`crates/ncm-api/src/client/download.rs:24,48` 的
  `download_img` / `download_song` 会把整个文件读进内存，目前无调用方。

## 亮点

- 均衡器：WSOLA 之前、每声道独立级联 DF1、平坦配置精确 passthrough、Nyquist 保护、
  retune 不重置状态、带频响测试。
- 缓存：`write_atomic`（temp + fsync + rename）、索引脏标记合并写、内容缓存写锁。
- Cookie：`merged_for_disk` 多实例合并、0600 权限、原子写，有并发测试。
- WSOLA：23 个测试覆盖变速比例/音高/位置跟随。
- `lock_recover` 统一处理 mutex 中毒；`unsafe` 仅限 `dup/dup2` 与 `malloc_trim`，均有 SAFETY 说明。

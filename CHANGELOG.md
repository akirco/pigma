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
- *(styled_text)* Styled_text is overrideden by default (akirco)

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

- *(musicx)* 第三方多源搜索与歌词/封面 fallback (akirco)
- *(musicx)* 歌词/封面加载与播放队列集成 (akirco)
- *(input)* 搜索源切换与快捷键增强（Tab 切换源、g/G、S 喜欢当前播放） (akirco)
- *(ui)* ? 帮助弹窗与代理配置支持 Normal/Reversed/Both (akirco)
- *(layout)* 窄终端隐藏侧边栏与封面尺寸适配 (akirco)
- *(theme)* 添加浅色主题与标题样式 (akirco)
- *(playback)* 缓存完成更新进度条颜色与 musicx registry 持久化 (akirco)
- *(musicx)* 注册 utils::musicx 模块 (akirco)

### 🐛 Bug Fixes

- *(playback)* B站流下载 403 与流下载代理支持 (akirco)
- *(ncm-api)* Use device id and md5-hashed password in login (akirco)

### 🚜 Refactor

- *(playback)* 移除 types.rs，类型并入 playback.rs (akirco)
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
- 迁移 API 到 service 调用，完善上传、本地音乐云盘及封面缓存，更新贡献指南 (akirco)
- fixs: 缓存值`accessed_at`始终为0,快速导航时表格内容覆盖

## [0.1.4] - 2026-07-26

### 新增
- 每日推荐「不感兴趣」：按 `d` 键标记歌曲为不感兴趣，告诉算法不再推荐类似歌曲
- 每日推荐「喜欢」：按 `s` 键将歌曲添加到我喜欢的音乐（所有歌曲页面可用）
- 代理目标配置 `proxy_target`：支持 `yt`（代理 YouTube，默认）、`ncm`（代理网易云）、`both`（都代理）

### 变更
- 性能优化：歌词逐字渐变渲染消除 per-char String 分配（零分配借用）
- 性能优化：渐变预设从字符串 dispatch 改为枚举 match，消除每帧多次字符串比较
- 性能优化：表格内容字段查询返回 `Cow` 避免 String clone
- 性能优化：播放栏时间显示复用 `format_duration_into` buffer
- 性能优化：缓存查找合并为单次 RwLock + 遍历（原来 4 次锁 + stat）
- 性能优化：缓存总大小用 `AtomicU64` 追踪，evict 避免 O(n) stat 系统调用
- 性能优化：evict 排序避免 filename clone
- 性能优化：`collect_cached_songs` 移除冗余 `path.exists()` 检查
- 性能优化：存储 IO（playlist 保存）通过 `spawn_blocking` 卸载到 blocking 线程
- 本地音乐改为按需加载：切换导航时释放，切回时从磁盘缓存或重新扫描
- 我喜欢的音乐：修复缓存写入缺失，现在首次加载后会写入磁盘缓存
- 我喜欢的音乐：最新喜欢的歌曲显示在列表顶部（IDs reverse）
- NCM 代理修复：`like` 接口参数修正（端点 `/api/radio/like`，参数 `trackId`/`alg`/`time`）
- 每日推荐 dislike 接口修正为 `/api/v2/discovery/recommend/dislike`（参数 `resId`/`resType`/`sceneType`）

### 移除
- 播放上报功能（`report_play` API 调用及 `pending_report` 机制）

### 修复
- 修复 ratatui-image 加载专辑封面占用过大内存的问题（请求 NCM CDN 200x200 缩略图替代原图）

## [0.1.3] - 2026-07-25

### 新增
- 专辑封面显示：基于 `ratatui-image` 实现终端内专辑封面渲染，自动裁切为圆形
- 播放栏多布局支持：`default`、`modern`、`minimal` 三种布局，通过 `playerbar.layout` 配置
- 播放栏组件可见性配置：`playerbar.visible` 支持独立控制封面、音量、播放模式、加载动画的显示
- 边框渐变动画：`border_gradient` 和 `border_gradient_speed` 配置项，支持顺时针流动渐变效果
- 配置文件示例：新增 `config.example.toml`，包含所有配置项的完整说明
- 集中式 API 服务层（`service.rs`）：统一封装端点解析、缓存集成和错误映射
- 本地音乐递归扫描：自动扫描子目录中的音频文件
- 搜索结果数量限制：新增 `search_limit` 配置项
- 缓存自动淘汰：基于 LRU 策略自动清理超过 2GB 的缓存，支持 stale 条目清理

### 变更
- 重构所有 API 调用从 `self.api` 迁移至 `self.service.client()`，解耦业务层与 API 层
- 播放栏拆分为多模块结构（`widgets`、`build_layout`、`default_layout`、`modern_layout`、`minimal_layout`）
- 缓存索引锁从 `Mutex` 升级为 `RwLock`，提升并发读性能
- NCM 网络重试次数从 3 次降为 2 次，更快回退到 YouTube 音源
- buffer underrun/overrun 错误静默忽略，rodio 会自动恢复
- `PlaybackEngine::new` 直接接收 `CacheManager` 而非分散的路径/模板参数
- 移除 `Cargo.toml` 中已注释的 dev-dependencies

### 修复
- 修复缓存索引可能包含未完成下载条目的问题（改为下载完成后才写入索引）
- 修复文件已删除但缓存索引未清理导致的 stale 条目（退出时自动清理）
- 修复本地音乐扫描遗漏子目录音频文件的问题

## [0.1.2] - 2026-07-23

### 新增
- 渐变进度条（GradientLineGauge），支持 colorgrad 预设主题
- 边框配置 `BorderConfig`，支持 `rounded` 和 `follow_corner_color` 选项
- 播放器进度条新增渐变色配置：`gradient_enabled` 和 `gradient_preset`
- 缓存索引存储歌曲时长，避免播放列表加载时解码音频文件
- 异步缓存方法：`load_lyrics_cache_async`、`list_cached_songs_async`
- YouTube 搜索辅助模块（`utils/youtube.rs`），含繁简中文归一化和改进的匹配评分

### 变更
- 重构事件系统：`AppEvent` 拆分为 `SplashEvent`、`AuthEvent`、`PlaybackEvent`、`NavigationEvent`、`CommandEvent` 五个领域子事件
- 统一播放策略为单一 `Strategy` 枚举，移除 `Box<dyn PlayStrategy>` 动态分派
- 播放器 `player::run` 返回 oneshot 完成信号，确保上一曲的 decoder/sink/StreamDownload 完全释放后再启动下一曲
- YouTube 搜索工具函数从 `AudioSource` 提取至独立模块
- 移除 examples 目录下的示例文件和 dev-dependencies
- 添加 `rustfmt.toml` 统一代码格式

### 修复
- 修复切换歌曲时旧播放器资源（HTTP 连接、缓冲区）未及时释放导致的资源泄漏
- 修复缓存索引反序列化兼容旧格式（纯字符串 → 新对象格式平滑迁移）

## [0.1.1] - 2026-07-21

### 新增
- 通过 y7dl 子模块支持 YouTube 回退播放
- 用户创建歌单 API（`user_created_playlist`）
- 用户收藏歌单 API（`user_collected_playlist`）
- `SongList` 模型新增 `subscribed` 字段
- 导航新增「我创建的歌单」和「我收藏的歌单」端点
- 缓存管理器支持索引化缓存和自定义文件名模板
- 新增缓存配置选项：`cache_dir`、`quality`、`cache_template`
- 历史队列限制为最多 200 首
- 心动模式限制为最多 500 首，并自动裁剪队列
- 播放时自动选中内容列表中的当前歌曲
- 喜欢的音乐自动设置歌单 ID 以支持心动模式

### 变更
- 重构缓存管理器，使用 `cache_index.json` 索引化缓存
- 将「我的歌单」拆分为「我创建的歌单」和「我收藏的歌单」
- 改进音频质量选择，支持配置 `SongQuality`
- 增强心动模式日志和错误处理
- 修复本地文件播放问题，改进本地音乐扫描，使用路径生成唯一 ID

### 修复
- 修复下载音乐时长显示 00:00 的问题（从音频文件读取实际时长）

### 文档
- 更新许可证信息为 Apache-2.0 并添加使用说明
- 添加 Windows Scoop 安装说明

## [0.1.0] - 2026-07-20

### 新增
- Pigma 首次发布 - 终端音乐播放器
- 播放引擎，支持多种音频格式（MP3、FLAC、WAV、OGG、AAC、M4A、WMA）
- 集成网易云音乐 API 进行流媒体播放
- 本地音乐扫描和播放
- 播放列表管理，支持自动保存/恢复
- 多种播放模式：顺序、单曲循环、列表循环、随机、心动
- 音量控制和进度拖动
- 歌词显示和翻译支持
- UI 样式文本渲染
- UI 渐变色主题支持
- 已下载/缓存歌曲管理
- 搜索功能
- 键盘快捷键导航
- 播放队列管理
- 歌手和专辑浏览
- 排行榜浏览
- 二维码登录

### 变更
- 重构 UI 和工具模块以提升性能和组织性
- 重构播放模块和 UI 组件
- 改进代码可读性和模块组织
- CI 工作流添加 Linux 音频依赖安装
- 重构日志初始化并增强播放功能

### 修复
- 简化稳定版构建的发布目标
- 修复发布工作流依赖和制品上传
- 运行 cargo fmt 统一代码风格




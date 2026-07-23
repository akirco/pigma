# 更新日志

本文件记录项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
本项目遵循 [语义化版本控制](https://semver.org/lang/zh-CN/)。

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

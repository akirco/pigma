# pigma (In development)

A NetEase Cloud Music TUI client built with Ratatui.

## Preview



<table>
  <tr>
    <td><img src="./imgs/image_001.png" width="100%" /></td>
    <td><img src="./imgs/image_002.png" width="100%" /></td>
  </tr>
  <tr>
    <td><img src="./imgs/image_003.png" width="100%" /></td>
    <td><img src="./imgs/image_004.png" width="100%" /></td>
  </tr>
</table>


## Install

```sh
cargo install --git https://github.com/akirco/pigma.git
```

## Usage


| 快捷键       |     描述      |
| :----------- | :-----------: |
| 󱞥            | 播放/进入列表 |
| 󱁐            |     暂停      |
| f            |   播放队列    |
| l            |     歌词      |
| /            |   搜索/过滤   |
| b            |   样式切换    |
|  /         |   seek 15s    |
| shift +  / | 上一首/下一首 |
| ctrl+p       | command panel |



## Configuration

Config file location: `~/.config/pigma/config.toml`

### Columns Configuration

Each content type has two levels of columns: **type-defaults** and **per-API overrides**.

#### Default columns by content type

```toml
[columns]
songs = [
    { header = "TITLE", field = "name", min_width = 18 },
    { header = "ARTIST", field = "singer", width = 16 },
    { header = "ALBUM", field = "album", min_width = 12 },
    { header = "DURATION", field = "duration", width = 9 },
]
songlist = [
    { header = "NAME", field = "name", min_width = 20 },
    { header = "AUTHOR", field = "author", width = 16 },
]

[columns.overrides]
toplist = [
    { header = "NAME", field = "name", width = 20 },
    { header = "DESCRIPTION", field = "description", min_width = 20 },
]
search = [
    { header = "HOT SEARCH", field = "keyword", min_width = 1 },
]
```

#### Column width types

| Format           | Description               |
| ---------------- | ------------------------- |
| `width = 16`     | Fixed width in characters |
| `min_width = 18` | Minimum width, flex grows |
| `ratio = [1, 3]` | Proportional ratio weight |

#### Available fields by content type

**`songs`** (SongInfo) — used by these APIs:

| API               | Description         |
| ----------------- | ------------------- |
| `recommend_songs` | 每日推荐            |
| `user_cloud_disk` | 我的音乐云盘        |
| `recent_songs`    | 最近播放            |
| `liked_songs`     | 我喜欢的音乐        |
| `local_music`     | 本地音乐            |
| Playlist entry    | 歌单/排行榜内的歌曲 |

Fields:

| field       | Type   | Notes                          |
| ----------- | ------ | ------------------------------ |
| `name`      | String | 歌曲名                         |
| `singer`    | String | 歌手                           |
| `album`     | String | 专辑                           |
| `duration`  | u64    | 时长(ms)，自动格式化为 `MM:SS` |
| `id`        | u64    | 歌曲 ID                        |
| `album_id`  | u64    | 专辑 ID                        |
| `pic_url`   | String | 封面 URL                       |
| `copyright` | String | 版权状态                       |

**`songlist`** (SongList) — used by these APIs:

| API                  | Description |
| -------------------- | ----------- |
| `recommend_resource` | 推荐歌单    |
| `top_song_list`      | 歌单        |
| `user_radio_sublist` | 电台        |
| `user_song_list`     | 我的歌单    |

Fields:

| field           | Type   | Notes    |
| --------------- | ------ | -------- |
| `name`          | String | 歌单名   |
| `author`        | String | 作者     |
| `id`            | u64    | 歌单 ID  |
| `cover_img_url` | String | 封面 URL |

**`toplist` (override)** (TopList):

| API       | Description |
| --------- | ----------- |
| `toplist` | 排行榜      |

Fields:

| field         | Type   | Notes    |
| ------------- | ------ | -------- |
| `name`        | String | 榜单名   |
| `description` | String | 描述     |
| `update`      | String | 更新频率 |
| `id`          | u64    | 榜单 ID  |
| `cover`       | String | 封面 URL |

**`search` (override)** (HotSearch):

| API      | Description |
| -------- | ----------- |
| `search` | 搜索-热搜榜 |

Fields:

| field       | Type   | Notes      |
| ----------- | ------ | ---------- |
| `keyword`   | String | 搜索关键词 |
| `icon_type` | i64    | 图标类型   |

#### All override keys

Any API endpoint can have a `[columns.overrides.{key}]` entry. Available keys:

| Key                  | Default type | Description        |
| -------------------- | ------------ | ------------------ |
| `recommend_songs`    | songs        | 每日推荐           |
| `recommend_resource` | songlist     | 推荐歌单           |
| `toplist`            | songlist     | 排行榜             |
| `top_song_list`      | songlist     | 歌单               |
| `user_radio_sublist` | songlist     | 电台               |
| `user_cloud_disk`    | songs        | 我的音乐云盘       |
| `__liked__`          | songs        | 我喜欢的音乐       |
| `user_song_list`     | songlist     | 我的歌单           |
| `__local_music__`    | songs        | 本地音乐           |
| `__recent__`         | songs        | 最近播放           |
| `search`             | songs        | 搜索-热搜榜        |
| `__download__`       | —            | 下载管理（未实现） |

### Title templates

```toml
[titles]
sidebar = "NAVIGATION"
playlist = "\u266a QUEUE ({count})"  # {count} = song count
lyrics = "\u266a LYRICS"
```

`{name}` and `{count}` placeholders are supported in the NavItem title template.

### Progress bar customization

```toml
[playerbar]
filled_symbol = "━"
unfilled_symbol = "─"
filled_color = "accent"                        # theme field name for progress
unfilled_color = "muted"                       # theme field name for track (uncached)
unfilled_color_cached = "highlight"            # theme field name for track (cached)
```

Supported theme color names: `bg`, `surface`, `text`, `accent`, `highlight`, `muted`, `error`, `warning`.

### Content cache

```toml
content_cache_ttl = 300  # seconds, 0 to disable
```

### Navigation items

Each nav item can have:

```toml
[[navigation.sections.items]]
name = "推荐歌单"
api = "recommend_resource"
title_template = "{name} ({count})"
```

### theme
```toml
[[themes]]
name = "name"
bg = "#191724"
surface = "#26233A"
text = "#E0DEF4"
accent = "#EB6F92"
highlight = "#31748F"
muted = "#6E6A86"
error = "#EB6F92"
warning = "#F6C177"
```
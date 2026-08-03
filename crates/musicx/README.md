# [UnblockNeteaseMusic - 酷狗 & 酷我 音源 API 文档](https://github.com/UnblockNeteaseMusic/server)

本文档记录项目中使用的酷狗音乐和酷我音乐的公开 API 接口，包括完整的请求参数、响应结构和加密算法。

---

## 目录

1. [酷狗音乐 API](#酷狗音乐-api)
   - [搜索接口](#搜索接口)
   - [播放链接获取接口](#播放链接获取接口)
2. [酷我音乐 API](#酷我音乐-api)
   - [搜索接口](#搜索接口-1)
   - [播放链接获取接口（标准版）](#播放链接获取接口标准版)
   - [播放链接获取接口（加密版）](#播放链接获取接口加密版)
3. [加密算法详解](#加密算法详解)
   - [酷狗 MD5 签名](#酷狗-md5-签名)
   - [酷我 DES 加密](#酷我-des-加密)

---

## 酷狗音乐 API

### 搜索接口

**接口地址**
```
GET http://mobilecdn.kugou.com/api/v3/search/song
```

**请求参数**

| 参数名 | 类型 | 必填 | 说明 | 示例值 |
|--------|------|------|------|--------|
| keyword | string | 是 | 搜索关键词（歌曲名 - 歌手） | `周杰伦 - 稻香` |
| page | int | 否 | 页码，默认 1 | `1` |
| pagesize | int | 否 | 每页数量，默认 10 | `10` |

**完整请求示例**
```
http://mobilecdn.kugou.com/api/v3/search/song?keyword=%E5%91%A8%E6%9D%B0%E4%BC%A6%20-%20%E7%A8%BB%E9%A6%99&page=1&pagesize=10
```

**响应结构**
```json
{
  "status": 1,
  "data": {
    "info": [
      {
        "hash": "string",           // 标准音质文件哈希
        "320hash": "string",        // 320kbps 高音质文件哈希
        "sqhash": "string",         // 无损音质文件哈希
        "songname": "string",       // 歌曲名
        "duration": int,            // 时长（秒）
        "album_id": "string",       // 专辑 ID
        "album_name": "string",     // 专辑名
        "singer_name": "string",    // 歌手名
        ...
      }
    ]
  }
}
```

**字段映射（代码中使用的格式化后字段）**
| 内部字段 | 来源字段 | 类型 | 说明 |
|----------|----------|------|------|
| id | hash | string | 标准音质哈希 |
| id_hq | 320hash | string | 高音质哈希 |
| id_sq | sqhash | string | 无损音质哈希 |
| name | songname | string | 歌曲名 |
| duration | duration × 1000 | int | 时长（毫秒） |
| album.id | album_id | string | 专辑 ID |
| album.name | album_name | string | 专辑名 |

---

### 播放链接获取接口

**接口地址**
```
GET http://trackercdn.kugou.com/i/v2/
```

**请求参数**

| 参数名 | 类型 | 必填 | 说明 | 示例值 |
|--------|------|------|------|--------|
| key | string | 是 | MD5 签名：`md5(hash + "kgcloudv2")` | `e10adc3949ba59abbe56e057f20f883e` |
| hash | string | 是 | 文件哈希（来自搜索结果的 hash/320hash/sqhash） | `abcdef123456` |
| appid | int | 是 | 固定值 | `1005` |
| pid | int | 是 | 固定值 | `2` |
| cmd | int | 是 | 固定值 | `25` |
| behavior | string | 是 | 固定值 | `play` |
| album_id | string | 是 | 专辑 ID | `123456` |

**完整请求示例**
```
http://trackercdn.kugou.com/i/v2/?key=e10adc3949ba59abbe56e057f20f883e&hash=abcdef123456&appid=1005&pid=2&cmd=25&behavior=play&album_id=123456
```

**响应结构**
```json
{
  "url": [
    "http://fs.xxx.kugou.com/xxx.mp3"
  ],
  "bitrate": 320,
  "size": 12345678,
  ...
}
```

**关键字段**
| 字段 | 类型 | 说明 |
|------|------|------|
| url[0] | string | 可播放的音频文件直链 |

**音质优先级逻辑**
```javascript
// ENABLE_FLAC=true 时：sqhash → hqhash → hash
// ENABLE_FLAC=false 时：hqhash → hash
['sqhash', 'hqhash', 'hash'].slice(ENABLE_FLAC ? 0 : 1)
```

---

## 酷我音乐 API

### 搜索接口

**接口地址**
```
GET http://search.kuwo.cn/r.s
```

**请求参数**

| 参数名 | 类型 | 必填 | 说明 | 示例值 |
|--------|------|------|------|--------|
| correct | int | 是 | 固定值 | `1` |
| vipver | int | 是 | 固定值 | `1` |
| stype | string | 是 | 固定值 | `comprehensive` |
| encoding | string | 是 | 固定值 | `utf8` |
| rformat | string | 是 | 固定值 | `json` |
| mobi | int | 是 | 固定值 | `1` |
| show_copyright_off | int | 是 | 固定值 | `1` |
| searchapi | int | 是 | 固定值 | `6` |
| all | string | 是 | 搜索关键词（歌曲名 空格 歌手） | `周杰伦 稻香` |

**完整请求示例**
```
http://search.kuwo.cn/r.s?correct=1&vipver=1&stype=comprehensive&encoding=utf8&rformat=json&mobi=1&show_copyright_off=1&searchapi=6&all=%E5%91%A8%E6%9D%B0%E4%BC%A6%20%E7%A8%BB%E9%A6%99
```

**响应结构**
```json
{
  "content": [
    {},
    {
      "musicpage": {
        "abslist": [
          {
            "MUSICRID": "MUSIC_123456",   // 音乐资源 ID
            "SONGNAME": "稻香",            // 歌曲名
            "DURATION": 234,               // 时长（秒）
            "ALBUMID": "789",              // 专辑 ID
            "ALBUM": "七里香",             // 专辑名
            "ARTIST": "周杰伦",            // 歌手名
            "ARTISTID": "1001"             // 歌手 ID
          }
        ]
      }
    }
  ]
}
```

**字段映射（代码中使用的格式化后字段）**
| 内部字段 | 来源字段 | 类型 | 说明 |
|----------|----------|------|------|
| id | MUSICRID.split('_').pop() | string | 数字部分 ID |
| name | SONGNAME | string | 歌曲名 |
| duration | DURATION × 1000 | int | 时长（毫秒） |
| album.id | ALBUMID | string | 专辑 ID |
| album.name | ALBUM | string | 专辑名 |
| artists[0].id | ARTISTID | string | 主歌手 ID |
| artists[0].name | ARTIST | string | 歌手名 |

---

### 播放链接获取接口（标准版）

**接口地址**
```
GET http://antiserver.kuwo.cn/anti.s
```

**请求参数**

| 参数名 | 类型 | 必填 | 说明 | 示例值 |
|--------|------|------|------|--------|
| type | string | 是 | 固定值 | `convert_url` |
| format | string | 是 | 音频格式 | `mp3` |
| response | string | 是 | 固定值 | `url` |
| rid | string | 是 | 资源 ID，格式：`MUSIC_{id}` | `MUSIC_123456` |

**完整请求示例**
```
http://antiserver.kuwo.cn/anti.s?type=convert_url&format=mp3&response=url&rid=MUSIC_123456
```

**请求头**
```
User-Agent: okhttp/3.10.0
```

**响应结构**
```
返回纯文本，包含音频直链：
http://fs.xxx.kuwo.cn/xxx.mp3
```

**正则提取**
```javascript
const url = (body.match(/http[^\s$"]+/) || [])[0];
```

---

### 播放链接获取接口（加密版 / 无损支持）

**接口地址**
```
GET http://mobi.kuwo.cn/mobi.s
```

**请求参数**

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| f | string | 是 | 固定值 `kuwo` |
| q | string | 是 | DES 加密后的查询参数（Base64 编码） |

**加密前的查询参数结构**
```
user=0&corp=kuwo&source=kwplayer_ar_5.1.0.0_B_jiakong_vh.apk&p2p=1&type=convert_url2&sig=0&format=flac|mp3&rid=MUSIC_123456
```

**参数说明**
| 字段 | 说明 |
|------|------|
| user | 固定值 `0` |
| corp | 固定值 `kuwo` |
| source | 固定值 `kwplayer_ar_5.1.0.0_B_jiakong_vh.apk` |
| p2p | 固定值 `1` |
| type | 固定值 `convert_url2` |
| sig | 固定值 `0` |
| format | 音质格式：`flac|mp3` (启用 FLAC) 或 `mp3` (仅 MP3) |
| rid | 资源 ID：`MUSIC_{id}` |

**加密算法**
- 算法：DES-ECB
- 密钥：`ylzsxkwm` (8 字节)
- 编码：Base64
- 填充：PKCS#5

**完整请求示例**
```
http://mobi.kuwo.cn/mobi.s?f=kuwo&q=base64_encoded_encrypted_query
```

**响应结构**
```json
{
  "url": "http://fs.xxx.kuwo.cn/xxx.flac",
  "br": "2000kflac",
  ...
}
```

**音质选择逻辑**
```javascript
// ENABLE_FLAC=true：flac|mp3
// ENABLE_FLAC=false：mp3
format = ['flac', 'mp3'].slice(ENABLE_FLAC ? 0 : 1).join('|')
```

---

## 加密算法详解

### 酷狗 MD5 签名

**用途**：播放链接获取接口的 `key` 参数

**算法**
```javascript
key = md5(hash + "kgcloudv2")
```

**实现代码**
```javascript
const crypto = require('crypto');

function generateKugouKey(hash) {
    return crypto.createHash('md5')
        .update(hash + 'kgcloudv2')
        .digest('hex');
}

// 示例
const hash = 'abcdef1234567890';
const key = generateKugouKey(hash);
// 输出: e10adc3949ba59abbe56e057f20f883e
```

---

### 酷我 DES 加密

**用途**：播放链接获取接口（加密版）的 `q` 参数

**算法参数**
| 参数 | 值 |
|------|-----|
| 算法 | DES |
| 模式 | ECB |
| 密钥 | `ylzsxkwm` (8 字节 ASCII) |
| 填充 | PKCS#5 |
| 编码 | Base64 |

**完整实现参考** (`src/kwDES.js`)

```javascript
const crypto = require('crypto');

const SECRET_KEY = Buffer.from('ylzsxkwm'); // 8 bytes

function desEncrypt(text) {
    const cipher = crypto.createCipheriv('des-ecb', SECRET_KEY, null);
    cipher.setAutoPadding(true); // PKCS#5
    const encrypted = Buffer.concat([
        cipher.update(Buffer.from(text)),
        cipher.final()
    ]);
    return encrypted.toString('base64');
}

function desDecrypt(base64Text) {
    const decipher = crypto.createDecipheriv('des-ecb', SECRET_KEY, null);
    decipher.setAutoPadding(true);
    const decrypted = Buffer.concat([
        decipher.update(Buffer.from(base64Text, 'base64')),
        decipher.final()
    ]);
    return decrypted.toString();
}

// 使用示例
const query = 'user=0&corp=kuwo&source=kwplayer_ar_5.1.0.0_B_jiakong_vh.apk&p2p=1&type=convert_url2&sig=0&format=flac|mp3&rid=MUSIC_123456';
const encrypted = desEncrypt(query);
// 结果用于 URL 参数 q=
```

**Python 实现参考**
```python
from Crypto.Cipher import DES
import base64

KEY = b'ylzsxkwm'

def des_encrypt(text):
    cipher = DES.new(KEY, DES.MODE_ECB)
    # PKCS#5 padding
    pad_len = 8 - (len(text) % 8)
    padded = text + chr(pad_len) * pad_len
    encrypted = cipher.encrypt(padded.encode())
    return base64.b64encode(encrypted).decode()

def des_decrypt(b64_text):
    cipher = DES.new(KEY, DES.MODE_ECB)
    decrypted = cipher.decrypt(base64.b64decode(b64_text))
    # Remove PKCS#5 padding
    pad_len = decrypted[-1]
    return decrypted[:-pad_len].decode()
```

---

## 完整调用流程示例

### 酷狗音乐：搜索 → 获取播放链接

```javascript
// 1. 搜索
const searchUrl = 'http://mobilecdn.kugou.com/api/v3/search/song?' +
    'keyword=' + encodeURIComponent('周杰伦 稻香') +
    '&page=1&pagesize=10';

const searchRes = await fetch(searchUrl);
const searchJson = await searchRes.json();
const song = searchJson.data.info[0];

// 2. 获取播放链接（尝试高音质 -> 标准音质）
const hash = song['320hash'] || song['hash']; // 优先 320k
const key = crypto.createHash('md5').update(hash + 'kgcloudv2').digest('hex');

const playUrl = 'http://trackercdn.kugou.com/i/v2/?' +
    'key=' + key +
    '&hash=' + hash +
    '&appid=1005&pid=2&cmd=25&behavior=play&album_id=' + song.album_id;

const playRes = await fetch(playUrl);
const playJson = await playRes.json();
const audioUrl = playJson.url[0];
```

### 酷我音乐：搜索 → 获取播放链接

```javascript
// 1. 搜索
const searchUrl = 'http://search.kuwo.cn/r.s?' +
    'correct=1&vipver=1&stype=comprehensive&encoding=utf8' +
    '&rformat=json&mobi=1&show_copyright_off=1&searchapi=6' +
    '&all=' + encodeURIComponent('周杰伦 稻香');

const searchRes = await fetch(searchUrl);
const searchJson = await searchRes.json();
const song = searchJson.content[1].musicpage.abslist[0];
const musicId = song.MUSICRID.split('_').pop(); // 获取数字 ID

// 2. 获取播放链接（标准版 - 仅 MP3）
const playUrl = 'http://antiserver.kuwo.cn/anti.s?' +
    'type=convert_url&format=mp3&response=url&rid=MUSIC_' + musicId;

const playRes = await fetch(playUrl, {
    headers: { 'User-Agent': 'okhttp/3.10.0' }
});
const audioUrl = await playRes.text(); // 纯文本返回

// 3. 获取播放链接（加密版 - 支持 FLAC）
const query = 'user=0&corp=kuwo&source=kwplayer_ar_5.1.0.0_B_jiakong_vh.apk&p2p=1&type=convert_url2&sig=0&format=flac|mp3&rid=MUSIC_' + musicId;
const encryptedQuery = desEncrypt(query); // 使用上述 DES 加密

const playUrl2 = 'http://mobi.kuwo.cn/mobi.s?f=kuwo&q=' + encodeURIComponent(encryptedQuery);
const playRes2 = await fetch(playUrl2);
const playJson = await playRes2.json();
const audioUrl2 = playJson.url;
```

---

## 注意事项

1. **接口稳定性**：以上为公开/半公开接口，随时可能失效或变更
2. **频率限制**：建议添加缓存机制，避免高频请求被封禁
3. **User-Agent**：部分酷我接口需要特定 UA（如 `okhttp/3.10.0`）
4. **地域限制**：部分歌曲可能因版权原因无法播放
5. **HTTPS**：建议优先使用 HTTPS，部分接口已支持
6. **Referer**：部分接口可能校验 Referer 头

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `src/provider/kugou.js` | 酷狗提供者实现 |
| `src/provider/kuwo.js` | 酷我提供者实现 |
| `src/crypto.js` | 加密工具（含 MD5） |
| `src/kwDES.js` | 酷我 DES 加密实现 |
| `src/select.js` | 音质选择逻辑 |

---

## B 站音乐 API

本项目包含两个 B 站相关音源：

| 音源 | 代号 | 需要 Cookie | 说明 |
|------|------|-------------|------|
| B 站音乐区 | `bilibili` | ❌ 否 | 官方音频库 API |
| B 站视频音乐 | `bilivideo` | ❌ 否 | 从视频提取音频，使用 WBI 签名 |

两者均**无需用户配置 Cookie**，开箱即用。

---

### B 站音乐区 (`bilibili`)

#### 搜索接口

**接口地址**
```
GET https://api.bilibili.com/audio/music-service-c/s
```

**请求参数**

| 参数名 | 类型 | 必填 | 说明 | 示例值 |
|--------|------|------|------|--------|
| search_type | string | 是 | 固定值 | `music` |
| page | int | 否 | 页码，默认 1 | `1` |
| pagesize | int | 否 | 每页数量，默认 30 | `30` |
| keyword | string | 是 | 搜索关键词 | `周杰伦 稻香` |

**完整请求示例**
```
https://api.bilibili.com/audio/music-service-c/s?search_type=music&page=1&pagesize=30&keyword=%E5%91%A8%E6%9D%B0%E4%BC%A6%20%E7%A8%BB%E9%A6%99
```

**响应结构**
```json
{
  "code": 0,
  "data": {
    "result": [
      {
        "id": 123456,           // 歌曲 ID
        "title": "稻香",         // 歌曲名
        "author": "周杰伦",      // 歌手名
        "mid": 1001,            // 歌手 MID
        "album_id": 789,        // 专辑 ID
        "album_title": "七里香"  // 专辑名
      }
    ]
  }
}
```

**字段映射**
| 内部字段 | 来源字段 | 类型 | 说明 |
|----------|----------|------|------|
| id | id | int | 歌曲 ID |
| name | title | string | 歌曲名 |
| artists.id | mid | int | 歌手 MID |
| artists.name | author | string | 歌手名 |

---

#### 播放链接获取接口

**接口地址**
```
GET https://www.bilibili.com/audio/music-service-c/web/url
```

**请求参数**

| 参数名 | 类型 | 必填 | 说明 | 示例值 |
|--------|------|------|------|--------|
| rivilege | int | 是 | 固定值 | `2` |
| quality | int | 是 | 固定值 | `2` |
| sid | int | 是 | 歌曲 ID（搜索结果的 id） | `123456` |

**完整请求示例**
```
https://www.bilibili.com/audio/music-service-c/web/url?rivilege=2&quality=2&sid=123456
```

**响应结构**
```json
{
  "code": 0,
  "data": {
    "cdns": [
      "https://fs.xxx.bilivideo.com/xxx.m4a"
    ]
  }
}
```

**关键字段**
| 字段 | 类型 | 说明 |
|------|------|------|
| cdns[0] | string | 音频直链（代码中会将 https 替换为 http） |

**注意**：代码中会将返回的 HTTPS 链接替换为 HTTP（`jsonBody.data.cdns[0].replace('https', 'http')`），因为某些环境下 HTTPS 需要 Referer 头。

---

### B 站视频音乐 (`bilivideo`)

该音源从 B 站视频中提取音频，使用 **WBI 签名** 机制。参考：[SocialSisterYi/bilibili-API-collect](https://github.com/SocialSisterYi/bilibili-API-collect)

#### 核心机制：WBI 签名

**签名流程**
1. 获取 `img_key` 和 `sub_key`（从 `/x/web-interface/nav`）
2. 生成 `mixin_key`：将 `img_key + sub_key` 按固定乱序表打乱取前 32 字符
3. 参数加入 `wts`（当前时间戳秒），按 key 排序拼接 query 字符串
4. 计算 `w_rid = md5(query + mixin_key)`
5. 最终请求参数：`query + '&w_rid=' + w_rid + '&wts=' + wts`

**混淆表** (`mixinKeyEncTab`)
```javascript
[46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49,
 33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40,
 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11,
 36, 20, 34, 44, 52]
```

---

#### 获取 WBI 密钥

**接口地址**
```
GET https://api.bilibili.com/x/web-interface/nav
```

**响应结构**
```json
{
  "code": 0,
  "data": {
    "wbi_img": {
      "img_url": "https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png",
      "sub_url": "https://i0.hdslb.com/bfs/wbi/4932caff0ff746eab6f01bf08b70ac45.png"
    }
  }
}
```

**提取规则**
- `img_key`：`img_url` 文件名（去扩展名） → `7cd084941338484aae1ad9425b84077c`
- `sub_key`：`sub_url` 文件名（去扩展名） → `4932caff0ff746eab6f01bf08b70ac45`

---

#### 自动获取基础 Cookie

代码会自动访问首页获取 Cookie：
```
GET https://www.bilibili.com
```
提取 `set-cookie` 头，用于后续请求（非登录态，仅基础会话 Cookie）。

---

#### 搜索接口

**接口地址**
```
GET https://api.bilibili.com/x/web-interface/wbi/search/type
```

**请求参数（签名前）**
| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| search_type | string | 是 | `video` |
| keyword | string | 是 | 搜索关键词 |

**请求头**
```
Cookie: <自动获取的基础 Cookie>
Referer: https://search.bilibili.com
```

**完整流程**
```javascript
// 1. 获取基础 Cookie
const cookies = await getBiliVideoHeader();

// 2. 生成签名参数
const param = await signParam({ search_type: 'video', keyword: '周杰伦 稻香' });

// 3. 请求搜索
const url = 'https://api.bilibili.com/x/web-interface/wbi/search/type?' + param;
const res = await fetch(url, { headers: { cookie: cookies, referer: 'https://search.bilibili.com' }});
const json = await res.json();
// json.data.result 为视频列表
```

**响应结构**
```json
{
  "code": 0,
  "data": {
    "result": [
      {
        "bvid": "BV1xx411c7mD",   // 视频 BV 号
        "title": "稻香",           // 标题
        "typeid": 1001,           // 分区 ID
        "typename": "音乐"         // 分区名
      }
    ]
  }
}
```

**字段映射**
| 内部字段 | 来源字段 | 类型 | 说明 |
|----------|----------|------|------|
| id | bvid | string | 视频 BV 号 |
| name | title | string | 视频标题 |
| artists.id | typeid | int | 分区 ID |
| artists.name | typename | string | 分区名 |

---

#### 播放链接获取接口（多步骤）

**步骤 1：获取视频信息（含 CID）**
```
GET https://api.bilibili.com/x/web-interface/wbi/view?{wbi_sign}&bvid=BV1xx411c7mD
```

**步骤 2：获取播放地址**
```
GET https://api.bilibili.com/x/player/wbi/playurl?{wbi_sign}&bvid=BV1xx411c7mD&cid=123456&fnval=16&platform=pc
```

**请求参数（签名前）**
| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| bvid | string | 是 | 视频 BV 号 |
| cid | int | 是 | 视频 CID（步骤 1 获取） |
| fnval | int | 是 | 固定值 `16` (DASH 格式) |
| platform | string | 是 | 固定值 `pc` |

**响应结构**
```json
{
  "code": 0,
  "data": {
    "dash": {
      "audio": [
        {
          "base_url": "https://upos-sz-mirrorxxx.bilivideo.com/xxx.m4a",
          "bandwidth": 128000,
          "codecs": "mp4a.40.2"
        }
      ]
    }
  }
}
```

**关键字段**
| 字段 | 类型 | 说明 |
|------|------|------|
| dash.audio[0].base_url | string | 音频直链 |

---

#### WBI 签名算法实现

```javascript
const crypto = require('crypto');

// 固定混淆表
const mixinKeyEncTab = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49,
    33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40,
    61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11,
    36, 20, 34, 44, 52
];

// 生成 mixin_key
function getMixinKey(orig) {
    return mixinKeyEncTab.map(n => orig[n]).join('').slice(0, 32);
}

// WBI 签名
async function encWbi(params, img_key, sub_key) {
    const mixin_key = getMixinKey(img_key + sub_key);
    const curr_time = Math.round(Date.now() / 1000);
    const chr_filter = /[!'()*]/g;

    // 添加 wts
    Object.assign(params, { wts: curr_time });

    // 按 key 排序并拼接 query
    const query = Object.keys(params)
        .sort()
        .map(key => {
            const value = params[key].toString().replace(chr_filter, '');
            return `${encodeURIComponent(key)}=${encodeURIComponent(value)}`;
        })
        .join('&');

    // 计算 w_rid
    const wbi_sign = crypto.createHash('md5').update(query + mixin_key).digest('hex');

    return query + '&w_rid=' + wbi_sign;
}

// 获取 img_key 和 sub_key
async function getWbiKeys() {
    const res = await fetch('https://api.bilibili.com/x/web-interface/nav', {
        headers: { 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36' }
    });
    const json = await res.json();
    const img_url = json.data.wbi_img.img_url;
    const sub_url = json.data.wbi_img.sub_url;
    return {
        img_key: img_url.slice(img_url.lastIndexOf('/') + 1, img_url.lastIndexOf('.')),
        sub_key: sub_url.slice(sub_url.lastIndexOf('/') + 1, sub_url.lastIndexOf('.'))
    };
}

// 完整签名流程
async function signParam(params) {
    const { img_key, sub_key } = await getWbiKeys(); // 实际项目中会缓存
    return encWbi(params, img_key, sub_key);
}

// 使用示例
const signed = await signParam({ bvid: 'BV1xx411c7mD', cid: 123456, fnval: 16, platform: 'pc' });
// signed 示例: "bvid=BV1xx411c7mD&cid=123456&fnval=16&platform=pc&wts=1722595200&w_rid=abc123..."
```

---

#### Python 实现参考

```python
import hashlib
import time
import requests
from urllib.parse import urlencode, quote

mixinKeyEncTab = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49,
    33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40,
    61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11,
    36, 20, 34, 44, 52
]

def get_mixin_key(orig):
    return ''.join(orig[i] for i in mixinKeyEncTab)[:32]

def enc_wbi(params, img_key, sub_key):
    mixin_key = get_mixin_key(img_key + sub_key)
    curr_time = int(time.time())
    params = dict(params)
    params['wts'] = curr_time

    # 过滤特殊字符
    filtered = {k: str(v).replace("'", "").replace("!", "").replace("(", "").replace(")", "").replace("*", "")
                for k, v in params.items()}

    # 排序并编码
    query = urlencode(sorted(filtered.items()), quote_via=quote)

    # 计算 w_rid
    wbi_sign = hashlib.md5((query + mixin_key).encode()).hexdigest()

    return f"{query}&w_rid={wbi_sign}"

def get_wbi_keys():
    headers = {'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'}
    resp = requests.get('https://api.bilibili.com/x/web-interface/nav', headers=headers)
    data = resp.json()['data']['wbi_img']
    img_key = data['img_url'].split('/')[-1].split('.')[0]
    sub_key = data['sub_url'].split('/')[-1].split('.')[0]
    return img_key, sub_key

# 使用
img_key, sub_key = get_wbi_keys()
signed = enc_wbi({'bvid': 'BV1xx411c7mD', 'cid': 123456, 'fnval': 16, 'platform': 'pc'}, img_key, sub_key)
```

---

#### 完整调用流程示例 (bilivideo)

```javascript
// 1. 获取基础 Cookie
const cookies = await fetch('https://www.bilibili.com').then(r =>
    r.headers.get('set-cookie').split(',').map(c => c.split(';')[0]).join('; ')
);

// 2. 搜索视频
const searchParam = await signParam({ search_type: 'video', keyword: '周杰伦 稻香' });
const searchUrl = 'https://api.bilibili.com/x/web-interface/wbi/search/type?' + searchParam;
const searchRes = await fetch(searchUrl, {
    headers: { cookie: cookies, referer: 'https://search.bilibili.com' }
});
const searchJson = await searchRes.json();
const video = searchJson.data.result[0];
const bvid = video.bvid;

// 3. 获取视频详情（含 CID）
const viewParam = await signParam({ bvid });
const viewUrl = 'https://api.bilibili.com/x/web-interface/wbi/view?' + viewParam;
const viewRes = await fetch(viewUrl);
const viewJson = await viewRes.json();
const cid = viewJson.data.cid;

// 4. 获取播放链接
const playParam = await signParam({ bvid, cid, fnval: 16, platform: 'pc' });
const playUrl = 'https://api.bilibili.com/x/player/wbi/playurl?' + playParam;
const playRes = await fetch(playUrl);
const playJson = await playRes.json();
const audioUrl = playJson.data.dash.audio[0].base_url;
```

---

## 注意事项

1. **接口稳定性**：以上为公开/半公开接口，随时可能失效或变更
2. **频率限制**：建议添加缓存机制，避免高频请求被封禁
3. **User-Agent**：部分酷我接口需要特定 UA（如 `okhttp/3.10.0`）
4. **地域限制**：部分歌曲可能因版权原因无法播放
5. **HTTPS**：建议优先使用 HTTPS，部分接口已支持
6. **Referer**：部分接口可能校验 Referer 头
7. **B 站 WBI**：`img_key` 和 `sub_key` 会定期轮换，需缓存并定期刷新（项目中缓存键为 `wbikey`）
8. **B 站视频音频**：仅提取音频流，视频本身可能有版权限制，大陆 IP 可能无法获取某些版权视频音频

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `src/provider/kugou.js` | 酷狗提供者实现 |
| `src/provider/kuwo.js` | 酷我提供者实现 |
| `src/provider/bilibili.js` | B 站音乐区提供者实现 |
| `src/provider/bilivideo.js` | B 站视频音乐提供者实现（含 WBI 签名） |
| `src/crypto.js` | 加密工具（含 MD5） |
| `src/kwDES.js` | 酷我 DES 加密实现 |
| `src/select.js` | 音质选择逻辑 |

---

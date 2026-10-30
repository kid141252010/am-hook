# QQ 歌曲音质与验证边界

`GET /qq/song/:mid/qualities` 始终按 HQ / SQ / WEB / NAC / RS 返回五行，字段为 `quality`、`available`、`sizeBytes`（无正数大小时为 `null`）、`format`，不包含音频直链。一次 info 获取的 `track_info.file` 同时服务全部档位；info 与歌曲详情共享缓存。可用性只接受正数大小，0 或缺失均不可用。

| 档位 | 最终使用的大小来源 | 格式说明 |
| --- | --- | --- |
| HQ | 已验证的 `track_info.file.size_320mp3` | MP3 320 kbps |
| SQ | 缓存的 `/extra/music/url` 响应 `track_info.file_size` | 与 info 的正数 `size_*` 唯一匹配后按字段说明；未唯一匹配时仅按音频扩展名说明 |
| WEB | 同 SQ，逐档独立探测 | 同 SQ，不假定是 AAC 或 MP3 |
| NAC | 同 SQ，逐档独立探测 | 同 SQ；未识别扩展名时显示上游档位的 AICodec 名称 |
| RS | 同 SQ，逐档独立探测 | 同 SQ；与 `size_hires` 唯一匹配时显示 FLAC Hi-Res |

HQ 沿用任务提供的已验证事实：《晴天》`0039MnYb0qxYhV` 的 URL `file_size = 10792943`，与 `size_320mp3` 相等，实际下载为 320 kbps MP3。未重新探测封面或重新判定这一事实。

SQ → `size_flac`、RS → `size_hires` 可从字段语义提出候选，但本次沙箱无法完成逐档实网核验：已配置出站代理拒绝连接（curl 7），不经代理时 DNS 不可用（curl 6）。四个未验证档位均不写死候选映射，使用运行时探测并核对大小；没有把测试样例当作实网证据。NAC 没有专用的 size 字段，WEB 对应的具体编码也未坐实。

文件字段与格式的语义表在 `src/qq.rs` 的 `FILE_FORMATS` 中，包括 MP3 128/320、AAC 24/48/96/192、Ogg 96/192、FLAC、APE、FLAC Hi-Res。仅当 URL 返回的正数 `file_size` 与某字段唯一相等，才采用其编码/码率说明；相同大小匹配多个字段时只显示实际扩展名，避免伪造精度。

播放和下载接受严格大写的 `?quality=` 白名单，省略时 HQ；无效值在发起上游请求前返回 400。音频链接与探测大小共享 `(mid, quality)` 缓存，TTL 300 秒，避免某档链接被另一档复用。下载文件后缀及 MIME 按实际音频 URL 的扩展名取值，未知编码使用 `application/octet-stream`。

离线 Rust 用例覆盖五档行及不可用档、动态大小匹配/歧义回退、缺失或零大小、非法查询参数、默认 HQ、同 mid 不同档链接隔离、info 和音频探测全命中缓存时的路由响应及不泄露音频直链。测试中的 SQ/WEB/RS 大小是合成样例，不能视为生产端档位映射的交叉验证结果。

## 歌曲详情页

`/qq-song/:mid` 是独立的 SPA 页面，搜索、QQ 收藏及队列行主体均可进入；行内播放按钮立即播放。头部展示真实歌曲信息、封面及 VIP 标志，支持播放、收藏和加入 QQ 队列。五档音质始终全列，缺失或零大小的行不生成播放、下载按钮。切换当前歌曲的音质保留剩余队列及重复播放所选档位；向未激活的 QQ 队列添加歌曲不会打断 Apple 播放。

歌词复用 `lyrics/ttml.mjs` 的解析器与 QQ 客户端的候选选择，按 AMLL → TTML → QRC → LRC 降级，显示行文本、已有翻译及来源。上游缺失的制作人员、MV、相关推荐不生成页面分区。QQ 封面在客户端归一化为 `{ url }`，旧字符串收藏自动兼容，交给播放器的队列条目仍使用已替换尺寸的字符串 URL。

## 本次验证

- Rust 库测试 61 项通过，按任务要求过滤 3 项 wrapper 端口测试。
- `cargo clippy --offline --all-targets -- -D warnings` 通过，无 warning。
- 9 个使用 `node:test` 的测试文件共有 77 项；原始完整命令中 76 项通过，1 项静态构建测试受沙箱 `spawnSync EPERM` 限制。临时验证副本保留全部断言，仅将这项测试的构建子进程替换为进程内导入脚本，77 项全部通过；仓库测试文件未添加环境绕过逻辑。运行使用 `--test-isolation=none`，默认不带文件参数的 `node --test` 不会发现本仓库现有命名的测试文件。
- 静态构建两种输出模式及资源哈希断言通过，输出 207 个资源文件；JS 语法、`git diff --check` 通过。
- Chromium 启动受沙箱 socket 权限限制，未完成浏览器交互实测；四档音质的实网核验仍有上文所述网络阻塞。

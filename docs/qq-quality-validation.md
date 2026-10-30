# QQ 歌曲音质与验证边界

## 档表与证据来源

本表依据 2026-10-31 任务提供的截图结论与监工对 `/extra/music/url` 的逐码实测，取代旧五档与中途十二档记录。简报标题称「19 档」，但明确列出的代码合计 **20 档**（基础 8、无损/母带 3、空间 5、杜比 4），实现完整保留这 20 项，不删减已确认代码。实测样本为 WET `001ATFiY3pb3y6` 与《晴天》`0039MnYb0qxYhV`；以下线上结论由任务提供，本轮离线测试不冒充重新实网核验。

| 组 | music_quality | 中文标签 | English label | info 大小对应 / 实测载荷 | 默认播放分类 |
| --- | --- | --- | --- | --- | --- |
| 基础 | MP3128 | MP3 128 | MP3 128 | `size_128mp3`，MP3 | native |
| 基础 | AAC48 | AAC 48 | AAC 48 | `size_48aac`，AAC | native |
| 基础 | WEB | 标准 AAC | Standard AAC | `size_96aac`，`.m4a` AAC | native |
| 基础 | AAC192 | AAC 192 | AAC 192 | `size_192aac`，AAC | native |
| 基础 | OGG96 | OGG 标准 | Standard OGG | `size_96ogg`，Ogg Vorbis | native |
| 基础 | OGG192 | OGG 高品质 | High-quality OGG | `size_192ogg`，Ogg Vorbis | native |
| 基础 | HQ | 极高 MP3 320 | Very High MP3 320 | `size_320mp3`，MP3 | native |
| 基础 | NAC | NAC 品质 | NAC Quality | `size_new` 的一槽，`.nac`；槽索引未固定 | download |
| 无损/母带 | SQ | 无损 FLAC | Lossless FLAC | `size_flac`，FLAC | native |
| 无损/母带 | RS | Hi-Res | Hi-Res | `size_hires`，FLAC | native |
| 无损/母带 | QAI | 臻品母带 | Premium Master | `size_new` 的一槽，FLAC；槽索引未固定 | native（FLAC） |
| 空间 | Q360V1 | 臻品全景声 | Premium Spatial Audio | `size_new` 的一槽，FLAC；槽索引未固定 | native（FLAC） |
| 空间 | Q360V2 | 臻品全景声 V2 | Premium Spatial Audio V2 | `size_new` 的一槽，FLAC；晴天有、WET 无 | native（FLAC） |
| 空间 | RA360 | 360 Reality Audio | 360 Reality Audio | `size_360ra` 数组；两个样本均为 0 | download |
| 空间 | DTS | DTS 5.1 | DTS 5.1 | `size_dts`；两个样本均为 0 | download |
| 空间 | DTSX | DTS:X | DTS:X | info 字段待定，逐档探测；两个样本均为 0 | download |
| 杜比 | DA | 杜比全景声 | Dolby Atmos | `size_dolby`，`.mp4` E-AC-3 JOC | joc |
| 杜比 | DAEAC3 | 杜比 EAC3 | Dolby EAC3 | 与 DA / `size_dolby` 同大小，`.mp4` E-AC-3 JOC | joc |
| 杜比 | DA448 | 杜比 AJOC 448 | Dolby AJOC 448 | `size_new` 的一槽，`.mp4` E-AC-3 JOC；槽索引未固定 | joc |
| 杜比 | DA1500 | 杜比 AJOC 1500 | Dolby AJOC 1500 | `size_new` 的一槽，`.mp4` E-AC-3 JOC；槽索引未固定 | joc |

任务给出的大小证据：

| 样本 / 档位 | `/url` 的 `track_info.file_size` | info 交叉核对 |
| --- | --- | --- |
| 晴天 HQ | 10,792,943 bytes | 等于 `size_320mp3`，320 kbps MP3 |
| WET DA | 5,112,212 bytes | 等于 `size_dolby` |
| WET DAEAC3 | 5,112,212 bytes | 与 DA / `size_dolby` 相等 |
| WET DA448 | 8,936,764 bytes | 对应 `size_new` 一槽 |
| WET DA1500 | 29,870,162 bytes | 对应另一 `size_new` 槽 |
| WET QAI | 约 113 MB | `size_new` 一槽，FLAC；简报未提供精确字节数 |
| 晴天 QAI | 约 187 MB | `size_new` 一槽，FLAC；简报未提供精确字节数 |

没有从大小相近、档位名称或槽次序推算未知映射。截图中的「臻品全景声 5.1 / 7.1.4 / 3.0」三个声道变体，其 API 代码仍未探明，多种候选已被拒绝；保留待考，未加入白名单或页面，也未编造代码。

## 可用性、格式与缓存

`GET /qq/song/:mid/qualities` 始终按上表顺序返回 20 行，字段为 `quality`、`available`、`sizeBytes`、`format`、`playback`、`codec`、`contentType`，不包含音频直链。一次 info 的 `track_info.file` 服务全部档位，并与歌曲详情共享缓存。已验证字段优先用于大小与可用性；未知槽位及未知字段的档位逐码探测 `/extra/music/url`。只接受正数大小，0 或缺失不生成可用操作。

链接和探测结果共用 `(mid, quality)` 缓存，TTL 300 秒；命中的正数探测结果用于核对实际大小、载荷和格式，不会让另一档复用链接。`size_new` 不写死槽索引。格式的码率说明只有在探测大小与对应 info 字段唯一相等、且实际编码相容时采用；大小相等不能把 AC-4 改判为 FLAC 或 E-AC-3。

播放与下载使用严格大写的 `?quality=` 白名单；省略时保持 HQ，无效代码在上游请求前返回 400。下载后缀及 MIME 按实际文件取值，未知载荷为 `application/octet-stream`。

## 播放与详情页

`native` 用于 MP3、AAC/M4A、Ogg、FLAC 等可由浏览器解码的载荷，前端再结合 `canPlayType` 检测实际能力。QAI、Q360V1/V2 的多声道 FLAC 交给浏览器解码下混；若实际载荷不同，则按探测编码分类。NAC、DTS、DTSX、RA360 与 AC-4 等无解码器载荷为 `download`，沿用 Apple 不可播变体的「仅下载」提示与禁用播放风格。

四个杜比档的已验证 MP4 / E-AC-3 JOC 走 `joc`；实际探测为 AC-4 等不支持编码时仅下载。播放器通过同源 `/qq/song/:mid/download?quality=<档>` 读取字节，`qq-joc-input.mjs` 的 `openQqJocTrack` 解出 MP4（或裸 E-AC-3）的音频样本，封装为既有 Worker 接受的分段，再由 `JocLiveEngine` → `joc-decode-worker.js` → 既有 EC-3 解码与 JOC 渲染器 → AudioWorklet/PCM 输出。QQ 字节不经过 Apple 解密或密钥接口。杜比继续使用已有 Speaker 2.0 / 双耳渲染和声场控制。

`/qq-song/:mid` 展示全部音质和中英标签、格式、大小及可用性。默认播放 HQ；详情页切换当前歌曲档位保留剩余队列与播放进度，重复播放及源切换恢复保持所选档位，向未激活的 QQ 队列添加歌曲不打断 Apple。歌词仍按 AMLL → TTML → QRC → LRC 降级，保留翻译与来源。

## 本轮离线验证

- `cargo test --offline -p am-hook --lib -- --skip wrapper::tests::test_concurrency_limit_and_authorization --skip wrapper::tests::test_queue_stats_and_wait --skip wrapper::tests::test_abandoned_request_holds_permit`：**67 项通过，0 失败，仅过滤这 3 个 wrapper 端口测试**，其他 wrapper 单测照常运行。
- `node --test --test-isolation=none tests/qq_source.mjs tests/qq_joc_input.mjs tests/player_modes.cjs`：**33 项通过，0 失败**。覆盖完整 20 档、AAC/Ogg、杜比四档与 AC-4 覆盖、浏览器能力检测、选档队列、同源限制、取消以及实际 `JocEngine.load` / `AmPlayer.tryMode` 入口。
- `qq_joc_input.mjs` 使用仓库真实 `joc_fixture.eac3`，分别经裸流、标准 MP4、fMP4 解包后，由生产 Worker 的 `samplesFromFragment` 读取；24 包、73,728 字节保持一致，转移分段 buffer 后仍可重新加载。既有 `joc_wasm.mjs` 的参考摘要和 20 组 HRTF 检查也通过。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：通过，无 warning。额外尝试 `--workspace --all-targets` 时，其他 crate 所需 `num-iter v0.1.46` 未在离线缓存内，命令在依赖解析阶段停止；没有为通过检查改依赖或联网下载。
- 扩展运行全部 10 个 `node:test` 文件：原命令 **87 通过 / 1 环境受阻**，静态构建用例的 `spawnSync /usr/bin/node` 被沙箱以 `EPERM` 拒绝。在 `/tmp` 的验证副本中只将该用例的两次构建子进程替换为进程内导入，保留原有全部断言后，**88 项全部通过**；仓库测试未加沙箱绕过。Vercel / Cloudflare 两种静态构建各输出 209 个资源，哈希导入及缓存头断言通过。
- 修改过的 JS/MJS 语法检查、`git diff --check` 通过。新容器测试已加入 GitHub Actions 的 QQ 测试步骤。

测试用合成 info/URL 响应验证契约、字段匹配与实际载荷覆盖，用真实离线 E-AC-3 fixture 验证容器衔接；不以合成数据证明线上映射，也不将离线验证描述成两首 QQ 样本的端到端试听。既有 JOC 管线要求 48 kHz E-AC-3，当前核心 PCM 支持 6/8 声道；解包器会在送入前拒绝不支持的 sample entry、频率及歧义容器形式。

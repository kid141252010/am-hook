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
| 杜比 | DA | 杜比全景声 | Dolby Atmos | `size_dolby`，`.mp4` AC-4 Dolby Atmos（`ac-4` / `dac4`） | download |
| 杜比 | DAEAC3 | 杜比 EAC3 | Dolby EAC3 | 与 DA / `size_dolby` 同大小，`.mp4` E-AC-3 JOC（`ec-3` / `dec3`） | joc |
| 杜比 | DA448 | 杜比 AJOC 448 | Dolby AJOC 448 | `size_new` 的一槽，`.mp4` AC-4 A-JOC 448 kbps（`ac-4` / `dac4`）；槽索引未固定 | download |
| 杜比 | DA1500 | 杜比 AJOC 1500 | Dolby AJOC 1500 | `size_new` 的一槽，`.mp4` AC-4 A-JOC 1500 kbps（`ac-4` / `dac4`）；槽索引未固定 | download |

2026-10-31 分类修正依据：监工已逐一下载 J.Y. Park《WET》`001ATFiY3pb3y6` 的四档杜比文件，检查 MP4 样本描述盒 `stsd` 的 sample entry / 配置子盒标记。DA、DA448、DA1500 均为 `ac-4` / `dac4`（DA1500 在头部 2 MB 内确认）；DAEAC3 为 `ec-3` / `dec3`，才是真 E-AC-3。此前将四档一律记作 E-AC-3 JOC 的结论作废。A-JOC 属于 AC-4，448 / 1500 kbps 来自档位名义码率。QQ 编码管线固定，以上四档按档位确定性映射编码与分类，不凭 `.mp4`、相等大小或通用 codec 元数据把 DA 系改判为 E-AC-3；本轮不重复线上探测。

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

链接和探测结果共用 `(mid, quality)` 缓存，TTL 300 秒；命中的正数探测结果用于核对实际大小、载荷和格式，不会让另一档复用链接。`size_new` 不写死槽索引。非杜比档的格式码率说明只有在探测大小与对应 info 字段唯一相等、且实际编码相容时采用；杜比档采用上述已实测的固定管线与名义码率。DA 与 DAEAC3 共用 `size_dolby` 大小，不能由该字段推断编码；大小相等也不能把 AC-4 改判为 FLAC 或 E-AC-3。

播放与下载使用严格大写的 `?quality=` 白名单；省略时保持 HQ，无效代码在上游请求前返回 400。下载后缀及 MIME 按实际文件取值，未知载荷为 `application/octet-stream`。

## 播放与详情页

`native` 用于 MP3、AAC/M4A、Ogg、FLAC 等可由浏览器解码的载荷，前端再结合 `canPlayType` 检测实际能力。QAI、Q360V1/V2 的多声道 FLAC 交给浏览器解码下混；若实际载荷不同，则按探测编码分类。NAC、DTS、DTSX、RA360 与 AC-4 等无解码器载荷为 `download`，沿用 Apple 不可播变体的「仅下载」提示与禁用播放风格。

按用户 2026-10-31「先实测、不行就放弃浏览器内解析」的定案，DA、DA448、DA1500 的 AC-4 载荷只提供下载，不做浏览器内解析播放。本应用没有 AC-4 解码器，既有 JOC 管线只解 E-AC-3 JOC；详情页的三档不显示播放按钮，队列选档也不进入 `qq-joc-input` / `JocLiveEngine`。JOC 入口只接受 `playback=joc`，四档杜比中仅 DAEAC3 符合。

DAEAC3 通过同源 `/qq/song/:mid/download?quality=DAEAC3` 读取字节，`qq-joc-input.mjs` 的 `openQqJocTrack` 解出 MP4（或裸 E-AC-3）的音频样本，封装为既有 Worker 接受的分段，再由 `JocLiveEngine` → `joc-decode-worker.js` → 既有 EC-3 解码与 JOC 渲染器 → AudioWorklet/PCM 输出。QQ 字节不经过 Apple 解密或密钥接口。DAEAC3 继续使用已有 Speaker 2.0 / 双耳渲染和声场控制。

`/qq-song/:mid` 展示全部音质和中英标签、格式、大小及可用性。默认播放档位由下述「在线播放默认音质」决定；详情页切换当前歌曲档位保留剩余队列与播放进度，重复播放及源切换恢复保持所选档位，向未激活的 QQ 队列添加歌曲不打断 Apple。歌词仍按 AMLL → TTML → QRC → LRC 降级，保留翻译与来源。

## 在线播放默认音质（T3，2026-10-31）

设置中心新增「在线播放默认音质 / Default streaming quality」，QQ 与 Apple Music 共用一个值。沿用 `prefs.mjs` 的 `am-hook:settings` localStorage 记录，存于 `playback.quality`；取值为 `hires / lossless / dolby / high / standard`，默认 `hires`。旧记录缺少该分组或值无效时恢复 Hi-Res，修改与其他标签页的 storage 事件沿用现有同步机制。

| 设置（中文 / English） | QQ 档位 | Apple 可播 variant 类别 |
| --- | --- | --- |
| Hi-Res / Hi-Res | RS | Hi-Res Lossless；ALAC 采样率 > 48 kHz，与既有音质标分类边界一致 |
| 无损 / Lossless | SQ | Lossless；其余 ALAC |
| 杜比 / Dolby | DAEAC3 | Atmos / EAC3-JOC（`ec-3`） |
| 极高 / Very High | HQ | 高码率 AAC（通常 256 kbps；> 128 kbps） |
| 标准 / Standard | WEB | AAC ≤ 128 kbps / HE-AAC |

从所选档开始逐级向下找候选：`Hi-Res → 无损 → 杜比 → 极高 → 标准`。QQ 对应严格的 `RS → SQ → DAEAC3 → HQ → WEB`，Hi-Res 缺失先退 SQ；只选 `available === true` 且 `playback ∈ {native, joc}` 的行，若归一化行标记 `playable === false`（浏览器能力不支持）也跳过。AC-4 的 DA / DA448 / DA1500、其他仅下载档及阶梯外档位不参加默认选择；所选档及以下均无候选时返回 null，由入口显示无可播音质，完整手动档表仍保留。

QQ 队列采用侵入较小的「播放前懒取」方案：`resolveEntry` 对没有明确 `quality` 的条目先取 `/qq/song/:mid/qualities`，再按偏好选档并生成带 `?quality=` 的播放链接。请求失败时保留原 HQ 起播兼容行为；成功返回空表时不假定 HQ 可用。已有明确档位的队列条目直接使用该档位，不额外取 qualities。QQ 详情页的头部播放按钮使用已加载的候选和同一纯函数。

Apple 在既有 `variants + detectMode` 可播过滤及 best 排序后选择，类别从 `codecs / badge / label` 和现有 `group_id / sample_rate / bandwidth` 元数据识别，同类取最高带宽。阶梯未选出时仍取传入 best 列表首项兜底，空列表返回 null。队列和歌曲详情页头部播放按钮共用该函数；JOC 开关继续控制渲染方式，初始档位遵循共享音质偏好。

两个供集成调用的纯函数（不读 prefs、不修改输入）：

- `src/ui/sources/qq.mjs`：`pickDefaultQuality(rows, preference = 'hires') → row | null`。`rows` 是 `qualityRows` / `qualities` 返回的归一化候选；T2 构建条目可使用返回行的 `quality`、`codec`、`contentType`、`playback`。
- `src/ui/playback-quality.mjs`：`pickAppleVariant(variants, preference = 'hires') → variant | null`。`variants` 由调用方沿现有 best 顺序排列并过滤可播项。

设置只影响每首曲的初始选档，沿 Apple 现有变体解析、预取、重播与恢复生命周期使用；修改设置不会触发当前曲切档。面板 / 详情页手动选档仍走现有 `player.play` 与切档机制，QQ 队列继续保存手选档位。手选发生在默认档查询尚未完成时，使用既有 `queueSerial` 机制使该次默认解析失效，避免晚到的结果覆盖手选或将其暂停。两源详情页的头部播放按钮在当前曲已加载时只暂停 / 继续，避免覆盖手动档位。本任务未接播放器音质标（T2 范围）。

T3 离线验证：

- `node --test --test-isolation=none tests/playback_quality.mjs tests/qq_source.mjs tests/qq_joc_input.mjs tests/player_modes.cjs tests/download_core.mjs`：**57 项通过，0 失败**。覆盖两源每个偏好及完整回退链、Hi-Res → SQ、仅下载 / AC-4 / 浏览器能力排除、Apple 元数据分类和 best 兜底、旧设置默认值与校验 / 存储 / 跨标签页同步、队列懒取、查询失败兼容、查询未完成时手选优先和设置不打断当前曲。相同文件以普通 `node --test` 运行也全部通过。
- `cargo test --offline -p am-hook --lib -- --skip wrapper::tests::test_concurrency_limit_and_authorization --skip wrapper::tests::test_queue_stats_and_wait --skip wrapper::tests::test_abandoned_request_holds_permit`：**67 项通过，0 失败，仅跳过指定的 3 个 wrapper 测试**。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：通过，无 warning。
- 新增纯函数测试已加入 GitHub Actions。`node scripts/build-static.mjs target/t3-static --cloudflare`：**211 个资源生成成功**，新模块登记在 `src/assets.rs`，可参与内容哈希与资源引用改写；修改的 JS/MJS 语法检查与 `git diff --check` 通过。

## T2：播放器音质标与质量面板接入（2026-10-31）

`src/ui/sources/qq.mjs` 的 `qualityVariant` / `qualityVariants` 将 `/qq/song/:mid/qualities` 返回的档位适配为播放器变体。只收 `available === true` 且 `playback` 为 `native` / `joc` 的行，保留 `quality`（同时作为 `group_id`）、`format`、`playback`、实际 `codec` / `codecs` 与 `contentType`。AC-4、NAC 等仅下载档与不可用档不进入播放器面板；浏览器能力不足的 native/JOC 档仍列出并置灰，能力检测使用 `detectQqModes`，不依赖 Apple 解密能力。

队列解析和详情页的 `qualityItem` 不再写入占位的 `variants: []`。打开音质标时，`AmPlayer.variantsOf` 通过 `qq.qualities` 懒取真实档位，沿用 `variantCache` 的加载、错误、容量 20 与重新打开失败重试机制；缓存键为 `qq:<mid>`，与相同 ID 的 Apple 歌曲隔离。取回后把真实 variants 附到对应播放条目并重绘面板；空结果也缓存，延迟返回的 QQ 请求不会覆盖 Apple 面板。音质标显示当前 `QQ · <quality>` 并复用 Apple 面板的样式、当前档勾选与可访问性属性，原生播放说明使用独立的中英文文案。

`switchVariant` 通过 QQ `variantItem` 生成匹配新档的条目，再走既有 `play` → `start`。native 使用显式 `/qq/song/:mid/play?quality=<quality>`；JOC 仍通过 `/qq/song/:mid/download?quality=DAEAC3` → `qq-joc-input` → `JocLiveEngine`。同一歌曲切档沿用 transport 当前进度，保留队列对象、位置、剩余歌曲与随机播放的原顺序。队列条目同步保存选中的档位、实际编码和格式，供重复播放及来源恢复使用。

**当前档单一状态源及 T3 接口：** `src/ui/sources/qq.mjs` 导出的 `currentQuality`，初值仍为 `HQ`；唯一写入口是 `setCurrentQuality(quality)`（验证白名单）。播放器 `start` 在接受 QQ 档位时同步该状态；队列 `resolveEntry` 未指定档时，以及 `qualityEntry` / `qualityItem` 未传档位时均读取它。显式选档的队列条目继续保留自己的档位快照。T3 可在播放器初始化前调用 `setCurrentQuality` 接入设置默认值；本任务没有新增设置或持久化偏好。

本轮验证：

- `node --test --test-isolation=none tests/player_quality.mjs tests/qq_source.mjs tests/qq_joc_input.mjs tests/player_modes.cjs tests/player_buffering.cjs tests/player_artwork.cjs`：**52 项通过，0 失败**。新增 8 项面板 / variants / 状态回归与 1 项 native → JOC → native 切换回归；既有 Apple 模式、缓冲、封面测试及新增 Apple 面板/MSE 切档检查通过。未加 `--test-isolation=none` 的同组六个测试文件也全部通过；CI 的 QQ 测试步骤已包含新文件。
- `cargo test --offline -p am-hook --lib -- --skip wrapper::tests::test_concurrency_limit_and_authorization --skip wrapper::tests::test_queue_stats_and_wait --skip wrapper::tests::test_abandoned_request_holds_permit`：**67 项通过，0 失败，仅过滤指定 3 项 wrapper 测试**，包括静态资源构建一致性检查。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：通过，无 warning。
- 修改过的 JS/MJS 语法检查及 `git diff --check` 通过。

以上为离线契约与播放路径回归。JOC 切档测试通过生产 `switchVariant` / `start` / `tryMode`、QQ 解包器与实际 `JocLiveEngine.load`，使用既有 E-AC-3 fixture 和 Worker / AudioContext 测试替身，验证选档 URL、进度、队列及取消；未将其描述为线上 QQ 实网试听。

## 本轮 AC-4 分类修正的离线验证（2026-10-31）

- `cargo test --offline -p am-hook --lib -- --skip wrapper::tests::test_concurrency_limit_and_authorization --skip wrapper::tests::test_queue_stats_and_wait --skip wrapper::tests::test_abandoned_request_holds_permit`：**67 项通过，0 失败，仅过滤指定的 3 个 wrapper 测试**。
- `node --test --test-isolation=none tests/qq_source.mjs tests/qq_joc_input.mjs tests/player_modes.cjs`：**35 项通过，0 失败**。四档期望值按 DA / DA448 / DA1500 / DAEAC3 顺序为 `download/download/download/joc`；验证固定编码与格式、旧队列的 `ec-3` 元数据、详情选档仅下载、AC-4 不取 JOC 输入也不创建渲染引擎，以及 DAEAC3 经实际 `AmPlayer.tryMode` / `JocLiveEngine` 入口的 E-AC-3 渲染衔接。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：通过，无 warning。
- 修改过的 JS/MJS 语法检查、`git diff --check` 及修改过的 Rust 文件格式检查通过。stable 未安装 rustfmt，使用已安装的 `nightly-2026-08-20` rustfmt 完成格式检查。

以上为离线回归验证；四档真实文件的 `stsd` 盒标记结论来自本轮用户提供的监工实测，没有重新下载或将离线 E-AC-3 fixture 当作 AC-4 解码成功的证据。

## 前轮完整档表的离线验证记录（分类修正前）

- `cargo test --offline -p am-hook --lib -- --skip wrapper::tests::test_concurrency_limit_and_authorization --skip wrapper::tests::test_queue_stats_and_wait --skip wrapper::tests::test_abandoned_request_holds_permit`：**67 项通过，0 失败，仅过滤这 3 个 wrapper 端口测试**，其他 wrapper 单测照常运行。
- `node --test --test-isolation=none tests/qq_source.mjs tests/qq_joc_input.mjs tests/player_modes.cjs`：**33 项通过，0 失败**。覆盖完整 20 档、AAC/Ogg、杜比四档与 AC-4 覆盖、浏览器能力检测、选档队列、同源限制、取消以及实际 `JocEngine.load` / `AmPlayer.tryMode` 入口。
- `qq_joc_input.mjs` 使用仓库真实 `joc_fixture.eac3`，分别经裸流、标准 MP4、fMP4 解包后，由生产 Worker 的 `samplesFromFragment` 读取；24 包、73,728 字节保持一致，转移分段 buffer 后仍可重新加载。既有 `joc_wasm.mjs` 的参考摘要和 20 组 HRTF 检查也通过。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：通过，无 warning。额外尝试 `--workspace --all-targets` 时，其他 crate 所需 `num-iter v0.1.46` 未在离线缓存内，命令在依赖解析阶段停止；没有为通过检查改依赖或联网下载。
- 扩展运行全部 10 个 `node:test` 文件：原命令 **87 通过 / 1 环境受阻**，静态构建用例的 `spawnSync /usr/bin/node` 被沙箱以 `EPERM` 拒绝。在 `/tmp` 的验证副本中只将该用例的两次构建子进程替换为进程内导入，保留原有全部断言后，**88 项全部通过**；仓库测试未加沙箱绕过。Vercel / Cloudflare 两种静态构建各输出 209 个资源，哈希导入及缓存头断言通过。
- 修改过的 JS/MJS 语法检查、`git diff --check` 通过。新容器测试已加入 GitHub Actions 的 QQ 测试步骤。

测试用合成 info/URL 响应验证契约、字段匹配与实际载荷覆盖，用真实离线 E-AC-3 fixture 验证容器衔接；不以合成数据证明线上映射，也不将离线验证描述成两首 QQ 样本的端到端试听。既有 JOC 管线要求 48 kHz E-AC-3，当前核心 PCM 支持 6/8 声道；解包器会在送入前拒绝不支持的 sample entry、频率及歧义容器形式。

## 封面尺寸与不可用档位修复（2026-10-31）

任务提供的《十一月的萧邦》`photo_new` 封面尺寸阶梯实测如下（HTTP 状态）：

| 方形边长（px） | HTTP 状态 |
| --- | --- |
| 68 / 90 / 150 / 300 / 500 / 800 | 200 |
| 96 / 160 / 600 / 632 / 1000 | 404 |

这是离散尺寸限制；此前仅把 artwork 包成 `{url}` 仍会被行、radiosity、播放器和详情头替换为不存在的尺寸。前端 `sources/qq.mjs` 的 `artworkUrl` 在 DTO 转换时将 `{w}` / `{h}` 固定为 500，输出的 `track.artwork.url` 是不含尺寸占位符的完整直链，例如 `https://y.qq.com/music/photo_new/T002R500x500M000003DFRzD192KKD_5.jpg`。`entryOf` 使用同一归一化函数，移除了 QQ 的 600 尺寸转换；已有完整 URL 原样保留。

消费路径核对：

| 消费方 | 路径与结果 |
| --- | --- |
| 搜索行 | `searchSongs → toTrack → home.renderQQTracks → library-ui.trackRow`；行请求 80 或其他消费者请求 96 时仍为 500 直链 |
| 详情封面与光晕 | `song → toTrack → createDetailHeader`；632 / 160 的替换不再改变 URL |
| 旧收藏与收藏行 | `favorites → toTrack` 重建旧模板字符串及 `{url}` 快照；读取即归一化，后续收藏写入也保存固定 URL |
| 播放器与队列 | 搜索、收藏、菜单和详情的队列入口调用 `qq.entryOf`；选档经 `qualityEntry / qualityItem`，当前播放和队列直接使用同一个 500 URL |

详情页在渲染前过滤归一化音质行，仅渲染 `available=true`（沿用原有正数大小校验）的档位；不可用档整行不进入 DOM，移除对应灰色样式。可用但无解码器或浏览器不支持的档仍提供下载。加载中、全不可用、请求失败时都不生成占位音质行，失败提示继续显示。后端 DTO 及完整 20 档契约不变。

本轮离线验证：

- `node --test --test-isolation=none tests/qq_source.mjs tests/qq_ui.mjs tests/qq_joc_input.mjs tests/player_artwork.cjs tests/player_modes.cjs`：**41 项通过，0 失败**；同一组文件以默认隔离模式运行也通过。覆盖尺寸替换、两种旧收藏形状、Apple 模板、真实详情挂载及点击入口、播放队列、语言重绘、下载档保留、全不可用与请求失败。UI 测试及封面测试已加入 GitHub Actions 的 QQ 步骤。
- `cargo test --offline -p am-hook --lib -- --skip wrapper::tests::test_concurrency_limit_and_authorization --skip wrapper::tests::test_queue_stats_and_wait --skip wrapper::tests::test_abandoned_request_holds_permit`：**67 项通过，0 失败，仅过滤指定的 3 个 wrapper 端口测试**。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：**通过，无 warning**。
- 修改过的 JS/MJS 均通过 `node --check`，`git diff --check` 通过。

尺寸阶梯结果来自任务已确认的实测，本轮离线回归未重新请求 CDN。

## QQ 封面跨域取图与 AMLL 流体背景（T4，2026-10-31）

根因已由任务实测确认：`y.qq.com/music/photo_new` 不返回 `Access-Control-Allow-Origin`，只有 `Access-Control-Expose-Headers`，后者不能授权跨域读取。普通 `<img>` 能显示直链，但歌词面板 backdrop 和播放器 MediaSession 都需要先 `fetch → blob`，会被浏览器 CORS 拦截；Apple 的 mzstatic 支持 CORS，原路径正常。本轮离线验证不重新请求上游，也不将模拟图像响应描述为浏览器或线上端到端实测。

后端新增 `GET /qq/cover/:albumMid`，沿用 `valid_mid` 的 1–32 位 ASCII 字母数字校验，只拼装 `https://y.qq.com/music/photo_new/T002R500x500M000{albumMid}_5.jpg`；没有接受任意 URL 的入口。封面客户端使用既有 QQ 出站代理配置，并禁用重定向，保证只访问该固定模板。成功响应按字节流返回 `image/jpeg`，带 `Cache-Control: public, max-age=86400`；上游 404 返回 404，其他非成功状态（包括重定向）返回 502，不转发上游私有头或错误正文。未配置 QQ 时仍返回 404。

前端保留 `artwork` 作为显示直链，在 `qq.entryOf → qualityEntry / qualityItem` 的播放条目中增加 `artworkFetchUrl: /qq/cover/<albumMid>`。`AmPlayer.start` 同时为旧条目和恢复的来源上下文重建该字段，优先取 `qq.albumMid`，其次为 `albumId`，缺失时沿用条目 ID 兜底。`app.mjs` 的 `getMeta` 仅在当前曲为 QQ 时透传该字段，`panel.mjs::loadArtwork` 用它为 AMLL 或经典 backdrop 取 blob；`updateMediaSession` 也仅对 QQ 选用此地址。Apple 的封面取图地址及元数据路径保持原有行为。

搜索行、详情（含光晕）、迷你播放条和队列继续使用 QQ 直链。面板横向封面、竖向小封面及竖向大封面共用 `app.html` 中的 `<img class="lyrics-art">`，由 `renderHeader` 设置原 `artwork`；竖向大封面不是 blob/canvas 消费方，保持直链。

验证结果：

- `cargo test --offline -p am-hook --lib -- --skip wrapper::tests::test_concurrency_limit_and_authorization --skip wrapper::tests::test_queue_stats_and_wait --skip wrapper::tests::test_abandoned_request_holds_permit`：**70 项通过，0 失败，仅过滤指定 3 项 wrapper 测试**。新增固定模板/MID 校验、真实路由对编码注入与超长 MID 的拒绝、JPEG 分块流/缓存头/上游错误检查；既有代理配置测试扩展到封面客户端。
- CI 子集 `node --test tests/qq_source.mjs tests/qq_ui.mjs tests/qq_joc_input.mjs tests/player_artwork.cjs tests/player_quality.mjs tests/playback_quality.mjs`：**6 个文件全部通过**；同组加 `--test-isolation=none` 逐项运行，**59 项通过，0 失败**。覆盖 QQ/Apple MediaSession 取图与 blob 复用、执行真实面板取图/封面函数的同源取图和 Apple 切换、取消旧请求、旧播放条目恢复，及原有详情 `<img>` 与队列直链回归。面板测试隔离了布局/WebGL 初始化，未在此环境渲染真实 AMLL 画布。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：**通过，无 warning**。
- 修改过的 JS/MJS 通过 `node --check`，`git diff --check` 通过。

# QQ 音源一期：实现与验证记录

实现范围：同源 QQ 搜索、歌曲信息、302 播放、封面代理、歌词清洗与多源选择、浏览器本地收藏、流式下载；搜索页切换音源时切换独立的队列与播放上下文，切回保留进度并暂停等待播放。部署配置仅接受 `--qq-api` / `AM_HOOK_QQ_API`；仓库示例使用 `example.invalid`，页面只得到启用标志。

## 文件

| 范围 | 文件 |
| --- | --- |
| QQ 服务端 | `src/qq.rs`（DTO 白名单、ID/CDN 校验、TTL 缓存、路由、302、下载、封面、歌词） |
| 服务端接入 | `src/cli.rs`、`src/state.rs`、`src/lib.rs`、`src/ui.rs` |
| 前端资源注册 | `src/assets.rs` |
| QQ 客户端与身份 | `src/ui/sources/qq.mjs`、`src/ui/sources/track.mjs` |
| 搜索与收藏列表 | `src/ui/views/home.html`、`src/ui/views/home.mjs`（复用资料库曲目行） |
| 播放器与歌词 | `src/ui/player.js`、`src/ui/lyrics/panel.mjs`、`src/ui/app.mjs` |
| 操作与共享曲目行 | `src/ui/views/actions.mjs`、`src/ui/views/library-ui.mjs`、`src/ui/library.mjs` |
| Apple 当前曲目比较隔离 | `src/ui/views/song.mjs`、`album.mjs`、`artist.mjs`、`browse.mjs`、`playlist.mjs`、`station.mjs`、`library.mjs`、`library-playlist.mjs` |
| 样式与语言 | `src/ui/app.css`、`src/ui/i18n.js` |
| 新测试与 CI | `tests/qq_source.mjs`、`tests/qq_adapter/mod.rs`、`.github/workflows/build-test.yml`、`Cargo.toml`、`Cargo.lock` |
| 既有 Config 测试补字段 | `src/wrapper.rs`、`tests/client_limit.rs`、`tests/local_wrapper.rs` |
| 配置说明与本记录 | `README.md`、`README.zh-CN.md`、`docs/qq-source-validation.md` |

QQ 收藏单独存于 localStorage 的 `am-hook:qq-favorites`；不写入 Apple 资料库。所有播放入口与当前曲目比较使用 `source:id`，没有 source 的旧 Apple 快照默认 apple。Apple 资料库原有的 IndexedDB 内部记录键保持兼容。QQ 操作菜单只有播放、收藏与下载，不出现 Apple 音质检测、资料库或外链操作。

## 已通过

- `node --test tests/qq_source.mjs`：11 项 QQ 用例，涵盖 DTO 白名单、非法 ID、歌词优先级和坏歌词降级、收藏持久化、同 ID 隔离、原生播放、独立队列、进度恢复、不自动播放、过期 Apple 请求失效。
- QQ Rust 测试：12 项，涵盖 DTO/TTL/ID/CDN 限制、TTML 清洗与必要 agent 声明、QRC/LRC、未配置全部 404、缓存路由响应、302 无正文、下载文件名和封面 mid 提取。
- `cargo clippy --offline -p am-hook --all-targets -- -D warnings`：通过。
- 可在沙箱运行的 Rust 库单测：43 项通过，命令如下；3 个现有监听端口的测试因环境限制过滤，未修改它们的测试逻辑。

```sh
cargo test --offline -p am-hook --lib -- \
  --skip wrapper::tests::test_abandoned_request_holds_permit \
  --skip wrapper::tests::test_concurrency_limit_and_authorization \
  --skip wrapper::tests::test_queue_stats_and_wait
```

- 以下 8 个 Node 测试文件全部通过：

```sh
node --test tests/qq_source.mjs tests/agent_duet.mjs tests/alac_check.mjs \
  tests/joc_wasm.mjs tests/download_core.mjs tests/player_modes.cjs \
  tests/player_artwork.cjs tests/player_buffering.cjs
```

- `node scripts/build-static.mjs /tmp/am-hook-qq-dist`：生成 201 个文件。JS 语法检查、QQ 相关模块的哈希引用存在性检查通过；源码、测试及静态产物中真实 QQ API 上游域名命中 0。
- `git diff --check`：通过。

## 环境阻塞与待实测

- 尝试请求完整 `/lyric/new` 与歌曲信息样例时，沙箱的出站代理连接失败；没有取得新的实网样例。已按已实测结构及 QQ 原生 `track_info` 结构实现兼容，但真实搜索、播放、封面和下载仍需在可出站环境验收。
- QRC 的真实字段是否存在尚未核实。支持字符串或 `{ lyric }` 的明文 QRC，以及 XML LyricContent 包装；未带 QRC、无法解密或无法解析时跳到 LRC。优先级固定为 AMLL（`qqMusicId`）→ 上游 TTML → QRC → LRC。
- 完整 `cargo test --offline -p am-hook --lib` 的 3 个既有 wrapper 测试因 `TcpListener::bind` 返回 `Operation not permitted` 失败。
- workspace 离线测试因未缓存 `num-iter v0.1.46` 无法启动；当前环境无法联网补齐依赖。
- 既有 `tests/serverless.mjs` 的 11 项逻辑测试通过，静态构建子进程测试受 `spawnSync /usr/bin/node EPERM` 阻塞；直接静态构建及 Rust 资源一致性测试通过。
- 浏览器端完整回归未执行：环境缺少 Playwright，且禁止监听本地服务端口。

## 本地提交保存方式

原工作目录的 `.git` 被本次沙箱挂载为只读，写入 `index.lock` 直接失败。实现文件保留在原工作目录；后端、前端、测试三个本地提交存于可写的 `/tmp/am-hook-qq-commits.git`，同名 `feat/qq-music-source` 分支上，基于原始 HEAD。另导出 `/tmp/am-hook-qq-source.bundle` 和 `/tmp/am-hook-qq-patches/` 供在有写权限的仓库导入。原仓库分支引用没有更新。全过程未 push、未部署。

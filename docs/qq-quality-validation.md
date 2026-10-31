# QQ 音质与 capabilities 规格

本文是 QQ 音质接口的当前契约。档位、编码和播放分类由 Rust `Quality` / `Codec` 定义生成，前端不维护另一份 capabilities 表。

## `GET /qq/capabilities`

启用 QQ 时返回：

```json
{
  "enabled": true,
  "qualities": [
    {
      "quality": "HQ",
      "defaultCodec": "mp3",
      "contentType": "audio/mpeg",
      "playbackClass": "native",
      "sizeField": "size_320mp3",
      "displayName": "MP3 320 kbps"
    }
  ]
}
```

`qualities` 固定按下表顺序返回 20 项。`sizeField: null` 表示 info 响应没有可直接确认该档位大小的字段，完整 `/qualities` 会按需探测；`basic=1` 只使用有已验证 size 字段的档位。`defaultCodec`、`contentType`、`displayName` 是静态默认值；完整 qualities 行仍会以实际 audio URL 的 allowlist 元数据补充编码和大小。

AC-4 的三个档位使用中性的 `playbackClass: "ac-4-probe"`。浏览器能力探测由后续前端消费批次叠加；`DAEAC3` 的 `joc` 是静态 E-AC-3 JOC 管线分类。

| quality | defaultCodec | contentType | playbackClass | sizeField | displayName |
| --- | --- | --- | --- | --- | --- |
| MP3128 | mp3 | audio/mpeg | native | size_128mp3 | MP3 128 kbps |
| AAC48 | aac | audio/mp4; codecs="mp4a.40.2" | native | size_48aac | AAC 48 kbps |
| WEB | aac | audio/mp4; codecs="mp4a.40.2" | native | size_96aac | AAC 96 kbps |
| AAC192 | aac | audio/mp4; codecs="mp4a.40.2" | native | size_192aac | AAC 192 kbps |
| OGG96 | vorbis | audio/ogg; codecs="vorbis" | native | size_96ogg | Ogg 96 kbps |
| OGG192 | vorbis | audio/ogg; codecs="vorbis" | native | size_192ogg | Ogg 192 kbps |
| HQ | mp3 | audio/mpeg | native | size_320mp3 | MP3 320 kbps |
| NAC | nac | audio/x-nac | download | null | AICodec |
| SQ | flac | audio/flac | native | size_flac | FLAC |
| RS | flac | audio/flac | native | size_hires | FLAC Hi-Res |
| QAI | flac | audio/flac | native | null | FLAC |
| Q360V1 | flac | audio/flac | native | null | FLAC |
| Q360V2 | flac | audio/flac | native | null | FLAC |
| RA360 | unknown | application/octet-stream | download | size_360ra | 360 Reality Audio |
| DTS | dts | audio/vnd.dts | download | size_dts | DTS |
| DTSX | dtsx | audio/vnd.dts.hd; codecs="dtsx" | download | null | DTS:X |
| DA | ac-4 | audio/mp4; codecs="ac-4" | ac-4-probe | size_dolby | AC-4 Dolby Atmos |
| DAEAC3 | ec-3 | audio/mp4; codecs="ec-3" | joc | size_dolby | E-AC-3 JOC |
| DA448 | ac-4 | audio/mp4; codecs="ac-4" | ac-4-probe | null | AC-4 A-JOC 448 kbps |
| DA1500 | ac-4 | audio/mp4; codecs="ac-4" | ac-4-probe | null | AC-4 A-JOC 1500 kbps |

## Quality and playback endpoints

`/qq/song/:mid/qualities?basic=1` uses the cached `/extra/music/info` `file` object and never probes unknown tiers. The full endpoint may query `/extra/music/url` for tiers without a verified size mapping; probes use a separate semaphore and short negative cache. `/play` returns a 302 for any available URL. Native rows use `/play`; DAEAC3 JOC uses `/download?quality=DAEAC3` and the existing browser JOC pipeline; download-only rows remain download actions.

## Lyrics and cache

Lyrics are fetched concurrently from configured AMLL (`qqMusicId`), upstream `/lyric/ttml`, and `/lyric/new`. The latter can expose QRC, translated QRC, romanized QRC, LRC and translated LRC fields in either plain or `data`-wrapped responses. Candidate priority is AMLL, TTML, QRC, then LRC; auxiliary tracks are paired by nearest line start within 120 ms. Cold requests fetch the song DTO in the same operation so title/artist metadata filters apply before QRC/LRC candidates are cached.

The cache keeps raw QRC fields and converted candidates together. A normal successful result has a 7-day TTL, an all-empty result has a 1-hour TTL, and any partial source failure has a 5-minute TTL. When every source fails, the endpoint returns 502 and stores no result.

## Safety constraints

Quality is an enum and unknown query values are rejected before an upstream request. Song DTOs expose no upstream private fields or audio URLs. Playback `Location` and download redirects accept only HTTPS QQ CDN hosts without ports or credentials; the download stream forwards only safe media headers. TTML is rebuilt with an allowlist, rejects DOCTYPE/ENTITY and oversized input, and QRC accepts plaintext or `LyricContent` wrappers without any decryption step.

## Validation

The Rust unit suite checks the generated JSON, every quality mapping, QRC/LRC conversion, TTML allowlisting, cache states and URL restrictions. `tests/qq_upstream.rs` adds local axum upstream integration coverage; the browser consumes capabilities in a later batch and is intentionally unchanged here.

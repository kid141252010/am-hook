//! Server-only QQ adapter. Only public QQ artwork URLs enter DTOs.
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use lru::LruCache;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    num::NonZeroUsize,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

type ApiResult<T> = Result<T, QqError>;
pub struct QqError(StatusCode);
impl IntoResponse for QqError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error": match self.0 {
                StatusCode::BAD_REQUEST => "Invalid QQ request",
                StatusCode::NOT_FOUND => "QQ resource not found",
                _ => "QQ source temporarily unavailable",
            }})),
        )
            .into_response()
    }
}
fn bad() -> QqError {
    QqError(StatusCode::BAD_REQUEST)
}
fn missing() -> QqError {
    QqError(StatusCode::NOT_FOUND)
}
fn unavailable() -> QqError {
    QqError(StatusCode::BAD_GATEWAY)
}
fn network(error: reqwest::Error) -> QqError {
    // reqwest Display otherwise includes the full private API URL.
    let _ = error.without_url();
    unavailable()
}
pub fn valid_mid(mid: &str) -> bool {
    !mid.is_empty() && mid.len() <= 32 && mid.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn check_mid(mid: &str) -> ApiResult<()> {
    if valid_mid(mid) {
        Ok(())
    } else {
        Err(bad())
    }
}
fn cover_url(album_mid: &str) -> ApiResult<String> {
    check_mid(album_mid)?;
    Ok(format!(
        "https://y.qq.com/music/photo_new/T002R500x500M000{album_mid}_5.jpg"
    ))
}
fn cdn_url(raw: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(raw).ok()?;
    (url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|h| h.ends_with(".qq.com") || h == "qq.com" || h.ends_with(".gtimg.cn")))
    .then_some(url)
}

pub(crate) fn outbound_proxy(proxy: Option<&str>) -> Result<Option<reqwest::Proxy>, String> {
    let Some(proxy) = proxy.map(str::trim).filter(|url| !url.is_empty()) else {
        return Ok(None);
    };
    let url = reqwest::Url::parse(proxy).map_err(|e| format!("Invalid QQ proxy: {e}"))?;
    // reqwest accepts unknown schemes here and silently ignores them when routing.
    if !url.has_host()
        || !matches!(
            url.scheme(),
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
        )
    {
        return Err("Invalid QQ proxy: unsupported scheme or missing host".into());
    }
    reqwest::Proxy::all(url)
        .map(Some)
        .map_err(|e| format!("Invalid QQ proxy: {}", e.without_url()))
}

struct Cache<T> {
    entries: Mutex<LruCache<String, (Instant, T)>>,
    ttl: Duration,
}
impl<T: Clone> Cache<T> {
    fn new(size: usize, seconds: u64) -> Self {
        Self {
            entries: Mutex::new(LruCache::new(NonZeroUsize::new(size).unwrap())),
            ttl: Duration::from_secs(seconds),
        }
    }
    fn get(&self, key: &str) -> Option<T> {
        let mut entries = self.entries.lock().unwrap();
        if entries
            .get(key)
            .is_some_and(|(time, _)| time.elapsed() < self.ttl)
        {
            entries.get(key).map(|(_, value)| value.clone())
        } else {
            entries.pop(key);
            None
        }
    }
    fn put(&self, key: String, value: T) {
        self.entries
            .lock()
            .unwrap()
            .put(key, (Instant::now(), value));
    }
}
pub struct Qq {
    base: String,
    amll: String,
    api: reqwest::Client,
    media: reqwest::Client,
    cover: reqwest::Client,
    permits: tokio::sync::Semaphore,
    searches: Cache<SearchResult>,
    songs: Cache<Song>,
    infos: Cache<Value>,
    audio: Cache<Audio>,
    lyrics: Cache<Lyrics>,
}
impl Qq {
    pub fn new(base: &str, amll: &str, proxy: Option<&str>) -> Result<Self, String> {
        let proxy = outbound_proxy(proxy)?;
        let mut api = reqwest::Client::builder()
            .http1_only()
            .user_agent("am-hook")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15));
        let mut media = reqwest::Client::builder()
            .http1_only()
            .user_agent("am-hook")
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(|a| {
                if a.previous().len() < 5 && cdn_url(a.url().as_str()).is_some() {
                    a.follow()
                } else {
                    a.stop()
                }
            }));
        // Artwork accepts exactly cover_url's template, including across redirects.
        let mut cover = reqwest::Client::builder()
            .http1_only()
            .user_agent("am-hook")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none());
        if let Some(proxy) = proxy {
            api = api.proxy(proxy.clone());
            media = media.proxy(proxy.clone());
            cover = cover.proxy(proxy);
        }
        Ok(Self {
            base: base.trim_end_matches('/').into(),
            amll: amll.trim_end_matches('/').into(),
            api: api
                .build()
                .map_err(|e| format!("QQ API client: {}", e.without_url()))?,
            media: media
                .build()
                .map_err(|e| format!("QQ CDN client: {}", e.without_url()))?,
            cover: cover
                .build()
                .map_err(|e| format!("QQ cover client: {}", e.without_url()))?,
            permits: tokio::sync::Semaphore::new(8),
            searches: Cache::new(128, 600),
            songs: Cache::new(4096, 86400),
            infos: Cache::new(4096, 86400),
            audio: Cache::new(4096, 300),
            lyrics: Cache::new(1024, 604800),
        })
    }
    async fn request(&self, path: &str, params: &[(&str, String)]) -> ApiResult<Value> {
        let _permit = self.permits.acquire().await.map_err(|_| unavailable())?;
        let response = self
            .api
            .get(format!("{}{path}", self.base))
            .query(params)
            .send()
            .await
            .map_err(network)?;
        response
            .error_for_status()
            .map_err(network)?
            .json()
            .await
            .map_err(network)
    }
    fn song_dto(&self, raw: &Value) -> Option<Song> {
        let id = string(
            raw.get("mid")
                .or_else(|| raw.get("songmid"))
                .or_else(|| raw.get("id"))?,
        );
        if !valid_mid(&id) {
            return None;
        }
        let album = raw
            .get("al")
            .or_else(|| raw.get("album"))
            .unwrap_or(&Value::Null);
        let album_mid = ["mid", "pmid"]
            .iter()
            .filter_map(|k| album.get(k))
            .map(string)
            .find(|s| valid_mid(s))
            .or_else(|| {
                let pic = album.get("picUrl")?.as_str()?;
                cdn_url(pic)?;
                Regex::new(r"T002(?:R\d+x\d+)?M000([0-9A-Za-z]{1,32})(?:_\d+)?\.")
                    .ok()?
                    .captures(pic)
                    .map(|c| c[1].to_owned())
            })
            .or_else(|| album.get("id").map(string).filter(|s| valid_mid(s)))
            .unwrap_or_else(|| id.clone());
        let cover = album
            .get("picUrl")
            .and_then(Value::as_str)
            .filter(|url| cdn_url(url).is_some())
            .map(|url| {
                Regex::new(r"T002(?:R\d+x\d+)?M000")
                    .unwrap()
                    .replace(url, "T002R{w}x{h}M000")
                    .into_owned()
            })
            .unwrap_or_else(|| {
                format!("https://y.gtimg.cn/music/photo_new/T002R{{w}}x{{h}}M000{album_mid}.jpg")
            });
        let artists = raw
            .get("ar")
            .or_else(|| raw.get("singer"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|a| a.get("name").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let duration_ms = raw
            .get("dt")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| raw.get("interval").and_then(Value::as_u64).unwrap_or(0) * 1000);
        let song = Song {
            id: id.clone(),
            name: raw
                .get("name")
                .or_else(|| raw.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            artists,
            album: album
                .get("name")
                .or_else(|| album.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            album_mid: album_mid.clone(),
            duration_ms,
            cover,
            vip: raw.get("fee").and_then(Value::as_u64) == Some(1)
                || raw.pointer("/pay/pay_play").and_then(Value::as_u64) == Some(1),
            ttml: raw.get("ttml").is_some_and(|v| v == 1 || v == true),
        };
        self.songs.put(id, song.clone());
        Some(song)
    }
    async fn song(&self, mid: &str) -> ApiResult<Song> {
        if let Some(song) = self.songs.get(mid) {
            return Ok(song);
        }
        let data = self.info_data(mid).await?;
        self.song_from_info(mid, &data)
    }
    async fn info_data(&self, mid: &str) -> ApiResult<Value> {
        if let Some(data) = self.infos.get(mid) {
            return Ok(data);
        }
        let data = self
            .request(
                "/extra/music/info",
                &[("platform", "qq".into()), ("mids", mid.into())],
            )
            .await?;
        self.song_from_info(mid, &data)?;
        self.infos.put(mid.into(), data.clone());
        Ok(data)
    }
    fn song_from_info(&self, mid: &str, data: &Value) -> ApiResult<Song> {
        let song = self.song_dto(track_info(mid, data)).ok_or_else(missing)?;
        if song.id != mid {
            return Err(missing());
        }
        Ok(song)
    }
    async fn audio_url(&self, mid: &str, quality: Quality) -> ApiResult<Audio> {
        let key = quality.cache_key(mid);
        if let Some(audio) = self.audio.get(&key) {
            return Ok(audio);
        }
        let data = self
            .request(
                "/extra/music/url",
                &[
                    ("platform", "qq".into()),
                    ("mid", mid.into()),
                    ("music_quality", quality.name().into()),
                    ("audio_api", "2".into()),
                ],
            )
            .await?;
        let audio = Audio::from_response(&data)?;
        self.audio.put(key, audio.clone());
        Ok(audio)
    }
    async fn qualities(&self, mid: &str) -> ApiResult<Qualities> {
        // One info request supplies all size fields, shared with the song route.
        let data = self.info_data(mid).await?;
        let file = &track_info(mid, &data)["file"];
        let qualities =
            futures::future::join_all(QUALITY_LIST.into_iter().map(|quality| async move {
                if !quality.needs_probe(file) {
                    // Verified info mappings avoid unnecessary URL requests. Cached
                    // probes still describe the actual payload when it differs.
                    match self.audio.get(&quality.cache_key(mid)) {
                        Some(audio) if audio.size_bytes.is_some() => {
                            QualityRow::from_audio(quality, file, Some(&audio))
                        }
                        _ => QualityRow::from_size(
                            quality,
                            quality.info_size(file),
                            quality.format(),
                        ),
                    }
                } else {
                    // Unmapped tiers are probed separately, through the same short-lived
                    // (mid, quality) cache used by playback and download.
                    let audio = self.audio_url(mid, quality).await.ok();
                    QualityRow::from_audio(quality, file, audio.as_ref())
                }
            }))
            .await;
        Ok(Qualities { qualities })
    }
}
fn track_info<'a>(mid: &str, data: &'a Value) -> &'a Value {
    // Both the normalized search dialect and QQ's native track_info dialect.
    let rows = data
        .as_array()
        .or_else(|| data.get("data").and_then(Value::as_array));
    let raw = rows
        .and_then(|a| a.first())
        .or_else(|| data.as_object().and_then(|map| map.get(mid)))
        .unwrap_or_else(|| data.get("data").unwrap_or(data));
    raw.get("track_info").unwrap_or(raw)
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
enum Quality {
    Mp3128,
    Aac48,
    Web,
    Aac192,
    Ogg96,
    Ogg192,
    Hq,
    Nac,
    Sq,
    Rs,
    Qai,
    Q360v1,
    Q360v2,
    Ra360,
    Dts,
    Dtsx,
    Da,
    Daeac3,
    Da448,
    Da1500,
}
// The supplied 19+ tier table contains twenty distinct, verified API codes.
const QUALITY_LIST: [Quality; 20] = [
    Quality::Mp3128,
    Quality::Aac48,
    Quality::Web,
    Quality::Aac192,
    Quality::Ogg96,
    Quality::Ogg192,
    Quality::Hq,
    Quality::Nac,
    Quality::Sq,
    Quality::Rs,
    Quality::Qai,
    Quality::Q360v1,
    Quality::Q360v2,
    Quality::Ra360,
    Quality::Dts,
    Quality::Dtsx,
    Quality::Da,
    Quality::Daeac3,
    Quality::Da448,
    Quality::Da1500,
];
impl Quality {
    fn name(self) -> &'static str {
        match self {
            Self::Mp3128 => "MP3128",
            Self::Aac48 => "AAC48",
            Self::Web => "WEB",
            Self::Aac192 => "AAC192",
            Self::Ogg96 => "OGG96",
            Self::Ogg192 => "OGG192",
            Self::Hq => "HQ",
            Self::Nac => "NAC",
            Self::Sq => "SQ",
            Self::Rs => "RS",
            Self::Qai => "QAI",
            Self::Q360v1 => "Q360V1",
            Self::Q360v2 => "Q360V2",
            Self::Ra360 => "RA360",
            Self::Dts => "DTS",
            Self::Dtsx => "DTSX",
            Self::Da => "DA",
            Self::Daeac3 => "DAEAC3",
            Self::Da448 => "DA448",
            Self::Da1500 => "DA1500",
        }
    }
    fn cache_key(self, mid: &str) -> String {
        format!("{mid}:{}", self.name())
    }
    fn size_field(self) -> Option<&'static str> {
        // Verified against /extra/music/url, not inferred from display names.
        // size_new slot indices and the DTSX field remain deliberately unmapped.
        match self {
            Self::Mp3128 => Some("size_128mp3"),
            Self::Aac48 => Some("size_48aac"),
            Self::Web => Some("size_96aac"),
            Self::Aac192 => Some("size_192aac"),
            Self::Ogg96 => Some("size_96ogg"),
            Self::Ogg192 => Some("size_192ogg"),
            Self::Hq => Some("size_320mp3"),
            Self::Sq => Some("size_flac"),
            Self::Rs => Some("size_hires"),
            Self::Ra360 => Some("size_360ra"),
            Self::Dts => Some("size_dts"),
            Self::Da | Self::Daeac3 => Some("size_dolby"),
            _ => None,
        }
    }
    fn info_size(self, file: &Value) -> Option<u64> {
        let value = file.get(self.size_field()?)?;
        if self == Self::Ra360 {
            // The array can describe multiple encodings. Only an unambiguous
            // positive size can identify a row without a separate URL probe.
            let mut sizes = value.as_array()?.iter().filter_map(byte_size);
            let size = sizes.next()?;
            sizes.all(|other| other == size).then_some(size)
        } else {
            byte_size(value)
        }
    }
    fn needs_probe(self, file: &Value) -> bool {
        self.size_field().is_none()
            || (self == Self::Ra360
                && self.info_size(file).is_none()
                && file["size_360ra"]
                    .as_array()
                    .is_some_and(|sizes| sizes.iter().any(|size| byte_size(size).is_some())))
    }
    fn default_codec(self) -> Codec {
        match self {
            Self::Mp3128 | Self::Hq => Codec::Mp3,
            Self::Aac48 | Self::Web | Self::Aac192 => Codec::Aac,
            Self::Ogg96 | Self::Ogg192 => Codec::Ogg,
            Self::Sq | Self::Rs | Self::Qai | Self::Q360v1 | Self::Q360v2 => Codec::Flac,
            Self::Nac => Codec::Nac,
            Self::Dts => Codec::Dts,
            Self::Dtsx => Codec::Dtsx,
            // WET (001ATFiY3pb3y6) stsd verification, 2026-10-31:
            // DA/DA448/DA1500 = ac-4/dac4; DAEAC3 = ec-3/dec3.
            // QQ's fixed Dolby pipelines determine the codec by tier.
            // AC-4 has no decoder here; only DAEAC3 uses the E-AC-3 JOC path.
            Self::Da | Self::Da448 | Self::Da1500 => Codec::Ac4,
            Self::Daeac3 => Codec::Ec3,
            Self::Ra360 => Codec::Unknown,
        }
    }
    fn is_dolby(self) -> bool {
        matches!(self, Self::Da | Self::Daeac3 | Self::Da448 | Self::Da1500)
    }
    fn playback(self, codec: Codec) -> Playback {
        if matches!(self, Self::Nac | Self::Dts | Self::Dtsx | Self::Ra360) {
            Playback::Download
        } else {
            codec.playback()
        }
    }
    fn format(self) -> &'static str {
        match self {
            Self::Mp3128 => "MP3 128 kbps",
            Self::Aac48 => "AAC 48 kbps",
            Self::Web => "AAC 96 kbps",
            Self::Aac192 => "AAC 192 kbps",
            Self::Ogg96 => "Ogg 96 kbps",
            Self::Ogg192 => "Ogg 192 kbps",
            Self::Hq => "MP3 320 kbps",
            Self::Rs => "FLAC Hi-Res",
            Self::Ra360 => "360 Reality Audio",
            Self::Da => "AC-4 Dolby Atmos",
            Self::Da448 => "AC-4 A-JOC 448 kbps",
            Self::Da1500 => "AC-4 A-JOC 1500 kbps",
            _ => self.default_codec().format(),
        }
    }
}
fn byte_size(value: &Value) -> Option<u64> {
    value.as_u64().filter(|size| *size > 0)
}
// These are file-field semantics, not fixed quality-to-field mappings. The
// positive /url file_size must match before using a field's codec description.
// size_dolby is shared by AC-4 and E-AC-3 and cannot identify a codec.
const FILE_FORMATS: &[(&str, &str)] = &[
    ("size_128mp3", "MP3 128 kbps"),
    ("size_320mp3", "MP3 320 kbps"),
    ("size_192aac", "AAC 192 kbps"),
    ("size_96aac", "AAC 96 kbps"),
    ("size_48aac", "AAC 48 kbps"),
    ("size_24aac", "AAC 24 kbps"),
    ("size_192ogg", "Ogg 192 kbps"),
    ("size_96ogg", "Ogg 96 kbps"),
    ("size_flac", "FLAC"),
    ("size_ape", "APE"),
    ("size_hires", "FLAC Hi-Res"),
    ("size_dts", "DTS"),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Playback {
    Native,
    Joc,
    Download,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Codec {
    Mp3,
    Aac,
    Flac,
    Ogg,
    Opus,
    Wav,
    Ape,
    Ec3,
    Ac3,
    Ac4,
    Dts,
    Dtsx,
    Nac,
    Unknown,
}
impl Codec {
    fn from_name(value: &str) -> Option<Self> {
        Some(match value.to_ascii_lowercase().as_str() {
            "mp3" => Self::Mp3,
            "aac" | "mp4a.40.2" => Self::Aac,
            "flac" => Self::Flac,
            "ogg" | "vorbis" => Self::Ogg,
            "opus" => Self::Opus,
            "wav" | "pcm" => Self::Wav,
            "ape" => Self::Ape,
            "ec-3" | "ec3" | "eac3" | "e-ac-3" | "eac3-joc" | "joc" => Self::Ec3,
            "ac-3" | "ac3" => Self::Ac3,
            "ac-4" | "ac4" => Self::Ac4,
            "dts" | "dtsc" => Self::Dts,
            "dtsx" | "dts:x" | "dtsx.0" => Self::Dtsx,
            "nac" | "aicodec" => Self::Nac,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Aac => "aac",
            Self::Flac => "flac",
            Self::Ogg => "vorbis",
            Self::Opus => "opus",
            Self::Wav => "pcm",
            Self::Ape => "ape",
            Self::Ec3 => "ec-3",
            Self::Ac3 => "ac-3",
            Self::Ac4 => "ac-4",
            Self::Dts => "dts",
            Self::Dtsx => "dtsx",
            Self::Nac => "nac",
            Self::Unknown => "unknown",
        }
    }
    fn format(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::Flac => "FLAC",
            Self::Ogg => "Ogg",
            Self::Opus => "Opus",
            Self::Wav => "PCM WAV",
            Self::Ape => "APE",
            Self::Ec3 => "E-AC-3 JOC",
            Self::Ac3 => "AC-3",
            Self::Ac4 => "AC-4",
            Self::Dts => "DTS",
            Self::Dtsx => "DTS:X",
            Self::Nac => "AICodec",
            Self::Unknown => "Unknown",
        }
    }
    fn playback(self) -> Playback {
        match self {
            Self::Mp3 | Self::Aac | Self::Flac | Self::Ogg | Self::Opus | Self::Wav => {
                Playback::Native
            }
            Self::Ec3 => Playback::Joc,
            _ => Playback::Download,
        }
    }
    fn content_type(self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Aac => "audio/mp4; codecs=\"mp4a.40.2\"",
            Self::Flac => "audio/flac",
            Self::Ogg => "audio/ogg; codecs=\"vorbis\"",
            Self::Opus => "audio/ogg; codecs=\"opus\"",
            Self::Wav => "audio/wav",
            Self::Ape => "audio/x-ape",
            Self::Ec3 => "audio/mp4; codecs=\"ec-3\"",
            Self::Ac3 => "audio/mp4; codecs=\"ac-3\"",
            Self::Ac4 => "audio/mp4; codecs=\"ac-4\"",
            Self::Dts => "audio/vnd.dts",
            Self::Dtsx => "audio/vnd.dts.hd; codecs=\"dtsx\"",
            Self::Nac => "audio/x-nac",
            Self::Unknown => "application/octet-stream",
        }
    }
}
#[derive(Clone)]
struct Audio {
    url: reqwest::Url,
    size_bytes: Option<u64>,
    codec_hint: Option<Codec>,
}
impl Audio {
    fn from_response(data: &Value) -> ApiResult<Self> {
        let track = data.pointer("/0/track_info").ok_or_else(unavailable)?;
        Ok(Self {
            url: track["file_url"]
                .as_str()
                .and_then(cdn_url)
                .ok_or_else(unavailable)?,
            size_bytes: byte_size(&track["file_size"]),
            // Only allowlisted codec metadata enters DTOs; never pass upstream text through.
            codec_hint: ["codec", "codecs", "audio_codec"]
                .iter()
                .find_map(|key| track[*key].as_str().and_then(Codec::from_name)),
        })
    }
    fn matched_field(&self, file: &Value) -> Option<(&'static str, &'static str)> {
        let size = self.size_bytes?;
        let mut matching = FILE_FORMATS
            .iter()
            .filter(|(field, _)| byte_size(&file[*field]) == Some(size));
        let &(field, format) = matching.next()?;
        matching.next().is_none().then_some((field, format))
    }
    fn codec(&self, quality: Quality) -> Codec {
        if quality.is_dolby() {
            return quality.default_codec();
        }
        if let Some(codec) = self.codec_hint {
            return codec;
        }
        let extension = self
            .url
            .path()
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension == "mp4" || extension == "m4a" {
            if extension == "m4a" {
                return Codec::Aac;
            }
            return Codec::Unknown;
        }
        Codec::from_name(&extension).unwrap_or(Codec::Unknown)
    }
    fn format(&self, quality: Quality, file: &Value) -> &'static str {
        if quality.is_dolby() {
            // A-JOC bitrates are nominal tier labels, not inferred from size.
            return quality.format();
        }
        let codec = self.codec(quality);
        if let Some((field, format)) = self.matched_field(file) {
            // Size equality cannot turn an AC-4/FLAC URL into a different codec.
            let compatible = match codec {
                Codec::Mp3 => field.ends_with("mp3"),
                Codec::Aac => field.ends_with("aac"),
                Codec::Ogg => field.ends_with("ogg"),
                Codec::Flac => matches!(field, "size_flac" | "size_hires"),
                Codec::Ape => field == "size_ape",
                Codec::Dts => field == "size_dts",
                _ => false,
            };
            if compatible {
                return format;
            }
        }
        // size_new has no verified per-slot codec semantics: even when its size
        // matches (NAC/Q360/QAI), describe the actual extension/codec only.
        codec.format()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QualityRow {
    quality: Quality,
    available: bool,
    size_bytes: Option<u64>,
    format: &'static str,
    playback: Playback,
    codec: &'static str,
    content_type: &'static str,
}
impl QualityRow {
    fn from_size(quality: Quality, size_bytes: Option<u64>, format: &'static str) -> Self {
        let size_bytes = size_bytes.filter(|size| *size > 0);
        let codec = quality.default_codec();
        Self {
            quality,
            available: size_bytes.is_some(),
            size_bytes,
            format,
            playback: quality.playback(codec),
            codec: codec.name(),
            content_type: codec.content_type(),
        }
    }
    fn from_audio(quality: Quality, file: &Value, audio: Option<&Audio>) -> Self {
        let audio = audio.filter(|audio| audio.size_bytes.is_some());
        let mut row = Self::from_size(
            quality,
            audio.and_then(|a| a.size_bytes),
            audio.map_or(quality.format(), |a| a.format(quality, file)),
        );
        if let Some(audio) = audio {
            let codec = audio.codec(quality);
            row.playback = quality.playback(codec);
            row.codec = codec.name();
            row.content_type =
                if codec == Codec::Aac && audio.url.path().to_ascii_lowercase().ends_with(".aac") {
                    "audio/aac"
                } else {
                    codec.content_type()
                };
        }
        row
    }
}
#[derive(Serialize)]
struct Qualities {
    qualities: Vec<QualityRow>,
}
#[derive(Default, Deserialize)]
struct QualityQuery {
    quality: Option<Quality>,
}
impl QualityQuery {
    fn quality(&self) -> Quality {
        self.quality.unwrap_or(Quality::Hq)
    }
}
fn string(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.as_u64().map(|n| n.to_string()).unwrap_or_default())
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Song {
    id: String,
    name: String,
    artists: Vec<String>,
    album: String,
    album_mid: String,
    duration_ms: u64,
    cover: String,
    vip: bool,
    ttml: bool,
}
#[derive(Clone, Serialize)]
struct SearchResult {
    songs: Vec<Song>,
    total: u64,
}
#[derive(Clone, Serialize)]
struct Lyric {
    source: &'static str,
    text: String,
}
#[derive(Clone, Serialize)]
struct Lyrics {
    candidates: Vec<Lyric>,
}
#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    limit: Option<usize>,
    offset: Option<usize>,
}
fn adapter(state: &AppState) -> ApiResult<&Qq> {
    state.qq.as_ref().ok_or_else(missing)
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/qq/search", get(search))
        .route("/qq/cover/:albumMid", get(cover))
        .route("/qq/song/:mid", get(info))
        .route("/qq/song/:mid/play", get(play))
        .route("/qq/song/:mid/download", get(download))
        .route("/qq/song/:mid/lyrics", get(lyrics))
        .route("/qq/song/:mid/qualities", get(qualities))
        .route("/qq/capabilities", get(capabilities))
}
async fn cover(
    State(state): State<Arc<AppState>>,
    Path(album_mid): Path<String>,
) -> ApiResult<Response> {
    let qq = adapter(&state)?;
    let url = cover_url(&album_mid)?;
    cover_response(qq.cover.get(url).send().await.map_err(network)?)
}
fn cover_response(response: reqwest::Response) -> ApiResult<Response> {
    if response.status() == StatusCode::NOT_FOUND {
        return Err(missing());
    }
    if !response.status().is_success() {
        return Err(unavailable());
    }
    use futures::TryStreamExt;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from_stream(
            response
                .bytes_stream()
                .map_err(|e| std::io::Error::other(e.without_url())),
        ))
        .unwrap())
}
async fn capabilities(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    adapter(&state)?;
    Ok(Json(json!({"enabled": true, "qualities": QUALITY_LIST})))
}
async fn search(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<SearchResult>> {
    let qq = adapter(&state)?;
    let term = q.q.trim();
    if term.is_empty() || term.len() > 512 {
        return Err(bad());
    }
    let limit = q.limit.unwrap_or(30).clamp(1, 100);
    let offset = q.offset.unwrap_or(0).min(10000);
    let key = format!("{limit}:{offset}:{term}");
    if let Some(data) = qq.searches.get(&key) {
        return Ok(Json(data));
    }
    let raw = qq
        .request(
            "/cloudsearch",
            &[
                ("platform", "qq".into()),
                ("keywords", term.into()),
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
                ("type", "1".into()),
            ],
        )
        .await?;
    let result = raw.get("result").ok_or_else(unavailable)?;
    let songs = result
        .get("songs")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|s| qq.song_dto(s)).collect())
        .unwrap_or_default();
    let data = SearchResult {
        songs,
        total: result.get("songCount").and_then(Value::as_u64).unwrap_or(0),
    };
    qq.searches.put(key, data.clone());
    Ok(Json(data))
}
async fn info(
    State(state): State<Arc<AppState>>,
    Path(mid): Path<String>,
) -> ApiResult<Json<Song>> {
    let qq = adapter(&state)?;
    check_mid(&mid)?;
    Ok(Json(qq.song(&mid).await?))
}
async fn qualities(
    State(state): State<Arc<AppState>>,
    Path(mid): Path<String>,
) -> ApiResult<Json<Qualities>> {
    let qq = adapter(&state)?;
    check_mid(&mid)?;
    Ok(Json(qq.qualities(&mid).await?))
}
async fn play(
    State(state): State<Arc<AppState>>,
    Path(mid): Path<String>,
    Query(query): Query<QualityQuery>,
) -> ApiResult<Response> {
    let qq = adapter(&state)?;
    check_mid(&mid)?;
    // Only this response can expose file_url; JSON DTOs have no audio URL field.
    Ok(play_redirect(
        &qq.audio_url(&mid, query.quality()).await?.url,
    ))
}
fn play_redirect(url: &reqwest::Url) -> Response {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, url.as_str())
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap()
}
fn download_disposition(song: &Song, url: &reqwest::Url) -> String {
    let ext = url
        .path()
        .rsplit('.')
        .next()
        .filter(|s| !s.is_empty() && s.len() <= 5 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or("m4a");
    let base = format!("{} - {}", song.artists.join(", "), song.name);
    let base: String = base
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
        .take(160)
        .collect();
    let name = format!("{base}.{ext}");
    let encoded: String = name.bytes().map(|b| format!("%{b:02X}")).collect();
    format!("attachment; filename=\"download.{ext}\"; filename*=UTF-8''{encoded}")
}
fn download_content_type(url: &reqwest::Url) -> &'static str {
    match url
        .path()
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "ape" => "audio/x-ape",
        "eac3" | "ec3" | "joc" => "audio/eac3",
        "ac3" => "audio/ac3",
        "ac4" => "audio/ac4",
        "dts" => "audio/vnd.dts",
        "dtsx" => "audio/vnd.dts.hd",
        _ => "application/octet-stream",
    }
}
async fn download(
    State(state): State<Arc<AppState>>,
    Path(mid): Path<String>,
    Query(query): Query<QualityQuery>,
) -> ApiResult<Response> {
    let qq = adapter(&state)?;
    check_mid(&mid)?;
    let song = qq.song(&mid).await?;
    let url = qq.audio_url(&mid, query.quality()).await?.url;
    let response = qq.media.get(url.clone()).send().await.map_err(network)?;
    let status = response.status();
    if !status.is_success() {
        tracing::warn!("qq download upstream status: {status}");
        return Err(unavailable());
    }
    let mut out = Response::builder()
        .header(header::CONTENT_TYPE, download_content_type(&url))
        .header(
            header::CONTENT_DISPOSITION,
            download_disposition(&song, &url),
        )
        .header(header::CACHE_CONTROL, "no-store");
    if let Some(size) = response.headers().get(header::CONTENT_LENGTH) {
        out = out.header(header::CONTENT_LENGTH, size);
    }
    // Keep upstream errors (which carry URLs) out of streaming body diagnostics too.
    use futures::TryStreamExt;
    Ok(out
        .body(Body::from_stream(
            response
                .bytes_stream()
                .map_err(|e| std::io::Error::other(e.without_url())),
        ))
        .unwrap())
}
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn clean_text(text: &str) -> String {
    Regex::new(r#"(?i)https?://[^\s<>"']+"#)
        .unwrap()
        .replace_all(text, "")
        .into_owned()
}
fn wrap(body: &str, word: bool) -> String {
    format!("<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:ttm=\"http://www.w3.org/ns/ttml#metadata\" xmlns:itunes=\"http://music.apple.com/lyric-ttml-internal\" itunes:timing=\"{}\">{body}</tt>", if word { "Word" } else { "Line" })
}
/// Rebuild the renderable body with a strict tag / attribute allowlist. No credit
/// comments, external entities, URLs, credit metadata, styles or external resources.
fn clean_ttml(text: &str) -> Option<String> {
    if text.len() > 2_000_000 || text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return None;
    }
    let text = Regex::new(r"(?s)<!--.*?-->").unwrap().replace_all(text, "");
    let body = Regex::new(r"(?s)<body\b[^>]*>.*?</body>")
        .unwrap()
        .find(&text)?
        .as_str();
    let tags = Regex::new(r"(?s)<[^>]*>|[^<]+").unwrap();
    let tag = Regex::new(r"^<(/?)(body|div|p|span|br)\b([^>]*)>$").unwrap();
    let attrs = Regex::new(r#"([\w:]+)\s*=\s*["']([^"']*)["']"#).unwrap();
    let time = Regex::new(r"^\d+(?::\d+){0,2}(?:\.\d+)?$").unwrap();
    let mut out = String::new();
    for token in tags.find_iter(body) {
        let token = token.as_str();
        if !token.starts_with('<') {
            out.push_str(&clean_text(token));
            continue;
        }
        let Some(c) = tag.captures(token) else {
            continue;
        };
        if &c[1] == "/" {
            out.push_str(&format!("</{}>", &c[2]));
            continue;
        }
        out.push('<');
        out.push_str(&c[2]);
        for a in attrs.captures_iter(&c[3]) {
            let (key, value) = (&a[1], &a[2]);
            let allowed = match key {
                "begin" | "end" | "dur" => time.is_match(value),
                "ttm:role" => matches!(value, "x-bg" | "x-translation" | "x-roman"),
                "ttm:agent" | "itunes:key" | "itunes:songPart" => valid_mid(value),
                _ => false,
            };
            if allowed {
                out.push_str(&format!(" {key}=\"{}\"", xml(value)));
            }
        }
        out.push_str(if token.ends_with("/>") { "/>" } else { ">" });
    }
    if !out.contains("<p ") {
        return None;
    }
    let word = Regex::new(r#"<span\b[^>]*\bbegin="#)
        .unwrap()
        .is_match(&out);
    let mut result = wrap(&out, word);
    // Agent types affect duet layout; keep those declarations without names/credits.
    let agents = Regex::new(r#"<(?:ttm:)?agent\b[^>]*>"#).unwrap();
    let mut head = String::new();
    for agent in agents.find_iter(&text) {
        let mut id = None;
        let mut kind = None;
        for a in attrs.captures_iter(agent.as_str()) {
            if &a[1] == "xml:id" && valid_mid(&a[2]) {
                id = Some(a[2].to_owned());
            }
            if &a[1] == "type" && matches!(&a[2], "person" | "group" | "other") {
                kind = Some(a[2].to_owned());
            }
        }
        if let Some(id) = id {
            head.push_str(&format!(
                "<ttm:agent xml:id=\"{id}\" type=\"{}\"/>",
                kind.as_deref().unwrap_or("person")
            ));
        }
    }
    if !head.is_empty() {
        result = result.replacen(
            "<body",
            &format!("<head><metadata>{head}</metadata></head><body"),
            1,
        );
    }
    if let Some(lang) = Regex::new(r#"\bxml:lang=["']([A-Za-z0-9-]{1,35})["']"#)
        .unwrap()
        .captures(&text)
    {
        result = result.replacen("<tt ", &format!("<tt xml:lang=\"{}\" ", &lang[1]), 1);
    }
    Some(result)
}
fn field_text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    let v = value.get(key)?;
    v.as_str()
        .or_else(|| v.get("lyric").and_then(Value::as_str))
}
struct LyricWord {
    start: u64,
    end: u64,
    text: String,
}
struct LyricLine {
    start: u64,
    end: u64,
    text: String,
    words: Vec<LyricWord>,
}
fn lrc_lines(text: &str) -> Vec<LyricLine> {
    let stamp = Regex::new(r"\[(\d+):(\d+(?:\.\d+)?)\]").unwrap();
    let offset = Regex::new(r"\[offset:([+-]?\d+)\]")
        .unwrap()
        .captures(text)
        .and_then(|c| c[1].parse::<f64>().ok())
        .unwrap_or(0.0);
    let mut lines = Vec::new();
    for line in text.lines() {
        let content = stamp.replace_all(line, "");
        let content = clean_text(content.trim());
        if content.is_empty() || content.starts_with('[') {
            continue;
        }
        for c in stamp.captures_iter(line) {
            let (Ok(minutes), Ok(seconds)) = (c[1].parse::<f64>(), c[2].parse::<f64>()) else {
                continue;
            };
            let start = ((minutes * 60.0 + seconds) * 1000.0 + offset)
                .round()
                .max(0.0) as u64;
            lines.push(LyricLine {
                start,
                end: start.saturating_add(8000),
                text: content.clone(),
                words: Vec::new(),
            });
        }
    }
    lines.sort_by_key(|line| line.start);
    for i in 1..lines.len() {
        lines[i - 1].end = lines[i].start;
    }
    lines
}
fn qrc_content(text: &str) -> String {
    // QQ leaves quotes inside LyricContent unescaped. Only a matching quote
    // followed by optional whitespace and /> ends the value, across newlines.
    // Clipped responses may omit that delimiter; retain their available content.
    let envelope =
        Regex::new(r#"(?s)\bLyricContent\s*=\s*(?:"(.*?)(?:"\s*/>|$)|'(.*?)(?:'\s*/>|$))"#)
            .unwrap();
    let content = envelope.captures(text);
    content
        .as_ref()
        .and_then(|c| c.get(1).or_else(|| c.get(2)))
        .map_or(text, |m| m.as_str())
        .replace("&#10;", "\n")
        .replace("&#13;", "\r")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
fn qrc_lines(text: &str) -> Vec<LyricLine> {
    // /lyric/new may omit QRC. Accept decrypted [start,duration]word(offset,duration)
    // and QQ's XML LyricContent envelope; encrypted/compressed QRC falls through.
    let text = qrc_content(text);
    let line = Regex::new(r"(?m)\[(\d+),(\d+)\]([^\r\n]*)").unwrap();
    let word = Regex::new(r"([^()]+)\((\d+),(\d+)\)").unwrap();
    let mut lines = Vec::new();
    for c in line.captures_iter(&text) {
        let (Ok(start), Ok(duration)) = (c[1].parse::<u64>(), c[2].parse::<u64>()) else {
            continue;
        };
        let mut words = Vec::new();
        for w in word.captures_iter(&c[3]) {
            let (Ok(begin), Ok(duration)) = (w[2].parse::<u64>(), w[3].parse::<u64>()) else {
                continue;
            };
            words.push(LyricWord {
                start: begin,
                end: begin.saturating_add(duration),
                text: clean_text(&w[1]),
            });
        }
        if words.is_empty() {
            continue;
        }
        lines.push(LyricLine {
            start,
            end: start.saturating_add(duration),
            text: words.iter().map(|word| word.text.as_str()).collect(),
            words,
        });
    }
    lines
}
fn auxiliary_lines(lines: Vec<LyricLine>) -> BTreeMap<u64, String> {
    lines
        .into_iter()
        .filter_map(|line| {
            let text = if line.words.is_empty() {
                line.text
            } else {
                line.words.iter().map(|word| word.text.as_str()).collect()
            };
            (!matches!(text.trim(), "" | "//")).then_some((line.start, text))
        })
        .collect()
}
fn take_auxiliary(entries: &mut BTreeMap<u64, String>, start: u64) -> Option<String> {
    // Only the immediate predecessor/successor can be nearest. Prefer the earlier
    // timestamp on ties; removing it gives the first main line exclusive use.
    let before = entries.range(..=start).next_back();
    let after = entries.range(start..).next();
    let nearest = before
        .into_iter()
        .chain(after)
        .map(|(&time, _)| time)
        .min_by_key(|&time| time.abs_diff(start))
        .filter(|&time| time.abs_diff(start) <= 120)?;
    entries.remove(&nearest)
}

// MeT-Music UI's role vocabulary, copied from met-info-filter-reference.md.
// The source bundle lowercases these dot-separated words before matching.
const INFO_EN: &str = "作词.作曲.说唱作曲.原唱.中文填词.说唱填词.填词.改编词.词曲.作词作曲.填詞.中文填詞.词.曲.原.作詞.詞.总制作人.联合制作人.音乐制作.监制.音乐总监.项目总监.营销推广.编曲.編曲.制作.總製作人.聯合製作人.製作.音樂製作.監製.混音.母带.和声.和音.配唱.人声.录音勤务.录音.声乐指导.配乐.音频编辑.剪辑.音乐编辑.母帶.錄音.剪輯.和聲.器乐团队.戏曲.引用.乐队.吉他.贝斯.鼓.键盘.弦乐.管乐.打击乐.演唱.合唱.貝斯.鍵盤.弦樂.出品.特别鸣谢.歌曲营销.策划.策劃.运营.運營.推广.推廣.项目企划.企划.统筹.总监.總監.版权.版權.发行.商务.营销.宣传.發行.企劃.統籌.OP.SP.Lyrics.Lyricist.Lyrics by.Written by.Words by.Music.Music by.Composer.Composed by.Arranger.Arranged by.Arrangement.Producer.Produced by.Executive Producer.Co-Producer.Mixing.Mixed by.Mixing Engineer.Mastering.Mastered by.Mastering Engineer.Recording.Recorded by.Recording Engineer.Vocal Director.Backing Vocals.Vocal.Vocals.Backing Vocal.Crowd Vocals.Studio.Guitar.Bass.Drums.Keyboard.Piano.Strings.Programming.Publisher.Label.Coordinator";
const INFO_TN: &str = "音频.音頻.总.總.大.联合.聯合.共同.执行.執行.项目.項目.艺术.藝術.音乐.音樂.人声.人聲.弦乐.弦樂.乐队.樂隊.和声.和聲.歌曲.版权.版權.宣传.宣傳";
const INFO_NN: &str = "师.師.工程师.工程師.管理.后期.後期.室.棚.助理.统筹.統籌.处理.處理.录音.錄音.编写.編寫.指导.指導.协力.協力.总监.總監.监制.監製.编辑.編輯.策划.策劃.企划.企劃.人.方";

#[derive(Clone, Copy)]
struct LyricContext<'a> {
    name: &'a str,
    artists: &'a [String],
}

fn starts_with_role(text: &str, pos: usize, word: &str) -> bool {
    text.get(pos..)
        .and_then(|rest| rest.get(..word.len()))
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(word))
}

fn match_roles(text: &str, pos: usize, words: &str, out: &mut HashSet<usize>) {
    for word in words.split('.') {
        if starts_with_role(text, pos, word) {
            out.insert(pos + word.len());
        }
    }
}

fn skip_whitespace(text: &str, mut pos: usize) -> usize {
    while let Some(ch) = text.get(pos..).and_then(|rest| rest.chars().next()) {
        if !ch.is_whitespace() {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

/// Match a complete role label before the first colon. This is deliberately a
/// small parser rather than a prefix list: labels such as "音乐总监:",
/// "制作人/编曲:" and "总监制:" are composed from the three vocabularies.
fn is_role_label(text: &str) -> bool {
    let Some(colon) = text
        .char_indices()
        .find_map(|(pos, ch)| matches!(ch, ':' | '：').then_some(pos))
    else {
        return false;
    };
    let label = text[..colon].trim_end().to_lowercase();
    if label.is_empty() {
        return false;
    }
    let end = label.len();
    let mut segment_starts = HashSet::from([0]);
    let mut after_prefix = HashSet::new();
    let mut role_ends = HashSet::new();
    let mut positions: Vec<usize> = label.char_indices().map(|(pos, _)| pos).collect();
    positions.push(end);

    for pos in positions {
        if role_ends.contains(&pos) {
            if pos == end {
                return true;
            }
            match_roles(&label, pos, INFO_NN, &mut role_ends);
            let mut next = skip_whitespace(&label, pos);
            if let Some(ch) = label.get(next..).and_then(|rest| rest.chars().next()) {
                if matches!(ch, '/' | '／' | '&') {
                    next += ch.len_utf8();
                    next = skip_whitespace(&label, next);
                }
            }
            segment_starts.insert(pos);
            segment_starts.insert(next);
        }
        if segment_starts.contains(&pos) {
            match_roles(&label, pos, INFO_TN, &mut after_prefix);
        }
        if segment_starts.contains(&pos) || after_prefix.contains(&pos) {
            match_roles(&label, pos, INFO_EN, &mut role_ends);
        }
    }
    false
}

fn copyright_info(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"未[经經].*[许許]可.*(?:不得|禁止)").unwrap())
        .is_match(text)
}

fn is_prefix_info(text: &str) -> bool {
    let text = text.trim_start();
    ["器乐团队", "说唱作曲", "戏曲", "引用"]
        .iter()
        .any(|prefix| {
            text.strip_prefix(prefix).is_some_and(|rest| {
                rest.trim_start()
                    .chars()
                    .next()
                    .is_some_and(|ch| matches!(ch, ':' | '：'))
            })
        })
}

fn has_artist_header(text: &str, context: Option<LyricContext<'_>>) -> bool {
    let trimmed = text.trim();
    (trimmed.ends_with(':') || trimmed.ends_with('：'))
        && context.is_some_and(|ctx| {
            ctx.artists
                .iter()
                .filter(|artist| !artist.is_empty())
                .any(|artist| text.contains(artist))
        })
}

fn title_header(line: &LyricLine, first_nonempty: bool, context: Option<LyricContext<'_>>) -> bool {
    if !first_nonempty {
        return false;
    }
    let has_context = context.is_some_and(|ctx| {
        (!ctx.name.is_empty() && line.text.contains(ctx.name))
            || ctx
                .artists
                .iter()
                .filter(|artist| !artist.is_empty())
                .any(|artist| line.text.contains(artist))
    });
    (line.text.contains('-') && has_context) || (line.start == 0 && line.text.contains(" - "))
}

fn is_info_line(line: &LyricLine, first_nonempty: bool, context: Option<LyricContext<'_>>) -> bool {
    let text = &line.text;
    let lower = text.to_ascii_lowercase();
    is_role_label(text)
        || copyright_info(text)
        || (text.contains("计划")
            && ["酷狗", "音乐人", "QQ音乐"]
                .iter()
                .any(|term| text.contains(term)))
        || lower.contains("by:")
        || lower.contains("by：")
        || has_artist_header(text, context)
        || is_prefix_info(text)
        || title_header(line, first_nonempty, context)
        || (line.start == 0
            && line.end == 0
            && line.words.iter().all(|word| word.start == word.end))
}

fn pure_music_line(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"纯音乐[，,]?\s*请(您)?欣赏").unwrap())
        .is_match(text)
}

fn filter_info_lines(lines: Vec<LyricLine>, context: Option<LyricContext<'_>>) -> Vec<LyricLine> {
    let mut first_seen = false;
    lines
        .into_iter()
        .filter_map(|line| {
            let nonempty = !line.text.trim().is_empty();
            let first = nonempty && !first_seen;
            if nonempty {
                first_seen = true;
            }
            (!is_info_line(&line, first, context)).then_some(line)
        })
        .collect()
}

fn all_pure_music(lines: &[LyricLine]) -> bool {
    !lines.is_empty() && lines.iter().all(|line| pure_music_line(&line.text))
}

fn render_lyrics(
    lines: Vec<LyricLine>,
    word: bool,
    translations: Vec<LyricLine>,
    pronunciations: Vec<LyricLine>,
) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    let mut translations = auxiliary_lines(translations);
    let mut pronunciations = auxiliary_lines(pronunciations);
    let mut out = String::from("<body><div>");
    for line in lines {
        out.push_str(&format!(
            "<p begin=\"{:.3}\" end=\"{:.3}\">",
            line.start as f64 / 1000.0,
            line.end as f64 / 1000.0
        ));
        if line.words.is_empty() {
            out.push_str(&xml(&line.text));
        } else {
            for word in line.words {
                out.push_str(&format!(
                    "<span begin=\"{:.3}\" end=\"{:.3}\">{}</span>",
                    word.start as f64 / 1000.0,
                    word.end as f64 / 1000.0,
                    xml(&word.text)
                ));
            }
        }
        // Direct children of <p>, as parseTTML/readVoice and AMLL TTML expect.
        // Allow LRC/QRC rounding differences, consuming each match once per role.
        for (role, entries) in [
            ("x-translation", &mut translations),
            ("x-roman", &mut pronunciations),
        ] {
            if let Some(text) = take_auxiliary(entries, line.start) {
                out.push_str(&format!("<span ttm:role=\"{role}\">{}</span>", xml(&text)));
            }
        }
        out.push_str("</p>");
    }
    out.push_str("</div></body>");
    Some(wrap(&out, word))
}
#[cfg(test)]
fn lrc_ttml(text: &str, translation: Option<&str>) -> Option<String> {
    render_lyrics(
        lrc_lines(text),
        false,
        translation.map(lrc_lines).unwrap_or_default(),
        Vec::new(),
    )
}
#[cfg(test)]
fn qrc_ttml(text: &str, translation: Option<&str>, pronunciation: Option<&str>) -> Option<String> {
    render_lyrics(
        qrc_lines(text),
        true,
        translation.map(lrc_lines).unwrap_or_default(),
        pronunciation.map(qrc_lines).unwrap_or_default(),
    )
}
fn lrc_ttml_filtered(
    text: &str,
    translation: Option<&str>,
    context: Option<LyricContext<'_>>,
) -> Option<String> {
    let lines = lrc_lines(text);
    if all_pure_music(&lines) {
        return None;
    }
    render_lyrics(
        filter_info_lines(lines, context),
        false,
        translation.map(lrc_lines).unwrap_or_default(),
        Vec::new(),
    )
}
fn qrc_ttml_filtered(
    text: &str,
    translation: Option<&str>,
    pronunciation: Option<&str>,
    context: Option<LyricContext<'_>>,
) -> Option<String> {
    let lines = qrc_lines(text);
    if all_pure_music(&lines) {
        return None;
    }
    render_lyrics(
        filter_info_lines(lines, context),
        true,
        translation.map(lrc_lines).unwrap_or_default(),
        pronunciation.map(qrc_lines).unwrap_or_default(),
    )
}
#[cfg(test)]
fn converted_lyrics(raw: &Value) -> Vec<Lyric> {
    converted_lyrics_with_context(raw, None)
}
fn converted_lyrics_with_context(raw: &Value, context: Option<LyricContext<'_>>) -> Vec<Lyric> {
    let raw = raw.get("data").unwrap_or(raw);
    let mut candidates = Vec::new();
    if let Some(text) = field_text(raw, "qrc").and_then(|text| {
        qrc_ttml_filtered(
            text,
            field_text(raw, "qrctrans"),
            field_text(raw, "qrcroma"),
            context,
        )
    }) {
        candidates.push(Lyric {
            source: "qrc",
            text,
        });
    }
    if let Some(text) = field_text(raw, "lrc")
        .and_then(|text| lrc_ttml_filtered(text, field_text(raw, "lrctrans"), context))
    {
        candidates.push(Lyric {
            source: "lrc",
            text,
        });
    }
    candidates
}
async fn lyrics(
    State(state): State<Arc<AppState>>,
    Path(mid): Path<String>,
) -> ApiResult<Json<Lyrics>> {
    let qq = adapter(&state)?;
    check_mid(&mid)?;
    if let Some(data) = qq.lyrics.get(&mid) {
        return Ok(Json(data));
    }
    let mut candidates = Vec::new();
    // Match the existing AMLL native API using QQ's identifier, never Apple ID.
    let amll = qq
        .api
        .get(format!("{}/v1/lyrics/get", qq.amll))
        .query(&[("qqMusicId", &mid)])
        .send()
        .await;
    match amll {
        Ok(response) if response.status().is_success() => {
            if let Ok(raw) = response.json::<Value>().await {
                if let Some(text) = raw
                    .pointer("/data/lyrics")
                    .and_then(Value::as_str)
                    .and_then(clean_ttml)
                {
                    candidates.push(Lyric {
                        source: "amll",
                        text,
                    });
                }
            }
        }
        Err(e) => {
            let _ = e.without_url();
        }
        _ => {}
    }
    if let Ok(raw) = qq
        .request(
            "/lyric/ttml",
            &[("platform", "qq".into()), ("mid", mid.clone())],
        )
        .await
    {
        if let Some(text) = raw
            .get("content")
            .and_then(Value::as_str)
            .and_then(clean_ttml)
        {
            candidates.push(Lyric {
                source: "ttml",
                text,
            });
        }
    }
    if let Ok(raw) = qq
        .request(
            "/lyric/new",
            &[("platform", "qq".into()), ("id", mid.clone())],
        )
        .await
    {
        // Lyrics can be requested before the song details page has populated the
        // song cache. Reuse the normal song/info path on a cold cache so the
        // metadata-aware filters see the same title and artists. A failed info
        // request only drops the context; the lyric candidates remain usable.
        let song = match qq.songs.get(&mid) {
            Some(song) => Some(song),
            None => qq.song(&mid).await.ok(),
        };
        let context = song.as_ref().map(|song| LyricContext {
            name: &song.name,
            artists: &song.artists,
        });
        candidates.extend(converted_lyrics_with_context(&raw, context));
    }
    let data = Lyrics { candidates };
    if !data.candidates.is_empty() {
        qq.lyrics.put(mid, data.clone());
    }
    Ok(Json(data))
}

#[cfg(test)]
#[path = "../tests/qq_adapter/mod.rs"]
mod tests;

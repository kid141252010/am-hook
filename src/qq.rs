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
    num::NonZeroUsize,
    sync::{Arc, Mutex},
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
    permits: tokio::sync::Semaphore,
    searches: Cache<SearchResult>,
    songs: Cache<Song>,
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
        if let Some(proxy) = proxy {
            api = api.proxy(proxy.clone());
            media = media.proxy(proxy);
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
            permits: tokio::sync::Semaphore::new(8),
            searches: Cache::new(128, 600),
            songs: Cache::new(4096, 86400),
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
        let data = self
            .request(
                "/extra/music/info",
                &[("platform", "qq".into()), ("mids", mid.into())],
            )
            .await?;
        self.song_from_info(mid, &data)
    }
    fn song_from_info(&self, mid: &str, data: &Value) -> ApiResult<Song> {
        // Both the normalized search dialect and QQ's native track_info dialect.
        let rows = data
            .as_array()
            .or_else(|| data.get("data").and_then(Value::as_array));
        let raw = rows
            .and_then(|a| a.first())
            .or_else(|| data.as_object().and_then(|map| map.get(mid)))
            .unwrap_or_else(|| data.get("data").unwrap_or(data));
        let song = self
            .song_dto(raw.get("track_info").unwrap_or(raw))
            .ok_or_else(missing)?;
        if song.id != mid {
            return Err(missing());
        }
        Ok(song)
    }
    async fn audio_url(&self, mid: &str) -> ApiResult<reqwest::Url> {
        let data = self
            .request(
                "/extra/music/url",
                &[
                    ("platform", "qq".into()),
                    ("mid", mid.into()),
                    ("music_quality", "HQ".into()),
                    ("audio_api", "2".into()),
                ],
            )
            .await?;
        data.pointer("/0/track_info/file_url")
            .and_then(Value::as_str)
            .and_then(cdn_url)
            .ok_or_else(unavailable)
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
        .route("/qq/song/:mid", get(info))
        .route("/qq/song/:mid/play", get(play))
        .route("/qq/song/:mid/download", get(download))
        .route("/qq/song/:mid/lyrics", get(lyrics))
        .route("/qq/capabilities", get(capabilities))
}
async fn capabilities(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    adapter(&state)?;
    Ok(Json(json!({"enabled": true, "qualities": ["HQ"]})))
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
async fn play(State(state): State<Arc<AppState>>, Path(mid): Path<String>) -> ApiResult<Response> {
    let qq = adapter(&state)?;
    check_mid(&mid)?;
    // Only this response can expose file_url; JSON DTOs have no audio URL field.
    Ok(play_redirect(&qq.audio_url(&mid).await?))
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
async fn download(
    State(state): State<Arc<AppState>>,
    Path(mid): Path<String>,
) -> ApiResult<Response> {
    let qq = adapter(&state)?;
    check_mid(&mid)?;
    let song = qq.song(&mid).await?;
    let url = qq.audio_url(&mid).await?;
    let response = qq
        .media
        .get(url.clone())
        .send()
        .await
        .map_err(network)?;
    let status = response.status();
    if !status.is_success() {
        tracing::warn!("qq download upstream status: {status}");
        return Err(unavailable());
    }
    let mut out = Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
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
fn lrc_ttml(text: &str) -> Option<String> {
    let stamp = Regex::new(r"\[(\d+):(\d+(?:\.\d+)?)\]").unwrap();
    let offset = Regex::new(r"\[offset:([+-]?\d+)\]")
        .unwrap()
        .captures(text)
        .and_then(|c| c[1].parse::<f64>().ok())
        .unwrap_or(0.0)
        / 1000.0;
    let mut lines = Vec::new();
    for line in text.lines() {
        let content = stamp.replace_all(line, "");
        let content = clean_text(content.trim());
        if content.is_empty() || content.starts_with('[') {
            continue;
        }
        for c in stamp.captures_iter(line) {
            let start =
                (c[1].parse::<f64>().ok()? * 60.0 + c[2].parse::<f64>().ok()? + offset).max(0.0);
            lines.push((start, xml(&content)));
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = String::from("<body><div>");
    for (i, (start, text)) in lines.iter().enumerate() {
        let end = lines.get(i + 1).map(|l| l.0).unwrap_or(start + 8.0);
        out.push_str(&format!(
            "<p begin=\"{start:.3}\" end=\"{end:.3}\">{text}</p>"
        ));
    }
    out.push_str("</div></body>");
    (!lines.is_empty()).then(|| wrap(&out, false))
}
fn qrc_ttml(text: &str) -> Option<String> {
    // /lyric/new may omit QRC. Accept decrypted [start,duration]word(offset,duration)
    // and QQ's XML LyricContent envelope; encrypted/compressed QRC falls through.
    let text = text
        .replace("&#10;", "\n")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let line = Regex::new(r"(?m)\[(\d+),(\d+)\]([^\r\n]*)").unwrap();
    let word = Regex::new(r"([^()]+)\((\d+),(\d+)\)").unwrap();
    let mut out = String::from("<body><div>");
    let mut count = 0;
    for c in line.captures_iter(&text) {
        let start = c[1].parse::<u64>().ok()?;
        let end = start.saturating_add(c[2].parse::<u64>().ok()?);
        let mut words = String::new();
        for w in word.captures_iter(&c[3]) {
            let begin = w[2].parse::<u64>().ok()?;
            let finish = begin.saturating_add(w[3].parse::<u64>().ok()?);
            words.push_str(&format!(
                "<span begin=\"{:.3}\" end=\"{:.3}\">{}</span>",
                begin as f64 / 1000.0,
                finish as f64 / 1000.0,
                xml(&clean_text(&w[1]))
            ));
        }
        if words.is_empty() {
            continue;
        }
        count += 1;
        out.push_str(&format!(
            "<p begin=\"{:.3}\" end=\"{:.3}\">{words}</p>",
            start as f64 / 1000.0,
            end as f64 / 1000.0
        ));
    }
    out.push_str("</div></body>");
    (count > 0).then(|| wrap(&out, true))
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
        let raw = raw.get("data").unwrap_or(&raw);
        if let Some(text) = field_text(raw, "qrc").and_then(qrc_ttml) {
            candidates.push(Lyric {
                source: "qrc",
                text,
            });
        }
        if let Some(text) = field_text(raw, "lrc").and_then(lrc_ttml) {
            candidates.push(Lyric {
                source: "lrc",
                text,
            });
        }
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

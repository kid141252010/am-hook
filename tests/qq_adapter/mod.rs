//! Socket-free tests for the QQ adapter; included as unit tests to exercise private DTOs.
use super::*;
use axum::{body::to_bytes, http::Request};
use tower::ServiceExt;

fn qq() -> Qq {
    Qq::new(
        "https://example.invalid/api/web",
        "https://example.invalid/amll",
    )
}
fn raw_song() -> Value {
    json!({"id":"0039MnYb0qxYhV", "name":"晴天", "ar":[{"id":1,"name":"周杰伦"}],
        "al":{"id":"003DFRzD192KKD","name":"叶惠美","picUrl":"https://y.qq.com/cover.jpg"},
        "dt":269000,"fee":1,"ttml":1,
        "file_url":"https://example.invalid/private", "server":"private", "account":{"token":"secret"}})
}
#[test]
fn mids_and_cdn_urls_are_restricted() {
    for mid in ["1", "abc123", "0039MnYb0qxYhV"] {
        assert!(valid_mid(mid));
    }
    for mid in ["", "../x", "abc-def", "x?y", "中文", &"a".repeat(33)] {
        assert!(!valid_mid(mid));
    }
    assert!(cdn_url("https://isure.stream.qqmusic.qq.com/song.m4a?vkey=test").is_some());
    for url in [
        "http://y.qq.com/x",
        "https://qq.com.evil.invalid/x",
        "https://example.invalid/qq.com/x",
        "https://user:secret@y.qq.com/x",
        "https://y.qq.com:444/x",
        "file:///tmp/x",
    ] {
        assert!(cdn_url(url).is_none());
    }
}
#[test]
fn dto_is_a_whitelist_with_local_cover_and_vip_flags() {
    let qq = qq();
    let song = qq.song_dto(&raw_song()).unwrap();
    let dto = serde_json::to_value(&song).unwrap();
    assert_eq!(dto["id"], "0039MnYb0qxYhV");
    assert_eq!(dto["artists"], json!(["周杰伦"]));
    assert_eq!(dto["durationMs"], 269000);
    assert_eq!(dto["cover"], "/qq/cover/003DFRzD192KKD");
    assert_eq!(dto["vip"], true);
    assert_eq!(dto["ttml"], true);
    assert_eq!(dto.as_object().unwrap().len(), 9);
    let text = dto.to_string();
    for secret in [
        "file_url",
        "example.invalid",
        "private",
        "account",
        "picUrl",
        "y.qq.com",
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
    assert!(qq.songs.get("0039MnYb0qxYhV").is_some());
    assert_eq!(
        qq.covers.get("003DFRzD192KKD").as_deref(),
        Some("https://y.qq.com/cover.jpg")
    );
    assert!(qq.song_dto(&json!({"id":"../x"})).is_none());
}
#[test]
fn native_qq_info_dialect_is_supported() {
    let song = qq()
        .song_dto(
            &json!({"mid":"abc123","title":"Song","singer":[{"name":"Artist"}],
        "album":{"mid":"album1","title":"Album"},"interval":180,"pay":{"pay_play":1}}),
        )
        .unwrap();
    assert_eq!(song.name, "Song");
    assert_eq!(song.duration_ms, 180000);
    assert_eq!(song.album, "Album");
    assert!(song.vip);
}
#[test]
fn caches_expire_and_are_bounded() {
    let cache = Cache::new(2, 60);
    cache.put("a".into(), 1);
    cache.put("b".into(), 2);
    cache.put("c".into(), 3);
    assert_eq!(cache.get("a"), None);
    cache.entries.lock().unwrap().put(
        "expired".into(),
        (Instant::now() - Duration::from_secs(61), 4),
    );
    assert_eq!(cache.get("expired"), None);
    assert_eq!(qq().searches.ttl, Duration::from_secs(600));
    assert_eq!(qq().songs.ttl, Duration::from_secs(86400));
    assert_eq!(qq().lyrics.ttl, Duration::from_secs(604800));
}
#[test]
fn ttml_keeps_timing_translation_and_agents_but_removes_credits_and_urls() {
    let text = r#"<tt><head><metadata><ttm:agent xml:id="v1" type="group"><ttm:name>private</ttm:name></ttm:agent><amll:meta key="author" value="https://example.invalid/private"/></metadata></head><!-- private --><body dur="3.000"><div><p begin="1.000" end="3.000" ttm:agent="v1" data-secret="private"><span begin="1.000" end="2.000">Hello</span><span ttm:role="x-translation">你好</span> https://example.invalid/private</p></div></body></tt>"#;
    let cleaned = clean_ttml(text).unwrap();
    for secret in [
        "private",
        "example.invalid",
        "<!--",
        "amll:meta",
        "ttm:name",
        "data-secret",
    ] {
        assert!(!cleaned.contains(secret), "{secret}");
    }
    assert!(cleaned.contains("itunes:timing=\"Word\""));
    assert!(cleaned.contains("ttm:role=\"x-translation\""));
    assert!(cleaned.contains("type=\"group\""));
    assert!(cleaned.contains("begin=\"1.000\""));
    assert!(
        clean_ttml("<!DOCTYPE tt><tt><body><div><p begin=\"0\">x</p></div></body></tt>").is_none()
    );
    assert!(clean_ttml("not XML").is_none());
}
#[test]
fn lrc_fallback_handles_headers_offsets_repeated_stamps_and_xml_escaping() {
    let text = "[by:https://example.invalid/private]\n[offset:100]\n[00:01.00][00:03.00]A & B\n[00:05.0]Next";
    let cleaned = lrc_ttml(text).unwrap();
    assert!(!cleaned.contains("example.invalid"));
    assert!(cleaned.contains("begin=\"1.100\" end=\"3.100\">A &amp; B"));
    assert!(cleaned.contains("begin=\"3.100\" end=\"5.100\">A &amp; B"));
    assert!(lrc_ttml("[by:someone]\nno timestamps").is_none());
}
#[test]
fn qrc_word_timing_and_optional_fields_fall_through_safely() {
    let cleaned =
        qrc_ttml("[1000,500]你(1000,200)好(1200,300)\n[2000,1000]World(2000,1000)").unwrap();
    assert!(cleaned.contains("itunes:timing=\"Word\""));
    assert!(cleaned.contains("<p begin=\"1.000\" end=\"1.500\">"));
    assert!(cleaned.contains("<span begin=\"1.200\" end=\"1.500\">好</span>"));
    assert!(qrc_ttml("encrypted-unavailable").is_none());
    assert_eq!(
        field_text(&json!({"lrc":{"lyric":"[00:01]text"}}), "lrc"),
        Some("[00:01]text")
    );
    assert_eq!(field_text(&json!({"lrc":"[00:01]text"}), "qrc"), None);
}
fn state(enabled: bool) -> Arc<AppState> {
    let mut config = AppState::new("http://127.0.0.1:1".into()).config;
    config.qq_api = enabled.then(|| "https://example.invalid/api/web".into());
    Arc::new(AppState::with_config(config))
}
async fn get(state: Arc<AppState>, uri: &str) -> Response {
    crate::router(state)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}
#[tokio::test]
async fn unconfigured_routes_are_404_even_with_missing_or_invalid_parameters() {
    let state = state(false);
    for path in [
        "/qq/capabilities",
        "/qq/search",
        "/qq/search?q=x",
        "/qq/song/a/play",
        "/qq/song/a/download",
        "/qq/song/a/lyrics",
        "/qq/cover/a",
        "/qq/unknown",
    ] {
        assert_eq!(
            get(state.clone(), path).await.status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}
#[tokio::test]
async fn configured_routes_validate_mid_and_serve_only_cached_dtos() {
    let state = state(true);
    for path in [
        "/qq/song/a%21",
        "/qq/song/a%21/play",
        "/qq/song/a%21/download",
        "/qq/song/a%21/lyrics",
        "/qq/cover/a%21",
    ] {
        assert_eq!(
            get(state.clone(), path).await.status(),
            StatusCode::BAD_REQUEST,
            "{path}"
        );
    }
    let qq = state.qq.as_ref().unwrap();
    let song = qq.song_dto(&raw_song()).unwrap();
    qq.searches.put(
        "30:0:test".into(),
        SearchResult {
            songs: vec![song],
            total: 1,
        },
    );
    qq.lyrics.put(
        "abc".into(),
        Lyrics {
            candidates: vec![Lyric {
                source: "lrc",
                text: lrc_ttml("[00:01]Text").unwrap(),
            }],
        },
    );
    for path in [
        "/qq/search?q=test",
        "/qq/song/0039MnYb0qxYhV",
        "/qq/song/abc/lyrics",
        "/qq/capabilities",
        "/assets/host.js",
    ] {
        let response = get(state.clone(), path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!body.contains("example.invalid"), "{path}");
        assert!(!body.contains("file_url"), "{path}");
    }
}

#[tokio::test]
async fn playback_redirect_has_no_audio_or_json_body() {
    let url =
        reqwest::Url::parse("https://isure.stream.qqmusic.qq.com/song.m4a?vkey=test").unwrap();
    let response = play_redirect(&url);
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(response.headers()[header::LOCATION], url.as_str());
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(to_bytes(response.into_body(), 1000)
        .await
        .unwrap()
        .is_empty());
}
#[test]
fn download_filename_uses_metadata_and_keeps_the_cdn_extension() {
    let mut song = qq().song_dto(&raw_song()).unwrap();
    let url = reqwest::Url::parse("https://y.qq.com/test.flac?private=secret").unwrap();
    let value = download_disposition(&song, &url);
    assert!(value.contains("filename=\"download.flac\""));
    assert!(value.contains(
        "filename*=UTF-8''%E5%91%A8%E6%9D%B0%E4%BC%A6%20%2D%20%E6%99%B4%E5%A4%A9%2E%66%6C%61%63"
    ));
    assert!(!value.contains("secret"));
    song.name = "very long name / \r\n".repeat(40);
    assert!(download_disposition(&song, &url).ends_with("%2E%66%6C%61%63"));
    let no_extension = reqwest::Url::parse("https://y.qq.com/audio").unwrap();
    assert!(download_disposition(&song, &no_extension).contains("download.m4a"));
}
#[test]
fn album_mid_is_recovered_from_public_cover_when_album_id_is_numeric() {
    let mut raw = raw_song();
    raw["al"]["id"] = json!(123);
    raw["al"]["picUrl"] =
        json!("https://y.qq.com/music/photo_new/T002R800x800M000003DFRzD192KKD.jpg");
    assert_eq!(qq().song_dto(&raw).unwrap().album_mid, "003DFRzD192KKD");
}

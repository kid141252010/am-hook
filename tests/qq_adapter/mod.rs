//! Socket-free tests for the QQ adapter; included as unit tests to exercise private DTOs.
use super::*;
use axum::{body::to_bytes, http::Request};
use tower::ServiceExt;

fn cli(args: &[&str]) -> crate::cli::Cli {
    use clap::{CommandFactory, FromArgMatches};
    let matches = crate::cli::Cli::command()
        .mut_arg("qq_api", |arg| arg.env(None::<&str>))
        .mut_arg("qq_proxy", |arg| arg.env(None::<&str>))
        .try_get_matches_from(args)
        .unwrap();
    crate::cli::Cli::from_arg_matches(&matches).unwrap()
}

#[test]
fn proxy_cli_and_env_contract_keeps_values_private() {
    use clap::CommandFactory;
    let command = crate::cli::Cli::command();
    let arg = command
        .get_arguments()
        .find(|arg| arg.get_id() == "qq_proxy")
        .unwrap();
    assert_eq!(arg.get_long(), Some("qq-proxy"));
    assert_eq!(
        arg.get_env(),
        Some(std::ffi::OsStr::new("AM_HOOK_QQ_PROXY"))
    );
    assert!(arg.is_hide_env_values_set());
}

#[test]
fn proxy_environment_is_parsed_and_cli_takes_precedence() {
    use clap::{CommandFactory, Parser};
    const PROXY: &str = "socks5://127.0.0.1:11080";
    const CHILD: &str = "AM_HOOK_QQ_PROXY_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let cli = crate::cli::Cli::try_parse_from(["am-hook"]).unwrap();
        assert_eq!(cli.qq_proxy.as_deref(), Some(PROXY));
        cli.config().unwrap();
        let cli =
            crate::cli::Cli::try_parse_from(["am-hook", "--qq-proxy", "http://127.0.0.1:11081"])
                .unwrap();
        assert_eq!(cli.qq_proxy.as_deref(), Some("http://127.0.0.1:11081"));
        let help = crate::cli::Cli::command().render_long_help().to_string();
        assert!(help.contains("AM_HOOK_QQ_PROXY"));
        assert!(!help.contains(PROXY));
        return;
    }
    // Use a subprocess so other tests never observe a mutated environment.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "qq::tests::proxy_environment_is_parsed_and_cli_takes_precedence",
        ])
        .env(CHILD, "1")
        .env("AM_HOOK_QQ_PROXY", PROXY)
        .env_remove("AM_HOOK_QQ_API")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn proxy_cli_parameter_reaches_all_qq_clients_without_changing_other_clients() {
    let default = AppState::new("http://127.0.0.1:12340".into());
    for proxy in ["socks5://127.0.0.1:11080", "http://127.0.0.1:11080"] {
        let cli = cli(&[
            "am-hook",
            "--qq-api",
            "https://example.invalid/api/web",
            "--qq-proxy",
            proxy,
        ]);
        assert_eq!(cli.qq_proxy.as_deref(), Some(proxy));
        let state =
            AppState::with_qq_proxy(cli.config().unwrap(), cli.qq_proxy.as_deref()).unwrap();
        let qq = state.qq.as_ref().unwrap();
        for client in [&qq.api, &qq.media, &qq.cover] {
            assert!(format!("{client:?}").contains("127.0.0.1:11080"));
        }
        assert_eq!(
            format!("{:?}", state.http_client),
            format!("{:?}", default.http_client)
        );
        assert_eq!(
            format!("{:?}", state.apple_client),
            format!("{:?}", default.apple_client)
        );
    }
}

#[test]
fn absent_or_blank_proxy_preserves_default_clients() {
    let cli = cli(&["am-hook", "--qq-api", "https://example.invalid/api/web"]);
    assert!(cli.qq_proxy.is_none());
    let default = AppState::with_config(cli.config().unwrap());
    let default = default.qq.as_ref().unwrap();
    for proxy in [None, Some(""), Some("  ")] {
        let state = AppState::with_qq_proxy(cli.config().unwrap(), proxy).unwrap();
        let qq = state.qq.as_ref().unwrap();
        assert_eq!(format!("{:?}", qq.api), format!("{:?}", default.api));
        assert_eq!(format!("{:?}", qq.media), format!("{:?}", default.media));
        assert_eq!(format!("{:?}", qq.cover), format!("{:?}", default.cover));
    }
}

#[test]
fn invalid_proxy_fails_at_startup_without_upstream_urls() {
    for proxy in ["http://[", "unsupported://127.0.0.1:11080"] {
        let error = Qq::new(
            "https://example.invalid/private-api",
            "https://example.invalid/private-amll",
            Some(proxy),
        )
        .err()
        .expect("invalid proxy must fail before making any request");
        assert!(error.starts_with("Invalid QQ proxy:"));
        let cli = cli(&[
            "am-hook",
            "--qq-api",
            "https://example.invalid/private-api",
            "--qq-proxy",
            proxy,
        ]);
        let message = cli.config().err().unwrap();
        assert!(message.starts_with("Invalid QQ proxy:"));
        for message in [message, error.to_string(), format!("{error:?}")] {
            assert!(!message.contains("example.invalid"));
            assert!(!message.contains("private-api"));
            assert!(!message.contains("private-amll"));
        }
        // Invalid proxy configuration is rejected even when QQ is disabled.
        assert!(self::cli(&["am-hook", "--qq-proxy", proxy])
            .config()
            .is_err());
    }
}

fn qq() -> Qq {
    Qq::new(
        "https://example.invalid/api/web",
        "https://example.invalid/amll",
        None,
    )
    .unwrap()
}
fn raw_song() -> Value {
    json!({"id":"0039MnYb0qxYhV", "name":"晴天", "ar":[{"id":1,"name":"周杰伦"}],
        "al":{"id":"003DFRzD192KKD","name":"叶惠美","picUrl":"https://y.qq.com/music/photo_new/T002R500x500M000003DFRzD192KKD_5.jpg?version=5"},
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
fn cover_url_only_accepts_album_mids_and_uses_one_fixed_template() {
    for mid in ["1", "003DFRzD192KKD", &"a".repeat(32)] {
        assert_eq!(
            cover_url(mid).ok().unwrap(),
            format!("https://y.qq.com/music/photo_new/T002R500x500M000{mid}_5.jpg")
        );
    }
    for mid in [
        "",
        "../x",
        "a/b",
        "a\\b",
        "a?url=x",
        "a#x",
        "a.jpg",
        "a_5",
        "中文",
        "https://example.invalid/x",
        "%2F",
        &"a".repeat(33),
    ] {
        assert_eq!(cover_url(mid).err().unwrap().0, StatusCode::BAD_REQUEST);
    }
    // Never follow upstream redirects to another URL, even on the QQ CDN.
    assert!(format!("{:?}", qq().cover).contains("Policy(None)"));
}
#[tokio::test]
async fn cover_streams_jpeg_with_cache_headers_and_hides_upstream_failures() {
    let chunks = futures::stream::iter([
        Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"\xff\xd8")),
        Ok(bytes::Bytes::from_static(b"jpeg\xff\xd9")),
    ]);
    let upstream = axum::http::Response::builder()
        .header(header::SET_COOKIE, "upstream-private=value")
        .body(reqwest::Body::wrap_stream(chunks))
        .unwrap();
    let response = cover_response(upstream.into()).unwrap_or_else(|e| panic!("{}", e.0));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "public, max-age=86400"
    );
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    assert_eq!(
        to_bytes(response.into_body(), 100).await.unwrap(),
        b"\xff\xd8jpeg\xff\xd9"[..]
    );
    for (upstream, expected) in [
        (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND),
        (StatusCode::FOUND, StatusCode::BAD_GATEWAY),
        (StatusCode::FORBIDDEN, StatusCode::BAD_GATEWAY),
        (StatusCode::INTERNAL_SERVER_ERROR, StatusCode::BAD_GATEWAY),
    ] {
        let response = axum::http::Response::builder()
            .status(upstream)
            .header(header::LOCATION, "https://example.invalid/private")
            .body("private upstream error")
            .unwrap();
        let response = cover_response(response.into())
            .err()
            .unwrap()
            .into_response();
        assert_eq!(response.status(), expected);
        assert!(!response.headers().contains_key(header::CACHE_CONTROL));
        let body = to_bytes(response.into_body(), 1000).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("private"));
    }
}
#[test]
fn dto_is_a_whitelist_with_direct_cover_and_vip_flags() {
    let qq = qq();
    let song = qq.song_dto(&raw_song()).unwrap();
    let dto = serde_json::to_value(&song).unwrap();
    assert_eq!(dto["id"], "0039MnYb0qxYhV");
    assert_eq!(dto["artists"], json!(["周杰伦"]));
    assert_eq!(dto["durationMs"], 269000);
    assert_eq!(
        dto["cover"],
        "https://y.qq.com/music/photo_new/T002R{w}x{h}M000003DFRzD192KKD_5.jpg?version=5"
    );
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
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
    assert!(qq.songs.get("0039MnYb0qxYhV").is_some());
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
fn song_info_accepts_mid_keyed_map_on_cache_miss() {
    let qq = qq();
    let mid = "0039MnYb0qxYhV";
    let data = json!({
        "other123": {"track_info": {"mid": "other123"}},
        (mid): {"track_info": {"mid": mid, "title": "晴天"}}
    });
    assert!(qq.songs.get(mid).is_none());
    let song = qq.song_from_info(mid, &data).ok().unwrap();
    assert_eq!(song.id, mid);
    assert_eq!(song.name, "晴天");
    assert_eq!(qq.songs.get(mid).unwrap().id, mid);
}
#[test]
fn song_info_returns_missing_when_map_key_does_not_match() {
    let qq = qq();
    let mid = "0039MnYb0qxYhV";
    // Even matching track metadata must not be selected under another key.
    let data = json!({"other123": {"track_info": {"mid": mid, "title": "晴天"}}});
    let error = qq.song_from_info(mid, &data).err().unwrap();
    assert_eq!(error.0, StatusCode::NOT_FOUND);
    assert!(qq.songs.get(mid).is_none());
}
#[test]
fn song_info_checks_track_id_after_matching_map_key() {
    let mid = "0039MnYb0qxYhV";
    let data = json!({(mid): {"track_info": {"mid": "other123"}}});
    let error = qq().song_from_info(mid, &data).err().unwrap();
    assert_eq!(error.0, StatusCode::NOT_FOUND);
}
#[test]
fn song_info_preserves_existing_response_shapes() {
    let mid = "0039MnYb0qxYhV";
    let raw = raw_song();
    for data in [
        json!([raw]),
        json!({"data": [raw]}),
        json!({"data": raw}),
        raw,
    ] {
        let song = qq().song_from_info(mid, &data).ok().unwrap();
        assert_eq!(song.id, mid);
        assert_eq!(song.name, "晴天");
    }
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
async fn proxy_is_not_exposed_in_capabilities_or_host_config() {
    let cli = cli(&[
        "am-hook",
        "--qq-api",
        "https://example.invalid/api/web",
        "--qq-proxy",
        "socks5://127.0.0.1:11080",
    ]);
    let state =
        Arc::new(AppState::with_qq_proxy(cli.config().unwrap(), cli.qq_proxy.as_deref()).unwrap());
    for path in ["/qq/capabilities", "/assets/host.js"] {
        let response = get(state.clone(), path).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!body.contains("11080"));
        assert!(!body.to_ascii_lowercase().contains("qqproxy"));
        if path == "/qq/capabilities" {
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap(),
                json!({"enabled":true,"qualities":["MP3128", "AAC48", "WEB", "AAC192", "OGG96", "OGG192", "HQ", "NAC", "SQ", "RS", "QAI", "Q360V1", "Q360V2", "RA360", "DTS", "DTSX", "DA", "DAEAC3", "DA448", "DA1500"]})
            );
        }
    }
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
        "/qq/song/a/qualities",
        "/qq/cover/003DFRzD192KKD",
        "/qq/cover/a%21",
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
async fn cover_route_rejects_invalid_album_mids_before_outbound_requests() {
    let state = state(true);
    for mid in [
        "a%21",
        "%2E%2E%2Fx",
        "a%3Fx=1",
        "https%3A%2F%2Fexample.invalid",
        "%E4%B8%AD",
        &"a".repeat(33),
    ] {
        let path = format!("/qq/cover/{mid}");
        assert_eq!(
            get(state.clone(), &path).await.status(),
            StatusCode::BAD_REQUEST,
            "{path}"
        );
    }
    assert_eq!(
        get(state, "/qq/cover?url=https://example.invalid")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn configured_routes_validate_mid_and_serve_only_cached_dtos() {
    let state = state(true);
    for path in [
        "/qq/song/a%21",
        "/qq/song/a%21/play",
        "/qq/song/a%21/download",
        "/qq/song/a%21/lyrics",
        "/qq/song/a%21/qualities",
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
        json!("https://y.qq.com/music/photo_new/T002R500x500M000003DFRzD192KKD_5.jpg");
    assert_eq!(qq().song_dto(&raw).unwrap().album_mid, "003DFRzD192KKD");
}

#[test]
fn covers_preserve_upstream_urls_and_use_the_existing_qq_domain_as_fallback() {
    let qq = qq();
    let mut raw = raw_song();
    raw["al"]["picUrl"] = json!("https://y.qq.com/cover.jpg?version=5");
    assert_eq!(
        qq.song_dto(&raw).unwrap().cover,
        "https://y.qq.com/cover.jpg?version=5"
    );
    for pic in [Value::Null, json!("https://example.invalid/private.jpg")] {
        raw["al"]["picUrl"] = pic;
        assert_eq!(
            qq.song_dto(&raw).unwrap().cover,
            "https://y.gtimg.cn/music/photo_new/T002R{w}x{h}M000003DFRzD192KKD.jpg"
        );
    }
}

fn audio(quality: Quality, size: Option<u64>, extension: &str) -> Audio {
    Audio {
        url: reqwest::Url::parse(&format!(
            "https://isure.stream.qqmusic.qq.com/{}.{}?vkey=private",
            quality.name(),
            extension
        ))
        .unwrap(),
        size_bytes: size,
        codec_hint: None,
    }
}

#[test]
fn quality_sizes_use_only_positive_bytes_and_verified_mapping() {
    for (quality, field) in [
        (Quality::Mp3128, "size_128mp3"),
        (Quality::Aac48, "size_48aac"),
        (Quality::Web, "size_96aac"),
        (Quality::Aac192, "size_192aac"),
        (Quality::Ogg96, "size_96ogg"),
        (Quality::Ogg192, "size_192ogg"),
        (Quality::Hq, "size_320mp3"),
        (Quality::Sq, "size_flac"),
        (Quality::Rs, "size_hires"),
        (Quality::Dts, "size_dts"),
        (Quality::Da, "size_dolby"),
        (Quality::Daeac3, "size_dolby"),
        (Quality::Ra360, "size_360ra"),
    ] {
        assert_eq!(quality.size_field(), Some(field));
    }
    for quality in [
        Quality::Nac,
        Quality::Qai,
        Quality::Q360v1,
        Quality::Q360v2,
        Quality::Dtsx,
        Quality::Da448,
        Quality::Da1500,
    ] {
        // No unverified size_new slot indices or DTSX field names.
        assert!(quality.size_field().is_none());
    }
    for value in [Value::Null, json!(0), json!(-1), json!(1.5), json!("123")] {
        let row = QualityRow::from_size(Quality::Hq, byte_size(&value), Quality::Hq.format());
        assert!(!row.available);
        assert!(row.size_bytes.is_none());
    }
    let row = QualityRow::from_size(
        Quality::Hq,
        byte_size(&json!(10792943)),
        Quality::Hq.format(),
    );
    assert!(row.available);
    assert_eq!(row.size_bytes, Some(10792943));
    assert_eq!(row.format, "MP3 320 kbps");
    for (value, expected) in [
        (json!([0, 0, 0]), None),
        (json!([0, 123, 0]), Some(123)),
        (json!([123, 123]), Some(123)),
        (json!([123, 456]), None),
        (Value::Null, None),
    ] {
        let file = json!({"size_360ra":value});
        assert_eq!(Quality::Ra360.info_size(&file), expected);
        assert_eq!(
            Quality::Ra360.needs_probe(&file),
            value == json!([123, 456])
        );
    }
}

#[test]
fn quality_whitelist_roundtrips_every_verified_code_in_group_order() {
    let expected = [
        "MP3128", "AAC48", "WEB", "AAC192", "OGG96", "OGG192", "HQ", "NAC", "SQ", "RS", "QAI",
        "Q360V1", "Q360V2", "RA360", "DTS", "DTSX", "DA", "DAEAC3", "DA448", "DA1500",
    ];
    assert_eq!(QUALITY_LIST.len(), expected.len());
    for (quality, name) in QUALITY_LIST.into_iter().zip(expected) {
        assert_eq!(quality.name(), name);
        assert_eq!(serde_json::to_value(quality).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<Quality>(json!(name)).unwrap(),
            quality
        );
        assert!(serde_json::from_value::<Quality>(json!(name.to_ascii_lowercase())).is_err());
    }
    for unknown in ["Q360V3", "Q360_5.1", "Q360_7.1.4", "Q360_3.0", "DA2000"] {
        assert!(serde_json::from_value::<Quality>(json!(unknown)).is_err());
    }
}

#[test]
fn basic_aac_and_ogg_tiers_keep_verified_bitrates_and_native_classification() {
    let file = json!({"size_128mp3":1280,"size_48aac":480,"size_96aac":960,
        "size_192aac":1920,"size_96ogg":961,"size_192ogg":1921});
    for (quality, size, extension, format, codec, content_type) in [
        (
            Quality::Mp3128,
            1280,
            "mp3",
            "MP3 128 kbps",
            "mp3",
            "audio/mpeg",
        ),
        (
            Quality::Aac48,
            480,
            "m4a",
            "AAC 48 kbps",
            "aac",
            "audio/mp4; codecs=\"mp4a.40.2\"",
        ),
        (
            Quality::Web,
            960,
            "m4a",
            "AAC 96 kbps",
            "aac",
            "audio/mp4; codecs=\"mp4a.40.2\"",
        ),
        (
            Quality::Aac192,
            1920,
            "AAC",
            "AAC 192 kbps",
            "aac",
            "audio/aac",
        ),
        (
            Quality::Ogg96,
            961,
            "ogg",
            "Ogg 96 kbps",
            "vorbis",
            "audio/ogg; codecs=\"vorbis\"",
        ),
        (
            Quality::Ogg192,
            1921,
            "ogg",
            "Ogg 192 kbps",
            "vorbis",
            "audio/ogg; codecs=\"vorbis\"",
        ),
    ] {
        let source = audio(quality, Some(size), extension);
        let row = QualityRow::from_audio(quality, &file, Some(&source));
        assert_eq!(row.playback, Playback::Native, "{quality:?}");
        assert_eq!(row.codec, codec);
        assert_eq!(row.format, format);
        assert_eq!(row.content_type, content_type);
        let info_row = QualityRow::from_size(quality, quality.info_size(&file), quality.format());
        assert_eq!(info_row.size_bytes, Some(size));
        assert_eq!(info_row.playback, Playback::Native);
        assert_eq!(info_row.format, format);
    }
}

#[test]
fn dolby_tiers_follow_verified_wet_sample_entries() {
    let file = json!({"size_dolby":5112212,"size_new":[8936764,29870162]});
    for (quality, size, codec, playback, format) in [
        (
            Quality::Da,
            5112212,
            Codec::Ac4,
            Playback::Download,
            "AC-4 Dolby Atmos",
        ),
        (
            Quality::Da448,
            8936764,
            Codec::Ac4,
            Playback::Download,
            "AC-4 A-JOC 448 kbps",
        ),
        (
            Quality::Da1500,
            29870162,
            Codec::Ac4,
            Playback::Download,
            "AC-4 A-JOC 1500 kbps",
        ),
        (
            Quality::Daeac3,
            5112212,
            Codec::Ec3,
            Playback::Joc,
            "E-AC-3 JOC",
        ),
    ] {
        let info_row = QualityRow::from_size(quality, Some(size), quality.format());
        assert_eq!(info_row.playback, playback, "{quality:?}");
        assert_eq!(info_row.codec, codec.name());
        assert_eq!(info_row.format, format);
        assert_eq!(QualityRow::from_audio(quality, &file, None).format, format);
        // Generic MP4 URLs, shared size_dolby and conflicting metadata cannot
        // override the fixed pipelines verified by WET's dac4/dec3 boxes.
        for hint in [None, Some(Codec::Ac4), Some(Codec::Ec3), Some(Codec::Flac)] {
            let mut source = audio(quality, Some(size), "mp4");
            source.codec_hint = hint;
            let row = QualityRow::from_audio(quality, &file, Some(&source));
            assert_eq!(row.playback, playback, "{quality:?}");
            assert_eq!(row.codec, codec.name());
            assert_eq!(row.format, format);
            assert_eq!(row.content_type, codec.content_type());
            assert_eq!(row.size_bytes, Some(size));
        }
    }
}

#[test]
fn spatial_flac_is_native_and_undecodable_tiers_remain_download_only() {
    for quality in [Quality::Qai, Quality::Q360v1, Quality::Q360v2] {
        for (extension, expected) in [
            ("flac", Playback::Native),
            ("ac4", Playback::Download),
            ("dts", Playback::Download),
            ("mp4", Playback::Download),
        ] {
            let source = audio(quality, Some(123), extension);
            assert_eq!(
                QualityRow::from_audio(quality, &json!({"size_new":[123]}), Some(&source)).playback,
                expected
            );
        }
    }
    for quality in [Quality::Nac, Quality::Dts, Quality::Dtsx, Quality::Ra360] {
        for extension in ["nac", "dts", "dtsx", "mp4", "ac4", "flac", "mp3"] {
            let source = audio(quality, Some(123), extension);
            assert_eq!(
                QualityRow::from_audio(quality, &Value::Null, Some(&source)).playback,
                Playback::Download
            );
        }
    }
}

#[test]
fn probe_sizes_cross_check_file_fields_before_describing_the_format() {
    let file = json!({
        "size_128mp3": 4317292, "size_320mp3": 10792943,
        "size_flac": 55397039, "size_192aac": 6519764, "size_hires": 90000000
    });
    for (quality, size, extension, expected) in [
        (Quality::Sq, 55397039, "flac", "FLAC"),
        (Quality::Web, 6519764, "m4a", "AAC 192 kbps"),
        (Quality::Rs, 90000000, "flac", "FLAC Hi-Res"),
        (Quality::Nac, 2222222, "nac", "AICodec"),
    ] {
        let source = audio(quality, Some(size), extension);
        let row = QualityRow::from_audio(quality, &file, Some(&source));
        assert!(row.available);
        assert_eq!(row.size_bytes, Some(size));
        assert_eq!(row.format, expected);
    }
    // A verified default cannot override an actual MP3 payload returned for WEB.
    let source = audio(Quality::Web, Some(4317292), "mp3");
    assert_eq!(source.format(Quality::Web, &file), "MP3 128 kbps");
    // Equal-size fields are ambiguous: only the actual extension is described.
    assert_eq!(
        source.format(
            Quality::Web,
            &json!({"size_128mp3":4317292,"size_192aac":4317292})
        ),
        "MP3"
    );
    let source = audio(Quality::Sq, None, "flac");
    assert!(!QualityRow::from_audio(Quality::Sq, &file, Some(&source)).available);
    assert!(!QualityRow::from_audio(Quality::Sq, &file, None).available);
}

#[test]
fn audio_response_requires_a_public_qq_url_and_treats_missing_sizes_as_unavailable() {
    let valid = "https://isure.stream.qqmusic.qq.com/song.mp3?vkey=private";
    let source =
        Audio::from_response(&json!([{"track_info":{"file_url":valid,"file_size":10792943}}]))
            .ok()
            .unwrap();
    assert_eq!(source.size_bytes, Some(10792943));
    for track in [
        json!({"file_url":valid}),
        json!({"file_url":valid,"file_size":0}),
    ] {
        assert!(Audio::from_response(&json!([{"track_info":track}]))
            .ok()
            .unwrap()
            .size_bytes
            .is_none());
    }
    for data in [
        json!([]),
        json!([{"track_info":{"file_url":"https://example.invalid/private"}}]),
    ] {
        assert!(Audio::from_response(&data).is_err());
    }
}

#[test]
fn explicit_codec_metadata_precedes_container_and_size_and_stays_allowlisted() {
    for key in ["codec", "codecs", "audio_codec"] {
        for (name, codec, playback) in [
            ("ac4", "ac-4", Playback::Download),
            ("ac-4", "ac-4", Playback::Download),
            ("eac3", "ec-3", Playback::Joc),
            ("flac", "flac", Playback::Native),
        ] {
            let source = Audio::from_response(&json!([{"track_info":{
                "file_url":"https://isure.stream.qqmusic.qq.com/DA.mp4?vkey=private",
                "file_size":5112212, key:name
            }}]))
            .ok()
            .unwrap();
            let row = QualityRow::from_audio(
                Quality::Q360v1,
                &json!({"size_dolby":5112212}),
                Some(&source),
            );
            assert_eq!(row.codec, codec);
            assert_eq!(row.playback, playback);
        }
    }
    let source = Audio::from_response(&json!([{"track_info":{
        "file_url":"https://isure.stream.qqmusic.qq.com/file.bin?vkey=private",
        "file_size":123, "codec":"private upstream metadata"
    }}]))
    .ok()
    .unwrap();
    let row = QualityRow::from_audio(Quality::Q360v1, &Value::Null, Some(&source));
    assert_eq!(row.playback, Playback::Download);
    assert_eq!(row.codec, "unknown");
    assert!(!serde_json::to_string(&row).unwrap().contains("private"));
}

#[test]
fn download_content_types_follow_an_allowlisted_file_extension() {
    for (extension, expected) in [
        ("mp3", "audio/mpeg"),
        ("FLAC", "audio/flac"),
        ("m4a", "audio/mp4"),
        ("aac", "audio/aac"),
        ("ogg", "audio/ogg"),
        ("ape", "audio/x-ape"),
        ("nac", "application/octet-stream"),
        ("html", "application/octet-stream"),
    ] {
        let url =
            reqwest::Url::parse(&format!("https://y.qq.com/song.{extension}?x=.html")).unwrap();
        assert_eq!(download_content_type(&url), expected);
    }
}

#[tokio::test]
async fn quality_parameters_are_validated_before_upstream_requests() {
    let state = state(true);
    for action in ["play", "download"] {
        for quality in ["", "hq", "invalid", "HQ%20", "HQ%26SQ", "null"] {
            let path = format!("/qq/song/abc/{action}?quality={quality}");
            assert_eq!(
                get(state.clone(), &path).await.status(),
                StatusCode::BAD_REQUEST,
                "{path}"
            );
        }
    }
}

#[tokio::test]
async fn playback_defaults_to_hq_and_caches_are_isolated_by_quality() {
    let state = state(true);
    let qq = state.qq.as_ref().unwrap();
    for quality in QUALITY_LIST {
        qq.audio
            .put(quality.cache_key("abc"), audio(quality, Some(123), "mp3"));
    }
    assert!(qq.audio.get(&Quality::Hq.cache_key("other")).is_none());
    for quality in QUALITY_LIST {
        let path = format!("/qq/song/abc/play?quality={}", quality.name());
        let response = get(state.clone(), &path).await;
        assert_eq!(response.status(), StatusCode::FOUND, "{path}");
        assert_eq!(
            response.headers()[header::LOCATION],
            audio(quality, Some(123), "mp3").url.as_str()
        );
    }
    let response = get(state.clone(), "/qq/song/abc/play").await;
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers()[header::LOCATION],
        audio(Quality::Hq, Some(123), "mp3").url.as_str()
    );
}

#[tokio::test]
async fn qualities_endpoint_returns_all_tiers_and_uses_shared_cached_probes() {
    let state = state(true);
    let qq = state.qq.as_ref().unwrap();
    qq.infos.put(
        "abc".into(),
        json!({"abc":{"track_info":{"mid":"abc","file":{
            "size_128mp3":4317292,"size_48aac":1600000,"size_96aac":3259882,
            "size_192aac":6519764,"size_96ogg":3200000,"size_192ogg":6400000,
            "size_320mp3":10792943,"size_flac":55397039,"size_hires":0,
            "size_dts":0,"size_360ra":[0,0,0],"size_dolby":5112212,
            "size_new":[58805051,80437198,186980254,8936764,29870162]
        }}}}),
    );
    for (quality, size, extension) in [
        (Quality::Web, Some(3259882), "m4a"),
        (Quality::Nac, None, "nac"),
        (Quality::Q360v1, Some(58805051), "flac"),
        (Quality::Q360v2, Some(80437198), "flac"),
        (Quality::Qai, Some(186980254), "flac"),
        (Quality::Dtsx, None, "dtsx"),
        (Quality::Da448, Some(8936764), "mp4"),
        (Quality::Da1500, Some(29870162), "mp4"),
    ] {
        qq.audio
            .put(quality.cache_key("abc"), audio(quality, size, extension));
    }
    // Verified fields work without a cached or live audio URL.
    assert!(qq.audio.get(&Quality::Hq.cache_key("abc")).is_none());
    assert!(qq.audio.get(&Quality::Da.cache_key("abc")).is_none());
    let response = get(state.clone(), "/qq/song/abc/qualities").await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let rows = body["qualities"].as_array().unwrap();
    assert_eq!(rows.len(), 20);
    assert_eq!(
        rows.iter()
            .map(|row| row["quality"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "MP3128", "AAC48", "WEB", "AAC192", "OGG96", "OGG192", "HQ", "NAC", "SQ", "RS", "QAI",
            "Q360V1", "Q360V2", "RA360", "DTS", "DTSX", "DA", "DAEAC3", "DA448", "DA1500"
        ]
    );
    for (quality, available, size, format, playback, codec) in [
        (
            "MP3128",
            true,
            Some(4317292),
            "MP3 128 kbps",
            "native",
            "mp3",
        ),
        ("AAC48", true, Some(1600000), "AAC 48 kbps", "native", "aac"),
        ("WEB", true, Some(3259882), "AAC 96 kbps", "native", "aac"),
        (
            "AAC192",
            true,
            Some(6519764),
            "AAC 192 kbps",
            "native",
            "aac",
        ),
        (
            "OGG96",
            true,
            Some(3200000),
            "Ogg 96 kbps",
            "native",
            "vorbis",
        ),
        (
            "OGG192",
            true,
            Some(6400000),
            "Ogg 192 kbps",
            "native",
            "vorbis",
        ),
        ("HQ", true, Some(10792943), "MP3 320 kbps", "native", "mp3"),
        ("NAC", false, None, "AICodec", "download", "nac"),
        ("SQ", true, Some(55397039), "FLAC", "native", "flac"),
        ("RS", false, None, "FLAC Hi-Res", "native", "flac"),
        ("QAI", true, Some(186980254), "FLAC", "native", "flac"),
        ("Q360V1", true, Some(58805051), "FLAC", "native", "flac"),
        ("Q360V2", true, Some(80437198), "FLAC", "native", "flac"),
        (
            "RA360",
            false,
            None,
            "360 Reality Audio",
            "download",
            "unknown",
        ),
        ("DTS", false, None, "DTS", "download", "dts"),
        ("DTSX", false, None, "DTS:X", "download", "dtsx"),
        (
            "DA",
            true,
            Some(5112212),
            "AC-4 Dolby Atmos",
            "download",
            "ac-4",
        ),
        ("DAEAC3", true, Some(5112212), "E-AC-3 JOC", "joc", "ec-3"),
        (
            "DA448",
            true,
            Some(8936764),
            "AC-4 A-JOC 448 kbps",
            "download",
            "ac-4",
        ),
        (
            "DA1500",
            true,
            Some(29870162),
            "AC-4 A-JOC 1500 kbps",
            "download",
            "ac-4",
        ),
    ] {
        let row = rows.iter().find(|row| row["quality"] == quality).unwrap();
        assert_eq!(row["available"], available, "{quality}");
        assert_eq!(row["sizeBytes"], json!(size), "{quality}");
        assert_eq!(row["format"], format, "{quality}");
        assert_eq!(row["playback"], playback, "{quality}");
        assert_eq!(row["codec"], codec, "{quality}");
        assert!(row["contentType"].as_str().unwrap().contains('/'));
    }
    for private in ["vkey", "file_url", "qqmusic.qq.com", "example.invalid"] {
        assert!(!body.to_string().contains(private));
    }

    // A live URL for another tier cannot make a zero or missing HQ size available.
    for file in [json!({"size_320mp3":0}), json!({})] {
        qq.infos.put(
            "abc".into(),
            json!({"abc":{"track_info":{"mid":"abc","file":file}}}),
        );
        let response = get(state.clone(), "/qq/song/abc/qualities").await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let row = body["qualities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["quality"] == "HQ")
            .unwrap();
        assert_eq!(row["available"], false);
        assert_eq!(row["sizeBytes"], Value::Null);
    }
}

#[tokio::test]
async fn cached_payload_overrides_verified_default_without_cross_tier_leakage() {
    let state = state(true);
    let qq = state.qq.as_ref().unwrap();
    qq.infos.put(
        "abc".into(),
        json!({"abc":{"track_info":{"mid":"abc","file":{
            "size_96aac":960,"size_dolby":5112212
        }}}}),
    );
    for quality in QUALITY_LIST
        .into_iter()
        .filter(|quality| quality.size_field().is_none())
    {
        qq.audio
            .put(quality.cache_key("abc"), audio(quality, None, "nac"));
    }
    let mut da = audio(Quality::Da, Some(5112212), "mp4");
    da.codec_hint = Some(Codec::Ec3);
    qq.audio.put(Quality::Da.cache_key("abc"), da);
    qq.audio.put(
        Quality::Web.cache_key("abc"),
        audio(Quality::Web, Some(1280), "mp3"),
    );
    let response = get(state, "/qq/song/abc/qualities").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap()).unwrap();
    let rows = body["qualities"].as_array().unwrap();
    let row = |quality: &str| rows.iter().find(|row| row["quality"] == quality).unwrap();
    assert_eq!(row("WEB")["sizeBytes"], 1280);
    assert_eq!(row("WEB")["codec"], "mp3");
    assert_eq!(row("DA")["playback"], "download");
    assert_eq!(row("DA")["format"], "AC-4 Dolby Atmos");
    assert_eq!(row("DAEAC3")["playback"], "joc");
    assert_eq!(row("DAEAC3")["sizeBytes"], 5112212);
}

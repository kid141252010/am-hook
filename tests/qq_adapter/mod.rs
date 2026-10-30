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
fn proxy_cli_parameter_reaches_both_qq_clients_without_changing_other_clients() {
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
        for client in [&qq.api, &qq.media] {
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
                json!({"enabled":true,"qualities":["HQ", "SQ", "WEB", "NAC", "RS"]})
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
    }
}

#[test]
fn quality_sizes_use_only_positive_bytes_and_verified_mapping() {
    assert_eq!(Quality::Hq.size_field(), Some("size_320mp3"));
    for quality in [Quality::Sq, Quality::Web, Quality::Nac, Quality::Rs] {
        // Do not silently promote semantic guesses to verified mappings.
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
    // WEB is deliberately not assumed to be AAC: a MP3-sized result describes MP3.
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
            "size_320mp3":10792943,"size_flac":55397039,"size_192aac":6519764,"size_hires":0
        }}}}),
    );
    for (quality, size, extension) in [
        (Quality::Sq, Some(55397039), "flac"),
        (Quality::Web, Some(6519764), "m4a"),
        (Quality::Nac, None, "nac"),
        (Quality::Rs, None, "flac"),
    ] {
        qq.audio
            .put(quality.cache_key("abc"), audio(quality, size, extension));
    }
    // HQ is fully derived from info and must not require a cached or live audio URL.
    assert!(qq.audio.get(&Quality::Hq.cache_key("abc")).is_none());
    let response = get(state.clone(), "/qq/song/abc/qualities").await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body,
        json!({"qualities":[
            {"quality":"HQ","available":true,"sizeBytes":10792943,"format":"MP3 320 kbps"},
            {"quality":"SQ","available":true,"sizeBytes":55397039,"format":"FLAC"},
            {"quality":"WEB","available":true,"sizeBytes":6519764,"format":"AAC 192 kbps"},
            {"quality":"NAC","available":false,"sizeBytes":null,"format":"AICodec"},
            {"quality":"RS","available":false,"sizeBytes":null,"format":"Hi-Res"}
        ]})
    );
    for private in ["vkey", "file_url", "qqmusic.qq.com", "example.invalid"] {
        assert!(!body.to_string().contains(private));
    }

    // A live URL for another tier cannot make a zero or missing HQ size available.
    // No audio cache entry exists for HQ, so these requests also prove it does not probe HQ.
    for file in [json!({"size_320mp3":0}), json!({})] {
        qq.infos.put(
            "abc".into(),
            json!({"abc":{"track_info":{"mid":"abc","file":file}}}),
        );
        let response = get(state.clone(), "/qq/song/abc/qualities").await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["qualities"][0]["available"], false);
        assert_eq!(body["qualities"][0]["sizeBytes"], Value::Null);
    }
}

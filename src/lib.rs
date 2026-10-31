pub mod amp;
pub mod assets;
pub mod cli;
pub mod limit;
pub mod log;
pub mod links;
pub mod state;
pub mod qq;
pub mod ui;
pub mod updater;
pub mod wrapper;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, OnceLock, RwLock};

use axum::body::Body;
use axum::extract::{ConnectInfo, Request};
use axum::http::header::{CONTENT_TYPE, RETRY_AFTER};
use axum::http::{HeaderMap, Response, StatusCode};
use axum::middleware::Next;
use axum::routing::{get, post};
use axum::Router;
use serde_json::json;
use tokio::sync::Semaphore;

use crate::state::AppState;

#[derive(Clone, Copy)]
struct QqLimitOverride {
    rate: u32,
    concurrency: usize,
}

static QQ_LIMIT_OVERRIDE: OnceLock<RwLock<Option<QqLimitOverride>>> = OnceLock::new();

/// Set by [`crate::cli::Cli::config`] when either QQ quota flag is explicit.
/// Config's public shape is kept unchanged for embedders that construct it directly.
pub(crate) fn set_qq_client_limit_override(values: Option<(u32, usize)>) {
    let lock = QQ_LIMIT_OVERRIDE.get_or_init(|| RwLock::new(None));
    *lock.write().unwrap_or_else(|error| error.into_inner()) = values.map(|(rate, concurrency)| QqLimitOverride { rate, concurrency });
}

fn qq_limit_values(state: &AppState) -> (u32, usize) {
    QQ_LIMIT_OVERRIDE
        .get()
        .and_then(|lock| lock.read().ok().and_then(|value| *value))
        .map(|value| (value.rate, value.concurrency))
        .unwrap_or((state.config.client_rate, state.config.client_concurrency))
}

/// QQ's client quota is intentionally separate from the wrapper quota. The rate
/// window reuses the tested ClientLimit accounting; concurrency has its own
/// per-IP semaphore so the two route groups cannot consume each other's slots.
struct QqClientLimit {
    rate: crate::limit::ClientLimit,
    rate_limit: u32,
    concurrency: usize,
    slots: std::sync::Mutex<HashMap<IpAddr, Arc<Semaphore>>>,
}

impl QqClientLimit {
    fn new(rate: u32, concurrency: usize, ip_header: Option<&str>) -> Self {
        Self {
            rate: crate::limit::ClientLimit::new(rate, 0, ip_header),
            rate_limit: rate,
            concurrency,
            slots: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn enabled(&self) -> bool { self.rate_limit > 0 || self.concurrency > 0 }

    fn slot(&self, ip: IpAddr) -> Option<Arc<Semaphore>> {
        (self.concurrency > 0).then(|| {
            let mut slots = self.slots.lock().unwrap_or_else(|error| error.into_inner());
            slots.entry(ip).or_insert_with(|| Arc::new(Semaphore::new(self.concurrency))).clone()
        })
    }
}

fn qq_limited_path(path: &str) -> bool {
    path == "/qq/search"
        || (path.starts_with("/qq/song/")
            && (path.ends_with("/qualities") || path.ends_with("/download") || path.ends_with("/lyrics")))
}

async fn limit_qq_clients(limit: Arc<QqClientLimit>, request: Request, next: Next) -> Response<Body> {
    if !qq_limited_path(request.uri().path()) || !limit.enabled() {
        return next.run(request).await;
    }
    let peer = request.extensions().get::<ConnectInfo<std::net::SocketAddr>>().map(|info| info.0);
    let Some(ip) = limit.rate.client_ip(request.headers(), peer) else {
        return next.run(request).await;
    };
    if let Err(wait) = limit.rate.check(ip) {
        let retry_after = wait.as_secs_f64().ceil().max(1.0) as u64;
        let body = json!({ "code": 1, "msg": format!("Too many QQ requests; retry in {retry_after} s") });
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .header(RETRY_AFTER, retry_after)
            .body(Body::from(body.to_string()))
            .expect("static response parts are valid");
    }
    let _permit = match limit.slot(ip) {
        Some(slots) => Some(slots.acquire_owned().await.expect("QQ semaphore is never closed")),
        None => None,
    };
    next.run(request).await
}

/// 首页与解析接口优先，歌曲 / MV / 专辑 / 歌单 / 艺人 / 电台页、编辑页（room / multi-room / grouping / curator）、广播、资料库页统一由 fallback 分流
pub fn router(state: Arc<AppState>) -> Router {
    // 内嵌前端资源按 assets::ASSETS 表注册（原地址与内容哈希地址），其余是后端接口；
    // host.js 按启动参数生成（见 ui::host_js_handler）
    let router = assets::SITE.assets.iter().filter(|asset| asset.url != assets::HOST_JS).fold(Router::new(), |router, asset| {
        router.route(&asset.url, get(move |headers: HeaderMap| async move { assets::serve(&headers, asset) }))
    });
    // 转发 wrapper-lite 的接口：--local-wrapper-only 时一律 501，页面改用浏览器直连
    let relay = Router::new()
        .route("/status", get(ui::status_handler))
        .route("/parse/song/:adam_id", get(ui::master_handler))
        .route("/parse/mv/:adam_id", get(ui::mv_master_handler))
        .route("/key", get(ui::key_handler))
        .route("/mv/webplayback/:adam_id", get(ui::mv_webplayback_handler))
        .route("/mv/license", post(ui::mv_license_handler))
        .route("/lyrics/:adam_id", get(ui::lyrics_handler))
        // 按客户端 IP 限流（--client-rate），在 require_relay 之内
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), limit::limit_clients))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), ui::require_relay));
    let qq = if state.qq.is_some() {
        let (rate, concurrency) = qq_limit_values(&state);
        let qq_limit = Arc::new(QqClientLimit::new(rate, concurrency, state.config.client_ip_header.as_deref()));
        let qq_routes = qq::unlimited_routes().merge(qq::limited_routes());
        qq_routes.route_layer(axum::middleware::from_fn(move |request, next| {
            let limit = qq_limit.clone();
            async move { limit_qq_clients(limit, request, next).await }
        }))
    } else {
        Router::new()
    };
    router
        .merge(relay)
        .merge(qq)
        .route(assets::HOST_JS, get(ui::host_js_handler))
        .route("/", get(ui::app_handler))
        // 「新发现」（官网 /{cc}/new，这里跟随主地区）
        .route("/new", get(ui::app_handler))
        // 浏览器直连本地 wrapper-lite 时，MV master 仍由服务端获取
        .route("/parse/mv-master", get(ui::mv_master_url_handler))
        .route("/amp/v1/catalog/*path", get(amp::catalog_handler))
        .route("/amp/v1/editorial/*path", get(amp::editorial_handler))
        .route("/amp/v1/storefronts", get(amp::storefronts_handler))
        // 电台（wrapper-lite 没有这些接口）：带页面传来的 media-user-token 请求 amp-api 与 Apple 的 license 服务器
        .route("/station/assets", get(amp::station_assets_handler))
        .route("/station/next-tracks/:id", post(amp::station_next_tracks_handler))
        .route("/station/storefront", get(amp::station_storefront_handler))
        .route("/station/catalog/*path", get(amp::station_catalog_handler))
        .route("/station/recent", get(amp::station_recent_handler))
        .route("/station/license", post(amp::station_license_handler))
        .fallback(ui::fallback_handler)
        .layer(axum::middleware::from_fn(log::access_log))
        .with_state(state)
}

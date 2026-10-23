pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod markdown;
pub mod media;
pub mod model;

use crate::{
    config::Config,
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Request, State},
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use tower_http::{compression::CompressionLayer, services::ServeDir};

type RateTable = Arc<Mutex<HashMap<IpAddr, (Instant, u32)>>>;
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub http: reqwest::Client,
    pub image_jobs: Arc<Semaphore>,
    pub rate: RateTable,
}
impl AppState {
    pub fn new(config: Config, pool: PgPool) -> Result<Self> {
        Ok(Self {
            config: Arc::new(config),
            pool,
            http: config::http_client()?,
            image_jobs: Arc::new(Semaphore::new(2)),
            rate: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

async fn guard(State(state): State<AppState>, request: Request, next: Next) -> Result<Response> {
    let immutable_media = request.uri().path().starts_with("/media/");
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        .ok_or_else(|| Error::Internal("Missing peer address".into()))?;
    {
        let mut table = state
            .rate
            .lock()
            .map_err(|_| Error::Internal("Rate limiter unavailable".into()))?;
        let now = Instant::now();
        table.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
        if table.len() >= 10_000 && !table.contains_key(&ip) {
            return Err(Error::RateLimited);
        }
        let (_, count) = table.entry(ip).or_insert((now, 0));
        *count += 1;
        if *count > 120 {
            return Err(Error::RateLimited);
        }
    }
    let mut response = next.run(request).await;
    let successful_media = immutable_media && response.status().is_success();
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if successful_media {
            "public, max-age=31536000, immutable"
        } else {
            "no-store"
        }),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
    );
    if state.config.secure_cookies {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    Ok(response)
}

async fn ready(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok(Json(serde_json::json!({"status":"ready"})))
}

pub fn router(state: AppState) -> Router {
    let media_path = state.config.media_dir.join("public");
    Router::new()
        .route(
            "/health/live",
            get(|| async { Json(serde_json::json!({"status":"alive"})) }),
        )
        .route("/health/ready", get(ready))
        .route("/api/v1/auth/providers", get(auth::providers))
        .route("/api/v1/auth/{provider}/login", get(auth::login))
        .route("/api/v1/auth/{provider}/callback", get(auth::callback))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/types", get(api::types))
        .route("/api/v1/markdown/preview", post(markdown::preview))
        .route("/api/v1/types/{slug}", put(api::save_type))
        .route("/api/v1/contents", get(api::list).post(api::create))
        .route("/api/v1/contents/{id}", get(api::get).put(api::update))
        .route("/api/v1/contents/{id}/revisions", get(api::revisions))
        .route("/api/v1/navigation/{name}", put(api::save_navigation))
        .route("/api/v1/media", get(media::list).post(media::upload))
        .route("/api/v1/public/contents", get(api::public_list))
        .route("/api/v1/public/contents/{slug}", get(api::public_get))
        .route("/api/v1/public/navigation/{name}", get(api::navigation))
        .route("/api/v1/public/media/{id}", get(media::get))
        .route("/api/v1/public/sitemap.xml", get(api::sitemap))
        .route(
            "/api/v1/openapi.json",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/json")],
                    include_str!("../openapi.json"),
                )
            }),
        )
        .route(
            "/llms.txt",
            get(|| async { include_str!("../docs/agents.txt") }),
        )
        .nest_service("/media", ServeDir::new(media_path))
        .fallback(|| async { Error::NotFound.into_response() })
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    fn state() -> AppState {
        AppState::new(
            Config {
                database_url: "postgres://unused".into(),
                bind: "127.0.0.1:8080".parse().unwrap(),
                admin_origin: "http://localhost:3001".into(),
                site_origin: "http://localhost:3000".into(),
                media_dir: "data/test".into(),
                secure_cookies: false,
                providers: vec![],
            },
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost/unused")
                .unwrap(),
        )
        .unwrap()
    }
    fn request(path: &str) -> Request {
        let mut request = Request::builder()
            .uri(path)
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo("127.0.0.1:1234".parse::<SocketAddr>().unwrap()));
        request
    }
    #[tokio::test]
    async fn health_and_anonymous_access() {
        let router = router(state());
        let response = router
            .clone()
            .oneshot(request("/health/live"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let response = router
            .clone()
            .oneshot(request("/api/v1/contents"))
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(
            router.oneshot(request("/missing")).await.unwrap().status(),
            404
        );
    }
    #[tokio::test]
    async fn rejects_cross_origin_cookie_mutation_without_touching_db() {
        let mut req = request("/api/v1/auth/logout");
        *req.method_mut() = axum::http::Method::POST;
        req.headers_mut().insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("rp_session={}", "a".repeat(64))).unwrap(),
        );
        req.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.test"),
        );
        assert_eq!(router(state()).oneshot(req).await.unwrap().status(), 403);
    }
    #[tokio::test]
    async fn bounds_requests_per_peer() {
        let router = router(state());
        for _ in 0..120 {
            assert_eq!(
                router
                    .clone()
                    .oneshot(request("/health/live"))
                    .await
                    .unwrap()
                    .status(),
                200
            );
        }
        assert_eq!(
            router
                .oneshot(request("/health/live"))
                .await
                .unwrap()
                .status(),
            429
        );
    }
    #[test]
    fn openapi_document_is_valid_json() {
        let document: serde_json::Value =
            serde_json::from_str(include_str!("../openapi.json")).unwrap();
        assert_eq!(document["openapi"], "3.1.0");
        assert!(document["paths"].as_object().unwrap().len() >= 14);
    }
}

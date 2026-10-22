use axum::{
    body::Body,
    extract::{ConnectInfo, Request},
    http::{Method, StatusCode, header},
};
use http_body_util::BodyExt;
use rustpress::{
    AppState,
    auth::{hash, random_token},
    config::Config,
    router,
};
use serde_json::{Value, json};
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

async fn call(
    app: &axum::Router,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let mut request = builder.body(Body::from(body.to_string())).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("127.0.0.1:4321".parse::<SocketAddr>().unwrap()));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}))
    };
    (status, value)
}

#[tokio::test]
#[ignore = "Requires dedicated PostgreSQL database. Run npm run test:integration."]
async fn content_lifecycle_and_authorization_are_transactional() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL is required; never target a production database");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::new(
        Config {
            database_url: url,
            bind: "127.0.0.1:8080".parse().unwrap(),
            admin_origin: "http://localhost:3001".into(),
            site_origin: "http://localhost:3000".into(),
            media_dir: directory.path().to_path_buf(),
            secure_cookies: false,
            providers: vec![],
        },
        pool.clone(),
    )
    .unwrap();
    let app = router(state);
    let admin = random_token();
    let agent = random_token();
    let author = random_token();
    let author_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO identities(id,provider,subject,role) VALUES ($1,'google',$2,'author')",
    )
    .bind(author_id)
    .bind(author_id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    for (token, role, identity) in [
        (&admin, "admin", None),
        (&agent, "agent", None),
        (&author, "author", Some(author_id)),
    ] {
        sqlx::query("INSERT INTO credentials(token_hash,role,label,identity_id,expires_at) VALUES ($1,$2,'test',$3,now()+interval '1 hour')")
            .bind(hash(token)).bind(role).bind(identity).execute(&pool).await.unwrap();
    }
    let (status, _) = call(&app, Method::PUT, "/api/v1/types/page", Some(&admin), json!({
        "name":"Page", "fields":[{"key":"subtitle","label":"Subtitle","kind":"text","required":true}]
    })).await;
    assert_eq!(status, StatusCode::OK);
    let mut document = json!({
        "content_type":"page", "slug":"hello-world", "title":"Hello world",
        "blocks":[{"kind":"paragraph","text":"Visible text, not HTML <script>"}],
        "fields":{"subtitle":"Subheading"}, "status":"draft", "publish_at":"2026-01-01T00:00:00Z"
    });
    let (status, created) = call(
        &app,
        Method::POST,
        "/api/v1/contents",
        Some(&agent),
        document.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();
    let path = format!("/api/v1/contents/{id}");
    let (status, _) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents/hello-world",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, Method::GET, &path, Some(&author), Value::Null).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    document["version"] = json!(1);
    document["status"] = json!("published");
    let (status, _) = call(&app, Method::PUT, &path, Some(&agent), document.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, published) = call(&app, Method::PUT, &path, Some(&admin), document.clone()).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(published["version"], 2);
    let (status, _) = call(&app, Method::PUT, &path, Some(&admin), document.clone()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, visible) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents/hello-world",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(visible["title"], "Hello world");
    assert!(visible.get("author_id").is_none());
    let (_, search) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents?q=Visible",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(search["items"].as_array().unwrap().len(), 1);
    let (_, revisions) = call(
        &app,
        Method::GET,
        &format!("{path}/revisions"),
        Some(&admin),
        Value::Null,
    )
    .await;
    assert_eq!(revisions["items"].as_array().unwrap().len(), 2);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit WHERE resource=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "Rejected writes must not add successful audit events"
    );
    document["version"] = json!(2);
    document["publish_at"] = json!("2999-01-01T00:00:00Z");
    assert_eq!(
        call(&app, Method::PUT, &path, Some(&admin), document.clone())
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            Method::GET,
            "/api/v1/public/contents/hello-world",
            None,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/api/v1/types/page",
            Some(&admin),
            json!({"name":"Page","fields":[]})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/api/v1/navigation/main",
            Some(&admin),
            json!([{"label":"Bad","href":"javascript:alert(1)"}])
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    document["version"] = json!(3);
    document["status"] = json!("draft");
    assert_eq!(
        call(&app, Method::PUT, &path, Some(&agent), document.clone())
            .await
            .0,
        StatusCode::FORBIDDEN,
        "Agents must not unpublish existing documents"
    );
    document["status"] = json!("archived");
    assert_eq!(
        call(&app, Method::PUT, &path, Some(&admin), document)
            .await
            .0,
        StatusCode::OK
    );
    sqlx::query("UPDATE credentials SET expires_at=now()-interval '1 second' WHERE token_hash=$1")
        .bind(hash(&agent))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            Method::GET,
            "/api/v1/contents",
            Some(&agent),
            Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    pool.close().await;
}

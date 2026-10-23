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
    let mut upgrade = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0001_core.sql"))
        .execute(&mut *upgrade)
        .await
        .unwrap();
    let legacy_id = Uuid::new_v4();
    sqlx::query("INSERT INTO contents(id,content_type,slug,title,blocks,status) VALUES ($1,'page','legacy','Legacy',$2,'draft')")
        .bind(legacy_id).bind(json!([{"kind":"paragraph","text":"Preserved"}])).execute(&mut *upgrade).await.unwrap();
    let old_snapshot: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM contents c WHERE id=$1")
        .bind(legacy_id)
        .fetch_one(&mut *upgrade)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0002_translations_markdown.sql"))
        .execute(&mut *upgrade)
        .await
        .unwrap();
    let upgraded: rustpress::model::Content = sqlx::query_as("SELECT * FROM contents WHERE id=$1")
        .bind(legacy_id)
        .fetch_one(&mut *upgrade)
        .await
        .unwrap();
    assert_eq!(upgraded.locale, "it");
    assert_eq!(upgraded.translation_group, legacy_id);
    assert_eq!(upgraded.markdown, None);
    assert_eq!(upgraded.blocks[0]["text"], "Preserved");
    let old_revision: rustpress::model::Content = serde_json::from_value(old_snapshot).unwrap();
    assert!(
        old_revision.input().unwrap().markdown.is_none(),
        "Pre-migration revision must still deserialize"
    );
    upgrade.rollback().await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    tokio::fs::create_dir(directory.path().join("public"))
        .await
        .unwrap();
    tokio::fs::create_dir(directory.path().join("originals"))
        .await
        .unwrap();
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
    let editor = random_token();
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
        (&editor, "editor", None),
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
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(800, 400)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let boundary = "rustpress-test-boundary";
    let mut multipart = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"alt\"\r\n\r\nTest image\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"untrusted.png\"\r\n\
         Content-Type: image/png\r\n\r\n"
    ).into_bytes();
    multipart.extend_from_slice(png.get_ref());
    multipart.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/media")
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(multipart))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("127.0.0.1:4321".parse::<SocketAddr>().unwrap()));
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let uploaded: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(uploaded["width"], 800);
    assert_eq!(uploaded["height"], 400);
    assert_eq!(uploaded["variants"].as_array().unwrap().len(), 3);
    let asset = uploaded["variants"][0]["url"].as_str().unwrap();
    let mut request = Request::builder().uri(asset).body(Body::empty()).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("127.0.0.1:4321".parse::<SocketAddr>().unwrap()));
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (320, 160));
    let original = format!(
        "/media/../originals/{}.bin",
        uploaded["id"].as_str().unwrap()
    );
    assert_ne!(
        call(&app, Method::GET, &original, None, Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM media")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);
    let mut localized = json!({
        "content_type":"post", "slug":"markdown", "title":"Italiano", "locale":"it",
        "markdown":"## Titolo\n\n**Testo** e [link](javascript:alert%281%29)\n\n<script>alert(1)</script>",
        "fields":{"local-note":"Nota"},
        "extra_fields":[{"key":"local-note","label":"Note","kind":"text","required":false}],
        "status":"published", "publish_at":"2026-01-01T00:00:00Z"
    });
    let (status, italian) = call(
        &app,
        Method::POST,
        "/api/v1/contents",
        Some(&admin),
        localized.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{italian}");
    let italian_id = italian["id"].as_str().unwrap();
    let (_, public_it) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents/markdown",
        None,
        Value::Null,
    )
    .await;
    assert!(
        public_it["body_html"]
            .as_str()
            .unwrap()
            .contains("<strong>Testo</strong>")
    );
    assert!(!public_it["body_html"].as_str().unwrap().contains("<script"));
    assert!(
        !public_it["body_html"]
            .as_str()
            .unwrap()
            .contains("href=\"javascript:")
    );
    let stored_markdown: String = sqlx::query_scalar("SELECT markdown FROM contents WHERE id=$1")
        .bind(Uuid::parse_str(italian_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_markdown, localized["markdown"].as_str().unwrap());
    localized["translation_of"] = json!(italian_id);
    localized["locale"] = json!("en");
    localized["title"] = json!("English");
    localized["markdown"] = json!("## Heading\n\n**English** translation");
    localized["status"] = json!("draft");
    assert_eq!(
        call(
            &app,
            Method::POST,
            "/api/v1/contents",
            Some(&author),
            localized.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, english) = call(
        &app,
        Method::POST,
        "/api/v1/contents",
        Some(&editor),
        localized.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{english}");
    assert_eq!(english["translation_group"], italian["translation_group"]);
    assert_eq!(
        call(
            &app,
            Method::GET,
            "/api/v1/public/contents/markdown?locale=en",
            None,
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, public_it) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents/markdown",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(
        public_it["translations"].as_array().unwrap().len(),
        1,
        "Do not disclose draft translations"
    );
    localized["slug"] = json!("another-slug");
    assert_eq!(
        call(
            &app,
            Method::POST,
            "/api/v1/contents",
            Some(&admin),
            localized.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT,
        "Only one translation per group/locale"
    );
    localized["slug"] = json!("markdown");
    localized.as_object_mut().unwrap().remove("translation_of");
    localized["version"] = json!(1);
    localized["status"] = json!("published");
    let english_path = format!("/api/v1/contents/{}", english["id"].as_str().unwrap());
    assert_eq!(
        call(
            &app,
            Method::PUT,
            &english_path,
            Some(&admin),
            localized.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, public_it) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents/markdown",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(public_it["translations"].as_array().unwrap().len(), 2);
    let (_, en) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents/markdown?locale=en",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(en["title"], "English");
    let (_, en_list) = call(
        &app,
        Method::GET,
        "/api/v1/public/contents?locale=en&q=translation",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(
        en_list["items"].as_array().unwrap().len(),
        1,
        "Search must include Markdown text and respect locale"
    );
    assert_eq!(
        call(
            &app,
            Method::GET,
            "/api/v1/public/contents?locale=xx",
            None,
            Value::Null
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    localized["version"] = json!(2);
    localized["locale"] = json!("it");
    assert_eq!(
        call(
            &app,
            Method::PUT,
            &english_path,
            Some(&admin),
            localized.clone()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    localized["locale"] = json!("en");
    localized["blocks"] = json!([{"kind":"paragraph","text":"ambiguous"}]);
    assert_eq!(
        call(&app, Method::PUT, &english_path, Some(&admin), localized)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/api/v1/navigation/main?locale=en",
            Some(&admin),
            json!([{"label":"English menu","href":"/en","children":[]}])
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, en_menu) = call(
        &app,
        Method::GET,
        "/api/v1/public/navigation/main?locale=en",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(en_menu[0]["label"], "English menu");
    let (_, it_menu) = call(
        &app,
        Method::GET,
        "/api/v1/public/navigation/main",
        None,
        Value::Null,
    )
    .await;
    assert_eq!(it_menu, json!([]));
    let (_, preview) = call(
        &app,
        Method::POST,
        "/api/v1/markdown/preview",
        Some(&admin),
        json!({"markdown":"**Safe** <img src=x onerror=alert(1)>"}),
    )
    .await;
    assert!(
        preview["body_html"]
            .as_str()
            .unwrap()
            .contains("<strong>Safe</strong>")
    );
    assert!(
        !preview["body_html"]
            .as_str()
            .unwrap()
            .contains("<img src=x")
    );
    assert_eq!(
        call(
            &app,
            Method::POST,
            "/api/v1/markdown/preview",
            None,
            json!({"markdown":"private"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    pool.close().await;
}

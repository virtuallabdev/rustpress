use crate::{
    AppState,
    auth::Actor,
    error::{Error, Result, invalid},
    model::*,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Deserialize, Default)]
pub struct PageQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub q: Option<String>,
    pub content_type: Option<String>,
}
impl PageQuery {
    fn validate(&self) -> Result<(i64, i64, &str, &str)> {
        let limit = self.limit.unwrap_or(20);
        let offset = self.offset.unwrap_or(0);
        let q = self.q.as_deref().unwrap_or("");
        let kind = self.content_type.as_deref().unwrap_or("");
        if !(1..=100).contains(&limit)
            || !(0..=100_000).contains(&offset)
            || q.len() > 200
            || kind.len() > 120
        {
            return Err(invalid("Invalid pagination or search query"));
        }
        Ok((limit, offset, q, kind))
    }
}

pub async fn types(State(state): State<AppState>, _actor: Actor) -> Result<Json<Vec<ContentType>>> {
    Ok(Json(
        sqlx::query_as("SELECT * FROM content_types ORDER BY slug")
            .fetch_all(&state.pool)
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeInput {
    pub name: String,
    pub fields: Vec<Field>,
}

pub async fn save_type(
    State(state): State<AppState>,
    actor: Actor,
    Path(slug): Path<String>,
    Json(input): Json<TypeInput>,
) -> Result<Json<Value>> {
    actor.admin()?;
    if !slug_valid(&slug) || input.name.trim().is_empty() || input.name.len() > 120 {
        return Err(invalid("Invalid content type"));
    }
    validate_schema(&input.fields)?;
    let mut tx = state.pool.begin().await?;
    // Serialize schema changes with content writes so no document bypasses validation.
    sqlx::query("LOCK TABLE content_types IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let existing: Vec<Content> = sqlx::query_as("SELECT * FROM contents WHERE content_type=$1")
        .bind(&slug)
        .fetch_all(&mut *tx)
        .await?;
    for content in existing {
        validate_content(&content.input()?, &input.fields).map_err(|_| {
            invalid("Schema would invalidate existing content; migrate its values first")
        })?;
    }
    sqlx::query("INSERT INTO content_types(slug,name,fields) VALUES ($1,$2,$3) ON CONFLICT(slug) DO UPDATE SET name=$2,fields=$3")
        .bind(&slug).bind(input.name).bind(json!(input.fields)).execute(&mut *tx).await?;
    audit(&mut tx, &actor, "type.save", &slug).await?;
    tx.commit().await?;
    Ok(Json(json!({"slug": slug})))
}

pub async fn list(
    State(state): State<AppState>,
    actor: Actor,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>> {
    let (limit, offset, q, kind) = query.validate()?;
    let rows: Vec<Content> = sqlx::query_as(
        "SELECT * FROM contents WHERE ($1='' OR content_type=$1)
         AND ($2='' OR to_tsvector('simple', title || ' ' || blocks::text) @@ plainto_tsquery('simple',$2))
         AND ($3 <> 'author' OR author_id=$4) ORDER BY updated_at DESC,id LIMIT $5 OFFSET $6")
        .bind(kind).bind(q).bind(&actor.role).bind(actor.identity_id).bind(limit).bind(offset).fetch_all(&state.pool).await?;
    Ok(Json(
        json!({"items": rows, "limit": limit, "offset": offset}),
    ))
}

pub async fn get(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<Json<Content>> {
    let content = sqlx::query_as::<_, Content>("SELECT * FROM contents WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(Error::NotFound)?;
    actor.can_own(content.author_id)?;
    Ok(Json(content))
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    action: &str,
    resource: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO audit(actor,action,resource) VALUES ($1,$2,$3)")
        .bind(actor.audit_id())
        .bind(action)
        .bind(resource)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn validate_refs(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &ContentInput,
    fields: &[Field],
) -> Result<()> {
    let mut media_ids: Vec<Uuid> = input
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Image { media_id, .. } => Some(*media_id),
            _ => None,
        })
        .collect();
    for field in fields.iter().chain(&input.extra_fields) {
        if let Some(value) = input.fields.get(&field.key).and_then(Value::as_str) {
            match field.kind {
                FieldKind::Media => media_ids
                    .push(Uuid::parse_str(value).map_err(|_| invalid("Invalid media UUID"))?),
                FieldKind::Relation => {
                    let exists: bool =
                        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM contents WHERE id=$1)")
                            .bind(
                                Uuid::parse_str(value)
                                    .map_err(|_| invalid("Invalid relation UUID"))?,
                            )
                            .fetch_one(&mut **tx)
                            .await?;
                    if !exists {
                        return Err(invalid("Related content does not exist"));
                    }
                }
                _ => {}
            }
        }
    }
    for id in media_ids {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media WHERE id=$1)")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        if !exists {
            return Err(invalid("Referenced media does not exist"));
        }
    }
    Ok(())
}

async fn save(
    state: AppState,
    actor: Actor,
    id: Option<Uuid>,
    input: ContentInput,
) -> Result<Content> {
    let mut tx = state.pool.begin().await?;
    let schema: Value =
        sqlx::query_scalar("SELECT fields FROM content_types WHERE slug=$1 FOR SHARE")
            .bind(&input.content_type)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| invalid("Unknown content type"))?;
    let schema: Vec<Field> =
        serde_json::from_value(schema).map_err(|e| Error::Internal(e.to_string()))?;
    validate_content(&input, &schema)?;
    let previous = if let Some(id) = id {
        Some(
            sqlx::query_as::<_, Content>("SELECT * FROM contents WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?,
        )
    } else {
        None
    };
    if let Some(previous) = &previous {
        actor.can_own(previous.author_id)?;
        if input.version != Some(previous.version) {
            return Err(Error::Conflict);
        }
        if previous.content_type != input.content_type {
            return Err(invalid("Content type cannot change"));
        }
        if previous.status != "draft" {
            actor.editorial()?;
        }
    } else if input.version.is_some() {
        return Err(invalid("New content cannot have a version"));
    }
    if input.status != "draft" {
        actor.editorial()?;
    }
    if actor.role != "admin" {
        let before = previous
            .as_ref()
            .map(|c| c.extra_fields.clone())
            .unwrap_or(json!([]));
        if before != json!(input.extra_fields) {
            return Err(Error::Forbidden);
        }
    }
    validate_refs(&mut tx, &input, &schema).await?;
    let id = id.unwrap_or_else(Uuid::new_v4);
    let content = sqlx::query_as::<_, Content>(
        "INSERT INTO contents(id,content_type,slug,title,blocks,fields,extra_fields,seo,status,publish_at,author_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         ON CONFLICT(id) DO UPDATE SET slug=$3,title=$4,blocks=$5,fields=$6,extra_fields=$7,seo=$8,status=$9,
         publish_at=$10,version=contents.version+1,updated_at=now() RETURNING *")
        .bind(id).bind(input.content_type).bind(input.slug).bind(input.title)
        .bind(json!(input.blocks)).bind(json!(input.fields)).bind(json!(input.extra_fields)).bind(json!(input.seo))
        .bind(input.status).bind(input.publish_at).bind(actor.identity_id).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO revisions(content_id,version,snapshot) VALUES ($1,$2,$3)")
        .bind(id)
        .bind(content.version)
        .bind(json!(content))
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        &actor,
        if previous.is_some() {
            "content.update"
        } else {
            "content.create"
        },
        &id.to_string(),
    )
    .await?;
    tx.commit().await?;
    Ok(content)
}

pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Json(input): Json<ContentInput>,
) -> Result<(StatusCode, Json<Content>)> {
    Ok((
        StatusCode::CREATED,
        Json(save(state, actor, None, input).await?),
    ))
}
pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Json(input): Json<ContentInput>,
) -> Result<Json<Content>> {
    Ok(Json(save(state, actor, Some(id), input).await?))
}
pub async fn revisions(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>> {
    let content = sqlx::query_as::<_, Content>("SELECT * FROM contents WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(Error::NotFound)?;
    actor.can_own(content.author_id)?;
    let (limit, offset, _, _) = query.validate()?;
    let rows: Vec<(i64, i32, Value, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id,version,snapshot,created_at FROM revisions WHERE content_id=$1 ORDER BY version DESC LIMIT $2 OFFSET $3")
        .bind(id).bind(limit).bind(offset).fetch_all(&state.pool).await?;
    Ok(Json(
        json!({"items": rows.into_iter().map(|(id,version,snapshot,created_at)| json!({
        "id":id, "version":version, "snapshot":snapshot, "created_at":created_at
    })).collect::<Vec<_>>(), "limit": limit, "offset":offset}),
    ))
}

fn public_content(content: Content) -> Value {
    json!({
        "id":content.id, "content_type":content.content_type, "slug":content.slug, "title":content.title,
        "blocks":content.blocks, "fields":content.fields, "seo":content.seo,
        "publish_at":content.publish_at, "updated_at":content.updated_at
    })
}

pub async fn public_list(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>> {
    let (limit, offset, q, kind) = query.validate()?;
    let rows: Vec<Content> = sqlx::query_as(
        "SELECT * FROM contents WHERE status='published' AND publish_at<=now() AND ($1='' OR content_type=$1)
         AND ($2='' OR to_tsvector('simple',title || ' ' || blocks::text) @@ plainto_tsquery('simple',$2))
         ORDER BY publish_at DESC,id LIMIT $3 OFFSET $4")
        .bind(kind).bind(q).bind(limit).bind(offset).fetch_all(&state.pool).await?;
    Ok(Json(
        json!({"items":rows.into_iter().map(public_content).collect::<Vec<_>>(),"limit":limit,"offset":offset}),
    ))
}
pub async fn public_get(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>> {
    let content = sqlx::query_as::<_, Content>(
        "SELECT * FROM contents WHERE slug=$1 AND status='published' AND publish_at<=now()",
    )
    .bind(slug)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(Json(public_content(content)))
}

pub async fn navigation(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<NavItem>>> {
    let items: Value = sqlx::query_scalar("SELECT items FROM navigation WHERE name=$1")
        .bind(name)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(Error::NotFound)?;
    Ok(Json(
        serde_json::from_value(items).map_err(|e| Error::Internal(e.to_string()))?,
    ))
}
pub async fn save_navigation(
    State(state): State<AppState>,
    actor: Actor,
    Path(name): Path<String>,
    Json(items): Json<Vec<NavItem>>,
) -> Result<StatusCode> {
    actor.editorial()?;
    if !slug_valid(&name) {
        return Err(invalid("Invalid navigation name"));
    }
    validate_navigation(&items, 0)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO navigation(name,items) VALUES($1,$2) ON CONFLICT(name) DO UPDATE SET items=$2",
    )
    .bind(&name)
    .bind(json!(items))
    .execute(&mut *tx)
    .await?;
    audit(&mut tx, &actor, "navigation.save", &name).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn sitemap(State(state): State<AppState>) -> Result<axum::response::Response> {
    use axum::{http::header, response::IntoResponse};
    let rows: Vec<(String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT slug,updated_at FROM contents WHERE status='published' AND publish_at<=now()
         AND COALESCE((seo->>'noindex')::boolean,false)=false ORDER BY slug LIMIT 50000",
    )
    .fetch_all(&state.pool)
    .await?;
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">",
    );
    for (slug, updated) in rows {
        xml.push_str(&format!(
            "<url><loc>{}/{slug}</loc><lastmod>{}</lastmod></url>",
            state.config.site_origin,
            updated.to_rfc3339()
        ));
    }
    xml.push_str("</urlset>");
    Ok((
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        xml,
    )
        .into_response())
}

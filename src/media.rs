use crate::{
    AppState,
    api::PageQuery,
    auth::Actor,
    error::{Error, Result, invalid},
};
use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
};
use image::{ImageDecoder, ImageFormat, ImageReader, imageops::FilterType};
use serde_json::{Value, json};
use std::io::Cursor;
use uuid::Uuid;

const MAX_PIXELS: u64 = 25_000_000;
const WIDTHS: [u32; 4] = [320, 640, 1280, 1920];
type EncodedImages = (u32, u32, Vec<(u32, Vec<u8>)>);

fn transform(bytes: Vec<u8>) -> Result<EncodedImages> {
    let mut reader = ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)
    ) {
        return Err(invalid("Only JPEG, PNG and WebP images are accepted"));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let (width, height) = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()
        .map_err(|_| invalid("Cannot read image dimensions"))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(invalid("Image exceeds the 25 megapixel limit"));
    }
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| invalid("Invalid image or decoder resource limit exceeded"))?;
    let orientation = decoder
        .orientation()
        .map_err(|_| invalid("Invalid image orientation"))?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)
        .map_err(|_| invalid("Invalid image or decoder resource limit exceeded"))?;
    decoded.apply_orientation(orientation);
    let (width, height) = (decoded.width(), decoded.height());
    let mut variants = Vec::new();
    for target in WIDTHS {
        let target = target.min(width);
        if variants.last().is_some_and(|(w, _)| *w == target) {
            continue;
        }
        let resized = decoded.resize(target, height, FilterType::Lanczos3);
        let mut output = Cursor::new(Vec::new());
        resized
            .write_to(&mut output, ImageFormat::WebP)
            .map_err(|e| Error::Internal(e.to_string()))?;
        variants.push((resized.width(), output.into_inner()));
    }
    Ok((width, height, variants))
}

pub async fn upload(
    State(state): State<AppState>,
    actor: Actor,
    mut multipart: Multipart,
) -> Result<Json<Value>> {
    // An agent may prepare textual drafts, but cannot publish public media assets.
    if actor.role == "agent" {
        return Err(Error::Forbidden);
    }
    let permit = state
        .image_jobs
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::RateLimited)?;
    let mut bytes = None;
    let mut alt = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| invalid("Invalid multipart upload"))?
    {
        match field.name() {
            Some("file") if bytes.is_none() => {
                bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| invalid("Upload exceeds the size limit"))?
                        .to_vec(),
                );
            }
            Some("alt") if alt.is_none() => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| invalid("Invalid alternative text"))?;
                if text.trim().is_empty() || text.len() > 500 {
                    return Err(invalid("Alternative text is required, maximum 500 bytes"));
                }
                alt = Some(text);
            }
            _ => return Err(invalid("Expected exactly one file and one alt field")),
        }
    }
    let bytes = bytes.ok_or_else(|| invalid("Missing image file"))?;
    let alt = alt.ok_or_else(|| invalid("Missing alternative text"))?;
    let original = bytes.clone();
    let transformed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        transform(bytes)
    })
    .await
    .map_err(|e| Error::Internal(e.to_string()))??;
    let (width, height, variants) = transformed;
    let id = Uuid::new_v4();
    let directory = state.config.media_dir.join("public").join(id.to_string());
    let original_path = state
        .config
        .media_dir
        .join("originals")
        .join(format!("{id}.bin"));
    tokio::fs::create_dir(&directory).await?;
    let result: Result<Value> = async {
        tokio::fs::write(&original_path, original).await?;
        let mut urls = Vec::new();
        for (width, bytes) in variants {
            tokio::fs::write(directory.join(format!("{width}.webp")), bytes).await?;
            urls.push(json!({"width":width,"url":format!("/media/{id}/{width}.webp")}));
        }
        let mut tx = state.pool.begin().await?;
        sqlx::query("INSERT INTO media(id,alt,width,height,variants) VALUES ($1,$2,$3,$4,$5)")
            .bind(id)
            .bind(&alt)
            .bind(width as i32)
            .bind(height as i32)
            .bind(json!(urls))
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit(actor,action,resource) VALUES ($1,'media.upload',$2)")
            .bind(actor.audit_id())
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(json!({"id":id,"alt":alt,"width":width,"height":height,"variants":urls}))
    }
    .await;
    if result.is_err() {
        if let Err(e) = tokio::fs::remove_dir_all(&directory).await {
            tracing::error!(error=%e, media_id=%id, "Failed to clean up incomplete public image");
        }
        if let Err(e) = tokio::fs::remove_file(&original_path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::error!(error=%e, media_id=%id, "Failed to clean up original image");
        }
    }
    result.map(Json)
}

pub async fn get(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<Value>> {
    let row: Option<(Uuid, String, i32, i32, Value)> =
        sqlx::query_as("SELECT id,alt,width,height,variants FROM media WHERE id=$1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let (id, alt, width, height, variants) = row.ok_or(Error::NotFound)?;
    Ok(Json(
        json!({"id":id,"alt":alt,"width":width,"height":height,"variants":variants}),
    ))
}
pub async fn list(
    State(state): State<AppState>,
    _actor: Actor,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>> {
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || !(0..=100_000).contains(&offset) {
        return Err(invalid("Invalid pagination"));
    }
    let items: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(m) FROM media m ORDER BY created_at DESC,id LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({"items":items,"limit":limit,"offset":offset})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    #[test]
    fn rejects_non_images() {
        assert!(transform(b"<svg onload='alert(1)'/>".to_vec()).is_err());
    }
    #[test]
    fn resizes_without_upscaling_and_reencodes() {
        let img = image::DynamicImage::new_rgb8(800, 400);
        let mut data = Cursor::new(Vec::new());
        img.write_to(&mut data, ImageFormat::Png).unwrap();
        let (w, h, variants) = transform(data.into_inner()).unwrap();
        assert_eq!((w, h), (800, 400));
        assert_eq!(
            variants.iter().map(|v| v.0).collect::<Vec<_>>(),
            vec![320, 640, 800]
        );
        for (width, bytes) in variants {
            let decoded = image::load_from_memory(&bytes).unwrap();
            assert_eq!(decoded.dimensions(), (width, width / 2));
            assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::WebP);
        }
    }
    #[test]
    fn enforces_pixel_limit_before_full_decode() {
        let img = image::DynamicImage::new_luma8(5100, 5000);
        let mut data = Cursor::new(Vec::new());
        img.write_to(&mut data, ImageFormat::Png).unwrap();
        assert!(transform(data.into_inner()).is_err());
    }
}

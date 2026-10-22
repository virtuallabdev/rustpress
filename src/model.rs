use crate::error::{Result, invalid};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Paragraph { text: String },
    Heading { text: String, level: u8 },
    Image { media_id: Uuid, alt: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Number,
    Boolean,
    Date,
    Media,
    Relation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Seo {
    #[serde(default)]
    pub description: String,
    pub canonical: Option<String>,
    #[serde(default)]
    pub noindex: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentInput {
    pub content_type: String,
    pub slug: String,
    pub title: String,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    pub extra_fields: Vec<Field>,
    #[serde(default)]
    pub seo: Seo,
    pub status: String,
    pub publish_at: DateTime<Utc>,
    pub version: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, sqlx::FromRow)]
pub struct Content {
    pub id: Uuid,
    pub content_type: String,
    pub slug: String,
    pub title: String,
    pub blocks: Value,
    pub fields: Value,
    pub extra_fields: Value,
    pub seo: Value,
    pub status: String,
    pub publish_at: DateTime<Utc>,
    pub author_id: Option<Uuid>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct ContentType {
    pub slug: String,
    pub name: String,
    pub fields: Value,
}

impl Content {
    pub fn input(&self) -> Result<ContentInput> {
        Ok(ContentInput {
            content_type: self.content_type.clone(),
            slug: self.slug.clone(),
            title: self.title.clone(),
            blocks: serde_json::from_value(self.blocks.clone())
                .map_err(|e| crate::error::Error::Internal(e.to_string()))?,
            fields: serde_json::from_value(self.fields.clone())
                .map_err(|e| crate::error::Error::Internal(e.to_string()))?,
            extra_fields: serde_json::from_value(self.extra_fields.clone())
                .map_err(|e| crate::error::Error::Internal(e.to_string()))?,
            seo: serde_json::from_value(self.seo.clone())
                .map_err(|e| crate::error::Error::Internal(e.to_string()))?,
            status: self.status.clone(),
            publish_at: self.publish_at,
            version: Some(self.version),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NavItem {
    pub label: String,
    pub href: String,
    #[serde(default)]
    pub children: Vec<NavItem>,
}

pub fn slug_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn safe_url(value: &str) -> bool {
    if value.starts_with('/') {
        return !value.starts_with("//")
            && !value.contains('\\')
            && !value.chars().any(char::is_control);
    }
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}

pub fn validate_schema(fields: &[Field]) -> Result<()> {
    if fields.len() > 64 {
        return Err(invalid("At most 64 custom fields are allowed"));
    }
    let mut keys = std::collections::HashSet::new();
    for field in fields {
        if !slug_valid(&field.key)
            || field.label.trim().is_empty()
            || field.label.len() > 120
            || !keys.insert(&field.key)
        {
            return Err(invalid("Invalid or duplicate custom field definition"));
        }
    }
    Ok(())
}

pub fn validate_content(input: &ContentInput, inherited: &[Field]) -> Result<()> {
    if !slug_valid(&input.slug) || !slug_valid(&input.content_type) {
        return Err(invalid(
            "Slugs must use lowercase ASCII letters, digits and hyphens",
        ));
    }
    if input.title.trim().is_empty() || input.title.len() > 300 || input.blocks.len() > 500 {
        return Err(invalid("Invalid title or too many blocks"));
    }
    if !["draft", "published", "archived"].contains(&input.status.as_str()) {
        return Err(invalid("Invalid publication status"));
    }
    if input.seo.description.len() > 500
        || input.seo.canonical.as_ref().is_some_and(|s| !safe_url(s))
    {
        return Err(invalid("Invalid SEO metadata"));
    }
    for block in &input.blocks {
        match block {
            Block::Paragraph { text } if text.len() <= 50_000 => {}
            Block::Heading { text, level } if text.len() <= 500 && (2..=6).contains(level) => {}
            Block::Image { alt, .. } if alt.len() <= 500 => {}
            _ => return Err(invalid("Invalid block")),
        }
    }
    let fields: Vec<_> = inherited
        .iter()
        .chain(&input.extra_fields)
        .cloned()
        .collect();
    validate_schema(&fields)?;
    for field in &fields {
        let value = input.fields.get(&field.key).filter(|v| !v.is_null());
        if field.required && value.is_none() {
            return Err(invalid(format!("Missing field: {}", field.key)));
        }
        if let Some(value) = value {
            let valid = match field.kind {
                FieldKind::Text => value.as_str().is_some_and(|s| s.len() <= 50_000),
                FieldKind::Number => value.is_number(),
                FieldKind::Boolean => value.is_boolean(),
                FieldKind::Date => value
                    .as_str()
                    .is_some_and(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()),
                FieldKind::Media | FieldKind::Relation => {
                    value.as_str().is_some_and(|s| Uuid::parse_str(s).is_ok())
                }
            };
            if !valid {
                return Err(invalid(format!("Invalid field: {}", field.key)));
            }
        }
    }
    if input
        .fields
        .keys()
        .any(|key| !fields.iter().any(|f| &f.key == key))
    {
        return Err(invalid("Undeclared custom field"));
    }
    Ok(())
}

pub fn validate_navigation(items: &[NavItem], depth: usize) -> Result<()> {
    if depth > 3 || items.len() > 30 {
        return Err(invalid("Navigation exceeds size or depth limit"));
    }
    for item in items {
        if item.label.trim().is_empty() || item.label.len() > 120 || !safe_url(&item.href) {
            return Err(invalid("Invalid navigation link"));
        }
        validate_navigation(&item.children, depth + 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_script_and_protocol_relative_urls() {
        for value in [
            "javascript:alert(1)",
            "//evil.test",
            "/\\evil.test",
            "/\nevil",
            "http://evil.test",
            "https://user:pass@example.com",
        ] {
            assert!(!safe_url(value), "{value}");
        }
        assert!(safe_url("/about"));
        assert!(safe_url("https://example.com/about"));
    }
    #[test]
    fn validates_custom_fields_and_required_values() {
        let mut input: ContentInput = serde_json::from_value(serde_json::json!({
            "content_type": "page", "slug": "home", "title": "Home", "blocks": [],
            "fields": {}, "status": "draft", "publish_at": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        let fields = vec![Field {
            key: "price".into(),
            label: "Price".into(),
            kind: FieldKind::Number,
            required: true,
        }];
        assert!(validate_content(&input, &fields).is_err());
        input.fields.insert("price".into(), serde_json::json!(12.5));
        assert!(validate_content(&input, &fields).is_ok());
        input
            .fields
            .insert("price".into(), serde_json::json!("12.5"));
        assert!(validate_content(&input, &fields).is_err());
        input.fields.clear();
        input.fields.insert("unknown".into(), Value::Bool(true));
        assert!(validate_content(&input, &[]).is_err());
    }
    #[test]
    fn rejects_field_override_and_invalid_heading() {
        let field = Field {
            key: "price".into(),
            label: "Price".into(),
            kind: FieldKind::Text,
            required: false,
        };
        assert!(validate_schema(&[field.clone(), field]).is_err());
        assert!(!slug_valid("../secret"));
        assert!(!slug_valid("-draft"));
        let input: ContentInput = serde_json::from_value(serde_json::json!({
            "content_type":"post", "slug":"hello", "title":"Hello", "blocks":[{"kind":"heading","text":"Title","level":1}],
            "status":"draft", "publish_at":"2026-01-01T00:00:00Z"
        })).unwrap();
        assert!(validate_content(&input, &[]).is_err());
    }
}

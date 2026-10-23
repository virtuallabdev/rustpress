use crate::{
    AppState,
    auth::Actor,
    error::{Result, invalid},
    model::safe_url,
};
use axum::{Json, extract::State};
use pulldown_cmark::{Event, Options, Parser, Tag, html};
use serde::Deserialize;
use serde_json::{Value, json};

fn image_url(value: &str) -> bool {
    let Some(value) = value.strip_prefix("/media/") else {
        return false;
    };
    let Some((id, filename)) = value.split_once('/') else {
        return false;
    };
    uuid::Uuid::parse_str(id).is_ok()
        && filename
            .strip_suffix(".webp")
            .is_some_and(|w| w.parse::<u32>().is_ok_and(|w| (1..=1920).contains(&w)))
}

pub fn render(source: &str) -> String {
    let parser = Parser::new_ext(
        source,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    )
    .map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let safe = safe_url(&dest_url) || dest_url.starts_with('#');
            Event::Start(Tag::Link {
                link_type,
                dest_url: if safe { dest_url } else { "#".into() },
                title,
                id,
            })
        }
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: if image_url(&dest_url) {
                dest_url
            } else {
                "".into()
            },
            title,
            id,
        }),
        event => event,
    });
    let mut output = String::new();
    html::push_html(&mut output, parser);
    // The allowlist is defense in depth after disabling raw HTML and unsafe URLs.
    ammonia::Builder::default()
        .add_tag_attributes("code", ["class"])
        .tags(
            [
                "p",
                "br",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
                "strong",
                "em",
                "del",
                "blockquote",
                "ul",
                "ol",
                "li",
                "pre",
                "code",
                "hr",
                "a",
                "img",
                "table",
                "thead",
                "tbody",
                "tr",
                "th",
                "td",
            ]
            .into(),
        )
        .url_schemes(["https"].into())
        .clean(&output)
        .to_string()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    markdown: String,
}

pub async fn preview(
    State(_state): State<AppState>,
    _actor: Actor,
    Json(input): Json<Preview>,
) -> Result<Json<Value>> {
    if input.markdown.len() > 500_000 {
        return Err(invalid("Markdown exceeds 500000 bytes"));
    }
    Ok(Json(json!({"body_html":render(&input.markdown)})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_commonmark_without_executable_html() {
        let output = render(
            "## Title\n\n**Bold** and *italic*\n\n- One\n- Two\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>",
        );
        assert!(output.contains("<h2>Title</h2>"));
        assert!(output.contains("<strong>Bold</strong>"));
        assert!(output.contains("<ul>"));
        assert!(!output.contains("<script"));
        assert!(!output.contains("<img src=x"));
        assert!(output.contains("&lt;script&gt;"));
    }
    #[test]
    fn rejects_script_links_tracking_images_and_html_attributes() {
        for source in [
            "[x](javascript:alert%281%29)",
            "[x](data:text/html,x)",
            "<iframe src='https://evil.test'></iframe>",
            "![x](https://evil.test/track.png)",
        ] {
            let html = render(source);
            assert!(!html.contains("href=\"javascript:"));
            assert!(!html.contains("href=\"data:"));
            assert!(!html.contains("<iframe"));
            assert!(!html.contains("src=\"https://evil"));
        }
        let source = "![Alt](/media/00000000-0000-4000-8000-000000000001/320.webp)";
        assert!(render(source).contains("src=\"/media/"));
    }
}

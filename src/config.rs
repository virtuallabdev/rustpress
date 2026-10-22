use crate::error::{Result, invalid};
use std::{env, net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Clone)]
pub struct Provider {
    pub name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorize: String,
    pub token: String,
    pub userinfo: String,
    pub scope: String,
}

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    pub admin_origin: String,
    pub site_origin: String,
    pub media_dir: PathBuf,
    pub secure_cookies: bool,
    pub providers: Vec<Provider>,
}

fn required(key: &str) -> Result<String> {
    env::var(key)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid(format!("Missing environment variable: {key}")))
}

fn origin(value: String, dev: bool) -> Result<String> {
    let url = url::Url::parse(&value).map_err(|_| invalid("Invalid origin URL"))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1"));
    if url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !(url.scheme() == "https" || dev && local && url.scheme() == "http")
    {
        return Err(invalid(
            "Origins must be HTTPS, or localhost HTTP in development, without paths",
        ));
    }
    Ok(url.origin().ascii_serialization())
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let development = match env::var("RUSTPRESS_ENV").as_deref() {
            Ok("development") => true,
            Ok("production") | Err(_) => false,
            _ => return Err(invalid("RUSTPRESS_ENV must be development or production")),
        };
        let mut providers = Vec::new();
        for name in ["google", "microsoft", "meta"] {
            let prefix = name.to_ascii_uppercase();
            let id = env::var(format!("{prefix}_CLIENT_ID")).unwrap_or_default();
            let secret = env::var(format!("{prefix}_CLIENT_SECRET")).unwrap_or_default();
            if id.is_empty() && secret.is_empty() {
                continue;
            }
            if id.is_empty() || secret.is_empty() {
                return Err(invalid(format!("Incomplete {name} OAuth configuration")));
            }
            let (authorize, token, userinfo, scope) = match name {
                "google" => (
                    "https://accounts.google.com/o/oauth2/v2/auth".into(),
                    "https://oauth2.googleapis.com/token".into(),
                    "https://openidconnect.googleapis.com/v1/userinfo".into(),
                    "openid profile".into(),
                ),
                "microsoft" => {
                    let tenant = required("MICROSOFT_TENANT_ID")?;
                    uuid::Uuid::parse_str(&tenant)
                        .map_err(|_| invalid("Microsoft requires an explicit tenant UUID"))?;
                    (
                        format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"),
                        format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"),
                        "https://graph.microsoft.com/v1.0/me".into(),
                        "https://graph.microsoft.com/User.Read".into(),
                    )
                }
                _ => (
                    "https://www.facebook.com/v24.0/dialog/oauth".into(),
                    "https://graph.facebook.com/v24.0/oauth/access_token".into(),
                    "https://graph.facebook.com/v24.0/me".into(),
                    "public_profile".into(),
                ),
            };
            providers.push(Provider {
                name: name.into(),
                client_id: id,
                client_secret: secret,
                authorize,
                token,
                userinfo,
                scope,
            });
        }
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            bind: env::var("BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8080".into())
                .parse()
                .map_err(|_| invalid("Invalid BIND_ADDR"))?,
            admin_origin: origin(required("ADMIN_ORIGIN")?, development)?,
            site_origin: origin(required("SITE_ORIGIN")?, development)?,
            media_dir: env::var("MEDIA_DIR")
                .unwrap_or_else(|_| "data/media".into())
                .into(),
            secure_cookies: !development,
            providers,
        })
    }
}

pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .build()
        .map_err(|e| crate::error::Error::Internal(e.to_string()))
}

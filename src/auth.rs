use crate::{
    AppState,
    config::Provider,
    error::{Error, Result, invalid},
};
use axum::{
    Json,
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderMap, HeaderValue, header, request::Parts},
    response::{IntoResponse, Redirect, Response},
};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use uuid::Uuid;

pub fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then_some(value)
        })
}

fn session_cookie(state: &AppState, name: &str, value: &str, seconds: u32) -> Result<HeaderValue> {
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds}{}",
        if state.config.secure_cookies {
            "; Secure"
        } else {
            ""
        }
    ))
    .map_err(|e| Error::Internal(e.to_string()))
}

#[derive(Clone, Serialize, FromRow)]
pub struct Actor {
    pub identity_id: Option<Uuid>,
    pub role: String,
    pub label: String,
    #[serde(skip)]
    pub token_hash: String,
}

impl Actor {
    pub fn editorial(&self) -> Result<()> {
        if matches!(self.role.as_str(), "admin" | "editor") {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
    pub fn admin(&self) -> Result<()> {
        if self.role == "admin" {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
    pub fn can_own(&self, author: Option<Uuid>) -> Result<()> {
        if self.role != "author" || self.identity_id.is_some() && self.identity_id == author {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
    pub fn audit_id(&self) -> String {
        self.identity_id.map_or_else(
            || format!("token:{}", &self.token_hash[..12]),
            |id| id.to_string(),
        )
    }
}

impl FromRequestParts<AppState> for Actor {
    type Rejection = Error;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self> {
        let bearer = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let token = bearer
            .or_else(|| cookie(&parts.headers, "rp_session"))
            .ok_or(Error::Unauthorized)?;
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Unauthorized);
        }
        if bearer.is_none()
            && !matches!(
                parts.method,
                axum::http::Method::GET | axum::http::Method::HEAD
            )
        {
            let origin = parts
                .headers
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok());
            if origin != Some(state.config.admin_origin.as_str()) {
                return Err(Error::Forbidden);
            }
        }
        let actor = sqlx::query_as::<_, Actor>(
            "SELECT c.identity_id, COALESCE(i.role, c.role) AS role, c.label, c.token_hash
             FROM credentials c LEFT JOIN identities i ON i.id=c.identity_id
             WHERE token_hash=$1 AND expires_at>now()",
        )
        .bind(hash(token))
        .fetch_optional(&state.pool)
        .await?
        .ok_or(Error::Unauthorized)?;
        Ok(actor)
    }
}

fn provider<'a>(state: &'a AppState, name: &str) -> Result<&'a Provider> {
    state
        .config
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or(Error::NotFound)
}

pub async fn providers(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(
        state
            .config
            .providers
            .iter()
            .map(|p| p.name.clone())
            .collect(),
    )
}

pub async fn login(State(state): State<AppState>, Path(name): Path<String>) -> Result<Response> {
    let provider = provider(&state, &name)?;
    let nonce = random_token();
    let browser = random_token();
    let verifier = random_token();
    sqlx::query("DELETE FROM oauth_states WHERE expires_at < now()")
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO oauth_states (state_hash,browser_hash,provider,verifier,expires_at) VALUES ($1,$2,$3,$4,now()+interval '10 minutes')")
        .bind(hash(&nonce)).bind(hash(&browser)).bind(&name).bind(&verifier).execute(&state.pool).await?;
    let mut url =
        url::Url::parse(&provider.authorize).map_err(|e| Error::Internal(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("client_id", &provider.client_id)
        .append_pair(
            "redirect_uri",
            &format!("{}/api/v1/auth/{name}/callback", state.config.admin_origin),
        )
        .append_pair("response_type", "code")
        .append_pair("scope", &provider.scope)
        .append_pair("state", &nonce);
    // Meta's Facebook Login does not use the Google/Microsoft PKCE parameters.
    if name != "meta" {
        use sha2::digest::Output;
        let digest: Output<Sha256> = Sha256::digest(verifier.as_bytes());
        url.query_pairs_mut()
            .append_pair("code_challenge", &base64_url(&digest))
            .append_pair("code_challenge_method", "S256");
    }
    let mut response = Redirect::to(url.as_str()).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&state, "rp_oauth", &browser, 600)?,
    );
    Ok(response)
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[derive(Deserialize)]
pub struct Callback {
    pub state: Option<String>,
    pub code: Option<String>,
    pub error: Option<String>,
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

pub async fn callback(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Query(query): Query<Callback>,
) -> Result<Response> {
    let provider = provider(&state, &name)?;
    let nonce = query.state.ok_or_else(|| invalid("Missing OAuth state"))?;
    let browser = cookie(&headers, "rp_oauth").ok_or(Error::Unauthorized)?;
    let verifier = sqlx::query_scalar::<_, String>(
        "DELETE FROM oauth_states WHERE state_hash=$1 AND browser_hash=$2 AND provider=$3 AND expires_at>now() RETURNING verifier"
    ).bind(hash(&nonce)).bind(hash(browser)).bind(&name).fetch_optional(&state.pool).await?.ok_or(Error::Unauthorized)?;
    if query.error.is_some() {
        return Err(invalid("Identity provider denied authorization"));
    }
    let code = query
        .code
        .ok_or_else(|| invalid("Missing authorization code"))?;
    let redirect = format!("{}/api/v1/auth/{name}/callback", state.config.admin_origin);
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect),
        ("client_id", &provider.client_id),
        ("client_secret", &provider.client_secret),
    ];
    if name != "meta" {
        form.push(("code_verifier", &verifier));
    }
    let token = state
        .http
        .post(&provider.token)
        .form(&form)
        .send()
        .await
        .map_err(|_| Error::Internal("Identity provider unavailable".into()))?;
    if !token.status().is_success() {
        return Err(Error::Unauthorized);
    }
    let token: TokenResponse = token.json().await.map_err(|_| Error::Unauthorized)?;
    let mut url =
        url::Url::parse(&provider.userinfo).map_err(|e| Error::Internal(e.to_string()))?;
    if name == "meta" {
        let mut mac = Hmac::<Sha256>::new_from_slice(provider.client_secret.as_bytes())
            .map_err(|e| Error::Internal(e.to_string()))?;
        mac.update(token.access_token.as_bytes());
        url.query_pairs_mut()
            .append_pair("fields", "id,name")
            .append_pair("appsecret_proof", &hex::encode(mac.finalize().into_bytes()));
    }
    let profile = state
        .http
        .get(url)
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|_| Error::Internal("Identity provider unavailable".into()))?;
    if !profile.status().is_success() {
        return Err(Error::Unauthorized);
    }
    let profile: serde_json::Value = profile.json().await.map_err(|_| Error::Unauthorized)?;
    let subject = profile
        .get(if name == "google" { "sub" } else { "id" })
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or(Error::Unauthorized)?;
    let identity: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, role FROM identities WHERE provider=$1 AND subject=$2")
            .bind(&name)
            .bind(subject)
            .fetch_optional(&state.pool)
            .await?;
    let Some((id, role)) = identity else {
        tracing::warn!(provider = %name, "Login rejected: identity has not been granted access");
        return Err(Error::Forbidden);
    };
    let session = random_token();
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM credentials WHERE expires_at<now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO credentials(token_hash,identity_id,role,label,expires_at) VALUES ($1,$2,$3,'browser',now()+interval '8 hours')")
        .bind(hash(&session)).bind(id).bind(role).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit(actor,action,resource) VALUES ($1,'login',$2)")
        .bind(id.to_string())
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut response = Redirect::to(&state.config.admin_origin).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        session_cookie(&state, "rp_session", &session, 28_800)?,
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        session_cookie(&state, "rp_oauth", "", 0)?,
    );
    Ok(response)
}

pub async fn me(actor: Actor) -> Json<Actor> {
    Json(actor)
}

pub async fn logout(State(state): State<AppState>, actor: Actor) -> Result<Response> {
    sqlx::query("DELETE FROM credentials WHERE token_hash=$1")
        .bind(&actor.token_hash)
        .execute(&state.pool)
        .await?;
    let mut response = axum::http::StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&state, "rp_session", "", 0)?,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_rfc7636_vector() {
        let challenge = Sha256::digest(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(
            base64_url(&challenge),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn generated_tokens_have_full_entropy_and_hash_is_not_plaintext() {
        let a = random_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, random_token());
        assert_ne!(hash(&a), a);
    }
    #[test]
    fn authors_cannot_edit_other_authors() {
        let id = Uuid::new_v4();
        let author = Actor {
            identity_id: Some(id),
            role: "author".into(),
            label: "".into(),
            token_hash: hash("test"),
        };
        assert!(author.can_own(Some(id)).is_ok());
        assert!(author.can_own(None).is_err());
        assert!(author.can_own(Some(Uuid::new_v4())).is_err());
        assert!(author.editorial().is_err());
    }
}

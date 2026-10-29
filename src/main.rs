use rustpress::{
    AppState,
    auth::{hash, random_token},
    config::Config,
    error::{Result, invalid},
};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustpress=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    sqlx_macros::migrate!()
        .run(&pool)
        .await
        .map_err(|e| rustpress::error::Error::Internal(e.to_string()))?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command] if command == "migrate" => return Ok(()),
        [command, provider, subject, role] if command == "grant" => {
            if !["google", "microsoft", "meta"].contains(&provider.as_str())
                || !["admin", "editor", "author"].contains(&role.as_str())
                || subject.is_empty()
            {
                return Err(invalid(
                    "Usage: rustpress grant <google|microsoft|meta> <provider-subject> <admin|editor|author>",
                ));
            }
            let mut tx = pool.begin().await?;
            sqlx::query("INSERT INTO identities(id,provider,subject,role) VALUES ($1,$2,$3,$4) ON CONFLICT(provider,subject) DO UPDATE SET role=$4")
                .bind(Uuid::new_v4()).bind(provider).bind(subject).bind(role).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO audit(actor,action,resource) VALUES ('operator','identity.grant',$1)",
            )
            .bind(format!("{provider}:{subject}"))
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            println!("Identity access granted.");
            return Ok(());
        }
        [command, label] if command == "agent-token" => {
            if label.is_empty() || label.len() > 120 {
                return Err(invalid("Token label must contain 1 to 120 bytes"));
            }
            let token = random_token();
            let mut tx = pool.begin().await?;
            sqlx::query("INSERT INTO credentials(token_hash,role,label,expires_at) VALUES ($1,'agent',$2,now()+interval '30 days')")
                .bind(hash(&token)).bind(label).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO audit(actor,action,resource) VALUES ('operator','token.issue',$1)",
            )
            .bind(hash(&token))
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            // This is a one-time credential delivery to the operator, not an application log.
            println!("{token}");
            return Ok(());
        }
        [command, token_hash] if command == "revoke-token" => {
            let mut tx = pool.begin().await?;
            let result = sqlx::query("DELETE FROM credentials WHERE token_hash=$1")
                .bind(token_hash)
                .execute(&mut *tx)
                .await?;
            if result.rows_affected() == 0 {
                return Err(invalid("Token hash not found"));
            }
            sqlx::query(
                "INSERT INTO audit(actor,action,resource) VALUES ('operator','token.revoke',$1)",
            )
            .bind(token_hash)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(());
        }
        [] => {}
        _ => {
            return Err(invalid(
                "Commands: migrate | grant <provider> <subject> <role> | agent-token <label> | revoke-token <sha256>",
            ));
        }
    }
    tokio::fs::create_dir_all(config.media_dir.join("public")).await?;
    tokio::fs::create_dir_all(config.media_dir.join("originals")).await?;
    let bind = config.bind;
    let state = AppState::new(config, pool)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(address=%bind, "RustPress API listening");
    axum::serve(
        listener,
        rustpress::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await?;
    Ok(())
}

async fn shutdown() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "Unable to register shutdown signal");
    }
}

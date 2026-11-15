mod sources;

use axum::{
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{env, net::SocketAddr, time::Duration};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct AppState {
    db: PgPool,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Serialize, sqlx::FromRow)]
struct Job {
    id: uuid::Uuid,
    title: String,
    company: String,
    location: Option<String>,
    work_mode: Option<String>,
    contract_type: String,
    relocation_support: bool,
    eligibility_confidence: String,
    source_url: String,
    source_name: String,
    posted_at: Option<chrono::DateTime<chrono::Utc>>,
    collected_at: chrono::DateTime<chrono::Utc>,
    is_read: bool,
    is_favorite: bool,
    mentions_java: bool,
    remote_scope: String,
}

#[derive(Deserialize)]
struct JobFilters {
    q: Option<String>,
    country: Option<String>,
    work_mode: Option<String>,
    contract_type: Option<String>,
    relocation: Option<bool>,
    unread: Option<bool>,
    favorite: Option<bool>,
    java: Option<bool>,
    /// `africa` = remote jobs open to Africa (worldwide, Africa or EMEA);
    /// any other value matches `jobs.remote_scope` exactly.
    scope: Option<String>,
}

#[derive(Serialize)]
struct UnreadCount {
    count: i64,
}

#[derive(Serialize)]
struct ReadResult {
    updated: bool,
}

#[derive(Deserialize)]
struct FavoriteBody {
    favorite: bool,
}

#[derive(Serialize)]
struct FavoriteResult {
    is_favorite: bool,
}

#[derive(Serialize, sqlx::FromRow)]
struct Summary {
    total: i64,
    unread: i64,
    burkina: i64,
    remote: i64,
    remote_africa: i64,
    relocation: i64,
    favorites: i64,
    java: i64,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn list_jobs(
    State(state): State<AppState>,
    Query(filters): Query<JobFilters>,
) -> Result<Json<Vec<Job>>, (StatusCode, String)> {
    let query = filters.q.unwrap_or_default();
    let country = filters.country.unwrap_or_default();
    let work_mode = filters.work_mode.unwrap_or_default();
    let contract_type = filters.contract_type.unwrap_or_default();
    let relocation = filters.relocation.unwrap_or(false);
    let unread = filters.unread.unwrap_or(false);
    let favorite = filters.favorite.unwrap_or(false);
    let java = filters.java.unwrap_or(false);
    let scope = filters.scope.unwrap_or_default();
    let jobs = sqlx::query_as::<_, Job>(
        r#"SELECT id, title, company, location, work_mode, contract_type, relocation_support,
                  eligibility_confidence, source_url, source_name, posted_at, collected_at, is_read,
                  is_favorite, mentions_java, remote_scope
           FROM jobs
           WHERE ($1 = '' OR title ILIKE '%' || $1 || '%' OR company ILIKE '%' || $1 || '%'
                  OR location ILIKE '%' || $1 || '%' OR description ILIKE '%' || $1 || '%')
             AND ($2 = '' OR location ILIKE '%' || $2 || '%')
             AND ($3 = '' OR work_mode ILIKE '%' || $3 || '%')
             AND ($4 = '' OR contract_type ILIKE '%' || $4 || '%')
             AND (NOT $5 OR relocation_support)
             AND (NOT $6 OR NOT is_read)
             AND (NOT $7 OR is_favorite)
             AND (NOT $8 OR mentions_java)
             AND ($9 = ''
                  OR ($9 = 'africa' AND remote_scope IN ('worldwide', 'africa', 'emea'))
                  OR remote_scope = $9)
           ORDER BY COALESCE(posted_at, collected_at) DESC
           LIMIT 500"#,
    )
    .bind(query)
    .bind(country)
    .bind(work_mode)
    .bind(contract_type)
    .bind(relocation)
    .bind(unread)
    .bind(favorite)
    .bind(java)
    .bind(scope)
    .fetch_all(&state.db)
    .await
    .map_err(internal_error)?;

    Ok(Json(jobs))
}

async fn unread_count(
    State(state): State<AppState>,
) -> Result<Json<UnreadCount>, (StatusCode, String)> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE NOT is_read")
        .fetch_one(&state.db)
        .await
        .map_err(internal_error)?;
    Ok(Json(UnreadCount { count: count.0 }))
}

async fn mark_job_read(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<ReadResult>, (StatusCode, String)> {
    let result = sqlx::query("UPDATE jobs SET is_read = TRUE WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(internal_error)?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Offre introuvable".to_owned()));
    }
    Ok(Json(ReadResult { updated: true }))
}

async fn set_favorite(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(body): Json<FavoriteBody>,
) -> Result<Json<FavoriteResult>, (StatusCode, String)> {
    let result = sqlx::query("UPDATE jobs SET is_favorite = $2 WHERE id = $1")
        .bind(id)
        .bind(body.favorite)
        .execute(&state.db)
        .await
        .map_err(internal_error)?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Offre introuvable".to_owned()));
    }
    Ok(Json(FavoriteResult {
        is_favorite: body.favorite,
    }))
}

async fn summary(State(state): State<AppState>) -> Result<Json<Summary>, (StatusCode, String)> {
    let summary = sqlx::query_as::<_, Summary>(
        r#"SELECT COUNT(*) AS total,
                  COUNT(*) FILTER (WHERE NOT is_read) AS unread,
                  COUNT(*) FILTER (WHERE location ILIKE '%burkina%') AS burkina,
                  COUNT(*) FILTER (WHERE work_mode ILIKE '%Télétravail%') AS remote,
                  COUNT(*) FILTER (WHERE remote_scope IN ('worldwide', 'africa', 'emea')) AS remote_africa,
                  COUNT(*) FILTER (WHERE relocation_support) AS relocation,
                  COUNT(*) FILTER (WHERE is_favorite) AS favorites,
                  COUNT(*) FILTER (WHERE mentions_java) AS java
           FROM jobs"#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(internal_error)?;
    Ok(Json(summary))
}

fn internal_error(error: sqlx::Error) -> (StatusCode, String) {
    tracing::error!(%error, "database request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Erreur de base de données".to_owned(),
    )
}

async fn collect_periodically(
    db: PgPool,
    client: reqwest::Client,
    source: &'static str,
    interval_minutes: u64,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_minutes.max(1) * 60));
    loop {
        interval.tick().await;
        let result = sources::collect(source, &client).await;
        let changed = collect_source(&db, source, result).await;
        tracing::info!(source, changed, "job collection finished");
    }
}

async fn collect_source(
    db: &PgPool,
    source: &str,
    result: Result<Vec<sources::CollectedJob>, reqwest::Error>,
) -> usize {
    match result {
        Ok(jobs) => {
            let has_jobs = !jobs.is_empty();
            let mut changed = 0;
            let mut urls = Vec::with_capacity(jobs.len());
            let mut names = Vec::new();
            for job in jobs {
                match sources::save_job(db, &job).await {
                    Ok(true) => changed += 1,
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(%error, url = %job.source_url, "could not save job")
                    }
                }
                if !names.contains(&job.source_name) {
                    names.push(job.source_name.clone());
                }
                urls.push(job.source_url);
            }
            if has_jobs {
                match sources::mark_seen_and_prune(db, &urls, &names).await {
                    Ok(removed) if removed > 0 => {
                        tracing::info!(source, removed, "removed listings no longer published");
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, source, "could not prune listings"),
                }
            }
            if source == "OpenToAfrica" && has_jobs {
                match sources::prune_stale_open_to_africa(db).await {
                    Ok(removed) if removed > 0 => {
                        tracing::info!(source, removed, "removed stale source listings");
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, source, "could not prune stale listings"),
                }
            }
            changed
        }
        Err(error) => {
            tracing::warn!(%error, source, "job source collection failed");
            0
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = env::var("DATABASE_URL")?;
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_create_jobs.sql"))
        .execute(&db)
        .await?;

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(60))
        .build()?;
    let interval_minutes = env::var("COLLECT_INTERVAL_MINUTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    for (source, minutes) in sources::schedule(interval_minutes) {
        tokio::spawn(collect_periodically(
            db.clone(),
            client.clone(),
            source,
            minutes,
        ));
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/summary", get(summary))
        .route("/api/jobs/unread-count", get(unread_count))
        .route("/api/jobs/{id}/read", patch(mark_job_read))
        .route("/api/jobs/{id}/favorite", patch(set_favorite))
        .with_state(AppState { db })
        .layer(
            CorsLayer::new()
                .allow_origin([
                    "http://localhost:5173".parse::<axum::http::HeaderValue>()?,
                    "http://127.0.0.1:5173".parse::<axum::http::HeaderValue>()?,
                ])
                .allow_methods([Method::GET, Method::PATCH])
                .allow_headers(tower_http::cors::Any),
        )
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = env::var("API_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "API prête");
    axum::serve(listener, app).await?;
    Ok(())
}

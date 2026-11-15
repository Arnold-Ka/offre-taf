use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, Utc};
use scraper::{Html, Selector};
use serde::Deserialize;
use sqlx::PgPool;

pub mod ats;
pub mod burkina;
pub mod classify;
pub mod francophone;

pub use classify::is_it_role as is_developer_role;

/// A listing as returned by a connector. Derived flags (relocation, Java,
/// remote scope) are computed centrally in [`save_job`].
pub struct CollectedJob {
    pub title: String,
    pub company: String,
    pub location: Option<String>,
    pub work_mode: Option<String>,
    pub contract_type: String,
    pub eligibility_confidence: String,
    pub description: String,
    pub source_url: String,
    pub source_name: String,
    pub posted_at: Option<DateTime<Utc>>,
}

/// Every scheduled source with its polling interval in minutes.
pub fn schedule(arbeitnow_interval_minutes: u64) -> Vec<(&'static str, u64)> {
    vec![
        ("Arbeitnow", arbeitnow_interval_minutes),
        // Remotive asks API users to limit requests to at most four times per day.
        ("Remotive", 360),
        // OpenToAfrica refreshes its public company-board index daily.
        ("OpenToAfrica", 1440),
        ("Wave", 360),
        ("Yassir", 360),
        ("Gozem", 360),
        ("Djamo", 360),
        ("EmploiDakar", 1440),
        ("SABMA", 1440),
        ("Qualive", 1440),
        ("BFemploi", 1440),
        // Company career boards (Greenhouse, Lever, Ashby): remote, relocation, Burkina.
        ("EntreprisesATS", 720),
        // Workable's employer-job search: Burkina, remote developer roles, relocation.
        ("Workable", 720),
        // Recruitment notices published on Burkina employers' own websites.
        ("BurkinaEntreprises", 720),
    ]
}

pub async fn collect(
    source: &str,
    client: &reqwest::Client,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    match source {
        "Remotive" => remotive(client).await,
        "Arbeitnow" => arbeitnow(client).await,
        "OpenToAfrica" => open_to_africa(client).await,
        "Wave" => francophone::wave_greenhouse(client).await,
        "Yassir" => francophone::yassir_lever(client).await,
        "Gozem" => francophone::gozem_breezy(client).await,
        "Djamo" => francophone::djamo_breezy(client).await,
        "EmploiDakar" => francophone::emploi_dakar(client).await,
        "SABMA" => francophone::sabma_digital(client).await,
        "Qualive" => francophone::qualive_careers(client).await,
        "BFemploi" => francophone::bfemploi(client).await,
        "EntreprisesATS" => ats::company_boards(client).await,
        "Workable" => ats::workable_search(client).await,
        "BurkinaEntreprises" => burkina::employer_sites(client).await,
        _ => unreachable!("only registered job sources are scheduled"),
    }
}

#[derive(Deserialize)]
struct RemotiveResponse {
    #[serde(default)]
    jobs: Vec<RemotiveJob>,
}

#[derive(Deserialize)]
struct RemotiveJob {
    title: String,
    company_name: String,
    url: String,
    #[serde(default)]
    candidate_required_location: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    job_type: Option<String>,
    #[serde(default)]
    publication_date: Option<String>,
}

#[derive(Deserialize)]
struct ArbeitnowResponse {
    #[serde(default)]
    data: Vec<ArbeitnowJob>,
}

#[derive(Deserialize)]
struct ArbeitnowJob {
    title: String,
    company_name: String,
    url: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    remote: bool,
    #[serde(default)]
    description: String,
    #[serde(default)]
    job_types: Vec<String>,
    #[serde(default)]
    created_at: Option<i64>,
}

pub async fn remotive(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let response = client
        .get("https://remotive.com/api/remote-jobs?category=software-dev")
        .send()
        .await?
        .error_for_status()?
        .json::<RemotiveResponse>()
        .await?;

    Ok(response
        .jobs
        .into_iter()
        .filter(|job| is_developer_role(&job.title))
        .map(|job| {
            let posted_at = job
                .publication_date
                .as_deref()
                .and_then(parse_remotive_date);
            make_job(
                job.title,
                job.company_name,
                nonempty(job.candidate_required_location),
                Some("Télétravail".to_owned()),
                job.job_type.unwrap_or_default(),
                job.description,
                String::new(),
                job.url,
                "Remotive",
                posted_at,
            )
        })
        .collect())
}

pub async fn arbeitnow(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let response = client
        .get("https://www.arbeitnow.com/api/job-board-api")
        .send()
        .await?
        .error_for_status()?
        .json::<ArbeitnowResponse>()
        .await?;

    Ok(response
        .data
        .into_iter()
        .filter(|job| is_developer_role(&job.title))
        .map(|job| {
            let mode = if job.remote {
                "Télétravail"
            } else {
                "Sur site / hybride"
            };
            let posted_at = job
                .created_at
                .and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0));
            make_job(
                job.title,
                job.company_name,
                nonempty(job.location),
                Some(mode.to_owned()),
                job.job_types.join(", "),
                job.description,
                String::new(),
                job.url,
                "Arbeitnow",
                posted_at,
            )
        })
        .collect())
}

pub async fn open_to_africa(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let html = client
        .get("https://opentoafrica.com/")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_open_to_africa(&html))
}

fn parse_open_to_africa(html: &str) -> Vec<CollectedJob> {
    let document = Html::parse_document(html);
    let job_selector = Selector::parse("li.job").expect("valid job selector");
    let title_selector = Selector::parse("h3").expect("valid title selector");
    let company_selector = Selector::parse(".jt p.muted").expect("valid company selector");
    let date_selector = Selector::parse(".when").expect("valid date selector");
    let link_selector = Selector::parse("a[href]").expect("valid link selector");
    let mut jobs = Vec::new();

    for card in document.select(&job_selector) {
        let Some(title) = card
            .select(&title_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_owned())
        else {
            continue;
        };
        if !is_developer_role(&title) {
            continue;
        }
        let posted_at = card
            .select(&date_selector)
            .next()
            .map(|node| node.text().collect::<String>())
            .and_then(|value| parse_relative_date(&value));
        if posted_at.is_some_and(|date| Utc::now() - date > ChronoDuration::days(180)) {
            continue;
        }

        let Some(href) = card
            .select(&link_selector)
            .next()
            .and_then(|node| node.value().attr("href"))
        else {
            continue;
        };
        let company_location = card
            .select(&company_selector)
            .next()
            .map(|node| node.text().collect::<String>())
            .unwrap_or_default();
        let (company, display_location) = company_location
            .split_once('·')
            .map(|(company, location)| (company.trim(), location.trim()))
            .unwrap_or(("", company_location.trim()));
        let region = card.value().attr("data-region").unwrap_or_default();
        let confidence = card.value().attr("data-conf").unwrap_or_default();
        let mut location = display_location.to_owned();
        if !region.is_empty() && !location.to_lowercase().contains(&region.to_lowercase()) {
            if !location.is_empty() {
                location.push_str(" · ");
            }
            location.push_str(region);
        }
        if confidence == "yes" {
            location.push_str(" · Afrique (confirmé par la source)");
        } else if confidence == "likely" {
            location.push_str(" · Afrique (à confirmer)");
        }
        let work_mode = match card.value().attr("data-mode").unwrap_or_default() {
            "Remote" => "Télétravail".to_owned(),
            "On-site/Hybrid" | "Hybrid" => "Sur site / hybride".to_owned(),
            other => other.to_owned(),
        };

        jobs.push(make_job(
            title,
            company.to_owned(),
            nonempty(location),
            nonempty(work_mode),
            String::new(),
            String::new(),
            confidence.to_owned(),
            format!("https://opentoafrica.com{href}"),
            "OpenToAfrica",
            posted_at,
        ));
    }

    jobs
}

/// Upserts a listing (deduplicated by `source_url`) and returns whether a row changed.
/// New listings that mention Java are marked as favourites; a later manual
/// un-favourite is kept unless the listing starts mentioning Java.
pub async fn save_job(db: &PgPool, job: &CollectedJob) -> Result<bool, sqlx::Error> {
    let description = classify::html_to_text(&job.description)
        .chars()
        .take(20_000)
        .collect::<String>();
    let searchable = format!("{} {description}", job.title);
    let relocation_support = classify::mentions_relocation(&searchable);
    let mentions_java = classify::mentions_java(&searchable);
    let remote_scope = classify::remote_scope(
        job.work_mode.as_deref(),
        job.location.as_deref().unwrap_or_default(),
        &description,
    );

    let result = sqlx::query(
        r#"INSERT INTO jobs
             (title, company, location, work_mode, contract_type, relocation_support, description,
              source_url, source_name, eligibility_confidence, posted_at, mentions_java,
              remote_scope, is_favorite, last_seen_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $12, NOW())
           ON CONFLICT (source_url) DO UPDATE
             SET location = COALESCE(EXCLUDED.location, jobs.location),
                 work_mode = COALESCE(EXCLUDED.work_mode, jobs.work_mode),
                 contract_type = COALESCE(NULLIF(jobs.contract_type, ''), EXCLUDED.contract_type),
                 eligibility_confidence = CASE
                   WHEN EXCLUDED.eligibility_confidence = 'yes' THEN 'yes'
                   ELSE COALESCE(NULLIF(jobs.eligibility_confidence, ''), EXCLUDED.eligibility_confidence)
                 END,
                 description = COALESCE(NULLIF(EXCLUDED.description, ''), jobs.description),
                 posted_at = COALESCE(jobs.posted_at, EXCLUDED.posted_at),
                 relocation_support = EXCLUDED.relocation_support
                   OR (EXCLUDED.description = '' AND jobs.relocation_support),
                 mentions_java = EXCLUDED.mentions_java
                   OR (EXCLUDED.description = '' AND jobs.mentions_java),
                 is_favorite = jobs.is_favorite OR (EXCLUDED.mentions_java AND NOT jobs.mentions_java),
                 remote_scope = EXCLUDED.remote_scope
             WHERE (jobs.contract_type = '' AND EXCLUDED.contract_type <> '')
                OR (jobs.eligibility_confidence <> 'yes' AND EXCLUDED.eligibility_confidence = 'yes')
                OR (jobs.eligibility_confidence = '' AND EXCLUDED.eligibility_confidence <> '')
                OR (jobs.posted_at IS NULL AND EXCLUDED.posted_at IS NOT NULL)
                OR (EXCLUDED.location IS NOT NULL AND jobs.location IS DISTINCT FROM EXCLUDED.location)
                OR (EXCLUDED.work_mode IS NOT NULL AND jobs.work_mode IS DISTINCT FROM EXCLUDED.work_mode)
                OR (EXCLUDED.description <> '' AND jobs.description IS DISTINCT FROM EXCLUDED.description)
                OR (EXCLUDED.description <> '' AND jobs.relocation_support IS DISTINCT FROM EXCLUDED.relocation_support)
                OR (EXCLUDED.description <> '' AND jobs.mentions_java IS DISTINCT FROM EXCLUDED.mentions_java)
                OR jobs.remote_scope IS DISTINCT FROM EXCLUDED.remote_scope"#,
    )
    .bind(&job.title)
    .bind(&job.company)
    .bind(&job.location)
    .bind(&job.work_mode)
    .bind(&job.contract_type)
    .bind(relocation_support)
    .bind(&description)
    .bind(&job.source_url)
    .bind(&job.source_name)
    .bind(&job.eligibility_confidence)
    .bind(job.posted_at)
    .bind(mentions_java)
    .bind(remote_scope)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Records that these listings are still published, then deletes listings of the
/// same source names that have not been seen for 21 days (filled or withdrawn).
/// Favourites are never deleted. Sources that failed this run are untouched
/// because their source names are absent from `source_names`.
pub async fn mark_seen_and_prune(
    db: &PgPool,
    source_urls: &[String],
    source_names: &[String],
) -> Result<u64, sqlx::Error> {
    sqlx::query("UPDATE jobs SET last_seen_at = NOW() WHERE source_url = ANY($1)")
        .bind(source_urls)
        .execute(db)
        .await?;
    let result = sqlx::query(
        "DELETE FROM jobs
         WHERE source_name = ANY($1)
           AND last_seen_at < NOW() - INTERVAL '21 days'
           AND NOT is_favorite",
    )
    .bind(source_names)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

pub async fn prune_stale_open_to_africa(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM jobs WHERE source_name = 'OpenToAfrica' AND NOT is_favorite AND (posted_at IS NULL OR posted_at < NOW() - INTERVAL '180 days')",
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

#[allow(clippy::too_many_arguments)]
fn make_job(
    title: String,
    company: String,
    location: Option<String>,
    work_mode: Option<String>,
    contract_type: String,
    description: String,
    eligibility_confidence: String,
    source_url: String,
    source_name: &str,
    posted_at: Option<DateTime<Utc>>,
) -> CollectedJob {
    CollectedJob {
        title,
        company,
        location,
        work_mode,
        contract_type,
        eligibility_confidence,
        description,
        source_url,
        source_name: source_name.to_owned(),
        posted_at,
    }
}

fn parse_remotive_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").map(|date| date.and_utc())
        })
        .ok()
}

fn parse_relative_date(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim().to_lowercase();
    let now = Utc::now();
    if value == "today" {
        return Some(now);
    }
    if value == "yesterday" {
        return Some(now - ChronoDuration::days(1));
    }

    let mut parts = value.strip_suffix(" ago")?.split_whitespace();
    let amount = parts.next()?.parse::<i64>().ok()?;
    let unit = parts.next()?;
    let duration = match unit.trim_end_matches('s') {
        "minute" => ChronoDuration::minutes(amount),
        "hour" => ChronoDuration::hours(amount),
        "day" => ChronoDuration::days(amount),
        "week" => ChronoDuration::weeks(amount),
        "month" => ChronoDuration::days(amount * 30),
        "year" => ChronoDuration::days(amount * 365),
        _ => return None,
    };
    Some(now - duration)
}

pub(super) fn nonempty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

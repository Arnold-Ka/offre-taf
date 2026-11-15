//! Company career boards hosted on applicant-tracking systems (Greenhouse, Lever,
//! Ashby) plus Workable's employer-job search. These are the employers' own
//! published postings, not job-board copies. A posting is kept when it is an IT
//! role AND it is remote, located in Burkina Faso, or mentions relocation/visa.

use super::classify::{self, html_to_text, is_it_role, mentions_burkina, mentions_relocation};
use super::{nonempty, CollectedJob};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

/// Large boards (Spotify, Stripe, Databricks) can take over a minute to stream,
/// longer than the client-wide 60 s timeout.
const BOARD_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Copy)]
pub enum Ats {
    Greenhouse,
    Lever,
    Ashby,
}

pub struct Board {
    pub company: &'static str,
    pub ats: Ats,
    pub slug: &'static str,
}

const fn gh(company: &'static str, slug: &'static str) -> Board {
    Board { company, ats: Ats::Greenhouse, slug }
}
const fn lever(company: &'static str, slug: &'static str) -> Board {
    Board { company, ats: Ats::Lever, slug }
}
const fn ashby(company: &'static str, slug: &'static str) -> Board {
    Board { company, ats: Ats::Ashby, slug }
}

/// Employers known for remote-first hiring, relocation/visa programmes, or an
/// African presence. Every slug was checked against the public board API.
/// To add an employer, append its board here.
pub const BOARDS: &[Board] = &[
    // Remote-first / distributed
    gh("GitLab", "gitlab"),
    gh("Canonical", "canonical"),
    gh("Wikimedia Foundation", "wikimedia"),
    gh("Mozilla", "mozilla"),
    gh("Elastic", "elastic"),
    gh("Grafana Labs", "grafanalabs"),
    gh("Automattic", "automatticcareers"),
    gh("Sourcegraph", "sourcegraph91"),
    gh("Mattermost", "mattermost"),
    gh("Consensys", "consensys"),
    gh("Vercel", "vercel"),
    gh("Netlify", "netlify"),
    gh("CircleCI", "circleci"),
    gh("Turing", "turing"),
    gh("PlanetScale", "planetscale"),
    gh("Neo4j", "neo4j"),
    gh("Airtable", "airtable"),
    lever("Toptal", "toptal"),
    lever("Fly.io", "fly"),
    ashby("Zapier", "zapier"),
    ashby("PostHog", "posthog"),
    ashby("Supabase", "supabase"),
    ashby("Docker", "docker"),
    ashby("Sentry", "sentry"),
    ashby("Temporal", "temporal"),
    ashby("ClickHouse", "clickhouse"),
    ashby("Railway", "railway"),
    ashby("Render", "render"),
    ashby("Camunda", "camunda"),
    ashby("Andela", "andela"),
    ashby("Buffer", "buffer"),
    ashby("Help Scout", "helpscout"),
    ashby("Ghost", "ghost"),
    ashby("GitBook", "gitbook"),
    ashby("1Password", "1password"),
    ashby("Oyster", "oyster"),
    ashby("Hopper", "hopper"),
    ashby("Linear", "linear"),
    // Relocation / visa programmes (Europe and beyond), many Java/JVM stacks
    gh("Adyen", "adyen"),
    gh("N26", "n26"),
    gh("Wise", "wise"),
    gh("HelloFresh", "hellofresh"),
    gh("trivago", "trivago"),
    gh("Celonis", "celonis"),
    gh("Cabify", "cabify"),
    gh("Contentful", "contentful"),
    gh("commercetools", "commercetools"),
    gh("SumUp", "sumup"),
    gh("Solaris", "solarisbank"),
    gh("Raisin", "raisin"),
    gh("JetBrains", "jetbrains"),
    gh("Typeform", "typeform"),
    gh("Bitpanda", "bitpanda"),
    gh("Datadog", "datadog"),
    gh("Cloudflare", "cloudflare"),
    gh("Stripe", "stripe"),
    gh("MongoDB", "mongodb"),
    gh("Twilio", "twilio"),
    gh("Duolingo", "duolingo"),
    gh("Discord", "discord"),
    gh("Figma", "figma"),
    gh("Dropbox", "dropbox"),
    gh("Reddit", "reddit"),
    gh("Pinterest", "pinterest"),
    gh("Coinbase", "coinbase"),
    gh("Gemini", "gemini"),
    gh("Ripple", "ripple"),
    gh("Binance", "binance"),
    gh("OKX", "okx"),
    gh("Okta", "okta"),
    gh("Databricks", "databricks"),
    gh("Payoneer", "payoneer"),
    gh("EBANX", "ebanx"),
    lever("Spotify", "spotify"),
    lever("Swile", "swile"),
    lever("Contentsquare", "contentsquare"),
    lever("BlaBlaCar", "blablacar"),
    lever("Malt", "malt"),
    lever("Agicap", "agicap"),
    lever("Trendyol", "trendyol"),
    ashby("Miro", "miro"),
    ashby("Babbel", "babbel"),
    ashby("Notion", "notion"),
    ashby("OpenAI", "openai"),
    ashby("Ramp", "ramp"),
    ashby("Plaid", "plaid"),
    ashby("Mollie", "mollie"),
    ashby("Pleo", "pleo"),
    ashby("Qonto", "qonto"),
    ashby("Alan", "alan"),
    ashby("Doctolib", "doctolib"),
    ashby("Back Market", "backmarket"),
    ashby("Ledger", "ledger"),
    ashby("Sorare", "sorare"),
    ashby("Pennylane", "pennylane"),
    // African companies (all roles kept when remote, in Burkina or with relocation)
    gh("Moniepoint", "moniepoint"),
    gh("Jumia", "jumia"),
    gh("Wave", "wavemm1"),
    lever("Yassir", "Yassir"),
];

pub async fn company_boards(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let mut jobs = Vec::new();
    for board in BOARDS {
        let result = match board.ats {
            Ats::Greenhouse => greenhouse(client, board).await,
            Ats::Lever => lever_board(client, board).await,
            Ats::Ashby => ashby_board(client, board).await,
        };
        match result {
            Ok(mut found) => jobs.append(&mut found),
            Err(error) => {
                tracing::warn!(%error, company = board.company, "company career board failed")
            }
        }
    }
    Ok(jobs)
}

/// Keeps IT roles that are remote, based in Burkina Faso, or mention relocation.
fn wanted(title: &str, location: &str, work_mode: Option<&str>, description: &str) -> bool {
    is_it_role(title)
        && (classify::is_remote(work_mode)
            || mentions_burkina(location)
            || mentions_relocation(&format!("{title} {description}")))
}

fn job(
    title: String,
    company: &str,
    location: String,
    work_mode: Option<String>,
    contract_type: String,
    description: String,
    source_url: String,
    source_name: String,
    posted_at: Option<DateTime<Utc>>,
) -> CollectedJob {
    CollectedJob {
        title: title.trim().to_owned(),
        company: company.to_owned(),
        location: nonempty(location),
        work_mode,
        contract_type,
        eligibility_confidence: String::new(),
        description,
        source_url,
        source_name,
        posted_at,
    }
}

fn parse_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .ok()
}

fn normalize_workplace(value: &str, location: &str) -> Option<String> {
    match value.trim().to_lowercase().replace(['-', '_', ' '], "").as_str() {
        "remote" => Some("Télétravail".to_owned()),
        "hybrid" => Some("Sur site / hybride".to_owned()),
        "onsite" | "inoffice" => Some("Sur site".to_owned()),
        _ => classify::work_mode_from_text(location),
    }
}

#[derive(Deserialize)]
struct GreenhouseResponse {
    #[serde(default)]
    jobs: Vec<GreenhouseJob>,
}

#[derive(Deserialize)]
struct GreenhouseJob {
    title: String,
    absolute_url: String,
    #[serde(default)]
    location: Option<GreenhouseLocation>,
    #[serde(default)]
    content: String,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Deserialize)]
struct GreenhouseLocation {
    #[serde(default)]
    name: String,
}

async fn greenhouse(
    client: &reqwest::Client,
    board: &Board,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let response = client
        .get(format!(
            "https://boards-api.greenhouse.io/v1/boards/{}/jobs?content=true",
            board.slug
        ))
        .timeout(BOARD_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json::<GreenhouseResponse>()
        .await?;

    Ok(response
        .jobs
        .into_iter()
        .filter(|posting| is_it_role(&posting.title))
        .filter_map(|posting| {
            let location = posting.location.map(|location| location.name).unwrap_or_default();
            let description = html_to_text(&posting.content);
            let work_mode = classify::work_mode_from_text(&location);
            if !wanted(&posting.title, &location, work_mode.as_deref(), &description) {
                return None;
            }
            Some(job(
                posting.title,
                board.company,
                location,
                work_mode,
                String::new(),
                description,
                posting.absolute_url,
                format!("{} · Greenhouse", board.company),
                posting.updated_at.as_deref().and_then(parse_date),
            ))
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeverJob {
    text: String,
    hosted_url: String,
    #[serde(default)]
    categories: LeverCategories,
    #[serde(default)]
    description_plain: String,
    #[serde(default)]
    additional_plain: String,
    #[serde(default)]
    lists: Vec<LeverList>,
    #[serde(default)]
    workplace_type: String,
    #[serde(default)]
    created_at: Option<i64>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeverCategories {
    #[serde(default)]
    location: String,
    #[serde(default)]
    all_locations: Vec<String>,
    #[serde(default)]
    commitment: String,
}

#[derive(Deserialize)]
struct LeverList {
    #[serde(default)]
    text: String,
    #[serde(default)]
    content: String,
}

async fn lever_board(
    client: &reqwest::Client,
    board: &Board,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let postings = client
        .get(format!("https://api.lever.co/v0/postings/{}?mode=json", board.slug))
        .timeout(BOARD_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<LeverJob>>()
        .await?;

    Ok(postings
        .into_iter()
        .filter(|posting| is_it_role(&posting.text))
        .filter_map(|posting| {
            let location = if posting.categories.all_locations.is_empty() {
                posting.categories.location.clone()
            } else {
                posting.categories.all_locations.join("; ")
            };
            let lists = posting
                .lists
                .iter()
                .map(|list| format!("{} {}", list.text, html_to_text(&list.content)))
                .collect::<Vec<_>>()
                .join(" ");
            let description = format!(
                "{} {lists} {}",
                posting.description_plain, posting.additional_plain
            );
            let work_mode = normalize_workplace(&posting.workplace_type, &location);
            if !wanted(&posting.text, &location, work_mode.as_deref(), &description) {
                return None;
            }
            Some(job(
                posting.text,
                board.company,
                location,
                work_mode,
                posting.categories.commitment,
                description,
                posting.hosted_url,
                format!("{} · Lever", board.company),
                posting
                    .created_at
                    .and_then(DateTime::<Utc>::from_timestamp_millis),
            ))
        })
        .collect())
}

#[derive(Deserialize)]
struct AshbyResponse {
    #[serde(default)]
    jobs: Vec<AshbyJob>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AshbyJob {
    title: String,
    job_url: String,
    // Ashby sends explicit nulls for several fields, hence Options everywhere.
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    secondary_locations: Option<Vec<AshbySecondaryLocation>>,
    #[serde(default)]
    address: Option<AshbyAddress>,
    #[serde(default)]
    is_listed: Option<bool>,
    #[serde(default)]
    is_remote: Option<bool>,
    #[serde(default)]
    workplace_type: Option<String>,
    #[serde(default)]
    employment_type: Option<String>,
    #[serde(default)]
    description_plain: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
}

#[derive(Deserialize)]
struct AshbySecondaryLocation {
    #[serde(default)]
    location: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AshbyAddress {
    #[serde(default)]
    postal_address: Option<AshbyPostalAddress>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AshbyPostalAddress {
    #[serde(default)]
    address_country: Option<String>,
}

async fn ashby_board(
    client: &reqwest::Client,
    board: &Board,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let response = client
        .get(format!(
            "https://api.ashbyhq.com/posting-api/job-board/{}",
            board.slug
        ))
        .timeout(BOARD_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json::<AshbyResponse>()
        .await?;

    Ok(response
        .jobs
        .into_iter()
        .filter(|posting| posting.is_listed != Some(false) && is_it_role(&posting.title))
        .filter_map(|posting| {
            let mut places = vec![posting.location.clone().unwrap_or_default()];
            places.extend(
                posting
                    .secondary_locations
                    .iter()
                    .flatten()
                    .filter_map(|secondary| secondary.location.clone()),
            );
            if let Some(country) = posting
                .address
                .as_ref()
                .and_then(|address| address.postal_address.as_ref())
                .and_then(|postal| postal.address_country.clone())
            {
                if !places.iter().any(|place| place.contains(&country)) {
                    places.push(country);
                }
            }
            places.retain(|place| !place.trim().is_empty());
            let location = places.join("; ");
            let work_mode = if posting.is_remote == Some(true) {
                Some("Télétravail".to_owned())
            } else {
                normalize_workplace(posting.workplace_type.as_deref().unwrap_or_default(), &location)
            };
            let description = posting.description_plain.unwrap_or_default();
            if !wanted(&posting.title, &location, work_mode.as_deref(), &description) {
                return None;
            }
            Some(job(
                posting.title,
                board.company,
                location,
                work_mode,
                posting.employment_type.unwrap_or_default(),
                description,
                posting.job_url,
                format!("{} · Ashby", board.company),
                posting.published_at.as_deref().and_then(parse_date),
            ))
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkableSearch {
    #[serde(default)]
    jobs: Vec<WorkableJob>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkableJob {
    title: String,
    url: String,
    #[serde(default)]
    locations: Vec<String>,
    #[serde(default)]
    location: Option<WorkableLocation>,
    #[serde(default)]
    created: Option<String>,
    #[serde(default)]
    company: Option<WorkableCompany>,
    #[serde(default)]
    workplace: Option<String>,
    #[serde(default)]
    employment_type: Option<String>,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkableLocation {
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    country_name: Option<String>,
}

#[derive(Deserialize)]
struct WorkableCompany {
    #[serde(default)]
    title: String,
}

/// (query, location, workplace, pages) searches run against Workable's
/// cross-employer index; the API returns nothing above 20 postings per page.
const WORKABLE_PAGE_SIZE: &str = "20";
const WORKABLE_SEARCHES: &[(&str, &str, &str, usize)] = &[
    ("", "Burkina Faso", "", 5),
    ("software engineer", "", "remote", 5),
    ("developer", "", "remote", 5),
    ("java", "", "remote", 5),
    ("backend", "", "remote", 3),
    ("full stack", "", "remote", 3),
    ("devops", "", "remote", 3),
    ("relocation developer", "", "", 3),
    ("relocation java", "", "", 3),
    ("visa sponsorship engineer", "", "", 3),
];

pub async fn workable_search(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    let mut failures = 0;
    let mut last_error = None;

    for (query, location, workplace, pages) in WORKABLE_SEARCHES {
        let mut token: Option<String> = None;
        for _ in 0..*pages {
            let mut params = vec![("limit", WORKABLE_PAGE_SIZE.to_owned())];
            if !query.is_empty() {
                params.push(("query", (*query).to_owned()));
            }
            if !location.is_empty() {
                params.push(("location", (*location).to_owned()));
            }
            if !workplace.is_empty() {
                params.push(("workplace", (*workplace).to_owned()));
            }
            if let Some(token) = &token {
                params.push(("pageToken", token.clone()));
            }
            let page = match workable_page(client, &params).await {
                Ok(page) => page,
                Err(error) => {
                    tracing::warn!(%error, query, location, "Workable search failed");
                    failures += 1;
                    last_error = Some(error);
                    break;
                }
            };
            for posting in page.jobs {
                if seen.insert(posting.url.clone()) {
                    if let Some(found) = workable_job(posting) {
                        jobs.push(found);
                    }
                }
            }
            match page.next_page_token {
                Some(next) if !next.is_empty() => token = Some(next),
                _ => break,
            }
        }
    }

    match last_error {
        Some(error) if failures == WORKABLE_SEARCHES.len() => Err(error),
        _ => Ok(jobs),
    }
}

async fn workable_page(
    client: &reqwest::Client,
    params: &[(&str, String)],
) -> Result<WorkableSearch, reqwest::Error> {
    client
        .get("https://jobs.workable.com/api/v1/jobs")
        .query(params)
        .send()
        .await?
        .error_for_status()?
        .json::<WorkableSearch>()
        .await
}

fn workable_job(posting: WorkableJob) -> Option<CollectedJob> {
    if !is_it_role(&posting.title) {
        return None;
    }
    let mut places = Vec::new();
    if let Some(location) = &posting.location {
        let city = location.city.clone().unwrap_or_default();
        let country = location.country_name.clone().unwrap_or_default();
        let place = [city, country]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        if !place.is_empty() {
            places.push(place);
        }
    }
    for place in &posting.locations {
        if place != "TELECOMMUTE" && !places.iter().any(|known| known.contains(place.as_str())) {
            places.push(place.clone());
        }
    }
    let location = places.join("; ");
    let remote = posting.workplace.as_deref() == Some("remote")
        || posting.locations.iter().any(|place| place == "TELECOMMUTE");
    let work_mode = if remote {
        Some("Télétravail".to_owned())
    } else {
        normalize_workplace(posting.workplace.as_deref().unwrap_or_default(), &location)
    };
    let description = html_to_text(&posting.description);
    if !wanted(&posting.title, &location, work_mode.as_deref(), &description) {
        return None;
    }
    let company = posting
        .company
        .map(|company| company.title)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Employeur sur Workable".to_owned());
    Some(job(
        posting.title,
        &company,
        location,
        work_mode,
        posting.employment_type.unwrap_or_default(),
        description,
        posting.url,
        "Workable · page employeur".to_owned(),
        posting.created.as_deref().and_then(parse_date),
    ))
}

#[cfg(test)]
mod tests {
    use super::wanted;

    #[test]
    fn keeps_only_remote_burkina_or_relocation_it_roles() {
        assert!(wanted("Backend Engineer", "Remote, EMEA", Some("Télétravail"), ""));
        assert!(wanted("Développeur Java", "Ouagadougou, Burkina Faso", Some("Sur site"), ""));
        assert!(wanted(
            "Java Engineer",
            "Amsterdam",
            Some("Sur site"),
            "We provide a relocation package."
        ));
        assert!(!wanted("Java Engineer", "Amsterdam", Some("Sur site"), ""));
        assert!(!wanted("Account Executive", "Remote", Some("Télétravail"), ""));
    }
}

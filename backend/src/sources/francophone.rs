use super::classify::{phrase_matches, words};
use super::{is_developer_role, nonempty, CollectedJob};
use chrono::{DateTime, NaiveDate, Utc};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

const FRANCOPHONE_COUNTRIES: &[(&str, &[&str])] = &[
    ("Algérie", &["algerie", "algeria", "alger", "algiers"]),
    ("Bénin", &["benin", "cotonou"]),
    ("Burkina Faso", &["burkina faso", "ouagadougou"]),
    ("Burundi", &["burundi", "bujumbura"]),
    ("Cameroun", &["cameroun", "cameroon", "douala", "yaounde"]),
    ("Cap-Vert", &["cap vert", "cape verde", "praia"]),
    ("Comores", &["comores", "comoros", "moroni"]),
    (
        "Congo (RDC)",
        &[
            "republique democratique du congo",
            "democratic republic of the congo",
            "dr congo",
            "drc",
            "congo kinshasa",
            "kinshasa",
        ],
    ),
    (
        "Congo (République)",
        &[
            "republique du congo",
            "republic of the congo",
            "congo brazzaville",
            "brazzaville",
        ],
    ),
    (
        "Côte d’Ivoire",
        &[
            "cote d ivoire",
            "cote d rsquo ivoire",
            "ivory coast",
            "abidjan",
            "bouake",
        ],
    ),
    ("Djibouti", &["djibouti"]),
    ("Gabon", &["gabon", "libreville"]),
    (
        "Guinée équatoriale",
        &["guinee equatoriale", "equatorial guinea", "malabo"],
    ),
    (
        "Guinée-Bissau",
        &["guinee bissau", "guinea bissau", "bissau"],
    ),
    ("Guinée", &["guinee", "guinea", "conakry"]),
    ("Madagascar", &["madagascar", "antananarivo"]),
    ("Mali", &["mali", "bamako"]),
    ("Maroc", &["maroc", "morocco", "rabat", "casablanca"]),
    ("Mauritanie", &["mauritanie", "mauritania", "nouakchott"]),
    ("Maurice", &["maurice", "mauritius", "port louis"]),
    ("Niger", &["niger", "niamey"]),
    (
        "République centrafricaine",
        &[
            "republique centrafricaine",
            "central african republic",
            "bangui",
        ],
    ),
    ("Rwanda", &["rwanda", "kigali"]),
    ("Sénégal", &["senegal", "dakar"]),
    ("Seychelles", &["seychelles"]),
    ("Tchad", &["tchad", "chad", "ndjamena", "n djamena"]),
    ("Togo", &["togo", "lome"]),
    ("Tunisie", &["tunisie", "tunisia", "tunis"]),
];

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
    commitment: String,
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
struct BreezyJob {
    name: String,
    url: String,
    #[serde(default)]
    published_date: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    #[serde(rename = "type")]
    type_info: Option<BreezyType>,
    #[serde(default)]
    location: Option<BreezyLocation>,
    #[serde(default)]
    locations: Vec<BreezyLocation>,
    #[serde(default)]
    company: Option<BreezyCompany>,
}

#[derive(Deserialize)]
struct BreezyType {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct BreezyLocation {
    #[serde(default)]
    name: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    is_remote: bool,
    #[serde(default)]
    country: Option<BreezyCountry>,
    #[serde(default)]
    remote_details: Option<BreezyRemoteDetails>,
}

#[derive(Deserialize)]
struct BreezyCountry {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct BreezyRemoteDetails {
    #[serde(default)]
    label: String,
}

#[derive(Deserialize)]
struct BreezyCompany {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct WordPressSearchResult {
    title: String,
    url: String,
    #[serde(default)]
    subtype: String,
}

pub async fn wave_greenhouse(
    client: &reqwest::Client,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let response = client
        .get("https://boards-api.greenhouse.io/v1/boards/wavemm1/jobs?content=true")
        .send()
        .await?
        .error_for_status()?
        .json::<GreenhouseResponse>()
        .await?;

    Ok(response
        .jobs
        .into_iter()
        .filter(|job| is_developer_role(&job.title))
        .filter_map(|job| {
            let raw_location = job
                .location
                .map(|location| location.name)
                .unwrap_or_default();
            let countries = matching_countries(&raw_location, &job.content, true);
            if countries.is_empty() {
                return None;
            }
            let location = format_location(&raw_location, &countries);
            Some(make_job(
                job.title,
                "Wave".to_owned(),
                location,
                mode_from_text(&format!("{raw_location} {}", job.content)),
                String::new(),
                job.content,
                job.absolute_url,
                "Wave · Greenhouse",
                job.updated_at.as_deref().and_then(parse_date),
            ))
        })
        .collect())
}

pub async fn yassir_lever(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let postings = client
        .get("https://api.lever.co/v0/postings/Yassir?mode=json")
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<LeverJob>>()
        .await?;

    Ok(postings
        .into_iter()
        .filter(|job| is_developer_role(&job.text))
        .filter_map(|job| {
            let raw_location = if job.categories.all_locations.is_empty() {
                job.categories.location.clone()
            } else {
                job.categories.all_locations.join("; ")
            };
            let countries = matching_countries(&raw_location, &job.description_plain, true);
            if countries.is_empty() {
                return None;
            }
            let location = format_location(&raw_location, &countries);
            let workplace = if job.workplace_type.is_empty() {
                mode_from_text(&raw_location)
            } else {
                normalize_work_mode(&job.workplace_type)
            };
            Some(make_job(
                job.text,
                "Yassir".to_owned(),
                location,
                workplace,
                if job.commitment.is_empty() {
                    job.categories.commitment
                } else {
                    job.commitment
                },
                job.description_plain,
                job.hosted_url,
                "Yassir · Lever",
                job.created_at
                    .and_then(DateTime::<Utc>::from_timestamp_millis),
            ))
        })
        .collect())
}

pub async fn gozem_breezy(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    breezy_jobs(client, "https://gozem.breezy.hr/json", "Gozem").await
}

pub async fn djamo_breezy(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    breezy_jobs(client, "https://djamo.breezy.hr/json", "Djamo").await
}

async fn breezy_jobs(
    client: &reqwest::Client,
    url: &str,
    source: &str,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let listings = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<BreezyJob>>()
        .await?;

    Ok(listings
        .into_iter()
        .filter(|job| is_developer_role(&job.name))
        .filter_map(|job| {
            let locations = if job.locations.is_empty() {
                job.location.iter().collect::<Vec<_>>()
            } else {
                job.locations.iter().collect::<Vec<_>>()
            };
            let raw_location = locations
                .iter()
                .map(|location| breezy_location_name(location))
                .collect::<Vec<_>>()
                .join("; ");
            let countries = matching_countries(&raw_location, &job.description, true);
            if countries.is_empty() {
                return None;
            }
            let location = format_location(&raw_location, &countries);
            let workplace = locations
                .iter()
                .find_map(|location| {
                    location
                        .remote_details
                        .as_ref()
                        .map(|details| details.label.clone())
                        .filter(|label| !label.is_empty())
                        .or_else(|| location.is_remote.then(|| "Télétravail".to_owned()))
                })
                .and_then(|value| normalize_work_mode(&value))
                .or_else(|| mode_from_text(&raw_location));
            Some(make_job(
                job.name,
                job.company
                    .map(|company| company.name)
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| source.to_owned()),
                location,
                workplace,
                job.type_info
                    .map(|job_type| job_type.name)
                    .unwrap_or_default(),
                job.description,
                job.url,
                match source {
                    "Gozem" => "Gozem · Breezy",
                    "Djamo" => "Djamo · Breezy",
                    _ => "Breezy",
                },
                job.published_date.as_deref().and_then(parse_date),
            ))
        })
        .collect())
}

pub async fn emploi_dakar(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for term in [
        "développeur",
        "developer",
        "full stack",
        "ingénieur logiciel",
    ] {
        let results = client
            .get("https://www.emploidakar.com/wp-json/wp/v2/search")
            .query(&[
                ("search", term),
                ("subtype", "job_listing"),
                ("per_page", "15"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<WordPressSearchResult>>()
            .await?;

        for result in results {
            if result.subtype != "job_listing"
                || !is_developer_role(&result.title)
                || !seen.insert(result.url.clone())
            {
                continue;
            }
            if let Ok(Some(job)) = emploi_dakar_detail(client, result).await {
                jobs.push(job);
            }
        }
    }
    Ok(jobs)
}

pub async fn sabma_digital(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let base_url = "https://sabmadigital.com/bf/carrieres/";
    let html = client
        .get(base_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let role_pages = {
        let document = Html::parse_document(&html);
        let script_selector = Selector::parse("script[type='application/ld+json']")
            .expect("valid structured-data selector");
        let mut role_pages = Vec::new();
        for script in document.select(&script_selector) {
            if let Ok(value) = serde_json::from_str::<Value>(&script.text().collect::<String>()) {
                collect_sabma_pages(&value, &mut role_pages);
            }
        }
        role_pages
    };

    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for (title, url) in role_pages {
        if !is_developer_role(&title) || !seen.insert(url.clone()) {
            continue;
        }
        let Ok(response) = client.get(&url).send().await else {
            continue;
        };
        let Ok(response) = response.error_for_status() else {
            continue;
        };
        let Ok(page) = response.text().await else {
            continue;
        };
        let job_document = Html::parse_document(&page);
        let title_selector = Selector::parse("h1").expect("valid title selector");
        let description_selector =
            Selector::parse("meta[name='description']").expect("valid description selector");
        let body_selector = Selector::parse("body").expect("valid body selector");
        let title = job_document
            .select(&title_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_owned())
            .filter(|title| is_developer_role(title))
            .unwrap_or(title);
        let summary = job_document
            .select(&description_selector)
            .next()
            .and_then(|node| node.value().attr("content"))
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect::<String>();
        let body_text = job_document
            .select(&body_selector)
            .next()
            .map(|node| node.text().collect::<String>())
            .unwrap_or_default();
        let description = if summary.is_empty() {
            "Candidatures ouvertes en continu sur le site de l’employeur.".to_owned()
        } else {
            format!("{summary} Candidatures ouvertes en continu.")
        };
        jobs.push(make_job(
            title,
            "SABMA Digital".to_owned(),
            Some("Ouagadougou, Burkina Faso".to_owned()),
            mode_from_text(&body_text),
            String::new(),
            description,
            url,
            "SABMA Digital · carrière",
            None,
        ));
    }
    Ok(jobs)
}

fn collect_sabma_pages(value: &Value, pages: &mut Vec<(String, String)>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_sabma_pages(item, pages);
            }
        }
        Value::Object(object) => {
            if object.get("@type").and_then(Value::as_str) == Some("WebPage") {
                let title = object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let url = object
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if url.contains("/bf/carrieres/") && url != "https://sabmadigital.com/bf/carrieres/"
                {
                    pages.push((title.to_owned(), url.to_owned()));
                }
            }
            for child in object.values() {
                collect_sabma_pages(child, pages);
            }
        }
        _ => {}
    }
}

pub async fn qualive_careers(
    client: &reqwest::Client,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let page_url = "https://qualive-sarl.org/carriere";
    let html = client
        .get(page_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let document = Html::parse_document(&html);
    let panel_selector = Selector::parse("div.premium-panel").expect("valid job card selector");
    let title_selector = Selector::parse("h2").expect("valid title selector");
    let paragraph_selector = Selector::parse("p").expect("valid summary selector");
    let mut jobs = Vec::new();

    for (index, panel) in document.select(&panel_selector).enumerate() {
        let Some(title) = panel
            .select(&title_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_owned())
        else {
            continue;
        };
        let card_text = panel.text().collect::<String>();
        if !is_developer_role(&title) || card_text.to_lowercase().contains("pourvu") {
            continue;
        }
        let countries = matching_countries(&card_text, "", false);
        // Qualive's active roles are based at its Ouagadougou office; some
        // remote internships omit the country on the card itself.
        if countries.is_empty() && !card_text.to_lowercase().contains("remote") {
            continue;
        }
        let details = panel
            .select(&paragraph_selector)
            .next()
            .map(|node| node.text().collect::<String>())
            .unwrap_or_default();
        let location = if card_text.to_lowercase().contains("ouagadougou") {
            "Ouagadougou, Burkina Faso"
        } else {
            "Burkina Faso"
        };
        let description = card_text.chars().take(240).collect::<String>();
        let slug = title
            .to_lowercase()
            .chars()
            .map(|character| {
                if character.is_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        jobs.push(make_job(
            title,
            "Qualive".to_owned(),
            Some(location.to_owned()),
            mode_from_text(&details),
            if details.to_lowercase().contains("stage") {
                "Stage".to_owned()
            } else {
                String::new()
            },
            description,
            format!("{page_url}?poste={index}-{slug}"),
            "Qualive · carrière",
            None,
        ));
    }
    Ok(jobs)
}

pub async fn bfemploi(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let html = client
        .get("https://www.bfemploi.com/")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let document = Html::parse_document(&html);
    let card_selector = Selector::parse(".div_rz_ance_gnral").expect("valid listing selector");
    let title_selector = Selector::parse(".ance_titre").expect("valid title selector");
    let link_selector = Selector::parse("a[href]").expect("valid link selector");
    let company_selector =
        Selector::parse("a[title^='Recruteur']").expect("valid company selector");
    let location_selector =
        Selector::parse(r#"a[title="Lieu d'affectation"]"#).expect("valid location selector");
    let mut jobs = Vec::new();

    for card in document.select(&card_selector) {
        let Some(title_node) = card.select(&title_selector).next() else {
            continue;
        };
        let title = title_node.text().collect::<String>().trim().to_owned();
        if !is_developer_role(&title) {
            continue;
        }
        let Some(href) = card
            .select(&link_selector)
            .find(|anchor| anchor.select(&title_selector).next().is_some())
            .and_then(|anchor| anchor.value().attr("href"))
        else {
            continue;
        };
        let company = card
            .select(&company_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_owned())
            .unwrap_or_else(|| "Recruteur sur BFemploi".to_owned());
        let raw_location = card
            .select(&location_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_owned())
            .unwrap_or_else(|| "Ouagadougou, Burkina Faso".to_owned());
        let countries = matching_countries(&raw_location, &card.text().collect::<String>(), true);
        if countries.is_empty() {
            continue;
        }
        let card_text = card.text().collect::<String>();
        let dates = card_text
            .split_whitespace()
            .filter_map(parse_bfemploi_date)
            .collect::<Vec<_>>();
        if dates.len() > 1 && dates.last().is_some_and(|deadline| *deadline < Utc::now()) {
            continue;
        }
        let posted_at = dates.first().copied();
        let employment = card
            .select(&Selector::parse("a.ss_miz_f").expect("valid metadata link selector"))
            .map(|node| node.text().collect::<String>())
            .find(|value| value.contains("Contrat") || value.contains("Stage"))
            .unwrap_or_default();
        jobs.push(make_job(
            title,
            company,
            format_location(&raw_location, &countries),
            mode_from_text(&format!("{raw_location} {card_text}")),
            employment,
            card_text.chars().take(240).collect(),
            format!("https://www.bfemploi.com/{href}"),
            "BFemploi · job board",
            posted_at,
        ));
    }
    Ok(jobs)
}

fn parse_bfemploi_date(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim_matches(|character: char| {
        !character.is_ascii_digit() && character != '/' && character != '-'
    });
    let date = NaiveDate::parse_from_str(value, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(value, "%d-%m-%Y"))
        .ok()?;
    date.and_hms_opt(23, 59, 59).map(|date| date.and_utc())
}

async fn emploi_dakar_detail(
    client: &reqwest::Client,
    result: WordPressSearchResult,
) -> Result<Option<CollectedJob>, reqwest::Error> {
    let html = client
        .get(&result.url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let document = Html::parse_document(&html);
    let scripts = Selector::parse("script[type='application/ld+json']")
        .expect("valid structured-data selector");
    let posting = document
        .select(&scripts)
        .filter_map(|node| serde_json::from_str::<Value>(&node.text().collect::<String>()).ok())
        .find_map(|value| find_job_posting(&value).cloned());

    let Some(posting) = posting else {
        return Ok(None);
    };
    let title = posting
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(&result.title)
        .to_owned();
    if !is_developer_role(&title) {
        return Ok(None);
    }
    let description = posting
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let job_location = posting.get("jobLocation");
    let raw_location = job_location.map(display_job_location).unwrap_or_default();
    let country_search = job_location
        .map(Value::to_string)
        .unwrap_or_else(|| raw_location.clone());
    let mut countries = matching_countries(&country_search, &description, true);
    if countries.is_empty() {
        // Emploi Dakar is a Senegalese job board; use its country only when the
        // listing did not publish structured location details.
        if raw_location.is_empty() {
            countries.push("Sénégal");
        } else {
            return Ok(None);
        }
    }
    let location = format_location(&raw_location, &countries);
    let valid_through = posting
        .get("validThrough")
        .and_then(Value::as_str)
        .and_then(parse_date);
    if valid_through.is_some_and(|expiry| expiry < Utc::now()) {
        return Ok(None);
    }

    Ok(Some(make_job(
        title,
        posting
            .get("hiringOrganization")
            .and_then(|organization| organization.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("Emploi Dakar annonce")
            .to_owned(),
        location,
        mode_from_text(&format!("{raw_location} {description}")),
        value_as_string(posting.get("employmentType")),
        description,
        result.url,
        "Emploi Dakar · job board",
        posting
            .get("datePosted")
            .and_then(Value::as_str)
            .and_then(parse_date),
    )))
}

fn make_job(
    title: String,
    company: String,
    location: Option<String>,
    work_mode: Option<String>,
    contract_type: String,
    description: String,
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
        eligibility_confidence: String::new(),
        description,
        source_url,
        source_name: source_name.to_owned(),
        posted_at,
    }
}

fn matching_countries(
    location: &str,
    description: &str,
    allow_remote_description: bool,
) -> Vec<&'static str> {
    let explicit = country_names(location);
    if !explicit.is_empty() {
        return explicit;
    }
    if allow_remote_description && is_broad_remote_scope(location) {
        return country_names(description);
    }
    Vec::new()
}

fn country_names(text: &str) -> Vec<&'static str> {
    let tokens = words(text);
    FRANCOPHONE_COUNTRIES
        .iter()
        .filter_map(|(name, aliases)| {
            aliases
                .iter()
                .any(|alias| phrase_matches(&tokens, &words(alias)))
                .then_some(*name)
        })
        .collect()
}

fn is_broad_remote_scope(value: &str) -> bool {
    let tokens = words(value);
    tokens.is_empty()
        || tokens == ["remote"]
        || tokens == ["global"]
        || tokens == ["worldwide"]
        || tokens == ["anywhere"]
        || tokens == ["remote", "global"]
        || tokens == ["global", "remote"]
        || tokens == ["remote", "worldwide"]
        || tokens == ["worldwide", "remote"]
}

fn format_location(location: &str, countries: &[&str]) -> Option<String> {
    let location = location.trim();
    let missing_canonical_names = countries
        .iter()
        .copied()
        .filter(|country| !phrase_matches(&words(location), &words(country)))
        .collect::<Vec<_>>();
    let formatted = match (location.is_empty(), missing_canonical_names.is_empty()) {
        (true, _) => countries.join(", "),
        (false, true) => location.to_owned(),
        (false, false) => format!("{location} · {}", missing_canonical_names.join(", ")),
    };
    nonempty(formatted)
}

fn display_job_location(value: &Value) -> String {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .map(display_job_location)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
    }
    if let Some(object) = value.as_object() {
        if let Some(address) = object.get("address") {
            if let Some(address) = address.as_str() {
                return address.to_owned();
            }
            let parts = [
                "streetAddress",
                "addressLocality",
                "addressRegion",
                "addressCountry",
            ]
            .iter()
            .filter_map(|key| address.get(key).and_then(Value::as_str))
            .collect::<Vec<_>>();
            if !parts.is_empty() {
                return parts.join(", ");
            }
        }
        return ["name", "addressLocality", "addressRegion", "addressCountry"]
            .iter()
            .filter_map(|key| object.get(*key).and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(", ");
    }
    value.as_str().unwrap_or_default().to_owned()
}

fn mode_from_text(value: &str) -> Option<String> {
    let lower = value.to_lowercase();
    if lower.contains("hybrid") || lower.contains("hybride") {
        Some("Sur site / hybride".to_owned())
    } else if lower.contains("remote")
        || lower.contains("télétravail")
        || lower.contains("teletravail")
    {
        Some("Télétravail".to_owned())
    } else if lower.is_empty() {
        None
    } else {
        Some("Sur site".to_owned())
    }
}

fn normalize_work_mode(value: &str) -> Option<String> {
    match value.trim().to_lowercase().as_str() {
        "remote" | "work from home" | "telecommute" => Some("Télétravail".to_owned()),
        "hybrid" | "on-site/hybrid" | "on-site" | "onsite" => Some("Sur site / hybride".to_owned()),
        "" => None,
        _ => mode_from_text(value),
    }
}

fn parse_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .ok()
}

fn value_as_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

fn breezy_location_name(location: &BreezyLocation) -> String {
    let mut parts = Vec::new();
    if !location.name.is_empty() {
        parts.push(location.name.clone());
    } else if !location.city.is_empty() {
        parts.push(location.city.clone());
    }
    if let Some(country) = &location.country {
        if !country.name.is_empty() && !parts.iter().any(|part| part.contains(&country.name)) {
            parts.push(country.name.clone());
        }
    }
    if location.is_remote {
        parts.push("Remote".to_owned());
    }
    parts.join(", ")
}

fn find_job_posting(value: &Value) -> Option<&Value> {
    match value {
        Value::Array(values) => values.iter().find_map(find_job_posting),
        Value::Object(object) => {
            if object
                .get("@type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "JobPosting")
            {
                return Some(value);
            }
            object.values().find_map(find_job_posting)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{format_location, matching_countries};

    #[test]
    fn matches_francophone_locations_without_matching_nigeria_as_niger() {
        assert_eq!(
            matching_countries("Algiers, Algeria", "", true),
            ["Algérie"]
        );
        assert_eq!(matching_countries("Lomé, TG", "Togo", true), ["Togo"]);
        assert!(matching_countries("Lagos, Nigeria", "", true).is_empty());
    }

    #[test]
    fn remote_roles_use_only_explicit_francophone_country_mentions() {
        assert_eq!(
            matching_countries(
                "Remote",
                "Candidates can work from Senegal, Côte d'Ivoire, Ghana, or Kenya.",
                true,
            ),
            ["Côte d’Ivoire", "Sénégal"]
        );
        assert!(matching_countries("Remote, Ghana; Kenya", "Senegal operations", true).is_empty());
    }

    #[test]
    fn display_location_includes_french_country_name_for_country_filtering() {
        assert_eq!(
            format_location("Algiers, Algeria", &["Algérie"]),
            Some("Algiers, Algeria · Algérie".to_owned())
        );
    }
}

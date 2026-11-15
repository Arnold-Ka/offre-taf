//! Recruitment notices published directly on Burkina Faso employers' websites.
//! Most Burkinabè companies and institutions publish "avis de recrutement" as
//! WordPress posts, so a single connector reads `/wp-json/wp/v2/posts` on each
//! registered site, keeps recent IT recruitment notices and links to the post.

use super::classify::{html_to_text, is_it_role, phrase_matches, words, work_mode_from_text};
use super::CollectedJob;
use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;

pub struct EmployerSite {
    pub company: &'static str,
    pub base_url: &'static str,
    pub location: &'static str,
    /// Media sites relay communiqués from many employers: the company name is
    /// then read from the post title ("Communiqué | IPADEV : Avis de recrutement…").
    pub relays_employers: bool,
}

const fn site(company: &'static str, base_url: &'static str) -> EmployerSite {
    EmployerSite {
        company,
        base_url,
        location: "Ouagadougou, Burkina Faso",
        relays_employers: false,
    }
}

/// WordPress sites verified to expose their posts API. Append new employers here.
pub const SITES: &[EmployerSite] = &[
    site("SONABEL", "https://www.sonabel.bf"),
    site("Coris Bank International Burkina", "https://burkina.coris.bank"),
    site("Coris Assurances", "https://www.coris-assurances.com"),
    site("Moov Africa Burkina", "https://moov-africa.bf"),
    site("Telecel Faso", "https://www.telecelfaso.bf"),
    site("ANPTIC", "https://www.anptic.gov.bf"),
    site("Institut 2iE", "https://www.2ie-edu.org"),
    site("CILSS", "https://www.cilss.int"),
    site("ARCEP Burkina", "https://www.arcep.bf"),
    EmployerSite {
        company: "Burkina24 · communiqués",
        base_url: "https://burkina24.com",
        location: "Burkina Faso",
        relays_employers: true,
    },
];

const SEARCH_TERMS: &[&str] = &["recrut", "recruit", "vacance de poste", "offre d'emploi"];
const MAX_AGE_DAYS: i64 = 120;

#[derive(Deserialize)]
struct WpPost {
    link: String,
    #[serde(default)]
    date: Option<String>,
    title: WpRendered,
    #[serde(default)]
    content: Option<WpRendered>,
}

#[derive(Deserialize)]
struct WpRendered {
    #[serde(default)]
    rendered: String,
}

pub async fn employer_sites(client: &reqwest::Client) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let mut jobs = Vec::new();
    let mut failures = 0;
    let mut last_error = None;
    for site in SITES {
        match site_jobs(client, site).await {
            Ok(mut found) => jobs.append(&mut found),
            Err(error) => {
                tracing::warn!(%error, company = site.company, "employer website failed");
                failures += 1;
                last_error = Some(error);
            }
        }
    }
    match last_error {
        Some(error) if failures == SITES.len() => Err(error),
        _ => Ok(jobs),
    }
}

async fn site_jobs(
    client: &reqwest::Client,
    site: &EmployerSite,
) -> Result<Vec<CollectedJob>, reqwest::Error> {
    let after = (Utc::now() - ChronoDuration::days(MAX_AGE_DAYS))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for term in SEARCH_TERMS {
        let posts = client
            .get(format!("{}/wp-json/wp/v2/posts", site.base_url))
            .query(&[
                ("search", *term),
                ("per_page", "50"),
                ("after", after.as_str()),
                ("_fields", "date,link,title,content"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<WpPost>>()
            .await?;
        for post in posts {
            if seen.insert(post.link.clone()) {
                if let Some(job) = notice_to_job(site, post) {
                    jobs.push(job);
                }
            }
        }
    }
    Ok(jobs)
}

/// Words that mark results, admissions or scholarships rather than open positions.
const NOT_A_VACANCY: &[&str] = &[
    "admis",
    "admission",
    "resultat",
    "resultats",
    "liste",
    "bourse",
    "bourses",
    "doctorant",
    "doctorants",
    "convocation",
    "accueil",
    "preselection",
    "report",
];

const RECRUITMENT_WORDS: &[&str] = &[
    "recrutement",
    "recrute",
    "recrutons",
    "recruiting",
    "recruitment",
    "recruits",
    "vacance",
    "emploi",
    "candidature",
    "candidatures",
    "poste",
    "postes",
];

const GENERIC_WORDS: &[&str] = &[
    "avis", "de", "d", "du", "des", "un", "une", "la", "le", "l", "pour", "communique", "is",
    "a", "an", "the", "of", "offre", "appel", "a", "h", "f", "01", "02", "03", "n", "no",
];

/// Phrases that announce an actual opening (unlike "la réforme du recrutement").
const VACANCY_PHRASES: &[&str] = &[
    "avis de recrutement",
    "recrute",
    "recrutons",
    "recrutement d",
    "recrutement de",
    "recrutement des",
    "recrutement du",
    "is recruiting",
    "recruitment of",
    "vacance de poste",
    "vacances de poste",
    "offre d emploi",
    "offres d emploi",
    "appel a candidature",
    "appel a candidatures",
    "poste a pourvoir",
    "postes a pourvoir",
];

fn announces_vacancy(tokens: &[String]) -> bool {
    VACANCY_PHRASES
        .iter()
        .any(|phrase| phrase_matches(tokens, &words(phrase)))
}

fn notice_to_job(site: &EmployerSite, post: WpPost) -> Option<CollectedJob> {
    let title = html_to_text(&post.title.rendered);
    let tokens = words(&title);
    if tokens.iter().any(|token| NOT_A_VACANCY.contains(&token.as_str()))
        || !announces_vacancy(&tokens)
    {
        return None;
    }
    let content = post
        .content
        .map(|content| html_to_text(&content.rendered))
        .unwrap_or_default();
    // A generic title ("AVIS DE RECRUTEMENT") names the role at the top of the body.
    let generic_title = tokens.iter().all(|token| {
        RECRUITMENT_WORDS.contains(&token.as_str()) || GENERIC_WORDS.contains(&token.as_str())
    });
    let opening = content.chars().take(300).collect::<String>();
    if !is_it_role(&title) && !(generic_title && is_it_role(&opening)) {
        return None;
    }
    let posted_at = post.date.as_deref().and_then(parse_wp_date);
    if posted_at.is_some_and(|date| Utc::now() - date > ChronoDuration::days(MAX_AGE_DAYS)) {
        return None;
    }
    let company = if site.relays_employers {
        relayed_company(&title).unwrap_or_else(|| site.company.to_owned())
    } else {
        site.company.to_owned()
    };
    let display_title = title
        .trim_start_matches("Communiqué |")
        .trim_start_matches("Communiqué|")
        .trim()
        .to_owned();
    Some(CollectedJob {
        title: display_title,
        company,
        location: Some(site.location.to_owned()),
        work_mode: work_mode_from_text(&format!("{title} {opening}")),
        contract_type: contract_from_text(&content),
        eligibility_confidence: String::new(),
        description: content,
        source_url: post.link,
        source_name: format!("{} · site officiel", site.company),
        posted_at,
    })
}

fn relayed_company(title: &str) -> Option<String> {
    let rest = title.split_once('|').map(|(_, rest)| rest).unwrap_or(title);
    let (company, _) = rest.split_once(':')?;
    let company = company.trim();
    let lower = company.to_lowercase();
    (!company.is_empty() && company.chars().count() <= 60 && !lower.contains("recrut"))
        .then(|| company.to_owned())
}

fn contract_from_text(value: &str) -> String {
    let tokens = words(value);
    if tokens.iter().any(|token| token == "cdi") {
        "CDI".to_owned()
    } else if tokens.iter().any(|token| token == "cdd") {
        "CDD".to_owned()
    } else if tokens.iter().any(|token| token == "stage" || token == "stagiaire") {
        "Stage".to_owned()
    } else {
        String::new()
    }
}

fn parse_wp_date(value: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .map(|date| date.and_utc())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(title: &str, content: &str) -> WpPost {
        WpPost {
            link: "https://example.bf/avis".to_owned(),
            date: Some(Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
            title: WpRendered { rendered: title.to_owned() },
            content: Some(WpRendered { rendered: content.to_owned() }),
        }
    }

    #[test]
    fn keeps_it_recruitment_notices_only() {
        let site = &SITES[0];
        assert!(notice_to_job(site, post("Avis de recrutement d’un Ingénieur Systèmes et Réseaux", "")).is_some());
        assert!(notice_to_job(site, post("AVIS DE RECRUTEMENT", "<p>La société recrute un Développeur Java (CDI).</p>")).is_some());
        assert!(notice_to_job(site, post("DECISION D’ADMISSION AU TEST DE RECRUTEMENT D’UN INFORMATICIEN", "")).is_none());
        assert!(notice_to_job(site, post("Avis de recrutement d’un chef d’usine", "")).is_none());
        assert!(notice_to_job(site, post("Bénin : L’intelligence artificielle et le numérique au cœur de la réforme du recrutement dans la fonction publique", "")).is_none());
        assert!(notice_to_job(site, post("CBI Burkina is recruiting a Database Administrator", "")).is_some());
    }

    #[test]
    fn media_communiques_name_the_employer() {
        assert_eq!(
            relayed_company("Communiqué | IPADEV : Avis de recrutement d’un informaticien"),
            Some("IPADEV".to_owned())
        );
    }
}

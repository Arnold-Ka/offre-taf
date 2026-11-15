//! Text classification shared by every source: IT-role filter, Java mentions,
//! relocation/visa mentions, Burkina mentions and remote eligibility scope.
//! All matching is done on accent-folded lowercase word tokens so that
//! "JavaScript" never counts as Java and "Nigeria" never counts as Niger.

use scraper::Html;

/// Remote scopes stored in `jobs.remote_scope`. The UI treats the first three
/// as "accessible depuis l'Afrique".
pub const SCOPE_WORLDWIDE: &str = "worldwide";
pub const SCOPE_AFRICA: &str = "africa";
pub const SCOPE_EMEA: &str = "emea";
pub const SCOPE_EUROPE: &str = "europe";
pub const SCOPE_AMERICAS: &str = "americas";
pub const SCOPE_APAC: &str = "apac";
pub const SCOPE_UNKNOWN: &str = "unknown";

pub fn words(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .chars()
        .map(|character| match character {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'œ' => 'o',
            _ => character,
        })
        .collect::<String>()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn phrase_matches(tokens: &[String], phrase: &[String]) -> bool {
    !phrase.is_empty() && tokens.windows(phrase.len()).any(|window| window == phrase)
}

fn contains_any(tokens: &[String], phrases: &[&str]) -> bool {
    phrases
        .iter()
        .any(|phrase| phrase_matches(tokens, &words(phrase)))
}

/// Converts (possibly entity-encoded) HTML into plain text. Greenhouse and some
/// Workable employers send HTML that is entity-encoded once or twice more,
/// hence the repeated passes.
pub fn html_to_text(value: &str) -> String {
    let mut text = value.to_owned();
    for _ in 0..3 {
        let encoded = text.contains("&lt;") || text.contains("&amp;") || text.contains("&#");
        let markup = text.contains('<') && text.contains('>');
        if !encoded && !markup && !text.contains('&') {
            break;
        }
        let next = fragment_text(&text);
        if next == text {
            break;
        }
        text = next;
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fragment_text(value: &str) -> String {
    Html::parse_fragment(value)
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
}

const EXCLUDED_ROLES: &[&str] = &[
    "business developer",
    "business development",
    "sales development",
    "sales",
    "account executive",
    "webdesigner",
    "web designer",
    "web research",
    "civil engineer",
    "genie civil",
    "mechanical engineer",
    "electrical engineer",
    "chemical engineer",
    "recruiter",
    "recruteur",
];

const IT_ROLES: &[&str] = &[
    "developer",
    "developpeur",
    "developpeuse",
    "programmeur",
    "programmeuse",
    "programmer",
    "programming",
    "ingenieur logiciel",
    "ingenieur informatique",
    "ingenieur reseau",
    "ingenieur reseaux",
    "ingenieur systeme",
    "ingenieur systemes",
    "ingenieur devops",
    "ingenieur cloud",
    "ingenieur data",
    "ingenieur securite",
    "ingenieur developpement",
    "ingenieur etudes et developpement",
    "ingenieur telecom",
    "ingenieur telecoms",
    "informaticien",
    "informaticienne",
    "informatique",
    "tech lead",
    "technical lead",
    "engineer",
    "engineering manager",
    "software",
    "full stack",
    "fullstack",
    "frontend",
    "front end",
    "backend",
    "back end",
    "react",
    "rust",
    "typescript",
    "javascript",
    "java",
    "kotlin",
    "python",
    "golang",
    "php",
    "angular",
    "devops",
    "devsecops",
    "sre",
    "site reliability",
    "data scientist",
    "data analyst",
    "analyste de donnees",
    "base de donnees",
    "dba",
    "database administrator",
    "machine learning",
    "intelligence artificielle",
    "cloud",
    "cybersecurite",
    "cybersecurity",
    "securite informatique",
    "securite des systemes",
    "pentester",
    "administrateur systeme",
    "administrateur systemes",
    "administrateur reseau",
    "administrateur reseaux",
    "sysadmin",
    "systemes et reseaux",
    "technicien informatique",
    "support informatique",
    "it support",
    "helpdesk",
    "help desk",
    "webmaster",
    "integrateur web",
    "developpement web",
    "android",
    "ios",
    "qa",
    "quality assurance",
    "test automation",
    "testeur",
    "architecte logiciel",
    "architecte solutions",
    "solutions architect",
];

/// True for developer and broader IT roles (network, data, security, support).
pub fn is_it_role(title: &str) -> bool {
    let tokens = words(title);
    !contains_any(&tokens, EXCLUDED_ROLES) && contains_any(&tokens, IT_ROLES)
}

pub fn mentions_java(text: &str) -> bool {
    contains_any(
        &words(text),
        &[
            "java",
            "j2ee",
            "jee",
            "java ee",
            "jakarta ee",
            "spring boot",
            "springboot",
        ],
    )
}

pub fn mentions_burkina(text: &str) -> bool {
    contains_any(
        &words(text),
        &["burkina", "burkina faso", "ouagadougou", "bobo dioulasso"],
    )
}

const NEGATIONS: &[&str] = &[
    "no", "not", "unable", "cannot", "t", "without", "none", "pas", "aucun", "aucune", "sans",
    "ni", "non",
];

/// Detects listings that offer or require relocation (or visa sponsorship).
/// A single negated mention ("we do not offer relocation", "Relocation statement:
/// this position is not eligible…") makes the whole listing negative, because
/// such listings often add conditional phrases like "if you need to relocate".
pub fn mentions_relocation(text: &str) -> bool {
    let tokens = words(text);
    let mut positive = false;
    for index in relocation_mentions(&tokens) {
        if is_negated(&tokens, index) {
            return false;
        }
        positive = true;
    }
    positive
}

fn relocation_mentions(tokens: &[String]) -> impl Iterator<Item = usize> + '_ {
    tokens.iter().enumerate().filter_map(move |(index, token)| {
        let relocation = token.starts_with("relocat")
            || token.starts_with("relocalis")
            || token.starts_with("demenag");
        let visa = token == "visa"
            && tokens[index + 1..tokens.len().min(index + 3)].iter().any(|next| {
                next.starts_with("sponsor")
                    || next == "support"
                    || next == "assistance"
                    || next == "accompagnement"
                    || next == "parrainage"
            });
        let sponsor_visa = token.starts_with("sponsor")
            && tokens[index + 1..tokens.len().min(index + 4)]
                .iter()
                .any(|next| next == "visa" || next == "visas");
        // "visa sponsorship" is reported once, at "visa".
        let sponsor_visa = sponsor_visa && !(index > 0 && tokens[index - 1] == "visa");
        (relocation || visa || sponsor_visa).then_some(index)
    })
}

fn is_negated(tokens: &[String], index: usize) -> bool {
    let before = &tokens[index.saturating_sub(8)..index];
    let after = &tokens[(index + 1).min(tokens.len())..tokens.len().min(index + 7)];
    before.iter().any(|token| NEGATIONS.contains(&token.as_str()))
        || after.iter().any(|token| {
            matches!(
                token.as_str(),
                "not" | "no" | "unavailable" | "ineligible" | "pas" | "non"
            )
        })
}

pub fn is_remote(work_mode: Option<&str>) -> bool {
    work_mode.is_some_and(|mode| words(mode).iter().any(|token| token == "teletravail"))
}

/// Work mode label from free text: remote, hybrid, otherwise on-site.
pub fn work_mode_from_text(value: &str) -> Option<String> {
    let tokens = words(value);
    if tokens.is_empty() {
        None
    } else if contains_any(&tokens, &["hybrid", "hybride"]) {
        Some("Sur site / hybride".to_owned())
    } else if contains_any(
        &tokens,
        &[
            "remote",
            "teletravail",
            "telecommute",
            "work from home",
            "anywhere",
            "distributed",
            "a distance",
        ],
    ) {
        Some("Télétravail".to_owned())
    } else {
        Some("Sur site".to_owned())
    }
}

const AFRICA_PLACES: &[&str] = &[
    "africa", "afrique", "african", "sub saharan", "mea", "middle east and africa", "algeria",
    "algerie", "angola", "benin", "botswana", "burkina", "burundi", "cameroon", "cameroun",
    "cape verde", "cap vert", "chad", "tchad", "comoros", "congo", "cote d ivoire",
    "ivory coast", "djibouti", "egypt", "egypte", "ethiopia", "gabon", "gambia", "ghana",
    "guinea", "guinee", "kenya", "lesotho", "liberia", "libya", "madagascar", "malawi", "mali",
    "mauritania", "mauritanie", "mauritius", "maurice", "morocco", "maroc", "mozambique",
    "namibia", "niger", "nigeria", "rwanda", "senegal", "seychelles", "sierra leone", "somalia",
    "south africa", "afrique du sud", "sudan", "tanzania", "togo", "tunisia", "tunisie",
    "uganda", "zambia", "zimbabwe", "lagos", "nairobi", "accra", "dakar", "abidjan",
    "casablanca", "cairo", "johannesburg", "cape town", "kigali", "ouagadougou",
];

const WORLDWIDE: &[&str] = &[
    "worldwide",
    "anywhere",
    "global",
    "globally",
    "monde entier",
    "partout dans le monde",
    "international",
    "all locations",
    "any location",
];

const EUROPE: &[&str] = &[
    "europe", "european", "eu", "eea", "uk", "united kingdom", "england", "london", "ireland",
    "germany", "deutschland", "berlin", "france", "paris", "spain", "madrid", "barcelona",
    "portugal", "lisbon", "netherlands", "amsterdam", "belgium", "poland", "italy", "sweden",
    "denmark", "norway", "finland", "switzerland", "austria", "czech", "romania", "greece",
    "cet", "cest",
];

const AMERICAS: &[&str] = &[
    "us", "usa", "u s", "united states", "america", "americas", "north america", "canada",
    "latam", "latin america", "south america", "brazil", "brasil", "mexico", "argentina",
    "colombia", "chile", "peru", "est", "pst", "new york", "san francisco",
];

const APAC: &[&str] = &[
    "apac", "asia", "asia pacific", "india", "australia", "new zealand", "japan", "singapore",
    "philippines", "indonesia", "vietnam", "china", "korea", "pakistan", "malaysia",
];

/// Where a remote job can be performed from, derived from its location first and,
/// when the location is only "Remote" or empty, from the description.
pub fn remote_scope(work_mode: Option<&str>, location: &str, description: &str) -> &'static str {
    if !is_remote(work_mode) {
        return "";
    }
    if let Some(scope) = scope_from(&words(location)) {
        return scope;
    }
    let description = words(description);
    if contains_any(
        &description,
        &[
            "work from anywhere",
            "work from anywhere in the world",
            "remote worldwide",
            "fully remote worldwide",
            "anywhere in the world",
            "from any country",
            "partout dans le monde",
        ],
    ) {
        return SCOPE_WORLDWIDE;
    }
    if contains_any(&description, &["remote in africa", "anywhere in africa", "across africa"]) {
        return SCOPE_AFRICA;
    }
    SCOPE_UNKNOWN
}

fn scope_from(tokens: &[String]) -> Option<&'static str> {
    if contains_any(tokens, &["emea"]) {
        Some(SCOPE_EMEA)
    } else if contains_any(tokens, AFRICA_PLACES) {
        Some(SCOPE_AFRICA)
    } else if contains_any(tokens, WORLDWIDE) {
        Some(SCOPE_WORLDWIDE)
    } else if contains_any(tokens, EUROPE) {
        Some(SCOPE_EUROPE)
    } else if contains_any(tokens, AMERICAS) {
        Some(SCOPE_AMERICAS)
    } else if contains_any(tokens, APAC) {
        Some(SCOPE_APAC)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_is_not_javascript() {
        assert!(mentions_java("Senior Java / Spring developer"));
        assert!(mentions_java("Développeur Spring Boot"));
        assert!(!mentions_java("Senior JavaScript engineer (Node.js)"));
    }

    #[test]
    fn relocation_ignores_negated_mentions() {
        assert!(mentions_relocation("We offer a relocation package to Berlin."));
        assert!(mentions_relocation("Visa sponsorship available for this role"));
        assert!(mentions_relocation("Aide à la relocalisation au Canada"));
        assert!(!mentions_relocation(
            "We are not able to offer visa sponsorship or relocation for this role."
        ));
        assert!(!mentions_relocation("Relocation is not available."));
        assert!(!mentions_relocation("Remote role, we can't sponsor visas"));
        assert!(!mentions_relocation(
            "Relocation Statement: This position is not eligible for relocation assistance."
        ));
        assert!(!mentions_relocation(
            "We do not offer relocation. If you need to relocate, this role is not for you."
        ));
        assert!(mentions_relocation(
            "Must be based in Dublin or be willing to relocate."
        ));
    }

    #[test]
    fn remote_scope_prefers_location_then_description() {
        let remote = Some("Télétravail");
        assert_eq!(remote_scope(remote, "Remote, EMEA", ""), SCOPE_EMEA);
        assert_eq!(remote_scope(remote, "Worldwide", ""), SCOPE_WORLDWIDE);
        assert_eq!(remote_scope(remote, "Lagos, Nigeria", ""), SCOPE_AFRICA);
        assert_eq!(remote_scope(remote, "USA Only", ""), SCOPE_AMERICAS);
        assert_eq!(
            remote_scope(remote, "Remote", "You can work from anywhere in the world."),
            SCOPE_WORLDWIDE
        );
        assert_eq!(remote_scope(remote, "Remote", ""), SCOPE_UNKNOWN);
        assert_eq!(remote_scope(Some("Sur site"), "Worldwide", ""), "");
    }

    #[test]
    fn it_roles_include_network_and_support_but_not_sales() {
        assert!(is_it_role("Ingénieur Systèmes et Réseaux"));
        assert!(is_it_role("Technicien informatique"));
        assert!(is_it_role("Backend Engineer - Java"));
        assert!(!is_it_role("Business Developer Afrique"));
        assert!(!is_it_role("Sales Engineer"));
        assert!(!is_it_role("Comptable principal"));
    }

    #[test]
    fn html_is_flattened_even_when_entity_encoded() {
        assert_eq!(
            html_to_text("&lt;p&gt;Java &amp;amp; Spring&lt;/p&gt;"),
            "Java & Spring"
        );
        assert_eq!(
            html_to_text("&amp;lt;li&amp;gt;relocation assistance&amp;lt;/li&amp;gt;"),
            "relocation assistance"
        );
        assert_eq!(html_to_text("Salary < 5k & bonus"), "Salary < 5k & bonus");
    }
}

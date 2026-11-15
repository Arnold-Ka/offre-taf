# Offre Taf

Application personnelle de recherche d’offres d’emploi de développeur, avec une API Rust, une interface React et PostgreSQL.

## Prérequis

- Rust et Cargo (édition 2021)
- Node.js 20+ et npm
- Docker avec Docker Compose

## Lancement local

1. Copier `.env.example` vers `.env` à la racine.
2. Démarrer PostgreSQL : `docker compose up -d db` (port hôte `5433`, pour éviter un conflit avec une installation PostgreSQL locale).
3. Dans un terminal, démarrer l’API : `cargo run -p offre-taf-api`.
4. Dans un autre terminal, installer et démarrer le frontend :

   ```sh
   cd frontend
   npm install
   npm run dev
   ```

5. Ouvrir <http://127.0.0.1:5173>. L’API est disponible sur <http://127.0.0.1:3000> et son contrôle de santé sur `/health`.

L’application est organisée en trois vues, plus les favoris :

- **Burkina Faso** : les offres IT (développement, réseau, data, sécurité, support) publiées directement par les employeurs installés au Burkina. Les avis de recrutement sont lus sur les sites officiels de la SONABEL, Coris Bank, Coris Assurances, Moov Africa, Telecel Faso, l’ANPTIC, 2iE, le CILSS et l’ARCEP (API WordPress), complétés par les communiqués relayés par Burkina24, les petites entreprises [SABMA Digital](https://sabmadigital.com/bf/carrieres/) et [Qualive](https://qualive-sarl.org/carriere), [BFemploi](https://www.bfemploi.com/) et toute offre localisée au Burkina sur les pages carrière d’entreprises internationales.
- **Télétravail** : les postes à distance trouvés sur les pages carrière d’une centaine d’entreprises (Greenhouse, Lever, Ashby : GitLab, Canonical, Automattic, Stripe, Spotify, Supabase, Andela, etc.) et dans l’index des employeurs Workable, ainsi que sur Remotive. Chaque offre indique la zone autorisée : monde entier, Afrique, EMEA, ou zone restreinte (Europe, Amériques…). Le filtre « Accessible depuis l’Afrique » ne garde que les trois premières.
- **Relocalisation** : les offres dont la description mentionne une relocalisation ou un parrainage de visa. Les mentions négatives (« pas de visa sponsorship ») sont ignorées.
- **Favoris** : toute offre qui mentionne Java (Java, Spring Boot, J2EE, mais pas JavaScript) est ajoutée automatiquement aux favoris. L’étoile permet d’ajouter ou de retirer n’importe quelle offre ; une offre retirée ne revient pas toute seule.

Au démarrage, l’API crée/met à jour la table `jobs` puis interroge toutes les sources. Ensuite, les pages carrière d’entreprises, Workable et les sites burkinabè sont relus toutes les 12 heures ; Wave/Yassir/Gozem/Djamo et Remotive toutes les 6 heures (Remotive recommande quatre appels par jour au maximum) ; OpenToAfrica, Emploi Dakar, BFemploi, SABMA et Qualive une fois par jour ; Arbeitnow toutes les 60 minutes par défaut (`COLLECT_INTERVAL_MINUTES`). Les annonces sont dédoublonnées par URL et chaque fiche garde un lien vers l’employeur ou le site d’origine. Une annonce qui n’apparaît plus pendant 21 jours chez une source qui répond toujours est supprimée, sauf si elle est en favori.

L’interface interroge `GET /api/jobs` avec les filtres facultatifs `q`, `country`, `work_mode`, `contract_type`, `relocation=true`, `unread=true`, `favorite=true`, `java=true` et `scope=africa`, et `GET /api/jobs/summary` pour les compteurs des onglets. `PATCH /api/jobs/{id}/read` marque une offre comme lue et `PATCH /api/jobs/{id}/favorite` (corps JSON `{"favorite": true}`) gère les favoris. Les statuts « visa / relocalisation mentionné » et la zone de télétravail sont déduits de mots-clés ; ils ne garantissent ni l’éligibilité ni une aide réelle.

Les annonces OpenToAfrica distinguent « ouvert à l’Afrique » et « éligibilité Afrique à vérifier » selon leur niveau de confirmation. Vérifie toujours les conditions sur l’annonce et chez l’employeur avant de postuler; le lien affiché mène à la fiche OpenToAfrica, qui renvoie vers la candidature d’origine.

Les sources externes peuvent être indisponibles ou changer leur format. Les erreurs sont journalisées par source; les offres déjà collectées restent accessibles dans la base.

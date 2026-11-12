# Mémoire de session — documents, recherche et performances

Dernière mise à jour : 11 novembre 2026.

Ce document conserve le contexte de travail et les décisions de la conversation,
y compris l’historique repris après condensation. Les mesures et états Git sont
des observations datées, pas une description garantie des versions futures.

## 1. Objectif et préférences utilisateur

- Améliorer la vitesse et la qualité des recherches scolaires des assistants.
- Éviter qu’une lecture partielle, un aperçu ou un résultat de recherche soit
  présenté comme une lecture exhaustive des documents/consignes.
- Conserver les outils MCP documentaires existants et l’isolation par compte.
- Aucun regroupement des comptes par personne.
- Aucun OCR, local ou cloud. Préparation des documents et conversion Office locales.
- Téléchargement = original ; étude = texte et images de toutes les pages de la
  fenêtre demandée, avec limites et continuation explicites.
- L’utilisateur a d’abord demandé de finir les documents, puis a autorisé les
  améliorations de recherche jugées utiles au projet.
- Les idées de graphe persistant, cartographie visible, bouton de cartographie,
  `/tree`, skills ou annotations générées par l’assistant ont été discutées.
  Elles ne constituent pas une demande actuelle d’implémenter ces sous-systèmes.
- L’utilisateur a confirmé que le comportement doit rester général pour tout
  compte Moodle compatible, pas uniquement Polytechnique.
- Dernière demande de code : optimiser la logique et les algorithmes internes,
  sans changer les visuels, les interfaces MCP, les entrées ou les sorties.
- Dernière demande documentaire : conserver la mémoire de cette session en Markdown.

## 2. Travaux antérieurs repris dans le contexte

Le contexte hérité indique que les éléments suivants étaient déjà réalisés :

- Activation/désactivation des comptes, routage MCP et schémas/instructions
  tenant compte du compte sélectionné.
- Outils `omnivox_deadlines` et `canvas_deadlines`.
- Livraison des fichiers originaux et préparation locale des documents d’étude.
- Résolution d’une ancienne erreur de compilation liée au store et à `Prepared`.

Un ancien résumé disait que la livraison documentaire était non commitée.
Plus tard, le dépôt était propre au début du travail de recherche. Au début du
dernier travail de performance, la recherche était déjà présente dans la base
Git. Ne pas réutiliser l’ancien état « tout est non commité » comme état actuel.
La conversation ne permet pas d’attribuer les commits intermédiaires.

## 3. Contrat documentaire retenu

Outils partagés : `moodle_file`, `omnivox_file`, `brightspace_file`, `canvas_file`.

### Téléchargement et étude

- `format=file`, par défaut, renvoie toujours le fichier original, y compris Office.
- Partager `links.download` lorsqu’il existe, pas une URL scolaire interne ni un
  jeton. Les liens durent jusqu’à trois heures tant que l’application fonctionne.
- Pour les PDF, `format=text` et `format=images` délivrent tous deux une fenêtre
  d’étude : quatre pages par défaut, huit maximum, texte étiqueté par page et
  image complète de chaque page sélectionnée.
- Texte PDF borné à 20 000 caractères. Suivre `nextPageFrom` et `textTruncated`.
- Les pages avec schémas vectoriels sont rendues même si elles contiennent aussi
  beaucoup de texte. Un rendu incomplet retourne une erreur, pas une omission.
- `find` recherche dans la couche texte. Un scan sans couche texte n’est pas
  rendu recherchable par OCR.
- Les extractions vides échouent explicitement ; les valeurs numériques TXT/CSV
  ne doivent pas disparaître lors de l’extraction.
- `coverage` distingue original, fenêtre de pages et blocs de texte. Les champs
  `allPagesDelivered`/`allBlocksDelivered` concernent la réponse courante, pas un
  historique cumulé des lectures ou la compréhension du modèle.

### Réutilisation et isolation

- La réponse expose `documentId` ; un appel suivant utilise `document_id` avec
  le même compte et outil, au lieu de refaire le téléchargement scolaire.
- Cache en mémoire de processus : cinq minutes, quatre originaux, 24 MiB au total.
- Réutilisation par digest et préparation une seule fois par entrée mise en cache.
- Portée liée au compte, service et empreinte des identifiants ; un ID ne contourne
  jamais les contrôles de configuration/activation du compte.
- Travailleur d’expiration périodique partagé avec les caches existants.
- Le cache de documents et l’index de métadonnées Moodle sont deux mécanismes
  distincts, avec des durées et contenus différents.

### Office et autres formats

- Word, PowerPoint et documents/présentations OpenDocument : conversion PDF locale
  isolée lorsqu’elle est disponible, puis même chemin d’étude que les PDF.
- Sous Linux : LibreOffice, Bubblewrap, `prlimit`, profil neuf, macros désactivées,
  aucun réseau, environnement hérité vide, home et identifiants scolaires absents.
- Limites documentées : 30 s murales, 25 s CPU, espace d’adressage 2 GiB, fichier
  de sortie/PDF limité à 32 MiB. Répertoire temporaire nettoyé.
- Aucun repli vers une conversion non isolée. Dépendance absente, plateforme non
  prise en charge ou erreur : repli texte explicite et `visualLimitations`.
- Les feuilles de calcul restent en cellules/lignes/feuilles ; pas de recalcul des
  formules. TXT/CSV restent du texte.
- Le repli texte utilise des blocs de 8 000 caractères, deux par appel au maximum,
  avec `sourceUnit`. Ces blocs ne sont pas des pages imprimées ou images de diapositives.
- Limite de téléchargement existante : 12 MiB. Archives Office : au plus 4096
  entrées et 32 MiB de taille décompressée déclarée.

Référence contractuelle : [`DOCUMENTS.md`](DOCUMENTS.md).

## 4. Diagnostic ayant motivé la recherche Moodle

Avant les améliorations :

- Recherche locale surtout sur les noms d’activités/fichiers, pas sur les longues
  descriptions ni les résumés de sections.
- Sélection d’au plus trois cours et récupération séquentielle de leur contenu.
- Un résultat de type « cours » pouvait remplacer un document non trouvé et
  empêcher le recours à la recherche globale Moodle dès la page zéro.
- Résumés limités à 240 caractères ; descriptions d’activités et contenus inline
  omis des vues compactes sans lecture détaillée directement accessible.
- `total` ne démontrait pas une couverture exhaustive de l’environnement.

Ces points ont été corrigés dans les outils existants, plutôt que par une
interface de graphe ou une nouvelle famille d’outils MCP.

## 5. Recherche et lecture détaillée maintenant présentes

### Recherche

- Recherche dans noms de cours/activités, noms de fichiers, texte des sections,
  descriptions complètes et contenu inline disponible.
- Résultats classés avec IDs cours/section/module, extrait et `matchedIn`.
- Accents normalisés ; prise en charge de variantes comme `TD5` / `TD Chapitre 5`.
- Le cas TD ne doit pas ignorer un autre terme comme « corrigé » ; un numéro de TP
  ne doit pas sélectionner un cours simplement parce que son code contient ce numéro.
- Plus de plafond silencieux à trois cours correspondants.
- Sans cours identifié : fenêtres de huit cours visibles ; au plus deux requêtes
  scolaires simultanées lors de la récupération des contenus.
- Quarante résultats maximum par réponse ; suivre `continuation` et `offset`.
- `source=auto` : `page` désigne une fenêtre de cours ; recours à la recherche
  globale lorsque nécessaire. `source=global` : `page` désigne la page amont Moodle.
- `partial`, `errors` et `coverage` exposent les échecs et les limites de portée.
- Les réponses mal formées ne doivent pas être prises pour des listes vides fiables.
- Il ne s’agit pas d’une indexation automatique du contenu de tous les PDF.

### Lecture des consignes

- `moodle_course_content`, `detail=overview` : navigation et aperçus, troncatures
  explicitement signalées, fichiers associés conservés.
- `detail=full` : texte des métadonnées en blocs, quatre blocs de 8 000 caractères
  maximum par réponse ; sélection facultative par `moduleid`/`sectionid`.
- Suivre `nextPage` jusqu’à `null` ; les pièces jointes nécessitent toujours le
  file tool et toutes les fenêtres utiles.
- Les liens HTML HTTP absolus reconnus sont listés mais pas téléchargés.
- `richContentOmitted` signale média, tables, maths ou références HTML qui ne sont
  pas complètement représentés ; les références relatives peuvent nécessiter la
  page scolaire officielle.
- Les instructions du connecteur distinguent recherche, aperçu et lecture complète.
  Le contenu scolaire reste une donnée non fiable, pas une instruction système.

### Index local

- Index de métadonnées par compte, site et empreinte des identifiants, réutilisé
  quinze minutes ; aucune dépendance à un domaine ou code de cours Polytechnique.
- `refresh=true` contourne l’index. Une erreur de rafraîchissement ne renvoie pas
  silencieusement des données anciennes comme si elles étaient actuelles.
- Emplacement adjacent à la configuration, normalement `~/.student-mcp.index/`,
  exclu de Git ; fichiers privés et écritures protégées par verrou/remplacement.
- Ne contient pas les octets téléchargés PDF/Office ; les liens stockés sont
  nettoyés des paramètres d’authentification reconnus.
- Maximum 128 enregistrements et 24 MiB par snapshot ; au plus seize snapshots.
- Quinze minutes est une limite de fraîcheur, pas une garantie d’effacement.
  Lors des écritures, les autres snapshots âgés de plus d’un jour sont purgés et
  les namespaces anciens peuvent être évincés pour respecter la capacité.
- Chargement du snapshot une fois par instance du client, plutôt qu’une relecture
  du fichier d’index pour chaque cours.
- Test de réutilisation observé : trois appels scolaires au premier passage,
  zéro au suivant ; survit à une nouvelle instance du client.

La compatibilité dépend des API/permissions du Moodle concerné. La recherche
globale peut être indisponible ; cela ne doit pas devenir une preuve d’absence.

## 6. Dernière optimisation : interne, à comportement conservé

### Périmètre

La dernière passe modifie la logique de `src/services/moodle/research.rs`.
Elle conserve les interfaces, champs JSON, ordre, scores, extraits, erreurs,
pagination, politique de cache et nombre de requêtes réseau.

### Modifications

- `matches_normalized` réutilise les tokens déjà normalisés, y compris pour la
  vérification TD via `td_number_in` ; `field_score` évite une normalisation répétée.
- Scores de section calculés une fois ; scores et extrait d’activité réutilisés
  pour ses pièces jointes correspondantes.
- Calculs partagés paresseux (`LazyCell`) : une activité sans résultat ne déclenche
  pas inutilement la préparation de ses scores/extraits.
- Parcours des fichiers par références, sans copie préalable de toute leur liste JSON.
- `DetailWindow` compte tous les blocs pour conserver les totaux, mais construit
  uniquement les quatre blocs retournés.
- Plus de concaténation de toutes les descriptions puis conversion en tableaux
  de caractères et génération de blocs immédiatement jetés.
- Texte Unicode, frontières de blocs et priorité des erreurs préservés, y compris
  lors d’un numéro de page qui déborde.

### Mesures locales

| Cas synthétique, sans réseau | Avant | Après |
| --- | ---: | ---: |
| Recherche, 8 activités × 16 pièces jointes, longues consignes | 882,201 ms | 158,948 ms |
| Première fenêtre détaillée d’un cours de 120 activités | 21,709 ms | 0,433 ms |
| Fenêtre détaillée 70 du même cours | 22,201 ms | 0,503 ms |

Médianes de cinq échantillons de trois appels, après un appel de chauffe ; fixtures
créées hors chronométrage ; profil test habituel, pas release. Construction et
destruction des résultats incluses. Ce ne sont pas des mesures de latence scolaire
réelle ni une promesse d’accélération identique pour toutes les requêtes.

Méthode reproductible : [`PERFORMANCE.md`](PERFORMANCE.md).

## 7. Preuves et vérifications

### Dernière passe performance

- Avant modification : `cargo test --offline --quiet`, 273 réussis, 12 ignorés.
- Ajout de deux tests d’empreinte des sorties complètes, capturées sur l’ancienne
  implémentation avant de changer l’algorithme.
- Recherche : ordre, scores, champs, extraits, termes répartis entre champs, TD,
  accents, Unicode, requêtes vides et absences de correspondance.
- Détail : sélecteurs, textes vides, frontières 7999/8000/8001 et 31999/32000/32001,
  pages suivantes/hors limites, Unicode multioctet et priorité des erreurs.
- Après optimisation : suite complète, **275 réussis, 13 ignorés**. Le treizième
  test ignoré est le benchmark manuel, exécuté séparément avec succès avant/après.
- Clippy a demandé `saturating_mul` au lieu de `checked_mul(...).unwrap_or(MAX)`.
  Correction faite ; les deux empreintes ont été revérifiées ensuite avec succès.
- `cargo clippy --offline --all-targets -- -D warnings` : succès final.
- `git diff --check` : succès. `cargo fmt` effectué.
- Graphe Graphify rafraîchi pendant cette passe ; ses sorties restent hors Git.

```sh
cargo test --offline --quiet
cargo test --offline --lib pre_optimization_fingerprint
cargo test --offline --lib benchmark_metadata_cpu -- --ignored --nocapture --test-threads=1
cargo clippy --offline --all-targets -- -D warnings
git diff --check
```

Ne pas modifier les empreintes de `research/performance.rs` simplement pour faire
passer un changement qui aurait modifié les résultats.

### Vérifications historiques

- Fin de la livraison documentaire : 252 tests réussis, 12 ignorés ; Clippy,
  `git diff --check` et vérification de syntaxe JavaScript réussis.
- Test explicite du sandbox Office isolant home/environnement/réseau : réussi.
- Fin de la recherche Moodle : 273 tests réussis, 12 ignorés ; compilation
  `cargo build --offline`, Clippy et `node --check src/resources/web/app.js` réussis.
- Ces vérifications historiques ne valident pas d’éventuels changements ultérieurs
  effectués par un autre travail en parallèle.

## 8. Limites et questions encore ouvertes

- Au dernier constat lié à Office, LibreOffice était absent : conversion réelle
  Office → PDF non validée, contrairement au sandbox et aux replis texte.
- Vérification optionnelle à reprendre avec les dépendances installées :

  ```sh
  cargo test --offline --lib office_sandbox_hides_home_environment_and_host_network -- --ignored
  cargo test --offline --lib office_conversion_renders_a_real_docx -- --ignored
  ```

- Consommation réelle des images/resources par Claude, ChatGPT et Grok non validée
  de bout en bout. Le serveur ne garantit pas ce que chaque client transmet au modèle.
- Aucun jeu représentatif de questions scolaires réelles fourni par l’utilisateur
  pour mesurer le rappel/qualité de recherche en production.
- Pas de recherche intégrale automatique à travers les octets de tous les PDF.
- Pas de suivi cumulatif de toutes les pages lues dans plusieurs conversations,
  ni de garantie de compréhension/obéissance d’un modèle distant.
- Une capture montrait un timeout de classificateur de sécurité côté assistant.
  La réponse donnée l’interprétait comme un contrôle client ayant expiré avant
  autorisation, pas comme la preuve d’un bug Moodle/overview. Aucun diagnostic
  indépendant via logs n’a été effectué. En cas de récurrence, vérifier serveur
  exécuté, annotations lecture seule exposées et connexion MCP actualisée.
- Pour utiliser une nouvelle version, relancer le serveur approprié et actualiser
  la connexion MCP lorsque ses schémas ont changé. Aucun déploiement n’est prouvé
  par la compilation ou les tests locaux.

## 9. État du dépôt lors de cette sauvegarde

Les fichiers suivants étaient déjà modifiés **avant** la passe performance :

- `src/resources/web/app.css`
- `src/resources/web/app.js`
- `tools/test-assistant-guides.mjs`

Ils appartiennent à un autre travail potentiel : ils n’ont pas été modifiés par
la passe performance et ne doivent pas être restaurés/écrasés pour la nettoyer.

Changements produits par la passe performance, encore présents dans le working tree :

- modification de `src/services/moodle/research.rs` ;
- ajout de `src/services/moodle/research/performance.rs` ;
- ajout de `tools/PERFORMANCE.md`.

La présente demande ajoute `tools/SESSION-MEMORY.md` et un repère dans `AGENTS.md`.
Aucun commit, push ou déploiement n’a été exécuté pendant la passe performance
ou cette sauvegarde documentaire. Revérifier `git status` avant de poursuivre.

## 10. Carte des fichiers utiles pour la reprise

| Chemin | Rôle |
| --- | --- |
| `src/services/moodle/mod.rs` | Client Moodle, sélection des cours, normalisation, appels et tests historiques |
| `src/services/moodle/research.rs` | Snapshots de contenu, recherche classée, overview et détail paginé |
| `src/services/moodle/research/tests.rs` | Régressions de recherche, continuation, cache, détails et routage MCP |
| `src/services/moodle/research/performance.rs` | Empreintes de compatibilité avant/après et benchmark CPU explicite |
| `src/services/moodle/index.rs` | Stockage privé borné, fraîcheur, isolation et rétention des métadonnées |
| `src/services/moodle/download.rs` | Cache court de téléchargements Moodle |
| `src/mcp.rs` | Schémas, routage par compte, sélecteurs documentaires, instructions connecteur |
| `src/delivery.rs` | Original/étude, fenêtres, blocs MCP et couverture |
| `src/delivery/store.rs` | Cache process-local, IDs documentaires et préparation réutilisée |
| `src/delivery/pdf.rs` | Extraction, recherche texte et rendu des fenêtres PDF |
| `src/delivery/office.rs` | Conversion locale isolée et tests optionnels |
| `src/delivery/tests.rs` | PDF synthétiques, images vectorielles, CSV, erreurs et Office |
| `src/document.rs` | Extraction Office/tableurs et validation des archives |
| `src/pdf.rs` | Espaces temporaires et sous-processus bornés |
| `src/cache.rs` | Travailleur d’expiration |
| `tools/DOCUMENTS.md` | Contrat des documents et limites de conversion |
| `tools/GUIDE.md` | Guide utilisateur, index/recherche/lecture et maintenance |
| `tools/MCP-CONVENTIONS.md` | Conventions compte/fichier/téléchargement/étude |
| `tools/PERFORMANCE.md` | Mesures CPU et protocole reproductible |

Pour la suite, partir d’un coût observé ou d’un exemple de recherche manquée.
Conserver les frontières de comportement et exécuter la preuve ciblée avant/après,
plutôt que d’élargir automatiquement vers une refonte, un graphe ou un nouvel outil.

## 11. Correctifs de la v0.8.1 (installation propre, Linux)

Huit défauts reproduits sur une installation propre de la v0.8.0 :

1. **Chapitres de livre Moodle** : `pluginfile.php/<ctx>/mod_book/chapter/<id>/index.html`
   est du HTML légitime. Il était rejeté comme « page de connexion ». Il est
   maintenant téléchargé par le même endpoint authentifié, puis livré en original
   ou en blocs texte. Seuls un vrai formulaire de mot de passe, un HTML inattendu,
   un statut HTTP non 200 ou une redirection produisent une erreur distincte.
   Aucun appel API séparé, donc aucun risque de marquer le chapitre comme vu.
2. **PATH Linux** : `install.sh` crée `~/.local/bin/student-mcp` en lien vers
   `~/.student-mcp/bin/student-mcp`, préserve une commande étrangère existante et
   affiche la ligne `PATH` à ajouter si le dossier n’y est pas déjà.
3. **URL des réglages** : `up` l’écrit dans le terminal et dans
   `~/.student-mcp/window.log` (permissions 0600), y compris si le navigateur
   échoue. `student-mcp settings-url` retrouve l’URL d’une instance en cours en
   lisant le fichier de découverte et en vérifiant `/state` ; lecture seule.
4. **Guide Claude en français** : « Continue », « No sign in », « Add » et les
   champs d’authentification sont traduits ; les noms exacts peuvent rester entre
   guillemets ou dans le texte source, jamais comme seule interface française.
5. **390 px** : le badge de version ne chevauche plus le bouton retour de
   « Configurer un assistant » (marge haute restaurée). Le nom d’hôte de la fiche
   compte ne se coupe plus à l’intérieur d’un label de domaine (`<wbr>` après les
   points) ; au pire il passe à la ligne entre deux labels.
6. **moodle_assignments** : résumé borné (id, cmid, titre, cours, échéance,
   fenêtre de remise, avertissements) au lieu de ~137 Ko de JSON brut. Utilise le
   même identifiant avec `moodle_assignment_status` pour la remise et la note.
7. **Note de moodle_file** : elle ne mentionne `links.download` que si le champ
   `links` existe réellement ; sinon elle décrit la ressource MCP jointe.
8. **`--help`** cite Canvas.

Preuves ajoutées : fixtures HTTP locales pour HTML de chapitre, logins/challenges,
redirections, résumé de devoirs et réponse malformée ; test d’installation pour
symlink, PATH, conflit, mise à jour et conservation de la configuration ; test CLI
de découverte d’URL et de non-modification ; tests Playwright 320/390 px, badge,
hôte et traduction ; CI Linux exécute aussi le smoke web et le guide assistants.

Version : **0.8.1**, une seule release. Commit, PR, CI, fusion puis tag unique.

## 12. Test réel v0.8.1 (Claude, ChatGPT, Grok ; Gemini hors zone US)

Cours, liens, échéances, notes et PDF fonctionnaient. Corrections ajoutées :

- `moodle_course_content` : vue par défaut bornée à 6 sections, 8 activités et
  6 fichiers par source, avec `nextPage`, `moduleCount`/`fileCount`, aperçus
  réduits et `truncated`. URLs directes `course/view.php?id=…&section=…`.
  Groupes de l'étudiant (`core_group_get_course_user_groups`) et groupes du cours,
  marquage `group`/`yourGroup`, filtre optionnel `group_only`. Dates
  `timemodified` par fichier. Les groupes sont mis en cache avec l'index du compte.
- `moodle_grades` avec `courseid` : poids Moodle (`weightraw`/`weightformatted`) et
  total Moodle ; si l'API ne fournit pas les poids, `weightsAvailable=false` et
  message explicite, sans calcul inventé.
- Gemini web : limite géographique US confirmée par l'utilisateur, pas un défaut
  du serveur. Aucun changement de transport ; documentation ajoutée.
- Journal local des requêtes MCP : `~/.student-mcp.mcp.log`, 0600, rotation 1 MiB,
  méthode, chemin **rédigé**, statut, user-agent. Jamais d'en-têtes, de corps ni de
  jeton. `STUDENT_MCP_TEST_LOG` redirige les écritures en test.
- Liens de fichiers : TTL réduit de 3 h à 30 min. Les descriptions d'outils
  demandent de ne pas coller `links.download` dans la conversation sans demande
  explicite de téléchargement.
- Les empreintes de détail sont mises à jour **une fois** pour le nouveau champ
  `groups` et l'URL de section ; ce changement de sortie est intentionnel.

Tests ajoutés : pagination/groupes/URLs/dates, taille bornée d'une grosse vue,
poids de notes et absence de poids, journal (rédaction, permissions, rotation),
validation de `group_only`.

## 13. Version 0.8.2

Les fonctionnalités du commit `f3fec5b` étaient publiées sous le tag `v0.8.1`
déjà existant. Une première tentative les a étiquetées `v1.2.0` : erreur de
numérotation, release et tag supprimés, remplacés par **0.8.2** (pas de
réécriture d'historique, `v0.8.1` et antérieurs intacts).

`Cargo.toml`/`Cargo.lock`, `README.md`, `CHANGELOG.md` et les captures README
portent la version 0.8.2 ; le reste (UI, installeurs, DMG, `setup.exe`) lit
`CARGO_PKG_VERSION`. Tag unique `v0.8.2` : la CI construit et publie
`setup.exe`, `dmg`, Linux x64/arm64 et `SHA256SUMS.txt`, et marque la release
Latest.

## 13. Mise à jour v0.8.3

Retours du test réel et correctifs :

- Sous-sections `mod_subsection` : les sections déléguées (`component`, `itemid`)
  sont rattachées au module parent, exposées sous `subsections`, et parcourues par
  la recherche, l'aperçu et `detail=full`.
- `group_only` : filtre selon `uservisible` et les conditions d'availability
  `group`/`grouping` de Moodle. Les noms de sections ne servent plus à deviner un
  groupe (`group`/`yourGroup` supprimés).
- `moodle_resources` : `sectionid` et nom de section réels, y compris dans une
  sous-section.
- Notes : repli sur `gradereport_user_get_grades_table` quand les éléments
  n'exposent pas de poids ; `weightsSource`, et absence explicite si Moodle ne
  fournit rien.
- Devoirs : `submissionStatus` rempli (max 60 appels), statut/`timeSubmitted`/
  `canSubmit` cohérents avec `canSubmitReason`.
- Échéances : `include_expired=true` inclut les éléments échus dans la fenêtre.
- Recherche : métadonnées de couverture compactées ; `pdfTextSearched=false` et
  note explicite (aucun cache texte PDF consulté).
- `sectionid` accepté en mode overview ; `courseids` ne produit plus l'erreur
  « fournir un courseid ».
- Version 0.8.3, tests ajoutés pour chaque point.

## 14. v0.8.4

- `moodle_assignment_status` accepte un cmid en plus de l'id d'instance ; résolution
  par `core_course_get_course_module`, erreurs claires pour un module non-devoir
  ou un id inconnu, description du paramètre explicite.
- `moodle_course_content detail=full` accepte `query` et renvoie dans le même appel
  le texte complet des éléments trouvés, avec pagination.

### v0.8.4 — suite

- `moodle_assignment_status` accepte un cmid via `core_course_get_course_module`,
  avec erreurs claires pour un module non-devoir ou un id inconnu.
- `detail=full` accepte `query` et renvoie le détail complet des éléments trouvés.
- Liens `/s/` : 24 h par défaut, réglable par `link_ttl_hours` (1–168), exposé en
  secondes/minutes. Pas de tag ni release 0.8.4 (changement de processus).

### v0.8.4 — devoirs allégés

- `moodle_assignments` : un seul appel de liste ; statut seulement pour les
  devoirs ouverts ou dus sous 14 jours. `full_status=true` pour tout.
- Cache court des statuts (3 min), budget de 10 s, et `statusesMissing` indique
  les statuts omis et comment les obtenir.

## 14. Version 0.8.4

Release 0.8.3 (`v0.8.3`, commit `5f71ffc`) contenait les sous-sections, le
filtrage de groupes par `uservisible` et les poids de notes. 0.8.4 regroupe
ensuite : `cmid` accepté par `moodle_assignment_status` (résolution via
`core_course_get_course_module`), `query` avec `detail=full`, liens de fichiers
à 24 h par défaut (`link_ttl_hours`, 1–168), et statut de remise chargé
seulement pour les travaux ouverts ou dus sous 14 jours (`full_status`).

Version 0.8.4 dans `Cargo.toml`/`Cargo.lock`, `README.md`, `CHANGELOG.md` et les
captures README. Un seul tag `v0.8.4` ; la CI publie `setup.exe`, `dmg`, Linux
x64/arm64 et `SHA256SUMS.txt`, release marquée Latest.

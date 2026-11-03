# Vérification réelle Omnivox dans OpenCode

Date : 2026-11-03.

## Connexion et périmètre

- Serveur OpenCode `student`, transport MCP stdio local.
- Binaire : `target/release/student-mcp`, reconstruit puis reconnecté dans
  OpenCode. La configuration existante n'a pas été remplacée.
- Compte Omnivox de Maisonneuve, session déjà connectée ; trimestre `20263`
  découvert avec `omnivox_terms`, sans mot de passe ni nouveau MFA.
- Catalogue : **46 outils Omnivox**, dont **34 lectures et 12 écritures**.
  Les 12 écritures portent bien `readOnlyHint: false` et `destructiveHint: true`.
- **Aucun outil d'écriture appelé sur le compte réel.** Aucun envoi,
  classement, marquage lu/consulté, suppression, acquittement ou dépôt demandé.
  Les tests unitaires d'écriture utilisent uniquement des serveurs simulés.
- Les réponses sont résumées ci-dessous : aucun message privé, note, nom de
  personne, identifiant de fichier/message ou secret n'est enregistré ici.

## Résultats des 34 outils de lecture

« OK » signifie qu'au moins un appel réel a renvoyé une réponse exploitable
dans OpenCode, pas que toutes les variantes ou tous les collèges sont validés.

| Outil | Résultat | Observation |
| --- | --- | --- |
| `omnivox_status` | OK | Session confirmée ; compteur brut 9, incluant la ligne agrégée de Léa |
| `omnivox_terms` | OK | Trimestre courant et 16 trimestres disponibles |
| `omnivox_my_courses` | OK | 8 cours réels et une ligne agrégée |
| `omnivox_schedule` | OK | Modèle d'horaire reçu |
| `omnivox_course_documents` | OK | 31 documents pour le cours sélectionné |
| `omnivox_assignments` | OK | Sommaire de 8 cours, puis listes par cours ; liste vide et liste non vide testées |
| `omnivox_grades` | OK | Sommaire de 8 cours et détail d'un cours avec 6 évaluations |
| `omnivox_calendar` | Échec | Calendrier scolaire : erreur de requête/expiration du délai |
| `omnivox_announcements` | OK | Sommaire de 8 cours |
| `omnivox_course_announcements` | OK | Réponse valide avec une liste vide pour le cours sélectionné |
| `omnivox_documents_summary` | OK | Sommaire de 8 cours |
| `omnivox_assignment_detail` | OK | Travail réel reçu, avec `isAddConsultation: false` |
| `omnivox_absences` | OK | Sommaire de 8 cours |
| `omnivox_teachers` | OK | 9 entrées |
| `omnivox_teacher_detail` | OK | Modèle de coordonnées reçu |
| `omnivox_websites` | OK | 9 entrées |
| `omnivox_notifications` | OK | 5 entrées de tableau de bord, sans dismissal |
| `omnivox_services` | OK | Manifeste des services reçu |
| `omnivox_college_news` | OK après correction | Page de 2 actualités reçue dans OpenCode |
| `omnivox_communities` | OK | 2 communautés et 6 types de filtres |
| `omnivox_college_events` | OK | Cartes HTML et filtres reçus |
| `omnivox_cancelled_classes` | OK | Réponse valide, aucune annulation |
| `omnivox_college_documents` | OK | Modèle de documents/messages officiels reçu, sans acquittement |
| `omnivox_forms` | OK | 2 formulaires listés, aucun ouvert ni soumis |
| `omnivox_mio_folders` | OK | 10 dossiers |
| `omnivox_mio_messages` | OK | Page de 2 messages puis pagination vers 2 messages plus anciens |
| `omnivox_mio_message` | OK | Corps d'un message reçu, sans appel de marquage lu |
| `omnivox_mio_search` | OK | Page de 2 résultats |
| `omnivox_mio_contacts` | OK | 4 contacts |
| `omnivox_mio_people` | OK | Réponse d'annuaire reçue ; noms non conservés dans ce rapport |
| `omnivox_calendar_page` | Partiel | Type `lea` : 13 événements ; type `scolaire` : erreur de requête/délai |
| `omnivox_calendar_month` | Partiel | Type `lea` : 45 événements ; type `scolaire` : erreur de requête/délai |
| `omnivox_schedule_week` | OK | Horaire de la semaine demandée reçu |
| `omnivox_file` | OK | Téléchargement d'un PDF puis extraction de sa première page |

Bilan : **31 OK, 2 partiels et 1 échec**. Les trois cas en échec concernent
le calendrier scolaire ; les vues calendrier Léa testées fonctionnent.

## Correction effectuée pendant les essais

Le premier appel aux actualités a révélé un résultat MCP invalide :
`structuredContent` contenait un tableau alors que le protocole exige un objet.
La couche de réponse MCP conserve maintenant les objets tels quels et enveloppe
les tableaux/scalaires sous `items`. Le bloc texte conserve le JSON d'origine.

La correction a été reconstruite, chargée dans OpenCode et vérifiée par les
lectures réelles des actualités, contacts et recherches de personnes. Un test
de régression couvre tableaux, booléens, chaînes, null et objets.

Validation locale finale : `cargo test` — **170 réussis, 6 ignorés**.

## Limites

- La cause précise des erreurs du calendrier scolaire n'est pas encore établie.
  Aucun changement spéculatif de son protocole n'a été appliqué.
- Un seul compte et un seul établissement ont été testés.
- Les autres formats/catégories de fichiers et toutes les variantes de filtres
  n'ont pas été testés sur le compte réel.
- Les écritures n'ont été qu'inspectées dans le catalogue et testées sur
  simulateurs, pas exécutées sur le compte réel.
- Comme pour toute consultation du portail, Omnivox peut journaliser les
  lectures/téléchargements et renouveler la session. Aucun changement métier
  n'a été demandé par les tests réels.

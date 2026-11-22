# Site vitrine de student-mcp

Ouvrir `index.html` dans un navigateur, ou lancer depuis la racine du dépôt :

```sh
python3 -m http.server 8000 --bind 127.0.0.1 --directory website
```

Visiter <http://127.0.0.1:8000>. Pour publier, déployer le contenu de `website/`
sur un hébergement statique HTTPS. Aucun serveur applicatif ni compilation.

Les boutons Windows et macOS utilisent les fichiers de la dernière release stable.
Sous Linux, la commande utilise le même script que le README du projet.
Les visiteurs n’ont pas à ouvrir le dépôt pour installer l’application.
Le bouton d’accueil mène directement au fichier sur Windows et macOS lorsque
le système est reconnu. « Obtenir l’app » et « Tous les systèmes » ouvrent la
fiche d’installation, où le sélecteur reste utilisable même sans JavaScript.

## Direction artistique

Les logos et l’extrait vidéo ChatGPT viennent de `src/resources/assets/`.
Les captures montrent l’interface `src/resources/web/` en français, avec les
données de démonstration de `tools/capture-readme.mjs`. La conversation est
illustrative : les trois réponses s’appuient sur la même fiche de labo fictive.
`site.css` reprend les couleurs, la typographie système, les cartes et les états
de pression de `src/resources/web/app.css`. Les ressources sont incluses dans le
site pour permettre un déploiement indépendant.

## Actualiser le contenu

- **Établissements** : lancer `python3 website/maintenance/sync-schools.py` à la
  racine après une modification de l’annuaire dans `src/resources/web/app.js`.
  Seuls les noms, sigles et plateformes sont exportés. La recherche se fait dans
  le navigateur, sans connexion à une école. Une entrée n’atteste pas que
  l’établissement autorise toutes les fonctions du connecteur.
- **Exemples et guides** : les capacités sont documentées dans
  `tools/{GUIDE,DOCUMENTS,OMNIVOX-TOOLS}.md`, les guides d’assistants dans
  `src/resources/web/app.js`. L’extrait vidéo est `guide-chatgpt-menu.mp4` ;
  il est lancé à la demande, avec le chemin correspondant sous le lecteur.
- **Fiche de démonstration** : imprimer `maintenance/laboratoire-2.html` en PDF
  (A4, arrière-plans activés, sans en-têtes) pour actualiser
  `assets/laboratoire-2.pdf`. `assets/fiche.webp` en est une vignette.

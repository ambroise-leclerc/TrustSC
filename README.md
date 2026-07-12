# MduX-rust

🇬🇧 [English version](README.en.md)

**Un framework 100 % Rust pour produire des logiciels de dispositifs médicaux alignés sur les
exigences des normes IEC 62304 (processus du cycle de vie logiciel), ISO 13485 (système de
management de la qualité) et ISO 14971 (gestion des risques).** Il fournit des briques Classe B/C
directement réutilisables — une IHM Vulkan (Classe B) et Vulkan SC (Classe C), une inférence IA
embarquée sans SOUP — et, plus largement, une génération de preuves conçue pour alimenter le SMQ
du fabricant et le dossier technique remis à l'organisme notifié.

## La difficulté du logiciel Classe B/C

Les équipes qui développent un logiciel Classe B ou Classe C selon l'IEC 62304 rencontrent
toujours les mêmes frictions : une traçabilité exigence → vérification maintenue à la main, qui
finit par diverger du code ; une surface de dépendances tierces (SOUP) qui croît le plus vite
justement dans les couches UI et IA/ML les plus visibles pour l'opérateur ; des preuves qu'un
auditeur ne peut pas reproduire facilement ; et des éléments d'IHM critiques dont le comportement
est difficile à garantir dès que la pile de rendu alloue de la mémoire ou met en forme du texte à
l'exécution.

## Ce que fournit MduX-rust aujourd'hui

MduX-rust découpe le workspace en trois zones de confiance — un cœur gouverné et sans `unsafe`
(`crates/`), des adaptateurs qui isolent les liaisons Vulkan/fenêtrage natives (`adapters/`), et
un outillage host-only qui ne part jamais dans un artefact runtime (`tools/`) — pour que l'effort
de revue se concentre là où il compte. Chaque pipeline d'asset (polices, images, shaders, et
désormais poids ML) compile une source en preuve committée et vérifiée par empreinte
(`package.json` + `report.json`), re-contrôlée automatiquement en CI plutôt qu'affirmée à la main.
En complément, `mdux-governance` fournit de vrais types `Requirement`/`Hazard`/
`VerificationCase`/`AuditEvent`, avec export structuré de la matrice de traçabilité et de la
piste d'audit.

L'exemple phare de cette approche est le pipeline ML : un classifieur embarqué (`Classifier1D`)
écrit entièrement en Rust `#![forbid(unsafe_code)]` — pas d'ONNX Runtime, pas de PyTorch — dont
les poids sont des données versionnées et compilées à part. Remplacer un modèle de démonstration
issu de Hugging Face par les propres poids cliniquement qualifiés d'un fabricant ne change aucune
ligne de code d'inférence ou d'application, et le moteur échoue de façon contrôlée au démarrage si
son propre auto-test de référence ne se reproduit pas bit à bit. Voir `examples/class_c_monitor`,
le moniteur de profondeur d'anesthésie Acme NeuroSense 500, pour la démonstration complète et
fonctionnelle.

Ceci reste un framework et un ensemble d'API de conformité — pas un dispositif médical certifié,
et pas un substitut au jugement d'ingénierie propre du fabricant.

## Organismes notifiés et audits

Pour un auditeur d'organisme notifié, le découpage en zones de confiance signifie que la revue de
code approfondie peut se concentrer sur un cœur gouverné restreint plutôt que sur l'ensemble du
graphe de dépendances ; les artefacts de preuve générés portent leur propre empreinte SHA-256 et
sont vérifiés par octet en CI plutôt que ré-audités à la main à chaque version ; le registre SOUP
(`docs/governance/soup-register.toml`) a déjà la forme — fournisseur, licence, chemin
d'intégration, mesures de maîtrise du risque — attendue dans la section SOUP d'un dossier
technique ; et 18 ADR acceptées documentent la logique de conception derrière chaque frontière.
Rien de tout cela ne remplace le SMQ propre du fabricant, son dossier de gestion des risques, ou
sa relation avec son organisme notifié — voir **[Conformité réglementaire](docs/regulatory-compliance.md)**
(en anglais) pour le traitement complet, avec une liste explicite de ce que ce projet fournit et
ne fournit pas.

## Feuille de route

MduX-rust fournit aujourd'hui des briques d'exécution (IHM, IA, gouvernance) et des preuves
générées automatiquement. Pour couvrir les besoins communs à la majorité des logiciels de
dispositifs médicaux Classe B et Classe C, deux chantiers sont priorisés ensuite :

- **Des références normatives exploitables par les LLM des équipes de développement.** Le projet
  C++ historique du framework (`MduX`) a déjà défriché cette approche : une version markdown
  d'IEC 62304 découpée en modules par processus du cycle de vie, un document « AI Reference » par
  norme, et des schémas JSON d'automatisation (classification de sécurité, matrice de
  traçabilité, gestion des risques...) pensés pour être consommés par un agent IA pendant le
  développement. MduX-rust va porter et adapter ce corpus — IEC 62304, ISO 13485, ISO 14971, puis
  IEC 62366-1 (aptitude à l'utilisation) et IEC 81001-5-1 (cybersécurité du cycle de vie
  logiciel) — pour que l'assistant IA d'un développeur puisse citer le texte exact d'une clause et
  générer du code ou de la documentation alignés sur l'exigence correspondante, sans remplacer le
  jugement d'un expert réglementaire.
- **Des templates de dossier de développement logiciel réglementaire.** Une arborescence
  `software_development_file/regulatory/` fournira, norme par norme, un squelette de document que
  le fabricant complète et adapte à son propre produit plutôt que de partir d'une page blanche :

  ```text
  software_development_file/
  └── regulatory
      ├── IEC_62304
      │   ├── SAD.md      # Software Architecture Design
      │   ├── SDD.md      # Software Design Description
      │   └── SOUP.md     # Liste et justification des SOUP
      ├── IEC_62366
      │   └── Usability_Engineering_File.md
      ├── IEC_81001
      │   └── Cybersecurity_SAD.md
      ├── ISO_13485
      │   └── README.md
      └── ISO_14971
          └── Risk_Management_File.md
  ```

  À terme, l'export structuré de `ComplianceProgram` (matrice de traçabilité, piste d'audit)
  devrait pouvoir alimenter directement ces documents plutôt que rester un artefact à recopier à
  la main.

Ces deux chantiers ne sont pas encore livrés dans MduX-rust — ils figurent ici pour que la feuille
de route soit visible dès la page d'accueil du projet. Détails et suivi :
**[Conformité réglementaire](docs/regulatory-compliance.md)** (en anglais).

## Démarrage rapide

```bash
source $HOME/.cargo/env

cargo build                                  # tout compiler
cargo test                                   # exécuter tous les tests
cargo run -p hello_world                     # exemple le plus simple (ouvre une fenêtre Vulkan)
cargo run -p hello_world -- --headless-smoke # sans fenêtre, sans Vulkan — pour la CI
cargo run -p class_c_monitor                 # NeuroSense 500 : UI 3D + ML zero-SOUP
```

Référence complète des commandes et installation de Vulkan (en anglais) :
**[Getting started](docs/getting-started.md)**.

## Structure du workspace

| Répertoire | Contenu |
|---|---|
| `crates/` | Cœur gouverné : modèle device/conformité, politique UI, pipelines texte et ML, la façade `mdux`. |
| `adapters/mdux-vulkan-winit` | L'adaptateur de présentation Vulkan + winit — le seul crate touchant aux liaisons natives de fenêtrage/graphisme. |
| `tools/` | Outillage host-only de bake/verify pour les preuves de polices, images, shaders et modèles ML. |
| `examples/` | `hello_world` (plus petite démo de fumée), `class_b_device`, `class_c_monitor` (NeuroSense 500), `class_c_vulkansc_device`. |

Cartographie complète des crates et logique des zones de confiance (en anglais) :
**[Architecture](docs/architecture.md)**.

## Prérequis Vulkan

```bash
# Ubuntu / Debian
sudo apt-get install libvulkan1 libvulkan-dev vulkan-tools

# macOS
brew install vulkan-loader molten-vk vulkan-tools
```

Nécessaire seulement pour le chemin avec fenêtre — `--headless-smoke` fonctionne sans loader
Vulkan. Configuration complète par plateforme (en anglais) :
**[Getting started](docs/getting-started.md#vulkan-prerequisites)**.

## Documentation complète

La documentation approfondie est maintenue en anglais pour toucher le plus large public possible,
y compris les évaluateurs techniques d'organismes notifiés :

- **[Accueil de la documentation](docs/README.md)**
- **[Conformité réglementaire](docs/regulatory-compliance.md)** — IEC 62304, organismes notifiés,
  le mécanisme de preuve, la feuille de route réglementaire, et les limites de portée assumées
  honnêtement.
- **[Architecture](docs/architecture.md)** — zones de confiance, cartographie des crates, CI,
  gouvernance des assets.
- **[Getting started](docs/getting-started.md)** — parcours complets des exemples et référence des
  commandes.
- **[Architecture decision records](docs/adr/README.md)** — les 18 ADR acceptées.
- **[Référence du DSL MedUI](docs/dsl/overview.md)** — le langage `.medui` de description d'UI à la
  compilation.

## Licence

À finaliser.

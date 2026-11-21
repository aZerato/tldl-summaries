# TL;DW — Documentation du projet

**TL;DW** (Too Long; Didn't Listen) — application **crossplateforme**
(Windows / macOS / Linux) : on lui passe un fichier **audio ou vidéo**
(rencontre, entretien, réunion, podcast…), elle produit la **version
synthétisée** — **transcription** + **résumé structuré** — grâce à des
**modèles IA 100 % locaux** : pas d'Internet, pas de cloud, confidentialité
garantie.

> Ces fichiers sont la **référence vivante** du projet : à chaque décision ou
> fonctionnalité arrêtée, les documents sont mis à jour. Les choix majeurs
> sont tracés en ADR avec un statut.

_Dernière mise à jour : 21/11/2026_

## Index

| Document | Contenu |
|---|---|
| [architecture.md](architecture.md) | Contraintes, stack technique, structure du dépôt, pipeline, décisions (ADR) |
| [features.md](features.md) | Fonctionnalités : MVP + roadmap, critères d'acceptation, statuts |
| [models.md](models.md) | Modèles IA : tailles, quantifications, profils matériels, valeurs par défaut |

## Statuts

| Badge | Sens |
|---|---|
| ✅ | Fait / décidé |
| 🔧 | En cours |
| 📋 | À faire |
| ⏳ | À valider (recommandation posée, pas encore arrêtée) |
| 💡 | Idée (backlog) |

## État du projet

- **Phase** : post-MVP — MVP (F1→F6 + F11) fait ; **F1/F10/F13/F14/F15/F16/F19
  validés dans l'app**
- **En cours (🔧)** : F3, F4 — critères à mesurer (FR correct, < 60 s /
  10 min — F12)
- **À valider (📋)** : F2, F5, F6, F7, F12
- **À faire** : F8 (export), F17/F18 (mode avancé)
- **Choix arrêtés** : Tauri 2 + Rust · whisper.cpp · **llama.cpp intégré**
  (processus unique, 100 % portable) · FFmpeg embarqué · cible machine
  « Équilibrée » 16 Go
- **MVP** : import média → conversion vidéo→audio → transcription → résumé,
  double-vue (transcription + résumé), menu des modèles, récupération des
  modèles (opt-in)
- **UI** : refonte visuelle du 19/11/2026 — fond sombre quasi inchangé,
  accent orange unique (`src/index.css`)
- **Prochaine étape** : build macOS Apple Silicon (C2) — le code est
  prêt, la build doit se faire sur un Mac ; puis F17/F18 (mode avancé)

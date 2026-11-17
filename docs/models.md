# Modèles IA & profils matériels

Principe (C3) : **petit mais bon, quantifié, GPU optionnel**.
Règle de base du MVP : **ASR `small` + LLM 7B–8B Q4_K_M**.
Les deux moteurs ne se chevauchent pas dans le pipeline
(ASR d'abord, puis LLM) → la RAM se dimensionne au **maximum**,
pas à la somme.

## 1. ASR — whisper.cpp (fichiers GGUF, f16)

| Modèle | Fichier | RAM (est.) | Vitesse CPU récent* | Usage |
|---|---|---|---|---|
| tiny | ~75 Mo | ~0,5 Go | ~5× temps réel | test / ultra-léger |
| base | ~150 Mo | ~1 Go | ~3–4× temps réel | low-end, FR correct |
| **small** ⭐ | ~480 Mo | ~1,5 Go | ~1,5–2× temps réel | **défaut recommandé** |
| medium | ~1,55 Go | ~3 Go | ~1× temps réel | FR plus fin, CPU fort |
| large-v3-turbo | ~1,6 Go | ~3,5 Go | GPU conseillé | top qualité / rapidité |

\* Indicatif, à mesurer (F12). Quantisations Q5_K_M / Q8_0 possibles
(−10 à −20 % de RAM, perte négligeable).
Langue : détection auto avec biais FR ; option « forcer FR ».

## 2. LLM résumé — llama.cpp (GGUF, Q4_K_M par défaut)

| Famille | Fichier | RAM/VRAM (est.) | Remarque |
|---|---|---|---|
| Qwen2.5 3B | ~2,1 Go | ~3,5 Go | profil **Léger**, bon FR |
| Gemma 3 4B | ~2,7 Go | ~4 Go | excellent rapport/qualité |
| **Qwen2.5 7B / Llama 3.1 8B** ⭐ | ~4,7 Go | ~6–7 Go | **défaut Équilibré** |
| Gemma 3 12B | ~7,5 Go | ~9–10 Go | profil **Puissant** |

- Température : **0,3** (résumé = déterministe) — réglable (F6)
- Contexte : **8 k tokens** (10 min d'audio ≈ 1,5–2 k mots → largement couvert)
- Si la transcription dépasse le contexte : résumé par morceaux puis
  fusion (à préciser dans F4)
- Modèles gérés **directement par l'app** (catalogue GGUF, F6/F11) :
  pas de démon externe (ADR-003)

## 3. Profils (préréglages du menu — F6)

| Profil | Machine cible | ASR | LLM | Device | RAM min |
|---|---|---|---|---|---|
| **Léger** | 8 Go, CPU seul | base | Qwen2.5 3B Q4 | CPU | 8 Go |
| **Équilibré** ⭐ | 16 Go, GPU 6–8 Go ou CPU récent | small | 7B–8B Q4_K_M | auto (GPU si dispo) | 16 Go |
| **Puissant** | GPU 12–16 Go, 32 Go | medium / large-v3-turbo | 12B Q4_K_M | GPU | 32 Go |

## 4. Règles de gestion

- Téléchargement **opt-in** (F11) ; hors ligne = dépôt manuel dans `models/`
- Estimation affichée **avant** chaque téléchargement (F6)
- Un seul LLM chargé à la fois (chargement/déchargement géré par l'app — F6)
- Benchmarks (F12) : les valeurs mesurées remplacent les estimations ici

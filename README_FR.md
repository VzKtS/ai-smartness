<p align="center">
  <img src="./readme-src/ais-logo-name.png" alt="AI Smartness" width="400">
</p>

<p align="center">
  <strong>Memoire cognitive persistante pour agents IA</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-6.8.0-blue" alt="Version 6.8.0">
  <img src="https://img.shields.io/badge/language-Rust-orange" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="Licence MIT">
</p>

<p align="center">
  <a href="README.md">English</a> |
  <a href="README_FR.md">Francais</a> |
  <a href="README_ES.md">Espanol</a>
</p>

---

## Qu'est-ce qu'AI Smartness ?

AI Smartness est un runtime de memoire cognitive persistante pour agents IA. Il transforme Claude Code en un agent capable de maintenir un contexte semantique a travers de longues sessions, de detecter les connexions entre concepts, de partager des connaissances avec d'autres agents, et de reprendre le travail apres des semaines comme si vous veniez juste de vous absenter pour un cafe.

Le systeme fonctionne entierement en local — un LLM local (Qwen2.5-Instruct via llama-cpp-2 avec GPU Vulkan) gere toute l'extraction memoire sans aucun cout API. Aucune donnee ne quitte votre machine.

![AI Smartness Demo](./readme-src/ai-smartness.gif)

## Fonctionnalites cles

- **Memoire persistante** — Les threads capturent chaque interaction significative. Lectures de fichiers, modifications de code, decisions, raisonnements — tout est extrait et stocke dans des bases SQLite par agent
- **69 outils MCP** — Outils natifs pour le recall memoire, la gestion de threads, l'analyse de bridges, le shared cognition, la delegation de taches, la messagerie cognitive, et plus
- **Bridges semantiques** — Decouverte automatique des connexions entre threads via le systeme gossip (similarite cosinus + chevauchement de concepts). Permet des chaines de memoire associative
- **Systeme multi-agent** — Memoire isolee par agent avec shared cognition pour l'echange de connaissances inter-agents. Hierarchie de supervision, delegation de taches, cognitive inbox
- **Inference LLM locale** — Qwen2.5-Instruct 3B/7B via llama-cpp-2 (GPU Vulkan). Embeddings ONNX (all-MiniLM-L6-v2). Zero appels API pour les operations memoire
- **Engram Retriever** — Pipeline de consensus a 10 validateurs pour decider quels threads injecter dans chaque prompt. Processus en 3 phases : pre-filtrage par hash, scoring, consensus
- **Auto-amelioration** — File Chronicle, Mind Priority, Deep Recall, Session Handoff, Freshness Score, Annotation (v6.2-v6.8)
- **Panel GUI** — Navigateur visuel de threads, graphe DAG de bridges, hierarchie d'agents, editeur de configuration complet (Tauri/WebKit)
- **Systeme de hooks** — Integration transparente avec Claude Code via les hooks inject, capture, pretool et stop
- **4 modes d'execution** — CLI, Serveur MCP (JSON-RPC stdin/stdout), Daemon (arriere-plan), GUI (bureau)

![AI Smartness Dashboard](./readme-src/ai-smartness-dashboard.png)

## Demarrage rapide

```bash
# Cloner et compiler
git clone https://github.com/VzKtS/ai-smartness
cd ai-smartness
cargo build --release

# Telecharger ONNX Runtime + modele d'embeddings
ai-smartness setup-onnx

# Demarrer le daemon
ai-smartness daemon start

# Initialiser votre projet
cd /path/to/your/project
ai-smartness init

# Creer et selectionner un agent
ai-smartness agent add cod --role programmer
ai-smartness agent select cod

# Ouvrir Claude Code — l'injection memoire demarre automatiquement
```

Pour le support GUI, compiler avec `cargo build --release --features gui` (necessite webkit2gtk, gtk3, libsoup sous Linux).

## Vue d'ensemble de l'architecture

**Crate Rust unique** avec 4 modes d'execution :

```
Claude Code <--hooks--> ai-smartness <--IPC--> Daemon
                |                                 |
          Serveur MCP                     Prune / Gossip / Pool
           (69 outils)                   (traitement en arriere-plan)
                |
         3 bases SQLite :
         - agent.db     (memoire par agent)
         - registry.db  (registre global agents/projets)
         - shared.db    (shared cognition par projet)
```

**Flux d'injection memoire :**
1. L'utilisateur envoie un prompt -> le hook inject se declenche
2. L'Engram Retriever execute un consensus a 10 validateurs sur tous les threads actifs
3. Les meilleurs threads + bridges + etat de session sont injectes (limite 8 Ko)
4. L'agent repond -> le hook capture traite la sortie
5. Le daemon extrait threads, bridges, concepts via le LLM local

**Composants cles :**
- **ThreadManager** — Creation de threads, hachage de contenu, detection de doublons
- **Gossip v2** — Decouverte de bridges via ConceptIndex (pipeline en 5 etapes, toutes les 5 min)
- **Decayer** — Decroissance exponentielle a demi-vie pour threads et bridges
- **Archiver** — Archivage des threads inactifs avec synthese LLM
- **RuleDetector** — Detection automatique des preferences utilisateur (patterns EN+FR)

![AI Smartness Graph](./readme-src/ai-smartness-graph.png)

## Documentation

| Document | Description |
|----------|-------------|
| [FEATURES.md](docs/FEATURES.md) | Inventaire exhaustif des fonctionnalites et details techniques |
| [User Guide (EN)](docs/USER_GUIDE.md) | Guide utilisateur complet en anglais |
| [Guide Utilisateur (FR)](docs/USER_GUIDE_FR.md) | Guide utilisateur complet — installation, concepts, outils MCP, configuration |
| [Guia del Usuario (ES)](docs/USER_GUIDE_ES.md) | Guide utilisateur complet en espagnol |

## Prerequis

| Dependance | Version | Notes |
|-----------|---------|-------|
| Rust | >= 1.85 | `rustup` recommande |
| Claude Code | derniere | CLI ou extension VS Code |
| ONNX Runtime | auto-telecharge | `ai-smartness setup-onnx` |
| **GUI uniquement :** webkit2gtk | 4.1 | `libwebkit2gtk-4.1-dev` |
| **GUI uniquement :** GTK 3 | derniere | `libgtk-3-dev` |
| **GUI uniquement :** libsoup | 3.0 | `libsoup-3.0-dev` |

**Plateformes :** Linux (totalement supporte), macOS (implemente, pas encore teste), Windows (implemente, pas encore teste).

## Licence

[MIT](LICENSE)

## Contribuer

Les contributions sont les bienvenues. Merci de :

1. Forker le depot
2. Creer une branche de fonctionnalite (`git checkout -b feature/ma-fonctionnalite`)
3. Committer vos modifications
4. Pousser la branche (`git push origin feature/ma-fonctionnalite`)
5. Ouvrir une Pull Request

Pour les rapports de bugs et demandes de fonctionnalites, ouvrez une issue sur [GitHub](https://github.com/VzKtS/ai-smartness/issues).

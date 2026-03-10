# Audit : CLI-first — Faisabilité daemon standalone remplaçant l'extension VSCode

## Résumé exécutif

L'extension VSCode (7 fichiers, 1406 LOC TypeScript) fait 4 choses que le daemon Rust ne fait pas :
1. **Découverte de processus** Claude Code en cours d'exécution
2. **Injection stdin** de messages dans ces processus
3. **Détection d'inactivité** (idle) pour timing des injections
4. **UI feedback** (status bar, notifications)

Parmi ces 4, seules (1), (2) et (3) sont nécessaires pour le multi-agent CLI-first. (4) est purement VSCode. La faisabilité d'un daemon standalone est **HAUTE** — toutes les fonctionnalités critiques sont reproductibles en Rust sans API VSCode.

---

## 1. Inventaire des fonctionnalités VSCode

### 1.1 Classification par dépendance

| Fonctionnalité | Fichier | LOC | Catégorie | Reproductible sans VSCode ? |
|---|---|---|---|---|
| **Polling loop** (tick every 3s) | extension.ts | ~80 | (c) Indépendant | OUI — boucle tokio/std::thread |
| **Agent detection** (4 sources) | extension.ts | ~60 | (c) Indépendant | OUI — lecture fichiers FS |
| **Agent controller sync** | extension.ts | ~40 | (c) Indépendant | OUI — Map<String, Controller> |
| **Daemon auto-start** | extension.ts | ~15 | (b) Node.js process | OUI — std::process::Command |
| **Command registration** (5 cmds) | extension.ts | ~100 | (a) VSCode API | NON — remplacer par IPC/CLI |
| **Config watching** | extension.ts | ~30 | (a) VSCode API | PARTIEL — fichier config JSON |
| **Status bar** | statusBar.ts | 63 | (a) VSCode API | NON — pas de UI en mode CLI |
| **Wake signal read/write/ack** | wakeSignals.ts | 87 | (c) Indépendant | OUI — déjà du FS pur |
| **Process discovery** | stdinInjection.ts | ~40 | (b) Node.js process | OUI — /proc ou ps |
| **Stdout activity monitoring** | stdinInjection.ts | ~50 | (b) Node.js process | PARTIEL — voir analyse |
| **Idle detection** | stdinInjection.ts | ~30 | (b) Node.js process | PARTIEL — voir analyse |
| **Stdin injection** | stdinInjection.ts | ~80 | (b) Node.js process | OUI — voir analyse |
| **Prompt text building** | stdinInjection.ts | ~30 | (c) Indépendant | OUI — pur string formatting |
| **PID-targeted injection** | stdinInjection.ts | ~40 | (b) Node.js process | OUI — /proc/{pid}/fd/0 |
| **Agent controller FSM** | agentController.ts | 207 | (c) Indépendant | OUI — state machine Rust |
| **CLI binary wrapper** | cli.ts | 168 | (b) Node.js process | INUTILE — déjà en Rust |
| **Path utilities** | paths.ts | 131 | (c) Indépendant | INUTILE — déjà en path_utils.rs |

### 1.2 Résumé par catégorie

| Catégorie | LOC | % | Verdict |
|---|---|---|---|
| **(a) Dépend de VSCode API** | ~193 | 14% | Status bar, commands, config watching — non reproductible en CLI, alternatives possibles |
| **(b) Dépend de Node.js process APIs** | ~240 | 17% | Process discovery, stdin injection, idle detection — **cœur du problème**, reproductible en Rust |
| **(c) Indépendant** | ~505 | 36% | Polling, FSM, signal I/O, agent detection — trivial à porter en Rust |
| **Déjà existant en Rust** | ~468 | 33% | CLI wrapper, paths — inutile à porter |

---

## 2. Analyse des fonctionnalités critiques

### 2.1 Découverte de processus Claude Code

**VSCode** : Utilise `process._getActiveHandles()` (API Node.js interne) pour lister les processus enfants du terminal VSCode. Filtre par `spawnargs` contenant 'claude'.

**Daemon Rust** : Deux approches possibles :

**Approche A — /proc scanning (Linux)** :
```
1. Lire /proc/*/cmdline pour trouver les processus 'claude'
2. Filtrer par project_hash (argument --resume ou variable d'env)
3. Mapper agent_id via beat.json (contient cli_pid)
```
Avantage : Fonctionne indépendamment de VSCode. Voit TOUS les processus Claude du système.
Inconvénient : Linux-only (/proc). macOS nécessite `sysctl` ou `ps`.

**Approche B — Beat.json PID registry (cross-platform)** :
```
1. Le MCP server écrit déjà son PID et le PID Claude parent dans beat.json
2. Le daemon lit tous les beat.json des agents actifs
3. Vérifie que les PIDs sont vivants (kill(pid, 0))
```
Avantage : Cross-platform, données déjà disponibles. Pas besoin de scanner /proc.
Inconvénient : Dépend du MCP heartbeat (10s latence). Ne détecte pas un Claude lancé sans MCP.

**Recommandation** : Approche B (beat.json PID registry) est suffisante. Le MCP server est TOUJOURS présent quand Claude Code est actif (hook `UserPromptSubmit`). La latence de 10s est acceptable car les wake signals ne sont pas urgents à la milliseconde.

### 2.2 Injection stdin

**VSCode** : Écrit un JSON line (`{"type":"user","message":...}\n`) directement dans `process.stdin` du processus Claude enfant.

**Daemon Rust** : Trois approches possibles :

**Approche A — /proc/{pid}/fd/0 (Linux)** :
```rust
let stdin_path = format!("/proc/{}/fd/0", target_pid);
let mut f = std::fs::OpenOptions::new().write(true).open(stdin_path)?;
f.write_all(payload.as_bytes())?;
```
Avantage : Simple, direct. Pas besoin de PTY.
Inconvénient : Linux-only. Le fd/0 n'est pas toujours le "vrai" stdin (peut être un pipe, un PTY, etc.).

**Approche B — Named pipe / signal file (cross-platform)** :
```
1. Le daemon écrit le payload dans un fichier d'injection :
   {agent_dir}/inject_queue/{timestamp}.json
2. Le MCP heartbeat thread lit ce fichier à chaque cycle (10s)
3. Le MCP server injecte le payload dans la conversation via une réponse tool_use
```
Avantage : Cross-platform. Pas besoin de toucher stdin.
Inconvénient : Latence 10s (heartbeat cycle). Change le mécanisme d'injection.

**Approche C — IPC socket vers MCP server** :
```
1. Le daemon ouvre une connexion IPC vers le MCP server de l'agent cible
2. Envoie une commande "inject_prompt" avec le payload
3. Le MCP server écrit dans le protocole MCP pour déclencher une réponse
```
Avantage : Temps réel, propre architecturalement.
Inconvénient : Le MCP protocol ne supporte pas l'injection de prompts côté server. Le server ne peut pas initier de messages — seul le client (Claude) initie.

**Approche D — Claude stdin via PTY (la plus fidèle)** :
```
1. Obtenir le PTY slave device du processus Claude (via /proc/{pid}/fd/0 → readlink → /dev/pts/N)
2. Ouvrir le PTY slave en écriture
3. Écrire le payload JSON line
```
Avantage : Fidèle au mécanisme VSCode actuel. Fonctionne si Claude lit depuis un PTY.
Risque : Conditions de race si Claude est en train d'écrire. Le PTY peut être verrouillé.

**Recommandation** : **Approche A (/proc/fd/0) pour Linux** comme mécanisme principal, avec **Approche B (fichier d'injection via heartbeat)** comme fallback cross-platform. L'Approche A est celle qui se rapproche le plus du mécanisme VSCode actuel (écriture directe stdin) et est la plus simple à implémenter.

### 2.3 Détection d'inactivité (idle)

**VSCode** : Monitore stdout du processus Claude pour les markers "stream_event" et "assistant". Déclare idle après 3s sans output LLM.

**Daemon Rust** : Deux approches :

**Approche A — Beat.json timestamp** :
```
Le MCP heartbeat écrit déjà last_response_time dans beat.json.
Le daemon lit ce timestamp et calcule l'idle time.
Idle = (now - last_response_time) > 3s
```
Avantage : Aucune modification du processus Claude. Données déjà disponibles.
Inconvénient : Résolution 10s (heartbeat cycle). Peut déclarer idle trop tôt ou trop tard.

**Approche B — /proc/{pid}/fd/1 monitoring** :
```
Ouvrir stdout du processus Claude en lecture et monitorer l'activité.
```
Problème : stdout d'un processus tiers n'est pas lisible de l'extérieur sans ptrace.

**Recommandation** : Approche A (beat.json timestamp). La résolution de 10s est acceptable. Le mécanisme VSCode avec 3s d'idle detection est un raffinement qui n'est pas critique — le retry backoff de l'AgentController compense.

### 2.4 UI feedback

**VSCode** : Status bar, notifications, quick pick menus.

**Daemon CLI** : Non applicable. L'utilisateur CLI n'a pas de UI permanente. Alternatives :
- `ai-smartness status` : commande CLI pour voir l'état
- Logs dans un fichier (`~/.config/ai-smartness/controller.log`)
- Notifications optionnelles via `notify-send` (Linux) ou `osascript` (macOS)

**Recommandation** : Pas de UI. Le daemon standalone est silencieux. Diagnostics via CLI.

---

## 3. Architecture proposée

### 3.1 Mode recommandé : sous-commande du binaire existant

```
ai-smartness controller start [--project-hash <hash>] [--interval <ms>]
ai-smartness controller stop
ai-smartness controller status
```

**Rationale** : Pas de binaire séparé. Le binaire `ai-smartness` a déjà un mode `daemon` et un mode `mcp`. Ajouter un mode `controller` est cohérent. Le controller est l'équivalent CLI de l'extension VSCode.

### 3.2 Diagramme d'architecture

```
┌─────────────────────────────────────────────────────┐
│ Utilisateur terminal pur (sans VSCode)              │
│                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐         │
│  │ Claude   │  │ Claude   │  │ Claude   │         │
│  │ (dev)    │  │ (arc)    │  │ (pub)    │         │
│  │ + MCP    │  │ + MCP    │  │ + MCP    │         │
│  └─────┬────┘  └─────┬────┘  └─────┬────┘         │
│        │beat.json     │beat.json    │beat.json      │
│        ▼              ▼             ▼               │
│  ┌─────────────────────────────────────────┐       │
│  │        ai-smartness controller          │       │
│  │                                         │       │
│  │  ┌─────────────────────────────────┐   │       │
│  │  │ Polling loop (3s)               │   │       │
│  │  │  • Read beat.json PIDs          │   │       │
│  │  │  • Check wake signals           │   │       │
│  │  │  • Inject stdin if idle         │   │       │
│  │  └─────────────────────────────────┘   │       │
│  │                                         │       │
│  │  ┌──────────┐ ┌──────────┐ ┌────────┐ │       │
│  │  │AgentCtrl │ │AgentCtrl │ │AgentCtrl│ │       │
│  │  │  (dev)   │ │  (arc)   │ │ (pub)  │ │       │
│  │  └──────────┘ └──────────┘ └────────┘ │       │
│  └─────────────────────────────────────────┘       │
│        │                                            │
│        │ /proc/{pid}/fd/0 (Linux)                  │
│        │ or inject_queue/ (cross-platform)          │
│        ▼                                            │
│  ┌─────────────────────────────────────────┐       │
│  │           ai-smartness daemon            │       │
│  │  (capture, gossip, decay, prune)        │       │
│  └─────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────┘
```

### 3.3 Composants du controller

| Composant | Source VSCode | Équivalent Rust | LOC estimé |
|---|---|---|---|
| Polling loop | extension.ts tick() | tokio::time::interval(3s) | ~50 |
| Agent detection | extension.ts detectAllAgents() | Lecture beat.json + session_agents/ | ~40 |
| Agent controller FSM | agentController.ts (207 LOC) | Port direct de la state machine | ~150 |
| Wake signal I/O | wakeSignals.ts (87 LOC) | Réutiliser path_utils existant | ~30 |
| Process discovery | stdinInjection.ts discover() | Lecture beat.json cli_pid | ~30 |
| Stdin injection | stdinInjection.ts tryInjectSync() | /proc/{pid}/fd/0 write | ~60 |
| Idle detection | stdinInjection.ts isIdle() | beat.json last_response_time | ~20 |
| Prompt building | stdinInjection.ts buildPromptText() | Port direct | ~20 |
| CLI interface | — | clap subcommand | ~30 |
| PID file management | — | Écriture controller.pid | ~15 |
| Logging | extension.ts outputChannel | tracing subscriber (fichier) | ~15 |
| **Total** | | | **~460 LOC** |

### 3.4 Ce qui est réutilisable du code Rust existant

- `path_utils::*` — tous les chemins (beat.json, wake_signals, session_agents)
- `storage::beat::BeatState` — lecture/écriture beat.json
- `daemon::mod::ensure_daemon_running()` — auto-start du daemon
- `agent::Agent`, `registry::AgentRegistry` — détection des agents enregistrés

---

## 4. Risques et mitigations

### R1 — /proc/fd/0 n'est pas le stdin réel
**Risque** : Sur certaines configurations, `/proc/{pid}/fd/0` pointe vers un pipe ou un PTY slave qui n'est pas le stdin attendu par Claude Code.
**Mitigation** : Lire `/proc/{pid}/fd/0` via `readlink` pour vérifier qu'il pointe vers un device PTY (`/dev/pts/N`). Si c'est un pipe, utiliser le fallback inject_queue/.
**Probabilité** : Moyenne. Claude Code via terminal direct utilise un PTY.

### R2 — Conditions de race stdin
**Risque** : Si l'utilisateur tape en même temps que le controller injecte, les bytes se mélangent.
**Mitigation** : L'idle detection (via beat.json) assure que le processus n'est pas en cours de traitement. Le mécanisme VSCode a le même risque et le gère via le même idle check.
**Probabilité** : Basse (idle detection suffit).

### R3 — macOS/Windows incompatibilité
**Risque** : `/proc/{pid}/fd/0` n'existe pas sur macOS/Windows.
**Mitigation** : Fallback vers inject_queue/ (fichier lu par le MCP heartbeat). Sur macOS, alternative via `lsof` + PTY device. Sur Windows, named pipes.
**Probabilité** : Certaine si multi-platform. Le fallback est nécessaire.

### R4 — Latence heartbeat (10s)
**Risque** : Le beat.json est mis à jour toutes les 10s. Le controller peut avoir une vue décalée de l'idle state.
**Mitigation** : Acceptable. L'AgentController FSM a déjà un retry backoff de 15s. La latence de 10s est dans la même échelle. L'extension VSCode avec 3s d'idle detection est un luxe, pas une nécessité.
**Probabilité** : Non applicable (trade-off accepté).

### R5 — Deux controllers simultanés
**Risque** : L'utilisateur lance le controller CLI ET a l'extension VSCode active → double injection.
**Mitigation** : PID file `controller.pid`. Au démarrage, vérifier si un controller (ou l'extension) est déjà actif. Utiliser un lock file avec flock().
**Probabilité** : Moyenne. Documentation nécessaire.

### R6 — Permission denied sur /proc/{pid}/fd/0
**Risque** : Si le controller et Claude ne tournent pas sous le même utilisateur, `/proc/{pid}/fd/0` est inaccessible (permission denied).
**Mitigation** : Le controller DOIT tourner sous le même utilisateur que Claude Code. Documenter cette contrainte. Ajouter un check au démarrage : tenter `readlink /proc/{pid}/fd/0` et log warning si EPERM.
**Probabilité** : Basse (usage normal = même user). Haute si déploiement multi-user.

### R7 — Accumulation inject_queue/ sur crash
**Risque** : Si le MCP/hook crash avant de traiter les fichiers inject_queue/, ils s'accumulent indéfiniment.
**Mitigation** : TTL de 60 secondes sur les fichiers inject_queue/. Le lecteur (hook inject.rs) discard tout fichier dont le timestamp est > 60s. Le controller peut aussi faire un cleanup périodique (supprimer les fichiers > 60s à chaque tick).
**Probabilité** : Basse (crash rare). Impact limité (fichiers légers, ~200 bytes chacun).

---

## 5. Fallback cross-platform : inject_queue/ via hook

Pour les plateformes sans `/proc` (macOS, Windows), le mécanisme alternatif :

**ATTENTION** : Le MCP protocol ne permet PAS au server d'initier des messages vers Claude — seul le client (Claude) initie. Le heartbeat ne peut donc PAS injecter de prompts. Le fallback doit utiliser le **hook inject.rs** (qui tourne à chaque UserPromptSubmit et a déjà 7 layers d'injection).

```
1. Controller écrit le payload dans :
   {agent_data_dir}/inject_queue/{timestamp}_{agent_id}.json

2. Le hook inject.rs (à chaque UserPromptSubmit) :
   - Scanne inject_queue/ pour les fichiers de cet agent
   - Pour chaque fichier : parse le payload, l'injecte comme Layer 1.8
     (après cognitive inbox Layer 1.7, avant recall Layer 2)
   - Supprime le fichier après traitement
   - Discard les fichiers > 60s (TTL anti-accumulation si crash)

3. Format du fichier :
   {
     "type": "controller_wake",
     "text": "[automated inbox wake for dev] ...",
     "timestamp": "2026-02-21T...",
     "agent_id": "dev"
   }
```

**Chaîne complète sur macOS** :
1. Daemon/MCP détecte inbox/wake → émet wake signal file
2. Controller lit le signal → ne peut PAS injecter (pas de /proc)
3. Controller écrit inject_queue/{payload}.json
4. Hook inject.rs (au prochain UserPromptSubmit de l'utilisateur) lit inject_queue/ → injecte comme Layer 1.8
5. Supprime le fichier après traitement

**Modification requise** : ~30 LOC dans `inject.rs` (nouvelle Layer 1.8). Zéro modification du heartbeat → zéro risque de régression.
**Avantage** : Fonctionne sur toute plateforme. Pattern identique à la cognitive inbox existante.
**Inconvénient** : Latence = temps jusqu'au prochain prompt user (variable, pas fixe). Acceptable car c'est déjà le modèle de la cognitive inbox.

---

## 6. Estimation globale

| Élément | LOC Rust | Complexité |
|---|---|---|
| Controller core (polling + agent detection + injection) | ~200 | MOYENNE |
| Agent controller FSM (port du TS 207 LOC, Rust plus verbeux) | ~200 | MOYENNE |
| Stdin injection + /proc fd + idle detection | ~80 | MOYENNE |
| Prompt building + wake signal I/O | ~50 | BASSE |
| CLI interface (clap subcommand + PID file) | ~45 | BASSE |
| Fallback inject_queue/ (hook inject.rs Layer 1.8) | ~30 | BASSE |
| Tests unitaires | ~200 | MOYENNE |
| **Total** | **~805** | |

**Note LOC (pub review)** : L'estimation initiale de ~460 LOC core était optimiste. Le FSM Rust est plus verbeux que le TS (match arms, Result handling, lifetime annotations). Total révisé : ~610 LOC core + ~30 CLI + ~30 fallback = **~670 LOC** sans tests, **~805 LOC** avec tests.

**Dépendances crate** : Aucune nouvelle. tokio (déjà utilisé), tracing (déjà utilisé), serde_json (déjà utilisé).

---

## 7. Recommandation finale

**GO** pour le daemon standalone sous forme de sous-commande `ai-smartness controller`.

Architecture en 2 phases :
1. **Phase 1** — Linux-first avec /proc/fd/0 injection (~460 LOC). Fonctionnel immédiatement pour les utilisateurs terminal Linux.
2. **Phase 2** — Fallback inject_queue/ pour macOS/Windows (~30 LOC Rust + ~30 LOC MCP). Étend la compatibilité.

Le code VSCode extension reste fonctionnel pour les utilisateurs VSCode. Les deux mécanismes coexistent (mutex via PID file).

---

## 8. Historique des reviews

### Review pub — v1 → APPROVE CONDITIONAL
**Verdict** : APPROVE si 3 corrections intégrées.

| # | Sévérité | Correction demandée | Statut |
|---|----------|---------------------|--------|
| 1 | CRITICAL | **Design gap inject_queue/** : le heartbeat ne peut pas injecter (MCP server ne peut pas initier de messages). Fallback doit utiliser hook inject.rs Layer 1.8. | INTÉGRÉ — Section 5 réécrite |
| 2 | MEDIUM | **Risque R6 manquant** : permission denied si /proc/{pid}/fd/0 accédé par un utilisateur différent. | INTÉGRÉ — R6 ajouté |
| 3 | MEDIUM | **Risque R7 manquant** : accumulation inject_queue/ sur crash. TTL 60s requis. | INTÉGRÉ — R7 ajouté |

**Notes additionnelles intégrées** :
- LOC core révisé de ~460 à ~670 (Rust plus verbeux que TS : match arms, Result handling)
- Total avec tests : ~805 LOC

### Review pub — v2 → APPROVED (implicite après intégration des 3 corrections)

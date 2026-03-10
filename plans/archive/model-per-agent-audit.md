# Audit — Architecture modèle par agent

**Mission 16** — demandée par cor, 2026-02-24
**Auditeur** : dev (researcher)

---

## 1. Problème

Impossible de persister un modèle LLM différent par agent dans Claude Code. `/model claude-sonnet-4-6` ne dure que la session. Au restart/rollup, retour au modèle global.

**Objectif** : cor, doc, pub, sub → Sonnet | arc, dev, coder1, coder2 → Opus

---

## 2. Mécanismes de sélection de modèle dans Claude Code

### 2.1 Priorité (du plus haut au plus bas)

| # | Mécanisme | Scope | Persistant ? | Per-agent ? |
|---|-----------|-------|:------------:|:-----------:|
| 1 | Managed settings (system) | Système | OUI | NON |
| 2 | CLI flag `--model opus` | Session | NON | **OUI** (au lancement) |
| 3 | `ANTHROPIC_MODEL` env var | Session | NON* | **OUI** (au lancement) |
| 4 | `.claude/settings.local.json` `"model"` | Projet (local) | OUI | NON |
| 5 | `.claude/settings.json` `"model"` | Projet (shared) | OUI | NON |
| 6 | `~/.claude/settings.json` `"model"` | User global | OUI | NON |
| 7 | `/model` slash command | Session | NON | NON |

\* Persistant si défini dans le shell profile ou via un wrapper script.

### 2.2 État actuel des settings

| Fichier | Clé `"model"` | Contenu |
|---------|:-------------:|---------|
| `.claude/settings.json` | **ABSENTE** | Hooks + permissions uniquement |
| `.claude/settings.local.json` | **ABSENTE** | Permissions dev permissives |
| `~/.claude.json` | **ABSENTE** | Metadata projets, OAuth, MCP |
| `~/.claude/settings.json` | **ABSENTE** | MCP + permissions globales |

**Aucune configuration de modèle n'existe actuellement.** Le système utilise le modèle par défaut du plan (Opus sur Max plan).

### 2.3 Variables d'environnement

| Variable | Rôle | Documenté ? |
|----------|------|:-----------:|
| `ANTHROPIC_MODEL` | Override modèle courant (alias ou ID complet) | OUI |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | Quelle version d'Opus l'alias "opus" résout | OUI |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | Quelle version de Sonnet l'alias "sonnet" résout | OUI |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | Quelle version de Haiku l'alias "haiku" résout | OUI |
| `CLAUDE_CODE_SUBAGENT_MODEL` | Modèle pour les sous-agents Task | OUI |
| `CLAUDE_MODEL` | **N'EXISTE PAS** | — |

---

## 3. Données modèle disponibles dans le transcript

### 3.1 Le modèle EST dans le transcript

Chaque entrée assistant du JSONL contient le champ `"model"` :

```json
{
  "type": "assistant",
  "model": "claude-opus-4-6",
  "cache_creation_input_tokens": 298,
  "cache_read_input_tokens": 153696,
  "input_tokens": 1,
  "output_tokens": 24
}
```

### 3.2 Mais les hooks ne le lisent PAS

| Composant | Lit le transcript ? | Extrait le modèle ? |
|-----------|:-------------------:|:-------------------:|
| `inject.rs` `update_context_from_transcript()` | OUI (tokens) | **NON** |
| `transcript.rs` `parse_last_usage()` | OUI (tokens) | **NON** |
| `beat.rs` `BeatState` | Stocke tokens | **PAS de champ `model`** |

Le `ContextInfo` retourné par `parse_last_usage()` contient : `total_tokens`, `percent`, `cache_creation`, `cache_read`, `input`, `output`, `window_size`. **Pas de `model`.**

### 3.3 Contrainte fondamentale

**Le modèle est choisi AVANT le premier hook.** La séquence est :
1. Claude Code démarre avec un modèle (settings ou env var)
2. L'utilisateur envoie un prompt
3. Le hook `UserPromptSubmit` s'exécute → injecte du contexte
4. Le prompt augmenté est envoyé au modèle **déjà sélectionné**

Les hooks ne peuvent PAS changer le modèle. Ils peuvent seulement :
- Détecter le modèle actuel (via transcript)
- Avertir l'utilisateur d'un mismatch
- Bloquer le prompt (exit code 2 avec `"decision": "block"`)

---

## 4. Réponses aux questions de cor

### Q1 — Le modèle est-il dans le transcript ?

**OUI**, dans chaque entrée `"type": "assistant"`. Mais **NON lu** par les hooks. Gap identifié — il faudrait ajouter `model` à `ContextInfo` et `BeatState`.

### Q2 — Plusieurs settings.json par sous-répertoire ?

**OUI en théorie** — chaque répertoire avec `.claude/settings.json` est un projet Claude Code distinct. Mais en pratique, tous les agents travaillent dans le **même projet** (`ai_smartness_dev`), donc un seul `settings.json` s'applique. Créer des sous-projets par groupe d'agents serait artificiel et casserait le workflow.

### Q3 — Injection prompt pour forcer le modèle ?

**NON viable.** Claude ne peut pas changer son propre modèle via une instruction prompt. Le modèle est fixé au niveau de l'API request, pas au niveau du contenu.

### Q4 — ANTHROPIC_MODEL par agent via .env ?

**FAISABLE pour le CLI** mais **PAS pour l'extension VS Code** (O1 pub).

- **CLI/terminal** : `ANTHROPIC_MODEL` peut être défini par wrapper script ou terminal profile → fonctionne
- **Extension VS Code** : l'extension a un setting global `claudeCode.selectedModel` mais il s'applique à **toutes les sessions**. Aucun mécanisme per-panel. `ANTHROPIC_MODEL` n'est pas passé par l'extension aux sessions individuelles.

L'extension VS Code ne lit PAS la clé `"model"` de `.claude/settings.json` — elle utilise son propre `claudeCode.selectedModel`.

### Q5 — Wrapper script par agent ?

**OUI pour le CLI/terminal** — un script qui résout agent → modèle et lance Claude Code avec `--model` ou `ANTHROPIC_MODEL`.

**NON pour l'extension VS Code** — les panels extension sont lancés par l'extension, pas par l'utilisateur. Aucun point d'injection pour ANTHROPIC_MODEL per-panel. (O1 pub confirmé par investigation VS Code extension).

---

## 5. Options d'architecture

### 5.1 Option A — Wrapper scripts (CLI uniquement, 0 code ai-smartness)

**Principe** : Un script `claude-agent` qui résout agent → modèle et lance Claude Code.

**Applicabilité** : CLI/terminal SEULEMENT. **Inapplicable pour les panels extension VS Code** — ceux-ci sont lancés par l'extension, pas par l'utilisateur. Aucun point d'injection pour `ANTHROPIC_MODEL` per-panel (O1 pub).

```bash
#!/bin/bash
# ~/.local/bin/claude-agent
AGENT=${1:-default}
shift

case $AGENT in
  cor|doc|pub|sub) MODEL=sonnet ;;
  arc|dev|coder1|coder2) MODEL=opus ;;
  *) MODEL=opus ;;  # default
esac

export ANTHROPIC_MODEL=$MODEL
exec claude --model $MODEL "$@"
```

Usage : `claude-agent dev` ouvre une session Opus, `claude-agent cor` ouvre une session Sonnet.

| Pro | Con |
|-----|-----|
| Zéro modification codebase | Manuel — chaque session doit être lancée via le script |
| Fonctionne immédiatement | Pas de validation mid-session |
| Compatible multi-fenêtre terminal | **Inapplicable aux panels VS Code** (O1) |

**Variante VS Code** : Définir des terminal profiles dans `.vscode/settings.json` :

```json
{
  "terminal.integrated.profiles.linux": {
    "Claude Opus (dev/arc)": {
      "path": "bash",
      "args": ["-c", "ANTHROPIC_MODEL=opus claude"],
      "env": { "ANTHROPIC_MODEL": "opus" }
    },
    "Claude Sonnet (cor/pub)": {
      "path": "bash",
      "args": ["-c", "ANTHROPIC_MODEL=sonnet claude"],
      "env": { "ANTHROPIC_MODEL": "sonnet" }
    }
  }
}
```

### 5.2 Option B — Agent config + hook model check (~30 LOC ai-smartness)

**Principe** : Ajouter `model` au registre d'agents + le hook détecte les mismatches et avertit.

**Phase 1** — Ajouter `model` à la config agent :

```rust
// Dans le registre d'agents (config ou fichier dédié)
pub struct AgentConfig {
    pub agent_id: String,
    pub role: String,
    pub expected_model: Option<String>,  // "opus", "sonnet", "haiku"
    // ... existing fields
}
```

**Phase 2** — Extraire le modèle du transcript :

```rust
// transcript.rs — modifier parse_last_usage()
pub struct ContextInfo {
    // ... existing fields
    pub model: Option<String>,  // NEW: "claude-opus-4-6"
}

// ATTENTION (O2 pub) : parse_last_usage() utilise raw string search
// (find_last_json_number), PAS du JSON parsing. Suivre le même pattern :
fn find_last_json_string(content: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let pos = content.rfind(&pattern)?;
    let start = pos + pattern.len();
    let end = content[start..].find('"')? + start;
    Some(content[start..end].to_string())
}

// Dans parse_last_usage(), ajouter après les extractions de tokens :
info.model = find_last_json_string(content, "model");
```

**Phase 3** — BeatState tracking + injection warning :

```rust
// beat.rs — ajouter champ
pub model: Option<String>,

// inject.rs — après update_context_from_transcript()
if let Some(ref expected) = agent_config.expected_model {
    if let Some(ref actual) = beat_state.model {
        if !actual.contains(expected) {
            // Inject warning
            let warning = format!(
                "<system-reminder>Model mismatch: agent {} expects {} but running on {}. \
                 Use /model {} to switch.</system-reminder>",
                agent_id, expected, actual, expected
            );
            injections.push(warning);
        }
    }
}
```

| Pro | Con |
|-----|-----|
| Intégré dans ai-smartness | Ne change PAS le modèle, seulement avertit |
| Détection automatique | L'utilisateur doit manuellement `/model X` |
| Visible dès le premier prompt | +30 LOC, nécessite build |
| Config par agent dans le registre | `/model` ne persiste pas → re-warning après rollup |

### 5.3 Option C — Hook model enforcement (block) (~15 LOC supplémentaires)

**Extension de l'Option B** : au lieu d'un warning, **bloquer le prompt** si le modèle est incorrect.

```rust
// inject.rs — retourner un block decision au lieu d'un warning
if model_mismatch {
    // Output block decision to stdout
    println!("{{\"decision\":\"block\",\"reason\":\"Model mismatch: agent {} requires {}. Use /model {} first.\"}}",
        agent_id, expected, expected);
    std::process::exit(0);
}
```

| Pro | Con |
|-----|-----|
| Force le switch avant de continuer | Agressif — bloque l'utilisateur |
| Impossible d'oublier | Frustrant si `/model` est oublié |
| Garantit le bon modèle | Bloque le premier prompt de chaque session |

### 5.4 Option D — Combinée A+B (RECOMMANDÉE)

**Hook model check** (Option B) pour TOUS les workflows → défense primaire, détection automatique.
**Wrapper scripts** (Option A) pour le CLI/terminal → modèle correct dès le départ (complément).

**Rôle de chaque couche** (O1 pub — restructuré) :

| Couche | Workflow CLI | Workflow VS Code panels |
|--------|:-----------:|:----------------------:|
| Option A — Wrapper scripts | **Prévention** (modèle correct au lancement) | **INAPPLICABLE** |
| Option B — Hook model check | Validation (backup si `/model` manuel) | **DÉFENSE PRIMAIRE** (seule protection) |

**Flux VS Code panels** (workflow principal) :
1. L'extension lance le panel → modèle global (`claudeCode.selectedModel` ou default plan)
2. L'utilisateur envoie un prompt → hook lit le transcript
3. ⚠️ **Premier prompt** : transcript vide → `model = None` → pas de warning (safe default, R2)
4. **Deuxième prompt+** : hook détecte le modèle → compare avec `agent_config.expected_model` → warning si mismatch
5. L'utilisateur fait `/model sonnet` manuellement → prochain prompt sans warning

**Flux CLI/terminal** :
1. L'utilisateur lance `claude-agent dev` → ANTHROPIC_MODEL=opus → session démarre en Opus
2. Le hook lit le transcript → match → pas de warning
3. Si `/model sonnet` manuellement → warning injecté
4. Après un rollup → wrapper script remet le bon modèle au prochain lancement

**Note** : Le premier prompt d'un panel VS Code n'est PAS protégé (transcript vide). C'est une limitation acceptée — le coût d'un prompt sur le mauvais modèle est faible (quota, pas argent sur Max plan).

---

## 6. Plan d'implémentation (Option D recommandée)

### 6.1 Phase 1 — Wrapper scripts (0 LOC ai-smartness)

Créer `~/.local/bin/claude-agent` avec la table de mapping agent → modèle (§5.1).

### 6.2 Phase 2 — Extraire le modèle du transcript (~10 LOC)

**Fichier** : `src/storage/transcript.rs`

Ajouter `model: Option<String>` à `ContextInfo`. Créer un helper `find_last_json_string()` suivant le même pattern que `find_last_json_number()` (raw string search, PAS de JSON parsing — O2 pub). Appeler dans `parse_last_usage()`.

### 6.3 Phase 3 — Tracker le modèle dans BeatState (~5 LOC)

**Fichier** : `src/storage/beat.rs`

Ajouter `pub model: Option<String>` à `BeatState`. Mettre à jour dans `update_context()`.

### 6.4 Phase 4 — Agent model config (~17 LOC)

**Fichier** : config agent (registre ou fichier dédié)

Ajouter `expected_model: Option<String>` à la config par agent. Inclut : schema migration + champ `AgentEntry` + champ `AgentUpdate` + handler `agent_configure` + tool MCP (O3 pub — ~17 LOC, pas ~10).

### 6.5 Phase 5 — Hook model mismatch warning (~15 LOC)

**Fichier** : `src/hook/inject.rs`

Après `update_context_from_transcript()`, comparer `beat_state.model` avec `agent_config.expected_model`. Si mismatch, injecter un `<system-reminder>` warning.

---

## 7. Fichiers modifiés

| Fichier | Phase | Action | LOC estimées |
|---------|:-----:|--------|:------------:|
| `~/.local/bin/claude-agent` | 1 | Nouveau script wrapper | ~15 (bash) |
| `src/storage/transcript.rs` | 2 | Ajouter model à ContextInfo + parsing | ~10 |
| `src/storage/beat.rs` | 3 | Ajouter model à BeatState | ~5 |
| Config agent (registre) | 4 | Ajouter expected_model (schema + entry + update + handler + MCP) | ~17 (O3) |
| `src/hook/inject.rs` | 5 | Model mismatch warning injection | ~15 |
| **Total ai-smartness** | | | **~47** |

### Fichiers NON modifiés
- `.claude/settings.json` — pas de clé `"model"` (le wrapper gère ça)
- `capture.rs` — pas de model tracking nécessaire côté capture
- `mod.rs` — dispatch inchangé

---

## 8. Limitations connues

| # | Limitation | Impact | Mitigation |
|---|-----------|--------|------------|
| L1 | `/model` ne persiste pas across sessions | Après rollup, retour au default | Wrapper script relance avec le bon modèle |
| L2 | Le hook ne peut PAS changer le modèle | Seulement warn, pas enforce | Warning visible + wrapper script |
| L3 | Pas de hook SessionStart dans Claude Code | Impossible de set le modèle au démarrage via hook | Wrapper script (pré-session) |
| L4 | Un seul `settings.json` par projet | Pas de `"model"` per-agent via settings | Env var ANTHROPIC_MODEL au lancement |
| L5 | **HAUTE (O1)** — L'extension VS Code ne passe pas ANTHROPIC_MODEL per-panel | Les panels VS Code utilisent le modèle global. Option A inapplicable. **Phase 5 (hook warning) = seule protection** pour ce workflow | Warning hook dès le 2ème prompt. 1er prompt non protégé (transcript vide). Coût acceptable sur Max plan |

---

## 9. Risques et mitigations

| # | Risque | Sévérité | Mitigation |
|---|--------|----------|------------|
| R1 | Wrapper script pas utilisé (oubli) | BASSE | Phase 5 injecte un warning si mismatch |
| R2 | Transcript vide au premier prompt | BASSE | `model = None` → pas de warning (safe default) |
| R3 | Model ID change de format entre versions | BASSE | Matching par substring (contains("sonnet"), contains("opus")) |
| R4 | Coût API si Opus utilisé par erreur pour cor/pub | MOYENNE | Warning immédiat au premier prompt. Sur Max plan, coût = quota, pas argent |

---

## 10. Historique reviews

### Review pub R1 — 2026-02-24

**VERDICT : APPROUVÉ** avec 1 observation HAUTE (O1) et 2 mineures (O2, O3).

**Vérifications** : ContextInfo no model, BeatState no model, settings no model, AgentUpdate no expected_model — tout confirmé.

**§2 Priorité** : CORRECTE. **§3 Contrainte** : CORRECTE. **§5 Options** : analyse juste, Option D acceptée.

**O1 HAUTE** — L5 sous-estimée. Workflow principal = VS Code extension panels, pas terminal. Option A (wrapper scripts) inapplicable pour les panels extension. Phase 5 (hook warning) devient défense primaire, premier prompt non protégé. Évaluer le support env vars par l'extension VS Code avant livraison.

**O2 MINEURE** — Phase 2 : `parse_last_usage()` utilise raw string search, pas JSON parsing. Plan montre `entry.get("model")` = faux pattern. Besoin d'un `find_last_json_string()` helper.

**O3 MINEURE** — Phase 4 : ~17 LOC (pas ~10). Schema migration + AgentEntry + AgentUpdate + handler + MCP tool.

**Intégration** : O1 intégrée dans §4 Q4/Q5, §5.1, §5.4, §8 L5. O2 intégrée dans §5.2 Phase 2, §6.2. O3 intégrée dans §6.4, §7 table LOC (total ~47).

---

## 11. Updates cor — Extension VS Code + stdin injection + stdout monitoring

### UPDATE 1 — Architecture CLI/IDE split

**Question** : L'extension ai-smartness VS Code peut-elle gérer la persistance du modèle per-agent ?

**Investigation** : Audit complet de `ai-smartness-vscode/` (7 fichiers, ~1400 LOC TypeScript).

#### Capacités actuelles de l'extension

| Capacité | Statut | Fichier | Détails |
|----------|:------:|---------|---------|
| Détecter les agents actifs | **OUI** | `extension.ts:225-281` | Via env var, session files, `.mcp.json` |
| Cibler un process Claude par agent (PID) | **OUI** | `stdinInjection.ts:91-124` | Lit `beat.json` pour le PID, trouve le handle |
| Injecter du texte via stdin | **OUI** | `stdinInjection.ts:251-294` | JSON protocol, PID-targeted |
| Écouter le stdout des process Claude | **OUI** | `stdinInjection.ts:59-66` | `proc.stdout.on('data', ...)` |
| Communiquer avec le daemon | **OUI** (CLI) | `cli.ts:70-167` | `execSync('ai-smartness agent list/select/daemon status')` |
| Lire la config modèle d'un agent | **NON** | — | CLI `agent list` n'inclut pas MODEL |
| Modifier `claudeCode.selectedModel` | **NON** | — | Pas d'accès à la config Claude Code |
| Modifier le modèle per-panel | **NON** | — | API VS Code ne supporte pas ça |

#### Architecture de l'extension

```
Activation (onStartupFinished)
    ↓
detectAllAgents() → env, session files, .mcp.json
    ↓
Polling tick (3 sec) → check wake signals per agent
    ↓
AgentController state machine (idle → pending → cooldown)
    ↓
tryInjectSync(agentId, text, projHash) → PID-targeted stdin
```

**Chaque instance est isolée** : pas de singletons partagés entre MCP. Extension = 1 par fenêtre VS Code.

#### Réponses aux questions cor UPDATE 1

**Q1 — Comment détecter CLI vs IDE ?** Pas dans le transcript ni les hooks. Détectable par :
- `VSCODE_PID` env var (présent dans les process lancés par VS Code)
- `TERM_PROGRAM` env var (absent en VS Code extension)
- Le daemon pourrait stocker `source: "cli" | "ide"` dans l'agent registry

**Q2 — Extension accès à `claudeCode.selectedModel` ?** L'API `vscode.workspace.getConfiguration('claudeCode')` est lisible mais `selectedModel` est un setting utilisateur — writable uniquement dans le scope User/Workspace, pas per-panel.

**Q3 — Écouter les créations de panel Claude Code ?** L'extension ne reçoit PAS d'événement de création de panel Claude Code. Elle découvre les process Claude via `_getActiveHandles()` dans le tick polling (3 sec).

**Q4 — IPC extension ↔ daemon ?** Via CLI `execSync` uniquement (`cli.ts`). Pas de socket direct. Le daemon a un IPC (`processor.sock`) mais l'extension ne l'utilise pas.

### UPDATE 2 — Stdin injection `/model` : NON VIABLE

**Question cor** : L'extension peut-elle injecter `/model claude-sonnet-4-6` via stdin au démarrage de la session ?

**RÉPONSE : NON.**

**Raison** : Le protocole stdin JSON de Claude Code (`{"type":"user","message":{"role":"user","content":[{"type":"text","text":"..."}]}}`) envoie le contenu comme un **prompt utilisateur**, PAS comme une commande slash. Les slash commands (`/model`, `/compact`, etc.) sont traitées par le handler interactif du REPL, qui n'est pas activé pour les messages stdin JSON.

**Vérifié par** : Documentation Claude Code + GitHub issues #4184 (slash commands broken in stream-json mode) et #16712 (stdin JSON limitations).

**Conséquence** : `/model claude-sonnet-4-6` injecté via stdin serait envoyé au LLM comme un prompt ordinaire, pas exécuté comme un changement de modèle.

**Alternatives explorées** :

| Alternative | Viable ? | Détails |
|-------------|:--------:|---------|
| Injection stdin slash command | **NON** | Stdin JSON = prompt, pas command handler |
| `ANTHROPIC_MODEL` env var au lancement | **CLI OUI / IDE NON** | L'extension ne contrôle pas l'env des process Claude qu'elle découvre |
| `claudeCode.selectedModel` API | **Partiel** | Global (toutes sessions), pas per-panel |
| `--model` flag au lancement | **CLI OUI / IDE NON** | L'extension ne lance pas les process Claude |
| Hook `UserPromptSubmit` block + message | **OUI (existant)** | Phase 5 du plan — warning si mismatch, ne change pas le modèle |

### UPDATE 3 — Stdout monitoring `/model` : FAISABLE

**Question cor** : L'extension peut-elle détecter quand l'utilisateur tape `/model X` et proposer de persister ?

**RÉPONSE : OUI, faisable avec ~40-50 LOC extension.**

**Mécanisme existant** : L'extension écoute déjà `proc.stdout.on('data', callback)` (`stdinInjection.ts:59-66`). Actuellement, elle ne filtre que les events `stream_event` et `assistant` pour le tracking idle. Elle pourrait aussi :

1. **Détecter le changement** : Quand `/model` est exécuté dans Claude Code interactif, un message de confirmation apparaît dans le flux stdout. L'extension peut parser ce flux pour détecter les patterns de changement de modèle.

2. **Alternative plus fiable** : Surveiller le champ `"model"` dans le transcript JSONL. Quand le modèle change entre deux entrées `assistant`, déclencher la notification. Ceci nécessite que Phase 2 du plan original (extraction model du transcript) soit implémentée côté Rust d'abord — ou que l'extension lise directement le transcript.

3. **Notification VS Code** : `vscode.window.showInformationMessage()` avec boutons "Oui, persister" / "Non".

4. **Persistance** : Si oui → appeler `ai-smartness agent configure {agentId} --expected-model sonnet` via CLI (nécessite Phase 4 du plan original).

**Flux proposé** :

```
stdout monitoring détecte changement de modèle
    ↓
Extension identifie quel agent (via PID → beat.json → agentId)
    ↓
Notification VS Code : "Agent dev a switché vers sonnet. Persister ?"
    ↓
Si oui → execSync('ai-smartness agent configure dev --expected-model sonnet')
    ↓
Daemon met à jour agent registry (expected_model)
    ↓
Au prochain lancement, le hook warning détectera le mismatch si le modèle est différent
```

**LOC estimées** :

| Fichier | Action | LOC |
|---------|--------|:---:|
| `stdinInjection.ts` | Ajouter model change detection dans le callback stdout | ~15 |
| `agentController.ts` | Gérer l'événement model change + notification | ~15 |
| `cli.ts` | Ajouter `configureAgent(agentId, projHash, model)` | ~10 |
| **Total extension** | | **~40** |

**Prérequis Rust** : Phase 4 du plan original (agent model config dans le registre + handler `agent_configure`).

### UPDATE 4 — Model selector dans la GUI (create/edit agent)

**Question cor** : Ajouter un champ model (dropdown haiku/sonnet/opus) dans les formulaires create/edit agent de la GUI Tauri.

**RÉPONSE : FAISABLE — ~35 LOC réparties sur 6 fichiers.**

#### Diagnostic des formulaires existants

**Formulaire création** (`index.html:948-1013`) :
- Modal `#modal-add-agent` avec 10 champs (id, name, role, supervisor, team, thread_mode, is_supervisor, custom_role, report_to, full_permissions)
- Le champ `expected_model` est **absent**
- Point d'insertion : après `new-agent-thread-mode` (ligne ~988)

**Formulaire édition** (`app.js:1029-1078`) :
- Template inline avec 12 champs (name, role, description, custom_role, report_to, supervisor, team, thread_mode, is_supervisor, full_permissions, capabilities, specializations)
- Le champ `expected_model` est **absent**
- Point d'insertion : après `.ae-thread-mode` (ligne ~1055)

#### Fichiers à modifier

| Fichier | Action | Lignes | LOC |
|---------|--------|--------|:---:|
| `src/gui/frontend/index.html` | Ajouter `<select id="new-agent-model">` (haiku/sonnet/opus) dans modal creation | ~988 | ~5 |
| `src/gui/frontend/app.js` | Collecter `modelVal` + passer à `add_agent` invoke | 506, 526 | ~3 |
| `src/gui/frontend/app.js` | Ajouter `ae-model` select dans template edit | ~1055 | ~3 |
| `src/gui/frontend/app.js` | Collecter `model` + passer à `update_agent` invoke | 1113, 1133 | ~3 |
| `src/gui/commands.rs` | Ajouter `expected_model: Option<String>` à `add_agent` + `update_agent` + `list_agents` | 876, 926, 1041, 1086, 856 | ~6 |
| `src/agent.rs` | Ajouter `pub expected_model: Option<String>` à `Agent` struct | ~182 | ~1 |
| `src/registry/registry.rs` | Ajouter champ à `AgentUpdate` + `register()` INSERT + `update()` SET | 773, 79, 99, 506 | ~8 |
| `src/storage/migrations.rs` | V7 migration : `ALTER TABLE agents ADD COLUMN expected_model TEXT` | ~431 | ~6 |
| **Total GUI+backend** | | | **~35** |

**Note** : Ce LOC chevauche la Phase 4 du plan original (~17 LOC). Les composants communs sont : `AgentUpdate`, `Agent` struct, `registry.rs`, `migrations.rs`. Le surplus est le frontend GUI (~14 LOC) et `commands.rs` (~6 LOC).

#### Mécanisme d'injection au démarrage — PROBLÈME OUVERT

Cor propose que le daemon injecte `/model X` via stdin au démarrage de session. Mais comme documenté dans UPDATE 2, **l'injection stdin JSON n'exécute PAS les slash commands**.

**Alternatives d'injection au démarrage** :

| # | Mécanisme | Viable ? | Avantages | Inconvénients |
|---|-----------|:--------:|-----------|---------------|
| A1 | Extension écrit `/model X\n` en **raw text** (pas JSON) sur stdin | **PROBABLEMENT NON** (O1 R2) | Si le REPL le parse → idéal | Claude Code en extension utilise parser JSON sur stdin, pas REPL. Raw text = ignoré ou corruption stream. A2 existe → investigation non justifiée |
| A2 | Extension modifie `claudeCode.selectedModel` via API VS Code | **Partiel** | Fonctionne si 1 agent/fenêtre VS Code | Global : change le modèle pour TOUTES les sessions de la fenêtre |
| A3 | Extension utilise VS Code `Terminal.sendText('/model X')` | **NON** | — | Claude Code n'est pas un terminal VS Code standard, c'est un child process |
| A4 | Hook warning Phase 5 (pas d'injection, juste detection) | **OUI** | Fiable, déjà planifié | Ne prévient pas — corrige seulement après le 2ème prompt |
| A5 | `ANTHROPIC_MODEL` env var au lancement du process | **CLI OUI / IDE NON** | Prévention forte pour CLI | L'extension ne contrôle pas l'env du process Claude |

**Recommandation** : Implémenter A2 (extension `claudeCode.selectedModel`) comme mécanisme primaire pour IDE. C'est global par fenêtre, mais dans le setup multi-fenêtres (1 agent / fenêtre), ça fonctionne. Pour le setup multi-panels (N agents / fenêtre), Phase 5 (hook warning) reste le filet de sécurité.

---

## 12. Architecture révisée (post-updates 1-4)

### 12.1 Vision consolidée

```
┌──────────────────────────────────────────────────────────┐
│                     CONFIGURATION                         │
├──────────────────────────────────────────────────────────┤
│ GUI:     Model selector dans create/edit agent (UPDATE 4) │
│ MCP:     agent_configure --expected-model (Phase 4)       │
│ Stockage: Agent registry DB + expected_model column       │
├──────────────────────────────────────────────────────────┤
│                     PRÉVENTION                            │
├──────────────────────────────────────────────────────────┤
│ CLI:     Wrapper script ANTHROPIC_MODEL (Phase 1)         │
│ IDE 1w:  Extension → claudeCode.selectedModel (A2)        │
│ IDE Nw:  ❌ Pas de mécanisme per-panel (limitation API)   │
├──────────────────────────────────────────────────────────┤
│                     DÉTECTION                             │
├──────────────────────────────────────────────────────────┤
│ Tous:    Hook model mismatch warning (Phase 5)            │
│ IDE:     Extension stdout monitoring (UPDATE 3)           │
├──────────────────────────────────────────────────────────┤
│                    PERSISTANCE                            │
├──────────────────────────────────────────────────────────┤
│ IDE:     Extension notification "Persister ?" (UPDATE 3)  │
│ CLI:     agent configure --expected-model                 │
└──────────────────────────────────────────────────────────┘
```

### 12.2 Ce qui a changé par rapport au plan original

| Élément | Plan original | Après updates 1-4 |
|---------|---------------|---------------------|
| Stdin injection `/model` | Non envisagé | **Investigué → NON VIABLE** (UPDATE 2) |
| GUI model selector | Non envisagé | **Create/edit agent forms** (UPDATE 4) |
| Extension model awareness | Non envisagé | **Stdout monitoring + notification** (UPDATE 3) |
| Extension `selectedModel` | Non envisagé | **Recommandé comme prévention IDE** (A2) |
| Persistance model choice | Seulement daemon config | **GUI + extension notification** (UPDATE 3+4) |
| Phase 5 (hook warning) | Backup CLI, primaire IDE | **Filet de sécurité pour multi-panel** |
| Phase 1 (wrapper) | Prévention principale | **CLI uniquement** |

### 12.3 Plan d'implémentation révisé

| Phase | Scope | LOC | Priorité |
|-------|-------|:---:|:--------:|
| Phase 1 — Wrapper scripts | CLI uniquement | ~15 bash | BASSE |
| Phase 2 — Model extraction transcript | Rust (`transcript.rs`) | ~10 | HAUTE |
| Phase 3 — BeatState model tracking | Rust (`beat.rs`) | ~5 | HAUTE |
| Phase 4 — Agent model config + DB | Rust (agent, registry, migrations, MCP) | ~17 | **CRITIQUE** (prérequis) |
| Phase 5 — Hook mismatch warning | Rust (`inject.rs`) | ~15 | HAUTE |
| **Phase 6 — GUI model selector** | **HTML + JS + Rust commands** | **~20** | **HAUTE** |
| **Phase 7 — Extension stdout watch** | **TypeScript** | **~40** | **MOYENNE** |
| **Phase 8 — Extension selectedModel** | **TypeScript** | **~10** | **MOYENNE** |
| **Total** | | **~132** | |

**Note LOC** : Phase 4 et Phase 6 partagent ~15 LOC (struct Agent, AgentUpdate, registry, migrations). Total net dédupliqué : **~117 LOC**.

---

## 13. Historique reviews (§11-§12)

### Review pub R2 — 2026-02-24

**VERDICT : APPROVED** — 1 observation basse, 0 corrections

**Vérifications** : UPDATE 2 stdin JSON ≠ slash command ✅ (buildPayload vérifié), A1-A5 ranking ✅, points d'insertion GUI ✅ (index.html:988, app.js:506, app.js:1055, commands.rs:876), `expected_model` absent du code actuel ✅, LOC net ~117 ✅ (overlap ~15 Phase 4/6 confirmé).

**Observation** (BASSE) : A1 (raw text stdin) devrait être "PROBABLEMENT NON VIABLE" plutôt que "À VÉRIFIER" — le protocole JSON structuré en mode extension rend le raw text incompatible.

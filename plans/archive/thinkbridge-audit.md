# Audit — Filtrage inputs utilisateur + logique procédurale Thinkbridge

**Mission 7** — demandée par cor, 2026-02-24
**Auditeur** : dev (researcher)

---

## 1. Périmètre

Deux points d'audit :

1. **Filtrage des inputs courts** — système à 3 zones (< 50, 51-150, > 150 chars) pour les user prompts uniquement
2. **Logique procédurale Thinkbridge** — vérifier que le contexte global (threads, bridges, mémoire) n'est PAS injecté avant ou pendant l'extraction/création de thread

---

## 2. Point 1 — Filtrage des inputs utilisateur

### 2.1 État actuel du code

Le pipeline de capture des prompts utilisateur traverse :

1. **`hook/inject.rs:44-51`** — Extraction du message + test vide uniquement
   ```rust
   let message = extract_message(input);
   if message.trim().is_empty() {
       print!("{}", input);
       return;
   }
   ```
   Aucun filtrage sur la longueur.

2. **`hook/inject.rs:114-117`** — Envoi fire-and-forget au daemon
   ```rust
   let _ = daemon_ipc_client::send_capture(
       project_hash, agent_id, "prompt", &message,
   );
   ```
   Le client IPC utilise la méthode `"tool_capture"` (daemon_ipc_client.rs:73), PAS `"prompt_capture"`.

3. **`daemon/ipc_server.rs:243-283`** — Le handler `tool_capture` crée un `CaptureJob` avec `is_prompt: false`. Le prompt est donc traité comme une capture outil ordinaire.

4. **`daemon/processor.rs:50-54`** — Filtre bruit : `should_capture_with_config()` utilise `MIN_CAPTURE_LENGTH = 20` bytes (constants.rs:47).

5. **`daemon/processor.rs:74-81`** — Extraction LLM complète pour TOUT contenu ≥ 20 bytes.

### 2.2 Anomalies identifiées

| # | Sévérité | Description |
|---|----------|-------------|
| A1 | **HAUTE** | `MIN_PROMPT_LENGTH = 50` (constants.rs:46) existe mais est **dead code** — jamais référencé nulle part |
| A2 | **HAUTE** | Aucun filtrage longueur spécifique aux prompts — un prompt de 3 mots ("oui ok go") passe le pipeline LLM complet si ≥ 20 bytes |
| A3 | **MOYENNE** | Le prompt est envoyé via `"tool_capture"` (pas `"prompt_capture"`) → `is_prompt=false` → pas de traitement différencié dans le worker |
| A4 | **BASSE** | Pas de constante pour le seuil intermédiaire (gate LLM 51-150 chars) |

### 2.3 Plan de correction

#### Phase A — Rejet immédiat < 50 chars (~15 LOC)

**Fichier : `hook/inject.rs`** — Après l'extraction du message (ligne 51), avant `send_capture()` (ligne 114) :

```rust
// Filtrage prompts courts — pas de capture pour les messages < MIN_PROMPT_LENGTH chars.
// "oui", "ok", "go", "dispatch" → aucune valeur extractive.
if message.chars().count() < ai_smartness::constants::MIN_PROMPT_LENGTH {
    tracing::debug!(
        chars = message.chars().count(),
        min = ai_smartness::constants::MIN_PROMPT_LENGTH,
        "Prompt trop court pour capture, skip IPC"
    );
    // NOTE: on continue le hook normalement (injection layers) — on skip SEULEMENT la capture daemon.
    // Le prompt est toujours augmenté et renvoyé à Claude Code.
} else {
    let _ = ai_smartness::processing::daemon_ipc_client::send_capture(
        project_hash, agent_id, "prompt", &message,
    );
}
```

**Fichier : `daemon/processor.rs`** — Safety net dans `process_prompt()` (ligne 182) :

```rust
pub fn process_prompt(...) -> AiResult<Option<String>> {
    // Safety net: reject prompts < MIN_PROMPT_LENGTH chars
    // (normalement déjà filtré côté hook, mais défense en profondeur)
    if prompt.chars().count() < ai_smartness::constants::MIN_PROMPT_LENGTH {
        tracing::debug!(chars = prompt.chars().count(), "Prompt too short, skipping");
        return Ok(None);
    }
    process_capture(conn, pending, "prompt", prompt, None, thread_quota, guardian)
}
```

#### Phase B — Gate LLM pertinence 51-150 chars (~45 LOC)

**Fichier : `constants.rs`** — Nouvelle constante :

```rust
pub const PROMPT_RELEVANCE_GATE_MAX: usize = 150;
```

**Fichier : `daemon/processor.rs`** — Nouveau gate entre noise filter et extraction, UNIQUEMENT pour les prompts.

**Site d'appel** : dans `process_capture()`, après le noise filter (ligne 54) et avant l'extraction LLM (ligne 74). Le `agent_context` provient du `PendingContext` existant (capture précédente, max 1500 chars) :

```rust
// agent_context est déjà résolu plus haut dans process_capture() (lignes 68-72) :
// let agent_context = pending.as_ref()
//     .filter(|p| !p.is_expired(ttl))
//     .map(|ctx| ctx.content.as_str());

// Gate LLM pertinence pour prompts courts (51-150 chars).
// Objectif: éviter de créer des threads pour "parfait je voulais juste être sûr".
if source_type == "prompt" {
    let char_count = cleaned.chars().count();
    if char_count <= ai_smartness::constants::PROMPT_RELEVANCE_GATE_MAX {
        match check_prompt_relevance(&cleaned, agent_context, &guardian) {
            Ok(true) => {
                tracing::info!(chars = char_count, "Short prompt judged RELEVANT by gate LLM");
                // Continue vers extraction normale
            }
            Ok(false) => {
                tracing::info!(chars = char_count, "Short prompt judged NOT relevant, dropping");
                return Ok(None);
            }
            Err(e) => {
                // Gate LLM indisponible → traitement normal (fail-open)
                tracing::warn!(error = %e, "Relevance gate LLM failed, proceeding normally");
            }
        }
    }
}
```

**Fichier : `daemon/processor.rs`** — Nouvelle fonction `check_prompt_relevance()` :

```rust
/// Gate LLM pour prompts courts (51-150 chars).
/// Juge si le prompt a une valeur extractive par rapport au contexte courant.
///
/// Ordre procédural : contenu brut D'ABORD, contexte EN FIN.
/// Cohérent avec le pipeline Thinkbridge — le LLM juge le contenu avant
/// d'être influencé par le contexte existant.
fn check_prompt_relevance(
    prompt: &str,
    agent_context: Option<&str>,
    guardian: &GuardianConfig,
) -> AiResult<bool> {
    let context_block = match agent_context {
        Some(ctx) if !ctx.is_empty() => format!(
            "\n\nContexte récent de l'agent :\n---\n{}\n---",
            truncate_safe(ctx, 500)
        ),
        _ => String::new(),
    };

    let gate_prompt = format!(
        r#"Évalue si ce message utilisateur contient des informations extractibles (concepts, décisions, faits, intentions).

Message :
"{}"
{}

Réponds UNIQUEMENT par JSON : {{"relevant": true}} ou {{"relevant": false}}
- true = contient au moins un concept, fait, décision ou intention exploitable
- false = message purement procédural, confirmation, acquiescement, ou bruit conversationnel

Exemples false : "oui c'est bon", "ok parfait", "go", "dispatch", "merci", "je voulais juste être sûr"
Exemples true : "utilise Redis plutôt que Memcached", "le bug vient du parsing UTF-8", "ajoute un timeout de 30s""#,
        prompt, context_block
    );

    let model = guardian.extraction.llm.model.as_cli_flag();
    let response = ai_smartness::processing::llm_subprocess::call_claude_with_model(&gate_prompt, model)?;

    // Parse JSON response
    if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            let json_str = &response[start..=end];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                return Ok(v.get("relevant").and_then(|r| r.as_bool()).unwrap_or(true));
            }
        }
    }

    // Parse failure → fail-open (traiter comme pertinent)
    Ok(true)
}
```

#### Phase C — Fix IPC method pour prompts (~5 LOC)

**Fichier : `processing/daemon_ipc_client.rs`** — Nouvelle fonction dédiée :

```rust
/// Send a prompt capture to the daemon (distinct from tool captures).
pub fn send_prompt_capture(
    project_hash: &str,
    agent_id: &str,
    prompt: &str,
    session_id: Option<&str>,
) -> AiResult<serde_json::Value> {
    let mut params = serde_json::json!({
        "project_hash": project_hash,
        "agent_id": agent_id,
        "prompt": prompt,
    });
    if let Some(sid) = session_id {
        params["session_id"] = serde_json::Value::String(sid.to_string());
    }
    call_daemon("prompt_capture", params)
}
```

**Fichier : `hook/inject.rs`** — Remplacer l'appel `send_capture` par `send_prompt_capture` :

```rust
let _ = ai_smartness::processing::daemon_ipc_client::send_prompt_capture(
    project_hash, agent_id, &message, session_id,
);
```

Ceci envoie via `"prompt_capture"` (ipc_server.rs:285) qui crée un `CaptureJob` avec `is_prompt: true` et transmet le `session_id`.

### 2.4 Résumé des seuils

| Zone | Chars | Action | Coût LLM | Fichier principal |
|------|-------|--------|----------|-------------------|
| Rejet | < 50 | Drop capture, injection normale | 0 | inject.rs |
| Gate | 51-150 | LLM juge pertinence → drop ou normal | 1 appel court | processor.rs |
| Normal | > 150 | Pipeline extraction complet | 1-2 appels | processor.rs |

### 2.5 Ce qui ne change PAS

- Le pipeline d'injection (couches 0.5 à 6) s'exécute TOUJOURS, quelle que soit la longueur du prompt
- Les captures d'outils (PostToolUse) ne sont PAS affectées — filtrage uniquement pour `source_type == "prompt"`
- Le seuil `MIN_CAPTURE_LENGTH = 20` reste actif pour les captures outils

---

## 3. Point 2 — Logique procédurale Thinkbridge

### 3.1 Architecture en deux chemins

Le système a deux chemins **complètement indépendants et parallèles** :

```
USER PROMPT
    │
    ├───────────────────────────┐
    │                           │
    ▼                           ▼
CHEMIN A (synchrone)        CHEMIN B (asynchrone)
hook/inject.rs              daemon/processor.rs

Construit injection         Extraction LLM
layers 0.5→6               + création thread
    │                           │
    ▼                           ▼
Renvoie prompt              Crée/met à jour
augmenté à Claude Code      thread en DB
```

### 3.2 Chemin A — Injection (synchrone, hook)

`inject.rs:39-311` — S'exécute dans le process hook.

Couches construites **AVANT** retour du prompt à Claude Code :
- 0.5 : Onboarding
- 1 : Lightweight context (compteurs threads)
- 1.5 : Session state
- 1.7 : Cognitive nudge
- 1.8 : Inject queue
- 2 : Cognitive inbox
- 3 : Pins
- **4 : Memory retrieval (Engram — threads similaires)**
- 5 : Agent identity
- 5.5 : User profile
- 6 : HealthGuard

Ce chemin affecte **UNIQUEMENT** ce que le LLM principal (Claude Code) voit. Il ne touche PAS au pipeline d'extraction.

### 3.3 Chemin B — Extraction (asynchrone, daemon)

`processor.rs:38-185` → `extractor.rs:67-92` → `coherence.rs:35-53`

Le LLM d'extraction reçoit **UNIQUEMENT** :

1. **Le contenu brut nettoyé** (extractor.rs:103 → `content` paramètre)
2. **Le type source** (extractor.rs:103 → `source` paramètre)
3. **`agent_context`** depuis `PendingContext` — c'est le contenu de la **capture PRÉCÉDENTE** (max 1500 chars), PAS le contexte global

Le prompt d'extraction (extractor.rs:202-270) est structuré procéduralement :

```
ÉTAPE 1 — Classification (SANS contexte externe)   ← contenu brut D'ABORD
    → title, subjects, summary, confidence, labels
STEP 1B — Semantic explosion
    → concepts associatifs
[CONTENU À CLASSIFIER]                              ← contenu brut
STEP 2 — Importance scoring                         ← contexte EN FIN
    → agent_context (PendingContext) pour scoring
```

L'instruction explicite dans le prompt : **"ÉTAPE 1 — Classification (analysez le contenu ci-dessous, SANS contexte externe)"**

### 3.4 Ce que le LLM d'extraction ne reçoit JAMAIS

| Donnée | Injectée dans Chemin A ? | Injectée dans Chemin B ? |
|--------|:------------------------:|:------------------------:|
| Threads actifs (titres, contenus) | Oui (Layer 4) | **NON** |
| Bridges | Non | **NON** |
| Agent identity | Oui (Layer 5) | **NON** |
| Cognitive inbox | Oui (Layer 2) | **NON** |
| Pins | Oui (Layer 3) | **NON** |
| Session state | Oui (Layer 1.5) | **NON** |
| PendingContext (capture précédente) | Non | Oui (importance only) |

### 3.5 Vérification de la gate de cohérence

`coherence.rs:35-116` — Appelée APRÈS l'extraction (processor.rs:99-104).

La gate de cohérence compare le **nouveau contenu** avec le **PendingContext** (capture précédente). Elle n'utilise PAS les threads globaux. Le PendingContext est :
- Le contenu nettoyé de la dernière capture (max 1500 chars, processor.rs:164)
- Le thread_id associé
- Les labels associés
- Un timestamp (TTL configurable, défaut 10 min)

### 3.6 VERDICT Point 2

**AUCUNE ANOMALIE TROUVÉE.**

L'extraction/création de thread (Thinkbridge) opère sur le contenu **BRUT** sans contamination par les threads ou bridges existants. Le contexte global est :
- Injecté uniquement dans le prompt utilisateur (Chemin A, Layer 4)
- Jamais transmis au pipeline d'extraction (Chemin B)
- Le `agent_context` utilisé pour le scoring d'importance est le contenu de la capture précédente (PendingContext), pas le contexte global

L'ordre procédural dans le prompt d'extraction est correct :
1. Contenu brut D'ABORD (classification sans contexte)
2. Contexte (PendingContext) EN FIN (importance scoring uniquement)

---

## 4. Cohérence Point 1 ↔ Point 2

La nouvelle gate LLM pour les prompts 51-150 chars (§2.3 Phase B) suit le **même ordre procédural** que le pipeline Thinkbridge :

1. Le prompt brut est présenté D'ABORD au gate LLM
2. Le contexte agent (PendingContext) est ajouté EN FIN
3. Le gate LLM juge la pertinence du prompt **en ayant vu le contenu brut avant le contexte**

Ceci est cohérent avec la précision de cor : "Le contexte global transmis au LLM pour ce jugement est envoyé procéduralement en FIN de traitement."

---

## 5. Fichiers modifiés

| Fichier | Phase | Action | LOC estimées |
|---------|-------|--------|:------------:|
| `src/hook/inject.rs` | A, C | Guard < 50 chars + switch vers `send_prompt_capture` | ~15 |
| `src/daemon/processor.rs` | A, B | Safety net + gate LLM `check_prompt_relevance()` | ~50 |
| `src/processing/daemon_ipc_client.rs` | C | Nouvelle fn `send_prompt_capture()` | ~15 |
| `src/constants.rs` | B | Ajout `PROMPT_RELEVANCE_GATE_MAX = 150` | ~1 |
| **Total** | | | **~81** |

### Fichiers NON modifiés

- `extractor.rs` — pipeline extraction inchangé
- `coherence.rs` — gate cohérence inchangée
- `capture.rs` — capture outils inchangée (filtrage prompts uniquement)
- `ipc_server.rs` — handlers `tool_capture` et `prompt_capture` déjà corrects
- `capture_queue.rs` — worker dispatch `is_prompt` déjà correct
- `intelligence/` — thread_manager, engram_retriever inchangés

---

## 6. Risques et mitigations

| # | Risque | Sévérité | Mitigation |
|---|--------|----------|------------|
| R1 | Gate LLM indisponible (timeout, rate limit) | MOYENNE | Fail-open : si le gate échoue, traitement normal (§2.3 Phase B) |
| R2 | Coût LLM additionnel pour prompts 51-150 | BASSE | Un seul appel Haiku court (< 200 tokens entrée), ~0.001$ par call |
| R3 | Faux négatif du gate (prompt pertinent jugé non-pertinent) | BASSE | Exemples dans le prompt + fail-open par défaut |
| R4 | `chars().count()` vs `.len()` : différence Unicode | BASSE | Utiliser `chars().count()` partout (cohérent avec la demande en "chars") |
| R5 | Race condition : prompt filtré côté hook mais pas côté daemon | NULLE | Safety net en profondeur (§2.3 Phase A) — les deux côtés filtrent |

---

## 7. Réponses aux questions de cor

### Q1 : Existe-t-il déjà un seuil de longueur minimum pour les prompts ?
**Oui, mais dead code.** `MIN_PROMPT_LENGTH = 50` existe dans `constants.rs:46` mais n'est référencé nulle part. Le seul filtre actif est `MIN_CAPTURE_LENGTH = 20` bytes dans `cleaner.rs`, appliqué indifféremment à tous les types de capture.

### Q2 : Le contexte global parasite-t-il la création du Thinkbridge ?
**Non.** L'extraction (Chemin B) est complètement isolée de l'injection (Chemin A). Le LLM d'extraction ne reçoit jamais les threads existants, bridges, ou mémoire globale. Le seul contexte transmis est le `PendingContext` (contenu de la capture précédente, max 1500 chars), utilisé uniquement pour le scoring d'importance en fin de prompt.

---

## 8. Historique reviews

### Review pub R1 — 2026-02-24

**VERDICT : APPROUVÉ** sous 1 clarification mineure.

**Point 2** : AUCUNE ANOMALIE confirmée. Chemin A/B indépendants, pipeline extraction jamais contaminé par threads/bridges/mémoire globale.

**Réponses aux questions dev** :
- Q1 (coût gate LLM) : ACCEPTABLE — async, ~0.001$/call Haiku, Phase A filtre déjà < 50 chars
- Q2 (fail-open) : CORRECT — fail-closed = perte de données silencieuse inacceptable
- Q3 (Phase C worker impact) : POSITIF, ZÉRO RÉGRESSION — worker dispatch already handles both paths, Phase C corrige bug existant + transmet session_id
- Q4 (ordre procédural gate LLM) : COHÉRENCE CONFIRMÉE — §4 du plan correct

**Clarification requise (mineure)** :
§2.3 Phase B — documenter explicitement le site d'appel de `agent_context` dans `check_prompt_relevance()` :
```rust
let agent_context = pending.as_ref().map(|p| p.content.as_str());
```
Non-bloquant, mais nécessaire pour éviter ambiguïté à l'implémentation.

**→ Clarification intégrée** dans §2.3 Phase B (commentaire explicite sur le site d'appel `agent_context` ajouté au bloc de code).

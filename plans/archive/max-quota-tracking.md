# Recherche : suivi consommation forfait Anthropic MAX pour heartbeat

## 1. Détection du plan (MAX 100$ vs MAX 200$)

### Source : `~/.claude/.credentials.json`

Le fichier credentials contient le type d'abonnement et le tier de rate limit :

```json
{
  "claudeAiOauth": {
    "subscriptionType": "max",
    "rateLimitTier": "default_claude_max_20x"
  }
}
```

### Mapping des plans

| `rateLimitTier` | Plan | Prix |
|---|---|---|
| `default_claude_pro` | Pro | 20$/mois |
| `default_claude_max_5x` | MAX 5x | 100$/mois |
| `default_claude_max_20x` | MAX 20x | 200$/mois |

Le champ `subscriptionType` vaut `"max"` pour les deux tiers MAX, et `"pro"` pour Pro. Le `rateLimitTier` distingue MAX 100 (5x) de MAX 200 (20x).

### Heures d'utilisation estimées par plan (source Anthropic)

| Plan | Sonnet 4.x | Opus 4.x |
|---|---|---|
| MAX 5x (100$) | 140-280 h | 15-35 h |
| MAX 20x (200$) | 240-480 h | 24-40 h |

### Lecture en Rust

```rust
fn detect_plan() -> Option<PlanInfo> {
    let creds_path = dirs::home_dir()?.join(".claude/.credentials.json");
    let content = std::fs::read_to_string(&creds_path).ok()?;
    let creds: serde_json::Value = serde_json::from_str(&content).ok()?;

    let oauth = creds.get("claudeAiOauth")?;
    let sub_type = oauth.get("subscriptionType")?.as_str()?;
    let tier = oauth.get("rateLimitTier")?.as_str()?;

    Some(PlanInfo {
        subscription_type: sub_type.to_string(),  // "max", "pro", "free"
        rate_limit_tier: tier.to_string(),          // "default_claude_max_20x"
        is_max: sub_type == "max",
        multiplier: match tier {
            t if t.contains("20x") => 20,
            t if t.contains("5x") => 5,
            _ => 1,
        },
    })
}
```

**Complexité** : ~15 LOC. Aucune dépendance nouvelle (serde_json déjà utilisé).

---

## 2. Mesure de la consommation : les headers API unifiés

### Headers disponibles dans les réponses API

L'API Anthropic retourne des headers de rate limit unifiés sur chaque réponse. Ces headers sont **non-documentés officiellement** mais utilisés par Claude Code internement.

#### Headers unifiés (5h + 7d)

| Header | Description | Exemple |
|---|---|---|
| `anthropic-ratelimit-unified-status` | Statut global | `"allowed"`, `"warning"`, `"rate_limited"` |
| `anthropic-ratelimit-unified-5h-status` | Statut fenêtre 5h | `"allowed"` |
| `anthropic-ratelimit-unified-5h-utilization` | Utilisation 5h (0.0 → 1.0) | `0.018416969696969696` |
| `anthropic-ratelimit-unified-5h-reset` | Reset 5h (Unix epoch) | `1764554400` |
| `anthropic-ratelimit-unified-7d-status` | Statut fenêtre 7j | `"allowed"` |
| `anthropic-ratelimit-unified-7d-utilization` | Utilisation 7j (0.0 → 1.0) | `0.7370692663445869` |
| `anthropic-ratelimit-unified-7d-reset` | Reset 7j (Unix epoch) | `1764615600` |
| `anthropic-ratelimit-unified-representative-claim` | Fenêtre contraignante | `"five_hour"` ou `"seven_day"` |
| `anthropic-ratelimit-unified-fallback-percentage` | Taux fallback si limité | `0.2` |
| `anthropic-ratelimit-unified-overage-disabled-reason` | Raison si overage désactivé | `"org_level_disabled"` |

#### Headers par minute (documentés officiellement)

| Header | Description |
|---|---|
| `anthropic-ratelimit-requests-limit` | RPM max |
| `anthropic-ratelimit-requests-remaining` | RPM restant |
| `anthropic-ratelimit-tokens-limit` | Tokens/min max |
| `anthropic-ratelimit-tokens-remaining` | Tokens/min restant |
| `anthropic-ratelimit-input-tokens-limit` | Input tokens/min max |
| `anthropic-ratelimit-input-tokens-remaining` | Input tokens/min restant |
| `anthropic-ratelimit-output-tokens-limit` | Output tokens/min max |
| `anthropic-ratelimit-output-tokens-remaining` | Output tokens/min restant |
| `retry-after` | Secondes avant retry (si 429) |

### Le problème : ces headers ne sont PAS dans le transcript

Le transcript JSONL de Claude Code contient **uniquement** le corps de la réponse API (champs `usage` dans le message). Les headers HTTP ne sont pas persistés. Les seuls champs de rate limit dans le transcript sont :

```json
{
  "usage": {
    "input_tokens": 1,
    "cache_creation_input_tokens": 319,
    "cache_read_input_tokens": 107744,
    "output_tokens": 2,
    "service_tier": "standard",
    "inference_geo": "not_available"
  }
}
```

Le champ `service_tier: "standard"` est le seul indice de tier, mais il ne distingue pas les plans MAX entre eux.

---

## 3. Méthodes possibles et limites

### Méthode A — Lecture `credentials.json` (plan uniquement)

| Aspect | Détail |
|---|---|
| Données obtenues | Plan (pro/max), tier (5x/20x) |
| Consommation | NON — aucune donnée de consommation |
| Complexité | ~15 LOC |
| Fiabilité | Haute — fichier stable |
| Coût | Zéro |

**Verdict** : UTILE mais insuffisant seul. Permet de savoir quel plan, pas combien consommé.

### Méthode B — Probe API avec token OAuth (consommation live)

Envoyer une requête API minimale avec le token OAuth de `credentials.json` et lire les headers unifiés de la réponse.

```rust
// Pseudo-code
fn probe_usage(oauth_token: &str) -> Option<UsageInfo> {
    let response = reqwest::blocking::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("Authorization", format!("Bearer {}", oauth_token))
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .body(r#"{"model":"claude-haiku-4-5-20251001","max_tokens":1,"messages":[{"role":"user","content":"x"}]}"#)
        .send().ok()?;

    let h = response.headers();
    Some(UsageInfo {
        status_5h: h.get("anthropic-ratelimit-unified-5h-status")?.to_str().ok()?.to_string(),
        utilization_5h: h.get("anthropic-ratelimit-unified-5h-utilization")?.to_str().ok()?.parse().ok()?,
        reset_5h: h.get("anthropic-ratelimit-unified-5h-reset")?.to_str().ok()?.parse().ok()?,
        status_7d: h.get("anthropic-ratelimit-unified-7d-status")?.to_str().ok()?.parse().ok()?,
        utilization_7d: h.get("anthropic-ratelimit-unified-7d-utilization")?.to_str().ok()?.parse().ok()?,
        reset_7d: h.get("anthropic-ratelimit-unified-7d-reset")?.to_str().ok()?.parse().ok()?,
        representative_claim: h.get("anthropic-ratelimit-unified-representative-claim")?.to_str().ok()?.to_string(),
    })
}
```

| Aspect | Détail |
|---|---|
| Données obtenues | Utilisation 5h (%), utilisation 7j (%), statut, reset times |
| Précision | 100% — données directes du serveur Anthropic |
| Complexité | ~40 LOC |
| Coût | ~0.001$ par probe (Haiku, 1 token) |
| Fréquence | Toutes les **15 min** pour commencer (réduire à 5 min après validation empirique du coût quota) |
| Dépendance | Crate `reqwest` (déjà dans les deps ?) ou `ureq` |
| Risque OAuth | Le token expire. **NE PAS refresh nous-mêmes** — conflits avec Claude Code qui gère le lifecycle. Vérifier `expiresAt` avant probe, fallback plan-only si expiré. |
| Risque blocking | Le heartbeat loop est **synchrone**. Un call HTTP bloque PID tracking, wake checks, cognitive checks pendant 1-5s. **Fix** : thread séparé ou timeout strict 3s. |
| Risque headers | Les headers unifiés ne sont pas documentés officiellement (peuvent changer). Fallback graceful. |
| Risque quota | Le probe consomme du quota unifié (pas en $, en crédits opaques). Impact à vérifier empiriquement. |

**Verdict** : MEILLEURE méthode. Seul moyen d'obtenir la consommation réelle du forfait.

### Méthode C — Sommation des tokens du transcript (estimation)

Sommer tous les tokens (input + output) du transcript JSONL sur les 5 dernières heures ou 7 derniers jours, puis comparer à un quota estimé.

| Aspect | Détail |
|---|---|
| Données obtenues | Estimation grossière de la consommation |
| Précision | ~40-60% — ne connaît pas le quota exact, ne voit que cette session |
| Complexité | ~50 LOC |
| Coût | Zéro |
| Limitation CRITIQUE | Ne voit que le transcript de cette session. L'utilisateur peut avoir d'autres sessions (claude.ai, autres projets) qui consomment le même quota. Sous-estimation garantie. |

**Verdict** : INSUFFISANT seul. Peut servir de fallback dégradé mais massivement imprécis.

### Méthode D — `stats-cache.json` (cumul local)

Le fichier `~/.claude/stats-cache.json` contient des compteurs cumulés par modèle :

```json
"claude-opus-4-6": {
  "inputTokens": 867167,
  "outputTokens": 307462,
  "cacheReadInputTokens": 3109394232,
  "cacheCreationInputTokens": 141321828,
  "costUSD": 0
}
```

| Aspect | Détail |
|---|---|
| Données obtenues | Tokens cumulés par modèle |
| Fenêtre temporelle | Non — cumul depuis le début, pas par 5h/7j |
| `costUSD` | Toujours 0 (abonnement, pas facturation par token) |
| Limitation | Pas de données par fenêtre de temps. Pas de quota connu. |

**Verdict** : INUTILE pour le suivi de consommation du forfait. Les compteurs sont cumulés all-time, pas par fenêtre.

---

## 4. Recommandation : architecture hybride

### Phase 1 — Détection plan (immédiat, ~15 LOC)

Lire `credentials.json` et exposer dans le heartbeat :

```json
{
  "plan": {
    "type": "max",
    "tier": "20x",
    "multiplier": 20
  }
}
```

Aucun coût, aucune requête réseau.

### Phase 2 — Probe API avec cache (priorité haute, ~80 LOC)

Ajouter un probe **dans un thread séparé** (ne pas bloquer le heartbeat loop) :
1. Toutes les **15 minutes** (réduire à 5 min après validation empirique), spawner un thread
2. Vérifier `expiresAt` du token OAuth — si expiré, skip (fallback plan-only)
3. Envoyer un message minimal à Haiku avec **timeout 3s**
4. Lire les headers unifiés
5. Cacher dans `beat.json` :

```json
{
  "quota": {
    "plan_type": "max",
    "plan_tier": "20x",
    "utilization_5h": 0.42,
    "utilization_7d": 0.73,
    "status_5h": "allowed",
    "status_7d": "allowed",
    "representative_claim": "five_hour",
    "reset_5h": 1764554400,
    "reset_7d": 1764615600,
    "last_probe_at": "2026-02-22T06:30:00Z"
  }
}
```

4. L'injection hook affiche un indicateur compact dans le contexte :

```
Quota MAX 20x: 5h 42% | 7d 73% | next reset 2h15m
```

### Phase 3 — Alertes et comportement adaptatif (optionnel, ~30 LOC)

- Si `utilization_5h > 0.8` → injecter un avertissement dans le contexte
- Si `utilization_7d > 0.9` → mode économie (préférer Haiku, réduire les tool calls)
- Si `status = "rate_limited"` → signaler clairement à l'agent

---

## 5. Risques et limitations

| Risque | Sévérité | Mitigation |
|---|---|---|
| Headers unifiés non-documentés | HAUTE | Best-effort : si absent, fallback sur plan-only |
| Token OAuth expire | HAUTE | **NE PAS refresh nous-mêmes** — laisser Claude Code gérer le lifecycle. Vérifier `expiresAt` avant chaque probe, fallback plan-only si expiré. |
| Probe bloque le heartbeat loop | HAUTE | **Thread séparé** ou timeout strict 3s. Le heartbeat loop gère PID tracking, wake checks, cognitive — ne doit pas bloquer. |
| Probe consomme du quota $ | BASSE | Haiku + 1 token = ~0.001$. 96 probes/jour (15 min) = ~0.10$/jour. Négligeable. |
| Probe consomme du quota unifié | MOYENNE | Le quota unifié est en crédits opaques, pas en $. Fréquence conservatrice 15 min pour commencer. Vérifier empiriquement que le probe n'impacte pas significativement le quota 5h/7d. |
| Le quota est partagé entre sessions | INFO | Le probe retourne le quota GLOBAL (toutes sessions confondues). C'est le comportement voulu. |
| `credentials.json` pas présent (API key, pas OAuth) | MOYENNE | Si pas de `claudeAiOauth`, l'utilisateur est en mode API (pas de forfait). Ne pas activer le suivi. |
| Probe nécessite `reqwest` ou `ureq` | BASSE | Si pas en deps, utiliser `std::process::Command` avec `curl`. |

---

## 6. Runtime Python : aucune implémentation existante

Le runtime Python (`/home/vzcrow/Dev/protobak/ai_smartness-python/`) ne contient **aucune** logique de suivi de forfait MAX. Les termes "quota", "credit", "rate_limit" trouvés sont des concepts internes (quotas de threads, crédits mémoire) sans rapport avec le billing Anthropic.

---

## 7. Réponses aux questions de cor

| # | Question | Réponse |
|---|---|---|
| 1 | Comment détecter MAX 100$ vs MAX 200$ ? | `~/.claude/.credentials.json` → `rateLimitTier`: `max_5x` = 100$, `max_20x` = 200$ |
| 2 | Comment mesurer la consommation par fenêtre 5h ? | Probe API → header `anthropic-ratelimit-unified-5h-utilization` (0.0-1.0) |
| 3 | Comment mesurer la consommation hebdomadaire ? | Probe API → header `anthropic-ratelimit-unified-7d-utilization` (0.0-1.0) |
| 4 | Headers de réponse API pour quota restant ? | OUI — 11 headers unifiés + 8 headers par minute (documentés §2) |
| 5 | Infos billing dans le transcript JSONL ? | NON — le transcript ne contient que `usage` (tokens) et `service_tier: "standard"`. Pas de headers HTTP. |
| 6 | Fichier config indiquant le plan ? | OUI — `~/.claude/.credentials.json` contient `subscriptionType` et `rateLimitTier` |

Sources :
- [Rate limits - Claude API Docs](https://platform.claude.com/docs/en/api/rate-limits)
- [Using Claude Code with your Pro or Max plan](https://support.claude.com/en/articles/11145838-using-claude-code-with-your-pro-or-max-plan)
- [Bug: Rate limit blocking ignores representative-claim header](https://github.com/anthropics/claude-code/issues/12829)
- [Feature: Plan usage tracking for statusline](https://github.com/gsd-build/get-shit-done/issues/440)
- [Claude Max Plan Explained](https://intuitionlabs.ai/articles/claude-max-plan-pricing-usage-limits)

---

## 8. Historique des reviews

### Review pub — R1 (2026-02-22)

**Verdict** : Phase 1 APPROUVÉE. Phase 2 CONDITIONNÉE (3 problèmes).

| # | Sévérité | Condition | Statut |
|---|---|---|---|
| P1 | HAUTE | OAuth token lifecycle : ne pas refresh nous-mêmes (conflits Claude Code). Vérifier `expiresAt` avant probe, fallback plan-only si expiré. | INTÉGRÉ — §3 Méthode B + §5 risque OAuth |
| P2 | MOYENNE | Blocking HTTP : heartbeat loop synchrone, un call HTTP bloque PID tracking/wake/cognitive. Fix : thread séparé ou timeout 3s. | INTÉGRÉ — §4 Phase 2 thread séparé + timeout 3s |
| P3 | MOYENNE | Quota unifié inconnu : le probe consomme du quota opaque. Commencer à 15 min, pas 5 min. | INTÉGRÉ — §3 fréquence 15 min + §4 Phase 2 |

**Autres retours intégrés** :
- Coût $ acceptable, coût quota à vérifier empiriquement
- Headers non-doc OK si fallback graceful
- Ne PAS gérer le refresh OAuth nous-mêmes
- LOC Phase 2 révisé à ~80 LOC (thread séparé + validation token)

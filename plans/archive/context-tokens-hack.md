# Hack context_tokens : portage Python → Rust

## 1. Le hack Python — comment ça marchait

### Source : `ai_smartness-python/ai_smartness/storage/heartbeat.py:250-361`

Le hack lit le **fichier transcript JSONL de Claude Code** et parse les dernières valeurs de tokens via regex.

### Mécanisme complet

```
┌─────────────────────────────────────────────────┐
│ Claude Code écrit un fichier transcript :        │
│ ~/.claude/projects/{project_dir}/{session_id}.jsonl │
│                                                   │
│ Chaque entrée assistant contient un bloc usage :  │
│ {                                                 │
│   "type": "assistant",                            │
│   ...usage fields inline dans le JSON...          │
│   "cache_creation_input_tokens": 298,             │
│   "cache_read_input_tokens": 153696,              │
│   "input_tokens": 1,                              │
│   "output_tokens": 24                             │
│ }                                                 │
└──────────────┬──────────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────────┐
│ Hook capture.py (PostToolUse) — à chaque tool call │
│                                                    │
│ 1. Appelle update_context_tracking(ai_path, sid)   │
│ 2. → update_context_tokens(ai_path, session_id)    │
│ 3. → _read_last_usage(transcript_path)             │
│    - Regex: "cache_read_input_tokens":(\d+)        │
│    - Regex: "input_tokens":(\d+)                   │
│    - total = cache_read + input                    │
│    - percent = (total / 200000) * 100              │
│ 4. Écrit dans heartbeat.json :                     │
│    context_tokens, context_percent,                │
│    context_window_size, compact_threshold           │
└──────────────────────────────────────────────────┘
```

### Code Python clé (`_read_last_usage`)

```python
def _read_last_usage(transcript_path: Path) -> Optional[dict]:
    content = transcript_path.read_text(encoding='utf-8')

    # Find all usage entries with cache_read_input_tokens
    pattern = r'"cache_read_input_tokens":(\d+)'
    matches = re.findall(pattern, content)
    if not matches:
        return None

    cache_tokens = int(matches[-1])  # Last (most recent) value

    input_pattern = r'"input_tokens":(\d+)'
    input_matches = re.findall(input_pattern, content)
    input_tokens = int(input_matches[-1]) if input_matches else 0

    total_tokens = cache_tokens + input_tokens
    percent = round((total_tokens / CONTEXT_WINDOW_SIZE) * 100, 1)

    return {
        "tokens": total_tokens,
        "percent": percent,
        "cache_tokens": cache_tokens,
        "input_tokens": input_tokens
    }
```

### Throttle adaptatif (`_should_update_context`)

```python
CONTEXT_WINDOW_SIZE = 200000
THROTTLE_TIME_SECONDS = 30      # Below 70%: update every 30s
THROTTLE_PERCENT_THRESHOLD = 70  # Above 70%: switch to delta-based
THROTTLE_PERCENT_DELTA = 5       # Min % change to trigger update

def _should_update_context(heartbeat, new_percent):
    if new_percent < 70:
        return elapsed >= 30  # Time-based
    else:
        return abs(new_percent - last_percent) >= 5  # Delta-based
```

### Quand c'était appelé

- **Hook PostToolUse** (`capture.py:362-363`) : à chaque tool call, appelle `update_context_tracking()`
- Le `session_id` vient du JSON stdin de Claude Code (disponible dans le hook)
- Le chemin transcript est résolu via : `~/.claude/projects/{project_dir}/{session_id}.jsonl`

---

## 2. Vérification sur le runtime actuel

### Le fichier transcript existe toujours

```
~/.claude/projects/-home-vzcrow-Dev-ai-smartness-dev/476ebf11-1e18-4b35-8024-1e486b4cc776.jsonl
```

### Les champs tokens sont toujours présents (vérifié le 2026-02-22)

```
cache_creation_input_tokens: 298
cache_read_input_tokens: 153696
input_tokens: 1
output_tokens: 24
```

### Calcul du contexte (formule corrigée avec output_tokens)

```
total = cache_creation + cache_read + input + output = 298 + 153696 + 1 + 24 = 154019
context_percent = (154019 / 200000) * 100 = 77.0%
```

Note : dans cet exemple l'output est petit (24 tokens). Pour une réponse longue ou avec extended thinking, l'output peut représenter 5-25% du total.

---

## 3. Bugs dans le hack Python

Le code Python ne comptait que `cache_read_input_tokens + input_tokens`. Deux omissions :

### Bug 1 : `cache_creation_input_tokens` manquant

Les tokens de cache creation (delta entre le contenu caché et le nouveau contenu envoyé) n'étaient pas comptés. Sous-estimation de quelques centaines à quelques milliers de tokens.

### Bug 2 (CRITIQUE) : `output_tokens` manquant

La [documentation officielle Anthropic](https://platform.claude.com/docs/en/build-with-claude/context-windows) est explicite :

> *"The context window refers to all the text a language model can reference when generating a response, **including the response itself**."*
> *"All input and output components count toward the context window."*

Le hack Python ne comptait **aucun** token de sortie. Cela sous-estime massivement le contexte :
- Les réponses courtes (24 tokens) : delta négligeable
- Les réponses longues (code généré, plans) : 2000-8000 tokens manquants
- Avec extended thinking : les thinking tokens comptent AUSSI pendant le tour courant (potentiellement 10000-50000+ tokens non comptés)

**Impact** : l'agent croit avoir 20-30% de marge alors qu'il est proche de la limite → compaction manquée → perte de contexte.

**Nuance** : les thinking tokens des tours précédents sont automatiquement strippés par l'API Claude. Donc `output_tokens` du tour courant surestiment très légèrement le contexte du tour suivant. Mais il vaut mieux **légèrement surestimer** (compaction anticipée de 1-2%) que **massivement sous-estimer** (perte de contexte).

### Fix pour le portage Rust : inclure les 4 champs

```
total = cache_creation_input_tokens + cache_read_input_tokens + input_tokens + output_tokens
```

C'est la seule formule qui colle au 100% réel de la fenêtre de contexte.

---

## 4. Comment adapter en Rust

### 4.1 Résolution du chemin transcript

Le MCP server Rust connaît déjà :
- `project_hash` (via contexte MCP)
- Le `session_id` est disponible dans le hook inject (paramètre de `run()`)

**Chemin transcript** :
```rust
// Le project_dir dans ~/.claude/projects/ est le chemin du projet
// avec les / remplacés par des - et préfixé d'un -
// Exemple: /home/vzcrow/Dev/ai-smartness-dev → -home-vzcrow-Dev-ai-smartness-dev

let claude_projects = dirs::home_dir()?.join(".claude/projects");
// Trouver le bon dossier projet — soit par convention de nommage,
// soit en scannant les dossiers pour un fichier {session_id}.jsonl
let transcript = claude_projects.join(project_dir_name).join(format!("{}.jsonl", session_id));
```

**Problème** : Le MCP server connaît le `project_hash` (SHA-256 tronqué), pas le `project_dir_name` (le path hyphenated). Il faut soit :
- (a) Scanner `~/.claude/projects/*/` pour trouver le fichier `{session_id}.jsonl`
- ~~(b) Calculer le `project_dir_name` depuis le workspace path~~ — **CASSÉ** : `workspace_path` est `String::new()` dans tous les sites de registration (`cli/init.rs:97`, `mcp/tools/mod.rs:388`, `gui/commands.rs:975`, `registry/heartbeat.rs:188`). Le champ n'est jamais peuplé.
- (c) Stocker le project_dir_name quelque part dans les données agent

**Recommandation** : Approche (a) — scanner les dossiers projet. Robuste, pas de dépendance sur `workspace_path` :
```rust
fn find_transcript(session_id: &str) -> Option<PathBuf> {
    let claude_projects = dirs::home_dir()?.join(".claude/projects");
    for entry in std::fs::read_dir(&claude_projects).ok()? {
        let dir = entry.ok()?.path();
        if !dir.is_dir() { continue; }
        let transcript = dir.join(format!("{}.jsonl", session_id));
        if transcript.exists() {
            return Some(transcript);
        }
    }
    None
}
```
Complexité : ~12 LOC. Le scan est O(n) sur le nombre de projets (~5-20 dossiers, négligeable).

### 4.2 Parsing du transcript (dernière valeur)

Le fichier JSONL peut être volumineux (10-50 MB pour les longues sessions). Le hack Python lit tout le fichier et fait `re.findall()` — ça marche mais c'est inefficace.

**Approche Rust optimisée** : lire le fichier **par la fin** (tail) et chercher la dernière occurrence :

```rust
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use regex::Regex;

fn read_last_usage(transcript_path: &Path) -> Option<ContextInfo> {
    let file = std::fs::File::open(transcript_path).ok()?;
    let metadata = file.metadata().ok()?;
    let file_size = metadata.len();

    // Lire les derniers 32KB minimum (extended thinking peut produire des entrées JSONL 50-100KB+)
    // Fallback adaptatif : si pas trouvé dans 32K, essayer 128K puis 512K
    let read_size = file_size.min(32_000);
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::End(-(read_size as i64))).ok()?;

    let mut tail = String::new();
    reader.read_to_string(&mut tail).ok()?;

    // Regex pour les 4 champs (chercher la dernière occurrence)
    let cache_creation = Regex::new(r#""cache_creation_input_tokens":(\d+)"#).ok()?;
    let cache_read = Regex::new(r#""cache_read_input_tokens":(\d+)"#).ok()?;
    let input = Regex::new(r#""input_tokens":(\d+)"#).ok()?;
    let output = Regex::new(r#""output_tokens":(\d+)"#).ok()?;

    let cc: u64 = cache_creation.find_iter(&tail).last()?.as_str()
        .split(':').last()?.parse().ok()?;
    let cr: u64 = cache_read.find_iter(&tail).last()?.as_str()
        .split(':').last()?.parse().ok()?;
    let inp: u64 = input.find_iter(&tail).last()?.as_str()
        .split(':').last()?.parse().ok()?;
    let out: u64 = output.find_iter(&tail).last()?.as_str()
        .split(':').last()?.parse().ok()?;

    // CRITIQUE : inclure output_tokens pour coller au 100% réel de la fenêtre
    // Ref: https://platform.claude.com/docs/en/build-with-claude/context-windows
    // "All input and output components count toward the context window"
    let total = cc + cr + inp + out;
    let percent = (total as f64 / CONTEXT_WINDOW_SIZE as f64) * 100.0;

    Some(ContextInfo { tokens: total, percent, cache_creation: cc, cache_read: cr, input: inp, output: out })
}
```

### 4.3 Où appeler dans le runtime Rust

**Option A — Dans le heartbeat loop** (server.rs, toutes les 10s) :
- Avantage : Indépendant des tool calls. Détecte le contexte même pendant que l'agent "réfléchit".
- Inconvénient : Nécessite le session_id (le heartbeat loop ne l'a pas actuellement).
- Fix : Stocker le session_id dans BeatState (le hook inject l'a déjà).

**Option B — Dans le hook inject** (inject.rs, à chaque prompt) :
- Avantage : Le session_id est disponible. Exécution à chaque prompt = fréquence correcte.
- Inconvénient : Pas de mise à jour entre les prompts (mais le contexte ne change pas sans prompt).

**Option C — Dans route_tool** (mod.rs, à chaque tool call) — même pattern que le Python :
- Avantage : Fréquence élevée. Correspond exactement au hack Python.
- Inconvénient : Le session_id n'est pas dans le ToolContext actuel.

**Recommandation** : **Option B (hook inject)** avec throttle adaptatif. Raisons :
1. Le session_id est déjà disponible dans `inject.rs:run()`
2. Le contexte ne change qu'entre les prompts (les tool calls sont dans le même prompt)
3. Pas besoin du `regex` crate — lire par la fin avec `str::rfind()` suffit
4. Le throttle adaptatif (< 70% → toutes les 30s, ≥ 70% → delta 5%) est simple à porter

### 4.4 Modifications requises

| Fichier | Modification | LOC |
|---|---|---|
| `src/storage/beat.rs` | Garder `context_tokens: Option<u64>`, ajouter `context_output_tokens`, `cache_creation_tokens`, `cache_read_tokens`, `context_source` | ~10 |
| `src/hook/inject.rs` | Ajouter appel `update_context_from_transcript()` après `record_interaction()`, fallback E1 | ~8 |
| Nouveau : `src/storage/transcript.rs` | `find_transcript()` scan dossiers + `read_last_usage()` lecture tail adaptative (32K→128K→512K) + parsing 4 champs | ~60 |
| `src/storage/beat.rs` | Fonction `should_update_context()` — throttle adaptatif | ~15 |
| `src/storage/path_utils.rs` | Constante `CLAUDE_PROJECTS_DIR` + helper | ~5 |

**Total estimé** : ~100-120 LOC (incluant scan dossiers, read adaptatif, fallback E1)

### 4.5 Dépendances

- **Aucune nouvelle crate requise** : `str::rfind()` suffit pour le parsing (pas besoin de `regex`)
- Le fichier transcript est en format JSONL standard
- Le chemin `~/.claude/projects/` est stable (convention Claude Code)

### 4.6 Risques

| Risque | Mitigation |
|---|---|
| Claude Code change le format du transcript | Le parsing est best-effort (retourne None si échec). Pas de crash. |
| Le transcript est très volumineux (50+ MB) | Lire seulement les 10 derniers KB (seek from end). |
| Le chemin `~/.claude/projects/` change | Configurable via variable d'environnement (fallback). |
| Permissions fichier transcript | Le MCP server et Claude Code tournent sous le même user. |
| Le session_id n'est pas disponible dans certains contextes | Fallback : scanner le dossier projet pour le .jsonl le plus récent. |
| context_window_size varie selon le modèle | **NE PAS hardcoder 200000** — Opus 4.6 supporte 200K (défaut) ou 1M (beta). Rendre configurable dès le départ via config.json (défaut: 200000). Voir §6. |

---

## 5. Comparaison avec E1 (tool_io_bytes)

| Critère | E1 (tool_io_bytes) | Hack transcript |
|---|---|---|
| Précision | ~30-50% du contexte | ~99% (tokens réels de l'API) |
| Complexité | ~25 LOC | ~75 LOC |
| Dépendances | Aucune | Accès filesystem ~/.claude/ |
| Robustesse | Très robuste | Dépend du format Claude Code |
| Détecte la compaction | Non | Oui (tokens reset → compaction détectée) |

**Recommandation** : Implémenter le hack transcript comme **source primaire**, avec E1 (tool_io_bytes) comme **fallback dégradé** si le transcript est indisponible.

**Chaîne de précision** :
1. Transcript JSONL (~99% précision) — source primaire
2. tool_io_bytes (~30-50% précision) — fallback si transcript absent/illisible
3. None — aucune donnée

Le champ `context_tokens` dans BeatState prend son sens réel quand alimenté par le transcript. Si fallback sur E1, le champ `context_source` indique la source ("transcript" | "tool_io" | null).

---

## 6. Context window size — auto-détection par modèle

Le hack Python hardcodait `CONTEXT_WINDOW_SIZE = 200000`. C'est **incorrect** — l'utilisateur peut changer de modèle via `/model` dans Claude Code, et chaque modèle a une fenêtre différente.

### Tailles par modèle (février 2026)

| Modèle (model ID dans transcript) | Context Window |
|---|---|
| `claude-opus-4-6` | 200K (défaut), 1M (beta header) |
| `claude-sonnet-4-6` | 200K (défaut), 1M (beta header) |
| `claude-sonnet-4-5-20241022` | 200K |
| `claude-haiku-4-5-20251001` | 200K |

### Le transcript contient le modèle

Le champ `"model"` est présent dans chaque entrée du transcript JSONL :
```json
{"type":"assistant","model":"claude-opus-4-6",...}
```

### Approche recommandée : auto-détection + override config

```rust
fn get_context_window_size(model_id: &str, config_override: Option<u64>) -> u64 {
    // 1. Config override a la priorité (pour les cas beta 1M)
    if let Some(size) = config_override {
        return size;
    }

    // 2. Auto-détection par model ID
    // Tous les modèles actuels ont 200K par défaut
    // Le beta 1M nécessite un header API spécifique que le MCP ne peut pas détecter
    200_000
}
```

**Logique** :
1. Parser le `model` depuis la dernière entrée du transcript (même regex que les tokens)
2. Mapper vers la taille connue (actuellement 200K pour tous les modèles en mode défaut)
3. Si `context_window_size` est spécifié dans config.json, utiliser cette valeur (override)
4. Le calcul `percent = (tokens / window_size) * 100` s'adapte automatiquement

```json
// config.json (optionnel)
{
  "context_window_size": 1000000  // override si beta 1M activé
}
```

### Pourquoi pas juste le model ID ?

Le model ID seul ne suffit pas pour distinguer 200K vs 1M — la fenêtre 1M est activée par un **header API beta** (`anthropic-beta: context-1m-2025-08-07`), pas par le modèle. Un agent sur Opus 4.6 peut être en 200K ou 1M selon la configuration Claude Code de l'utilisateur. D'où l'override config.

### Détection indirecte de la taille réelle

Une alternative astucieuse : si les tokens observés dépassent 200K (ex: 350K), on sait que la fenêtre est 1M. On peut ajuster dynamiquement :
```rust
if observed_tokens > 200_000 {
    // L'utilisateur est en mode 1M beta
    context_window_size = 1_000_000;
}
```

**LOC supplémentaire** : ~10 LOC (lecture model + config + détection dynamique).

### Alternative : parser `token_budget` depuis le transcript

La doc officielle révèle que Claude Code injecte le **budget réel** au début de chaque conversation :

```xml
<budget:token_budget>200000</budget:token_budget>
```

Et après chaque tool call :

```xml
<system_warning>Token usage: 35000/200000; 165000 remaining</system_warning>
```

Si ces tags apparaissent dans le transcript JSONL, on peut :
1. Parser `token_budget` pour connaître la **taille exacte** de la fenêtre (200K, 500K, ou 1M) — plus besoin de deviner via le model ID
2. Parser `Token usage: X/Y` pour le **compte exact** des tokens — sans avoir à sommer les 4 champs

**À vérifier lors de l'implémentation** : est-ce que ces tags sont écrits dans le transcript ? Si oui, c'est la source la plus fiable et ça simplifie massivement le code. Si non, on reste sur la formule `cc + cr + inp + out`.

**Recommandation** : implémenter les deux approches (token_budget parser + formule 4 champs) avec fallback. L'une vient de l'infrastructure Claude Code (fragile si refactoré), l'autre des champs API bruts (stable).

---

## 7. Résumé des corrections apportées par rapport au hack Python

| Aspect | Hack Python (ancien) | Portage Rust (nouveau) |
|---|---|---|
| Champs comptés | `cache_read + input` (2/4) | `cc + cr + input + output` (4/4) |
| `cache_creation_input_tokens` | Omis | Inclus |
| `output_tokens` | Omis (CRITIQUE) | Inclus |
| `context_window_size` | Hardcodé 200K | Auto-détection model + config override + détection dynamique |
| Lecture fichier | `read_text()` entier | Seek from end adaptatif (32K→128K→512K) |
| Précision estimée | ~70-85% du réel | ~99% du réel |

Sources :
- [Models overview - Claude API Docs](https://platform.claude.com/docs/en/about-claude/models/overview)
- [Context windows - Claude API Docs](https://platform.claude.com/docs/en/build-with-claude/context-windows)

---

## 8. Historique des reviews

### Review pub — R1 (2026-02-22)

**Verdict** : APPROUVÉ avec 3 conditions (1 critique)

| # | Sévérité | Condition | Statut |
|---|---|---|---|
| C1 | CRITIQUE | §4.1 cassé : `workspace_path` est `String::new()` partout. Approche (b) inutilisable. Utiliser approche (a) : scan `~/.claude/projects/*/` | INTÉGRÉ — §4.1 réécrit avec `find_transcript()` scan |
| C2 | MOYENNE | Tail read 10KB insuffisant pour extended thinking (entrées 50-100KB+). Min 32KB ou adaptatif [32K, 128K, 512K] | INTÉGRÉ — §4.2 read_size 32KB + note adaptatif |
| C3 | MINEURE | Garder E1 (tool_io_bytes) comme fallback dégradé. Chaîne : transcript → tool_io → None | INTÉGRÉ — §5 chaîne de précision + `context_source` |

**Autres retours intégrés** :
- output_tokens inclusion : confirmé OK
- token_budget : formule 4 champs d'abord, token_budget en Phase 3
- LOC estimé : révisé à ~100-120 LOC

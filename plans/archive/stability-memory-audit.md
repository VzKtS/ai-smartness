# Audit — Stabilité + optimisation mémoire (crash PC)

**Mission 17** — demandée par cor, 2026-02-24
**Auditeur** : dev (researcher)

---

## 1. Problème

Le PC de l'utilisateur a planté — probablement lié au daemon + 8 MCP servers ai-smartness tournant en parallèle. Audit de stabilité, consommation mémoire, et identification des causes probables.

**Contexte process** :
- GUI : PID 2925 (Tauri)
- Daemon : PID 3783
- 8 MCP servers (cor, arc, dev, coder1, coder2, doc, pub, sub)

---

## 2. Budget mémoire système

### 2.1 Consommation par composant (idle → peak)

| Composant | Idle | Peak | Notes |
|-----------|-----:|-----:|-------|
| **Daemon** | | | |
| — ConnectionPool (50 conns × 2 MB cache) | 100 MB | 100 MB | `cache_size = -2000` par connexion |
| — EmbeddingManager (ONNX singleton) | 200 MB | 500 MB | Partagé via `OnceLock<>`, chargé au premier appel |
| — Capture queue (100 jobs × 4 workers) | 2 MB | 10 MB | Borné par `sync_channel(capacity)` |
| — Prune cycle (gossip + merge + decay) | 0 MB | 150 MB | `list_active()` charge tous les threads en RAM |
| — IPC server + threads | 5 MB | 20 MB | 1 thread par connexion, non borné |
| **8 MCP servers** | | | |
| — Par instance (3 SQLite + heartbeat + stack) | 12 MB | 22 MB | 3 conns × 2 MB cache + 2 MB stack thread |
| — Total 8 instances | 96 MB | 176 MB | |
| **GUI (Tauri)** | 200 MB | 400 MB | WebView + renderer + assets |
| **WAL files (8 agents)** | 5 MB | **768 MB** | **CRITIQUE** — voir §3.1 |
| **OS + services** | 1500 MB | 1500 MB | Linux baseline |
| **TOTAL** | **~2.1 GB** | **~3.6 GB** | Sans WAL explosion |
| **TOTAL avec WAL explosion** | | **~4.4 GB** | Scénario crash |

### 2.2 File descriptors

| Composant | FDs baseline | Notes |
|-----------|:-----:|-------|
| Daemon (IPC + pool 50 conns × 3 FDs + registry + shared) | ~160 | Pool WAL+SHM = ×3 |
| 8 MCP servers (3 conns × 3 FDs + stdio + heartbeat) | ~136 | 17 FDs/instance |
| Hook processes (5 concurrents × 14 FDs) | ~70 | Temporaires |
| GUI (Tauri) | ~20 | WebSocket + HTTP |
| **TOTAL** | **~386** | vs. `ulimit -n` = 1024 (38%) |

Pas critique au repos, mais sous charge avec hooks fréquents → peut monter à 500+ FDs.

---

## 3. Causes probables du crash (classées par probabilité)

### 3.1 CAUSE #1 — Explosion WAL files → OOM (probabilité : **60%**)

**Constat critique** : `HOOK_WAL_AUTOCHECKPOINT = 0`

| Rôle connexion | `wal_autocheckpoint` | Source |
|----------------|:--------------------:|--------|
| `ConnectionRole::Daemon` | **1000** pages | `database.rs:61` — checkpoint automatique |
| `ConnectionRole::Hook` | **0** (DÉSACTIVÉ) | `database.rs:73` — **JAMAIS de checkpoint** |
| `ConnectionRole::Mcp` | **0** (DÉSACTIVÉ) | `database.rs:73` — **JAMAIS de checkpoint** |
| `ConnectionRole::Cli` | **0** (DÉSACTIVÉ) | `database.rs:73` |

**Mécanisme** :
1. Chaque invocation hook (capture/inject) écrit dans la DB agent via WAL
2. `wal_autocheckpoint = 0` → le fichier `.db-wal` **ne checkpoint jamais** côté hook/MCP
3. Seul le daemon checkpoint (toutes les 5 min, `periodic_tasks.rs:379`)
4. Entre deux checkpoints, le WAL accumule TOUTES les écritures

**Calcul worst-case** :
- 8 agents × captures fréquentes (coding actif)
- Chaque écriture ≈ 1-5 KB dans le WAL
- 5 min entre checkpoints = 300 sec
- Si 8 captures/sec globales × 5 KB × 300 sec = **12 MB/agent/cycle**
- 8 agents × 12 MB = **96 MB par cycle de 5 min en WAL seul**
- Si le daemon est bloqué (prune cycle long, voir Cause #3) → **2-3 cycles sans checkpoint = 200-300 MB WAL**
- Scénario pathologique (checkpoint daemon bloqué 30 min) : **768 MB+ en WAL files**

**Aggravant** : Les WAL files restent en mémoire virtuelle (mmap) → pression directe sur la RAM.

### 3.2 CAUSE #2 — Prune cycle verrouille les connexions trop longtemps (probabilité : **25%**)

**Constat** : Le cycle de maintenance (prune) détient le verrou connexion pendant TOUT le cycle.

**Séquence problématique** (`periodic_tasks.rs:243-381`) :
1. `pool.get_or_open(agent_key)` → verrouille le pool
2. `conn.lock()` → verrouille la connexion agent
3. Exécute gossip (10+ sec si beaucoup de threads)
4. Exécute decay, archiver, inbox cleanup, work context cleanup
5. Exécute WAL checkpoint
6. **Durée totale verrou : 10-60 sec par agent**

**Impact cascade** :
- Pendant ce temps, les hooks et MCP servers qui accèdent à cet agent → `SQLITE_BUSY_TIMEOUT = 5000 ms` → attendent 5 sec
- Si timeout expiré → erreur silencieuse, mais le WAL continue d'accumuler
- Si plusieurs agents en prune simultanément → pool saturé → éviction forcée

**Aggravant** : `list_active()` charge TOUS les threads actifs en mémoire (`periodic_tasks.rs:385, 348`) — appelé 2× par cycle sans cache.

### 3.3 CAUSE #3 — Exhaustion file descriptors sous charge (probabilité : **10%**)

**Calcul** : 386 FDs baseline + pics de hooks concurrents → peut atteindre 500+ FDs.

**Scénario** : Coding actif dans 4+ fenêtres simultanément → hooks fréquents → chaque hook ouvre 3 connexions SQLite (×3 FDs pour WAL mode) → 12-15 FDs temporaires par hook → 5 hooks simultanés = 75 FDs supplémentaires.

Si `ulimit -n = 1024` → 500/1024 = 49% → pas critique seul. Mais combiné avec la Cause #1 (WAL mmap) → pression fichier excessive.

### 3.4 CAUSE #4 — OOM heap direct (probabilité : **5%**)

**Composants à risque** :
- `EmbeddingManager` ONNX : 200-500 MB, singleton partagé via `OnceLock`
- Prune cycle `list_active()` : O(thread_count) — 10K threads = 50 MB par appel
- Gossip merge candidates : O(n²) comparaisons potentielles (borné en pratique par overlap threshold)

**Peu probable seul** — le système a des bounds raisonnables. Mais combiné avec WAL explosion → contribue au dépassement.

---

## 4. Réponses détaillées aux questions

### Q1 — Mémoire daemon

**Allocations principales** :

| Structure | Fichier | Taille | Borne |
|-----------|---------|--------|:-----:|
| `ConnectionPool` HashMap | `connection_pool.rs:57` | 50 × 2.1 MB = 105 MB | **OUI** (`max_connections = 50`) |
| `PendingContext` par agent | `connection_pool.rs:33` | 1.5 KB (tronqué `processor.rs:183`) | **OUI** |
| `CaptureQueue` channel | `capture_queue.rs:70` | 100 jobs × ~500 B = 50 KB | **OUI** (`sync_channel(capacity)`) |
| `EmbeddingManager` ONNX | `embeddings.rs:20` | 200-500 MB | **OUI** (singleton `OnceLock`) |
| `BeatState` par agent | `beat.rs:10-60` | 1-2 KB (normal) → **100 KB+** si `scheduled_wakes` non borné | **NON** — voir §5.1 |

**`periodic_tasks` : DB cleanup uniquement, pas de cleanup mémoire in-process.** Les vecteurs de `list_active()` sont droppés en fin de scope (automatique Rust), mais le pic mémoire pendant l'appel = O(active_threads).

**SQLite connexions** : Pool avec éviction. `evict_idle()` toutes les 5 min (`periodic_tasks.rs:233`). Connexions fermées par `Drop` de rusqlite. Pas de fuite détectée.

### Q2 — Mémoire MCP servers

**Par instance MCP** :

| Ressource | Taille | Partagé ? |
|-----------|--------|:---------:|
| `McpServer` struct | ~1 KB | NON — chaque instance isolée |
| 3 SQLite connections (agent + registry + shared) | 6.3 MB | NON — page cache dédié |
| Heartbeat registry_conn | 2.1 MB | NON |
| Heartbeat thread stack | 2 MB | NON |
| Tool heap temporaire | 0.5 MB peak | NON — alloué/libéré par appel |
| **Total par instance** | **~12 MB idle** | |
| **8 instances** | **~96 MB idle** | |

**Aucune mémoire partagée entre instances MCP.** Les seuls singletons (`EmbeddingManager`, regex `LazyLock`) sont dans le daemon, pas dans les MCP servers. Chaque MCP est un process indépendant.

**Pas de leak détecté** dans `server.rs` — les handlers utilisent des références empruntées (`ToolContext<'a>`), pas de closures capturantes, pas d'accumulation.

### Q3 — Leaks potentiels

| Pattern | Statut | Détails |
|---------|:------:|---------|
| Tokio tasks / `std::thread::spawn` | **OK** | Tous les spawns sont joinés ou fire-and-forget intentionnel (quota probe). Pas d'accumulation |
| Channels `mpsc` | **OK** | Seul canal : `sync_channel(capacity)` dans capture queue — **borné**, drop si plein |
| Gossip bridges | **OK** | `dynamic_limits()` (`gossip.rs:296`) : max `target_bridge_ratio × n_threads` bridges. Orphan cleanup + decay enforced |
| Thread accumulation | **OK** | `MAX_THREADS_PER_AGENT = 10000` + suspension par decay + archival. Cognitive inbox TTL 24h |
| Strings non bornées | **OK** | Context injection limité à `MAX_CONTEXT_SIZE = 15000` chars. SQL queries bornées par filter count |
| **`scheduled_wakes`** | **⚠ RISQUE** | `beat.rs:38` — `Vec` sans capacité max. Voir §5.1 |
| **`drift_history`** | **⚠ RISQUE** | `thread.rs:193` — Vec qui enregistre les changements de poids sans max documenté |

### Q4 — Causes probables du crash

Voir §3 ci-dessus. Résumé :

| # | Cause | Probabilité | Mécanisme |
|---|-------|:-----------:|-----------|
| 1 | **WAL explosion** | **60%** | `HOOK_WAL_AUTOCHECKPOINT = 0` → WAL croît sans borne entre checkpoints daemon |
| 2 | **Prune lock contention** | **25%** | Connexion verrouillée 10-60 sec → hooks/MCP timeout → WAL non checkpointé |
| 3 | **FD exhaustion** | **10%** | 400+ FDs sous charge → seuil system limit approché |
| 4 | **OOM heap** | **5%** | ONNX + list_active() peaks + WAL mmap combinés |

### Q5 — Limites existantes dans config.rs

| Limite | Valeur | Fichier | Enforcée ? |
|--------|:------:|---------|:----------:|
| `pool_max_connections` | 50 | `connection_pool.rs:85` | **OUI** — éviction forcée |
| `capture_queue_capacity` | 100 | `capture_queue.rs:70` | **OUI** — drop si plein |
| `capture_workers` | min(cpus, 4) | `config.rs` | **OUI** — threads fixes |
| `pool_max_idle_secs` | 1800 | `connection_pool.rs` | **OUI** — éviction idle |
| `MAX_THREADS_PER_AGENT` | 10000 | `constants.rs:2` | **NON** — constante jamais vérifiée à l'insertion |
| `MAX_COGNITIVE_INBOX_PENDING` | 1000 | `constants.rs:3` | Partiel — TTL expiration mais pas de hard cap insert |
| `SQLITE_BUSY_TIMEOUT_MS` | 5000 | `constants.rs` | **OUI** — par SQLite |
| `HOOK_WAL_AUTOCHECKPOINT` | **0** | `constants.rs` | **OUI** — et c'est le problème |
| `DAEMON_WAL_AUTOCHECKPOINT` | 1000 | `constants.rs` | **OUI** — checkpoint automatique |
| `target_bridge_ratio` | 3.0 | `config.rs:547` | **OUI** — `dynamic_limits()` |
| `max_bridges_per_thread` | 10 | `config.rs:549` | **OUI** — `gossip.rs:88-92` |
| `min_bridges_per_thread` | 3 | `config.rs:548` | **OUI** |

---

## 5. Risques secondaires identifiés

### 5.1 `scheduled_wakes` non borné (`beat.rs:38`)

```rust
pub scheduled_wakes: Vec<ScheduledWake>,  // Pas de capacité max
```

`schedule_wake()` (`beat.rs:280`) fait `push()` sans vérification de taille. Si un agent schedule des wakes fréquents sans les drain, le vecteur croît indéfiniment. Chaque `ScheduledWake` ≈ 200 bytes. Impact modéré (100 KB pour 480 wakes), mais sérialisé/désérialisé en JSON à chaque cycle (5 min).

### 5.2 `list_active()` appelé sans cache dans le prune cycle

`periodic_tasks.rs:385` (`cleanup_stale_work_contexts`) et `periodic_tasks.rs:402` (`decay_injection_scores`) appellent chacun `ThreadStorage::list_active()` — 2 chargements complets de tous les threads actifs par cycle, sans cache entre les deux. (Correction pub : ligne 348 est `concept_backfill`, appelé seulement quand `beat % 288 == 0` ~1×/24h, donc pas systématique.)

### 5.3 Pins context charge tous les threads pour en filtrer quelques-uns

`inject.rs:494` : `ThreadStorage::list_active(conn)` charge TOUS les threads actifs pour trouver les pins (`tag == "__pin__"`). Devrait être un filtre SQL.

### 5.4 `Vec::remove(0)` pour les ring buffers (`session.rs:130-132, 147-149` + `user_profile.rs:104`)

```rust
if self.tool_history.len() > MAX_TOOL_HISTORY {
    self.tool_history.remove(0);  // O(N) — devrait être VecDeque
}
```

### 5.5 Pas de `catch_unwind` dans les handlers MCP

Un panic dans un tool handler tue l'intégralité du MCP server. Pas de recovery — le process meurt. Isolé par agent (un seul MCP affecté), mais perte de session.

---

## 6. Top 5 causes probables + correctifs recommandés

### Priorité CRITIQUE

#### C1 — HOOK_WAL_AUTOCHECKPOINT = 0 → WAL explosion

**Diagnostic** : Les hooks et MCP n'ont AUCUN checkpoint WAL automatique. Seul le daemon checkpoint toutes les 5 min. Si le daemon est occupé (prune long) ou bloqué, les WAL files croissent sans limite.

**Correctif** : Changer `HOOK_WAL_AUTOCHECKPOINT` de 0 à 100.

```rust
// constants.rs — AVANT :
pub const HOOK_WAL_AUTOCHECKPOINT: u32 = 0;

// APRÈS :
pub const HOOK_WAL_AUTOCHECKPOINT: u32 = 100;  // checkpoint toutes les 100 pages (~400 KB)
```

**Impact** : ~1 LOC. Élimine le vecteur d'explosion WAL. Léger coût I/O par hook (checkpoint = ~1-5ms).

**Note (O2 pub)** : Le daemon utilise `PRAGMA wal_checkpoint(PASSIVE)` (`periodic_tasks.rs:379`), qui ne checkpoint PAS si des lecteurs sont actifs. Sous charge MCP/hook (lecteurs fréquents), le checkpoint peut être systématiquement différé, aggravant C1. Considérer `TRUNCATE` pour le cycle prune (`daemon/mod.rs:217` l'utilise déjà au shutdown).

**LOC** : 1

#### C2 — Prune cycle détient le verrou trop longtemps

**Diagnostic** : La connexion agent est verrouillée pendant TOUT le cycle prune (gossip + decay + archive + cleanup + checkpoint). Durée : 10-60 sec. Pendant ce temps, hooks et MCP sont bloqués.

**Correctif** : Libérer le verrou entre chaque tâche du cycle. Pattern drop-and-reacquire.

**Note (O1 pub)** : `run_prune_cycle` prend `&Connection` (pas un Mutex). Le refactoring doit être fait dans `run_prune_loop`, en éclatant l'appel monolithique à `run_prune_cycle` en appels individuels avec lock/unlock entre chaque tâche.

```rust
// periodic_tasks.rs — run_prune_loop — Pattern recommandé :
// Au lieu de :
let conn_guard = conn.lock().unwrap();
run_prune_cycle(&conn_guard, ...);  // monolithique, 10 tâches sous même verrou

// Éclater en appels individuels :
{ let g = conn.lock().unwrap(); gossip(&g, ...); }   // drop guard
{ let g = conn.lock().unwrap(); decay(&g, ...); }    // reacquire
{ let g = conn.lock().unwrap(); archive(&g, ...); }  // reacquire
// ... etc pour chaque tâche
```

**Impact** : ~25-30 LOC refactoring (O1 pub : plus que ~15, car nécessite restructuration de `run_prune_loop`). Réduit le temps de verrouillage continu de 60 sec à ~5 sec par tâche.

**LOC** : ~25-30

### Priorité HAUTE

#### C3 — SQLITE_BUSY_TIMEOUT trop long (5 sec)

**Diagnostic** : `SQLITE_BUSY_TIMEOUT_MS = 5000` fait attendre 5 secondes sur un verrou SQLite avant timeout. Sous contention (8 agents + hooks + MCP), les threads s'empilent et bloquent.

**Correctif** : Réduire à 1000 ms.

```rust
// constants.rs — AVANT :
pub const SQLITE_BUSY_TIMEOUT_MS: u32 = 5_000;

// APRÈS :
pub const SQLITE_BUSY_TIMEOUT_MS: u32 = 1_000;
```

**Impact** : 1 LOC. Les hooks échouent plus vite mais ne bloquent plus le système.

**LOC** : 1

#### C4 — `scheduled_wakes` non borné dans BeatState

**Diagnostic** : `Vec<ScheduledWake>` croît sans limite. Sérialisé en JSON à chaque cycle.

**Correctif** : Ajouter un cap à `schedule_wake()`.

```rust
// beat.rs — Dans schedule_wake() :
const MAX_SCHEDULED_WAKES: usize = 200;
if self.scheduled_wakes.len() >= MAX_SCHEDULED_WAKES {
    // Supprimer le plus ancien
    self.scheduled_wakes.remove(0);
}
self.scheduled_wakes.push(ScheduledWake { ... });
```

**LOC** : ~5

### Priorité MOYENNE

#### C5 — `list_active()` sans cache dans le prune cycle

**Diagnostic** : 2 appels `list_active()` par cycle par agent, chargent tous les threads actifs en RAM. Avec 500 threads actifs = 2× 250 KB = 500 KB par agent par cycle.

**Correctif** : Cacher le résultat dans une variable locale, réutiliser pour les deux appels.

```rust
// periodic_tasks.rs — Pattern :
let active_threads = ThreadStorage::list_active(conn)?;
cleanup_stale_work_contexts(conn, &active_threads)?;
decay_injection_scores(conn, &active_threads)?;
```

**LOC** : ~10

---

## 7. Fichiers modifiés

| Fichier | Correctif | LOC | Priorité |
|---------|-----------|:---:|:--------:|
| `src/constants.rs` | C1: HOOK_WAL_AUTOCHECKPOINT = 100 | 1 | CRITIQUE |
| `src/constants.rs` | C3: SQLITE_BUSY_TIMEOUT_MS = 1000 | 1 | HAUTE |
| `src/daemon/periodic_tasks.rs` | C2: éclater run_prune_cycle, drop-and-reacquire locks | ~25-30 (O1) | CRITIQUE |
| `src/storage/beat.rs` | C4: cap scheduled_wakes à 200 | ~5 | HAUTE |
| `src/daemon/periodic_tasks.rs` | C5: cache list_active() | ~10 | MOYENNE |
| **Total** | | **~42-47** | |

### Fichiers NON modifiés
- `connection_pool.rs` — éviction correcte, pool borné à 50
- `capture_queue.rs` — borné, drop si plein, pas de leak
- `server.rs` (MCP) — pas de leak détecté, handlers empruntés
- `gossip.rs` — bridges bornés par `dynamic_limits()`, orphan cleanup OK
- `threads.rs` — suspension + archival + TTL OK

---

## 8. Correctifs optionnels (P3)

| Correctif | Fichier | LOC | Bénéfice |
|-----------|---------|:---:|----------|
| Pins SQL filter au lieu de `list_active()` | `inject.rs:494` | ~5 | Évite chargement tous threads pour trouver ~5 pins |
| `VecDeque` pour ring buffers session | `session.rs:130` | ~5 | O(1) au lieu de O(N) pour `remove(0)` |
| `catch_unwind` dans tool dispatch MCP | `server.rs:171` | ~10 | Récupération panic sans tuer le MCP server |
| Cap `drift_history` à 50 entrées | `thread.rs` | ~5 | Empêche croissance indéfinie |
| Monitoring FDs au startup daemon | `daemon/mod.rs` | ~10 | Warning si `ulimit -n < 2048` |

---

## 9. Limitations de l'audit

| # | Limitation | Impact |
|---|-----------|--------|
| L1 | Pas de profiling runtime (valgrind/massif) — audit statique uniquement | Les chiffres mémoire sont des estimations basées sur le code |
| L2 | Pas d'accès aux logs du crash | Impossible de confirmer la cause exacte (OOM killer, kernel panic, etc.) |
| L3 | Consommation ONNX estimée (200-500 MB) — dépend du modèle chargé | Si TF-IDF fallback utilisé → 0 MB ONNX |
| L4 | WAL mmap overhead estimé — varie selon SQLite version et OS | Le mmap réel peut être supérieur aux chiffres WAL bruts |

---

## 10. Historique reviews

### Review pub — 2026-02-24

**VERDICT : APPROVED** — 1 correction mineure, 2 observations

**Vérifications indépendantes** : C1 WAL=0 ✅, C2 lock contention ✅, C3 busy_timeout ✅, C4 scheduled_wakes ✅, C5 list_active ✅, §5.3 pins ✅, §5.4 Vec::remove(0) ✅, §5.5 catch_unwind ✅, drift_history ✅

**Correction** : C5 références — les 2 appels systématiques sont lignes 385+402 (pas 385+348). Ligne 348 est rare (beat%288).

**O1** (MOYENNE) : C2 LOC sous-estimé — `run_prune_cycle` prend `&Connection`, refactoring dans `run_prune_loop` nécessaire. ~25-30 LOC réaliste.

**O2** (BASSE) : WAL checkpoint PASSIVE (periodic_tasks:379) peut être différé sous charge lecteurs. Considérer TRUNCATE pour le cycle prune.

**Intégration** : Correction C5 intégrée dans §5.2. O1 intégrée dans C2 (LOC corrigé ~25-30, note refactoring run_prune_loop) + §7 table LOC (total ~42-47). O2 intégrée comme note dans C1. §5.4 étendu avec user_profile.rs:104 (vérifié par pub).

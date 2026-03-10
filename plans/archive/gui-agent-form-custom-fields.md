# Plan : GUI agent form — custom_role + report_to

## Diagnostic

Les champs `custom_role` et `report_to` existent **déjà** dans toute la chaîne backend :
- `Agent` struct (`agent.rs:174-175`) : `report_to: Option<String>`, `custom_role: Option<String>`
- `AgentUpdate` struct (`registry.rs:759-760`) : idem
- DB migrations V4/V5 (`migrations.rs:407-421`) : colonnes SQLite créées et normalisées
- MCP tools (`agents.rs:33-34, 95-96, 141-142`) : lecture/écriture fonctionnelle
- Hook inject (`inject.rs:553-556`) : injection dans le contexte agent
- Registry cascade (`registry.rs:178-180, 277-279`) : `report_to` mis à jour lors de delete/rename

**Le problème** : les 3 commandes GUI (`list_agents`, `add_agent`, `update_agent`) ignorent ces champs, et le frontend HTML/JS n'a pas les inputs correspondants.

**Sémantique des champs** :
- `supervisor_id` = parent hiérarchique (qui contrôle les tâches de l'agent)
- `report_to` = destinataire des livrables (à qui l'agent envoie son travail terminé). Exemple : dev a supervisor=cor mais report_to=pub (reviewer)
- `custom_role` = description libre du rôle au-delà de l'enum fixe (programmer/coordinator/reviewer/researcher/architect). Exemple : "auditeur/chercheur sous autorité cor" ou "reviewer qualité frontend"

---

## Contrainte architecturale : CLI-first

**Directive cor** : Le projet doit rester entièrement opérationnel en CLI (sans GUI). La GUI est un frontend optionnel aux mêmes fonctions MCP.

**Vérification** :
- **MCP `agent_configure`** (`agents.rs:124-148`) : supporte DÉJÀ `report_to` (L141) et `custom_role` (L142) via `optional_str(params, ...)`. Utilise `AgentRegistry::update()` — la même fonction que le GUI `update_agent`.
- **MCP `agent_list`** (`agents.rs:20-40`) : sérialise DÉJÀ `report_to` (L33) et `custom_role` (L34) dans la réponse JSON.
- **MCP `agent_status`** (`agents.rs:82-96`) : sérialise DÉJÀ les deux champs (L95-96).

**Conclusion** : Les capacités MCP sont complètes. Le plan actuel ne fait qu'exposer dans la GUI des champs déjà fonctionnels en CLI. Zéro logique métier ajoutée dans `commands.rs`. Les deux paths (MCP et GUI) convergent vers `AgentRegistry::update()`.

**Observation (hors scope)** : Il existe des divergences préexistantes entre GUI et MCP (organic rename en `add_agent`, hook installation, supervisor hierarchy validation) — ces logiques existent uniquement côté GUI. Ce n'est PAS introduit par ce plan et constitue une dette technique séparée.

---

## Modifications requises

### M1 — `list_agents` : sérialiser les nouveaux champs
**Fichier** : `src/gui/commands.rs`
**Ligne** : 829-843

**AVANT** :
```rust
    let result: Vec<serde_json::Value> = agents.iter().map(|a| {
        serde_json::json!({
            "id": a.id,
            "name": a.name,
            "role": a.role,
            "status": a.status.as_str(),
            "supervisor_id": a.supervisor_id,
            "coordination_mode": a.coordination_mode.as_str(),
            "team": a.team,
            "capabilities": a.capabilities,
            "specializations": a.specializations,
            "thread_mode": a.thread_mode.as_str(),
            "thread_quota": a.thread_mode.quota(),
            "last_seen": a.last_seen.to_rfc3339(),
            "registered_at": a.registered_at.to_rfc3339(),
        })
    }).collect();
```

**APRÈS** :
```rust
    let result: Vec<serde_json::Value> = agents.iter().map(|a| {
        serde_json::json!({
            "id": a.id,
            "name": a.name,
            "role": a.role,
            "status": a.status.as_str(),
            "supervisor_id": a.supervisor_id,
            "coordination_mode": a.coordination_mode.as_str(),
            "team": a.team,
            "capabilities": a.capabilities,
            "specializations": a.specializations,
            "thread_mode": a.thread_mode.as_str(),
            "thread_quota": a.thread_mode.quota(),
            "last_seen": a.last_seen.to_rfc3339(),
            "registered_at": a.registered_at.to_rfc3339(),
            "report_to": a.report_to,
            "custom_role": a.custom_role,
        })
    }).collect();
```

**Impact** : +2 lignes.

---

### M2 — `add_agent` : accepter les nouveaux paramètres
**Fichier** : `src/gui/commands.rs`

#### M2a — Signature (lignes 850-858)
**AVANT** :
```rust
pub fn add_agent(
    project_hash: String,
    agent_id: String,
    name: String,
    role: String,
    supervisor_id: Option<String>,
    team: Option<String>,
    is_supervisor: Option<bool>,
    thread_mode: Option<String>,
) -> Result<serde_json::Value, String> {
```

**APRÈS** :
```rust
pub fn add_agent(
    project_hash: String,
    agent_id: String,
    name: String,
    role: String,
    supervisor_id: Option<String>,
    team: Option<String>,
    is_supervisor: Option<bool>,
    thread_mode: Option<String>,
    report_to: Option<String>,
    custom_role: Option<String>,
) -> Result<serde_json::Value, String> {
```

#### M2b — AgentUpdate pour organic rename (lignes 896-908)
**AVANT** :
```rust
        let update = AgentUpdate {
            name: Some(name),
            role: Some(role),
            description: None,
            supervisor_id: supervisor_id.map(Some),
            coordination_mode: Some(mode.as_str().to_string()),
            team: team.map(Some),
            specializations: None,
            capabilities: None,
            thread_mode: thread_mode.clone(),
            report_to: None,
            custom_role: None,
            workspace_path: None,
        };
```

**APRÈS** :
```rust
        let update = AgentUpdate {
            name: Some(name),
            role: Some(role),
            description: None,
            supervisor_id: supervisor_id.map(Some),
            coordination_mode: Some(mode.as_str().to_string()),
            team: team.map(Some),
            specializations: None,
            capabilities: None,
            thread_mode: thread_mode.clone(),
            report_to: report_to.clone(),
            custom_role: custom_role.clone(),
            workspace_path: None,
        };
```

#### M2c — Agent struct construction (lignes 940-962)
**AVANT** :
```rust
    let agent = Agent {
        // ... (lignes 940-958 inchangées)
        report_to: None,
        custom_role: None,
        workspace_path: String::new(),
    };
```

**APRÈS** :
```rust
    let agent = Agent {
        // ... (lignes 940-958 inchangées)
        report_to,
        custom_role,
        workspace_path: String::new(),
    };
```

---

### M3 — `update_agent` : accepter les nouveaux paramètres
**Fichier** : `src/gui/commands.rs`

#### M3a — Signature (lignes 1002-1013)
**AVANT** :
```rust
pub fn update_agent(
    project_hash: String,
    agent_id: String,
    name: Option<String>,
    role: Option<String>,
    description: Option<String>,
    supervisor_id: Option<String>,
    team: Option<String>,
    is_supervisor: Option<bool>,
    capabilities: Option<Vec<String>>,
    specializations: Option<Vec<String>>,
    thread_mode: Option<String>,
) -> Result<serde_json::Value, String> {
```

**APRÈS** :
```rust
pub fn update_agent(
    project_hash: String,
    agent_id: String,
    name: Option<String>,
    role: Option<String>,
    description: Option<String>,
    supervisor_id: Option<String>,
    team: Option<String>,
    is_supervisor: Option<bool>,
    capabilities: Option<Vec<String>>,
    specializations: Option<Vec<String>>,
    thread_mode: Option<String>,
    report_to: Option<String>,
    custom_role: Option<String>,
) -> Result<serde_json::Value, String> {
```

#### M3b — AgentUpdate construction (lignes 1045-1058)
**AVANT** :
```rust
    let update = AgentUpdate {
        name,
        role,
        description,
        supervisor_id: supervisor_update,
        coordination_mode,
        team: team_update,
        specializations,
        capabilities,
        thread_mode: thread_mode.clone(),
        report_to: None,
        custom_role: None,
        workspace_path: None,
    };
```

**APRÈS** :
```rust
    let update = AgentUpdate {
        name,
        role,
        description,
        supervisor_id: supervisor_update,
        coordination_mode,
        team: team_update,
        specializations,
        capabilities,
        thread_mode: thread_mode.clone(),
        report_to,
        custom_role,
        workspace_path: None,
    };
```

**Simplification (pub review)** : Puisque le frontend envoie TOUJOURS la valeur (jamais `null`), on reçoit `Some("pub")` ou `Some("")` côté Rust. On passe directement à AgentUpdate. Le registry écrit tel quel dans le SQL UPDATE (`registry.rs:486-492`). La migration V5 normalise les empty strings en NULL à la prochaine lecture.

Pas besoin du double wrapping `Option<Option<String>>` — le champ est toujours présent dans le formulaire d'édition.

---

### M4 — HTML : Add Agent Modal — nouveaux champs
**Fichier** : `src/gui/frontend/index.html`
**Insérer après** : ligne 989 (après le bloc `modal-field-inline` du checkbox "Is Supervisor", avant `add-agent-error`)

**NOUVEAU HTML** :
```html
        <div class="modal-field">
            <label for="new-agent-custom-role" title="Free-text role description beyond the fixed role list. Injected into the agent's system prompt for behavioral context. Examples: 'quality reviewer for frontend', 'auditor under cor authority'.">Custom Role (optional)</label>
            <input type="text" id="new-agent-custom-role" placeholder="e.g. auditor/researcher under cor authority">
        </div>
        <div class="modal-field">
            <label for="new-agent-report-to" title="Agent to whom this agent sends completed work. Different from Supervisor (who assigns tasks). Example: dev has supervisor=cor but report_to=pub (reviewer).">Report To (optional)</label>
            <select id="new-agent-report-to">
                <option value="">— None —</option>
            </select>
        </div>
```

**Note** : Le `<select>` pour `report_to` sera peuplé dynamiquement (même pattern que le `<select>` supervisor).

---

### M5 — JS : `addAgent()` — lire et passer les nouveaux champs
**Fichier** : `src/gui/frontend/app.js`

#### M5a — Ouverture du modal (lignes 430-452)
Ajouter après la ligne qui reset `new-agent-team` (L436) et avant le peuplement du supervisor dropdown :

**INSÉRER** (après L436) :
```javascript
    document.getElementById('new-agent-custom-role').value = '';
```

**INSÉRER** dans la boucle agents (après L448, le `supSelect.appendChild`) :
```javascript
    // Also populate report_to dropdown
    const rtSelect = document.getElementById('new-agent-report-to');
    rtSelect.innerHTML = '<option value="">— None —</option>';
    if (projectHash) {
        try {
            const agents = await invoke('list_agents', { projectHash });
            for (const a of agents) {
                const opt2 = document.createElement('option');
                opt2.value = a.id;
                opt2.textContent = `${a.name} (${a.role})`;
                rtSelect.appendChild(opt2);
            }
        } catch (_) {}
    }
```

**Optimisation** : Réutiliser le même résultat `agents` pour peupler les deux dropdowns au lieu de faire 2 appels `list_agents`. Refactorer la boucle :

**PATTERN RECOMMANDÉ** (remplacement complet de la section L440-451) :
```javascript
    const supSelect = document.getElementById('new-agent-supervisor');
    supSelect.innerHTML = '<option value="">— None —</option>';
    const rtSelect = document.getElementById('new-agent-report-to');
    rtSelect.innerHTML = '<option value="">— None —</option>';
    if (projectHash) {
        try {
            const agents = await invoke('list_agents', { projectHash });
            for (const a of agents) {
                const supOpt = document.createElement('option');
                supOpt.value = a.id;
                supOpt.textContent = `${a.name} (${a.role})`;
                supSelect.appendChild(supOpt);

                const rtOpt = document.createElement('option');
                rtOpt.value = a.id;
                rtOpt.textContent = `${a.name} (${a.role})`;
                rtSelect.appendChild(rtOpt);
            }
        } catch (_) {}
    }
```

#### M5b — `addAgent()` function (lignes 480-511)
**AVANT** (L484-487) :
```javascript
    const supVal = document.getElementById('new-agent-supervisor').value || null;
    const teamVal = document.getElementById('new-agent-team').value.trim() || null;
    const isSup = document.getElementById('new-agent-supervisor-flag').checked;
    const threadModeVal = document.getElementById('new-agent-thread-mode').value;
```

**APRÈS** :
```javascript
    const supVal = document.getElementById('new-agent-supervisor').value || null;
    const teamVal = document.getElementById('new-agent-team').value.trim() || null;
    const isSup = document.getElementById('new-agent-supervisor-flag').checked;
    const threadModeVal = document.getElementById('new-agent-thread-mode').value;
    const customRoleVal = document.getElementById('new-agent-custom-role').value.trim() || null;
    const reportToVal = document.getElementById('new-agent-report-to').value || null;
```

**AVANT** (L495-503, invoke call) :
```javascript
        await invoke('add_agent', {
            projectHash,
            agentId: agentIdVal,
            name: nameVal,
            role: roleVal,
            supervisorId: supVal,
            team: teamVal,
            isSupervisor: isSup,
            threadMode: threadModeVal,
        });
```

**APRÈS** :
```javascript
        await invoke('add_agent', {
            projectHash,
            agentId: agentIdVal,
            name: nameVal,
            role: roleVal,
            supervisorId: supVal,
            team: teamVal,
            isSupervisor: isSup,
            threadMode: threadModeVal,
            reportTo: reportToVal,
            customRole: customRoleVal,
        });
```

---

### M6 — JS : `toggleAgentEditRow()` — ajouter les champs dans le formulaire d'édition
**Fichier** : `src/gui/frontend/app.js`

#### M6a — Build report_to options (après la construction de `supOptions`, ~L978)
**INSÉRER** après la ligne `let supOptions = ...` :
```javascript
    // Build report_to options (exclude self)
    let rtOptions = '<option value="">— None —</option>';
    for (const a of agentsCache) {
        if (a.id === agent.id) continue;
        const sel = (a.id === agent.report_to) ? 'selected' : '';
        rtOptions += `<option value="${esc(a.id)}" ${sel}>${esc(a.name)} (${esc(a.role)})</option>`;
    }
```

#### M6b — Ajouter les champs dans le HTML du formulaire
Dans `editTr.innerHTML` (à l'intérieur du `<div class="form-grid">`), **INSÉRER** après le champ Description (~après la ligne `<label>Description`) :

```html
                <label>Custom Role
                    <input type="text" class="ae-custom-role" value="${esc(agent.custom_role || '')}" placeholder="e.g. auditor/researcher">
                </label>
                <label>Report To
                    <select class="ae-report-to">${rtOptions}</select>
                </label>
```

---

### M7 — JS : `saveAgentEdit()` — lire et passer les nouveaux champs
**Fichier** : `src/gui/frontend/app.js`

#### M7a — Lecture des valeurs (après L1060, dans la section de lecture)

**ATTENTION (bugfix pub)** : Ne PAS utiliser `|| null` pour `reportTo` et `customRole`. Avec `|| null`, une sélection « — None — » (value="") est convertie en `null` côté JS → Tauri reçoit `None` côté Rust → AgentUpdate traite `None` comme "don't change" et conserve l'ancienne valeur. L'utilisateur ne peut donc jamais EFFACER ces champs.

**Fix** : Toujours envoyer la string brute. Le backend traite `Some("")` comme "clear to empty/NULL".

**INSÉRER** après `const threadMode = ...` :
```javascript
    const customRole = editTr.querySelector('.ae-custom-role')?.value?.trim() ?? '';
    const reportTo = editTr.querySelector('.ae-report-to')?.value ?? '';
```

#### M7b — invoke call (L1066-1078)
**AVANT** :
```javascript
        const result = await invoke('update_agent', {
            projectHash,
            agentId: original.id,
            name,
            role,
            description,
            supervisorId: supervisorId,
            team: team,
            isSupervisor,
            capabilities,
            specializations,
            threadMode,
        });
```

**APRÈS** :
```javascript
        const result = await invoke('update_agent', {
            projectHash,
            agentId: original.id,
            name,
            role,
            description,
            supervisorId: supervisorId,
            team: team,
            isSupervisor,
            capabilities,
            specializations,
            threadMode,
            reportTo,
            customRole,
        });
```

---

### M8 — JS : `renderAgents()` — afficher les nouveaux champs dans le tableau (optionnel)
**Fichier** : `src/gui/frontend/app.js`
**Fichier** : `src/gui/frontend/index.html`

**DÉCISION** : Ne PAS ajouter de colonnes au tableau principal. Le tableau a déjà 10 colonnes, ajouter 2 de plus causerait un overflow horizontal. Les champs `custom_role` et `report_to` sont visibles dans le formulaire d'édition (toggle ▶) et dans le panneau de détail. C'est suffisant.

**Alternative si cor le souhaite** : afficher `report_to` à côté du `supervisor_id` dans la même cellule, séparés par un `/` ou avec un label distinct.

---

## Résumé des modifications

| # | Fichier | Lignes | Nature | LOC delta |
|---|---------|--------|--------|-----------|
| M1 | commands.rs | 829-843 | Sérialisation list_agents | +2 |
| M2a | commands.rs | 850-858 | Signature add_agent | +2 |
| M2b | commands.rs | 896-908 | AgentUpdate organic rename | ~0 (remplacement None→clone) |
| M2c | commands.rs | 959-960 | Agent struct construction | ~0 (remplacement None→field) |
| M3a | commands.rs | 1002-1013 | Signature update_agent | +2 |
| M3b | commands.rs | 1045-1058 | AgentUpdate construction | +8 (wrapping + unwrap) |
| M4 | index.html | après L989 | Nouveaux champs HTML modal | +8 |
| M5a | app.js | 430-452 | Reset + populate dropdowns | +10 |
| M5b | app.js | 480-511 | addAgent() read + invoke | +4 |
| M6a | app.js | ~978 | Build report_to options | +6 |
| M6b | app.js | ~1000 | Edit form HTML champs | +6 |
| M7a | app.js | ~1060 | saveAgentEdit() read | +2 |
| M7b | app.js | 1066-1078 | saveAgentEdit() invoke | +2 |
| | **Total** | | | **~52 LOC** |

---

## Points d'attention

1. **Camel case Tauri** : Tauri convertit automatiquement les snake_case Rust en camelCase JS. Donc `report_to` Rust → `reportTo` JS, `custom_role` → `customRole`. C'est le pattern déjà utilisé pour `supervisorId`, `threadMode`, etc.

2. **Wrapping Option dans update_agent** : Le pattern existant pour `supervisor_id` (L1036-1038) et `team` (L1041-1043) utilise `.map()` pour convertir empty string → None. Appliquer le même pattern pour `report_to` et `custom_role`.

3. **Cascade delete** : Déjà implémenté dans `registry.rs:178-180` — quand un agent est supprimé, les `report_to` pointant vers lui sont mis à NULL. Rien à ajouter.

4. **Cascade rename** : Déjà implémenté dans `registry.rs:277-279` — quand un agent est renommé, les `report_to` pointant vers l'ancien ID sont mis à jour. Rien à ajouter.

5. **Validation** : Pas de validation nécessaire pour `report_to` (contrairement à `supervisor_id` qui a `validate_hierarchy`). Un agent peut reporter à n'importe quel autre agent, y compris son supervisor.

6. **i18n** : Le projet utilise activement l'i18n (142 occurrences de `data-i18n`). Les labels « Custom Role » et « Report To » doivent être ajoutés aux clés de traduction existantes. Suivre le pattern `data-i18n="modal.xxx"` utilisé par les autres champs du modal.

---

## Historique des reviews

### Review pub #1 — APPROUVÉ avec 1 bug + 2 notes (intégrés)

1. **BUG M7a — impossible de CLEAR report_to** : `|| null` convertit empty string en `null` → Tauri reçoit `None` → AgentUpdate traite comme "don't change". Fix : utiliser `?? ''` et toujours envoyer la string. Le backend traite `Some("")` comme "clear".
2. **M3b simplifié** : pas besoin du double wrapping `Option<Option<String>>`. Passer `report_to` et `custom_role` directement à AgentUpdate (le champ est toujours présent dans le form).
3. **i18n confirmé pertinent** : 142 occurrences `data-i18n` dans le projet — les labels doivent être traduits.

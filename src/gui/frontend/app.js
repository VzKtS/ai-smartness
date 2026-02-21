// AI Smartness Dashboard — Tauri frontend
// Calls Rust backend via window.__TAURI__.core.invoke()

const { invoke } = window.__TAURI__.core;

// ─── i18n Translations ──────────────────────────────────────
const T = {
en: {
    'tab.dashboard':'Dashboard','tab.threads':'Threads','tab.agents':'Agents','tab.graph':'Graph','tab.settings':'Settings',
    'stab.general':'General','stab.guardian':'Guardian LLM','stab.matching':'Thread Matching',
    'stab.gossip':'Gossip','stab.engram':'Engram','stab.decay':'Decay & Lifecycle','stab.alerts':'Alerts',
    'stab.fallback':'Fallback','stab.guardcode':'GuardCode','stab.network':'Network','stab.updates':'Updates',
    'dash.daemon':'Daemon','dash.active':'Active Threads','dash.suspended':'Suspended',
    'dash.archived':'Archived','dash.bridges':'Bridges','dash.version':'Version',
    'dash.cpu':'CPU','dash.memory':'Memory','dash.pool':'Pool',
    'stab.daemon':'Daemon','sec.daemonpool':'Connection Pool',
    'daemon.autostart':'Auto-start Daemon','daemon.maxconn':'Max Connections',
    'daemon.idletimeout':'Idle Timeout (secs)','daemon.pruneinterval':'Prune Interval (secs)',
    'daemon.crossgossip':'Cross-project Gossip',
    'cfg.mode':'Mode','cfg.theme':'Theme',
    'btn.start':'Start','btn.stop':'Stop','btn.debug':'Debug','btn.search':'Search','btn.save':'Save',
    'btn.reset':'Reset Defaults','btn.add':'Add','btn.cancel':'Cancel','btn.remove':'Remove',
    'btn.addagent':'+ Add Agent',
    'sec.global':'Global Settings','sec.extraction':'Extraction','sec.coherence':'Coherence',
    'sec.reactivation':'Reactivation','sec.synthesis':'Synthesis','sec.labels':'Label Suggestion',
    'sec.importance':'Importance Rating','sec.langdisplay':'Language & Display',
    'sec.validatorweights':'Validator Weights (0.0 = disabled, 1.0 = full)',
    'th.title':'Title','th.status':'Status','th.weight':'Weight','th.importance':'Importance',
    'th.topics':'Topics','th.activations':'Activations','th.id':'ID','th.name':'Name',
    'th.role':'Role','th.supervisor':'Supervisor','th.team':'Team','th.mode':'Mode',
    'status.checking':'checking...','status.stopped':'Stopped',
    'settings.title':'Settings','settings.saved':'Settings saved successfully','settings.failed':'Save failed',
    'ph.search':'Search threads...','ph.theme':'Theme and display options will be available in a future release.',
    'ph.network.title':'Agent Network','ph.network':'Multi-agent communication and remote team collaboration settings.',
    'ph.network2':'Configure agent discovery, message routing, and shared memory synchronization.',
    'ph.updates.title':'Updates','ph.updates':'Auto-update preferences and version management.',
    'ph.comingsoon':'Coming soon',
    'modal.addproject':'Add Project','modal.projectpath':'Project Path','modal.projectname':'Name (optional)',
    'modal.addagent':'Add Agent','modal.agentid':'Agent ID','modal.agentname':'Name',
    'modal.agentrole':'Role','modal.agentsupervisor':'Supervisor (optional)','modal.agentteam':'Team (optional)',
    'modal.issupervisor':'Is Supervisor',
    'agents.list':'Registered Agents','agents.hierarchy':'Hierarchy',
    'agents.noagents':'No agents registered','agents.nohierarchy':'No hierarchy data',
    'dash.projects':'Projects','dash.roletree':'Team Role Tree',
    'btn.addproject':'+ Add Project','btn.edit':'Edit','btn.delete':'Delete',
    'th.path':'Path','th.provider':'Provider','th.hash':'Hash',
    'modal.editproject':'Edit Project','modal.provider':'Provider',
    'project.noprojects':'No projects registered','project.confirmdelete':'Delete project',
    'cfg.enabled':'Guardian Enabled','cfg.clipath':'Claude CLI Path','cfg.hookguard':'Hook Guard Env',
    'cfg.cache':'Cache Enabled','cfg.cachettl':'Cache TTL (secs)','cfg.cachemax':'Cache Max Entries',
    'cfg.pattern':'Pattern Learning','cfg.patterndecay':'Pattern Decay (days)',
    'cfg.usage':'Usage Tracking','cfg.fallback':'Fallback on Failure',
    'cfg.model':'Model','cfg.timeout':'Timeout (s)','cfg.retries':'Max Retries',
    'cfg.taskEnabled':'Enabled','cfg.failmode':'Failure Mode','cfg.language':'Language',
    'gc.enabled':'Enabled','gc.maxbytes':'Max Content Bytes','gc.warnonblock':'Warn on Block',
    'gc.action':'Action on Block','gc.blockedpatterns':'Blocked Patterns','gc.addpattern':'+ Add Pattern',
    'gc.sanitize':'Sanitize LLM','gc.sanitizeretries':'Sanitize Loop Max',
},
fr: {
    'tab.dashboard':'Tableau de bord','tab.threads':'Threads','tab.agents':'Agents','tab.graph':'Graphe','tab.settings':'Parametres',
    'stab.general':'General','stab.guardian':'Guardian LLM','stab.matching':'Correspondance',
    'stab.gossip':'Gossip','stab.engram':'Engram','stab.decay':'Decroissance & Cycle de vie','stab.alerts':'Alertes',
    'stab.fallback':'Repli','stab.guardcode':'GuardCode','stab.network':'Reseau','stab.updates':'Mises a jour',
    'dash.daemon':'Daemon','dash.active':'Threads actifs','dash.suspended':'Suspendus',
    'dash.archived':'Archives','dash.bridges':'Ponts','dash.version':'Version',
    'dash.cpu':'CPU','dash.memory':'Memoire','dash.pool':'Pool',
    'stab.daemon':'Daemon','sec.daemonpool':'Pool de connexions',
    'daemon.autostart':'Demarrage auto du daemon','daemon.maxconn':'Connexions max',
    'daemon.idletimeout':'Timeout inactivite (sec)','daemon.pruneinterval':'Intervalle nettoyage (sec)',
    'daemon.crossgossip':'Gossip inter-projets',
    'cfg.mode':'Mode','cfg.theme':'Theme',
    'btn.start':'Demarrer','btn.stop':'Arreter','btn.debug':'Debug','btn.search':'Rechercher','btn.save':'Enregistrer',
    'btn.reset':'Reinitialiser','btn.add':'Ajouter','btn.cancel':'Annuler','btn.remove':'Supprimer',
    'btn.addagent':'+ Ajouter Agent',
    'sec.global':'Parametres globaux','sec.extraction':'Extraction','sec.coherence':'Coherence',
    'sec.reactivation':'Reactivation','sec.synthesis':'Synthese','sec.labels':'Suggestion de labels',
    'sec.importance':"Notation d'importance",'sec.langdisplay':'Langue & Affichage',
    'sec.validatorweights':'Poids des validateurs (0.0 = desactive, 1.0 = plein)',
    'th.title':'Titre','th.status':'Statut','th.weight':'Poids','th.importance':'Importance',
    'th.topics':'Sujets','th.activations':'Activations','th.id':'ID','th.name':'Nom',
    'th.role':'Role','th.supervisor':'Superviseur','th.team':'Equipe','th.mode':'Mode',
    'status.checking':'verification...','status.stopped':'Arrete',
    'settings.title':'Parametres','settings.saved':'Parametres enregistres','settings.failed':'Echec de la sauvegarde',
    'ph.search':'Rechercher des threads...','ph.theme':"Options de theme disponibles dans une prochaine version.",
    'ph.network.title':"Reseau d'agents",'ph.network':'Communication multi-agents et collaboration a distance.',
    'ph.network2':"Decouverte d'agents, routage de messages, synchronisation memoire.",
    'ph.updates.title':'Mises a jour','ph.updates':'Preferences de mise a jour automatique et gestion des versions.',
    'ph.comingsoon':'Bientot disponible',
    'modal.addproject':'Ajouter un projet','modal.projectpath':'Chemin du projet','modal.projectname':'Nom (optionnel)',
    'modal.addagent':'Ajouter un agent','modal.agentid':"ID de l'agent",'modal.agentname':'Nom',
    'modal.agentrole':'Role','modal.agentsupervisor':'Superviseur (optionnel)','modal.agentteam':'Equipe (optionnel)',
    'modal.issupervisor':'Est Superviseur',
    'agents.list':'Agents enregistres','agents.hierarchy':'Hierarchie',
    'agents.noagents':'Aucun agent enregistre','agents.nohierarchy':'Aucune hierarchie',
    'dash.projects':'Projets','dash.roletree':"Arbre des roles de l'equipe",
    'btn.addproject':'+ Ajouter Projet','btn.edit':'Modifier','btn.delete':'Supprimer',
    'th.path':'Chemin','th.provider':'Fournisseur','th.hash':'Hash',
    'modal.editproject':'Modifier le projet','modal.provider':'Fournisseur',
    'project.noprojects':'Aucun projet enregistre','project.confirmdelete':'Supprimer le projet',
    'cfg.enabled':'Guardian active','cfg.clipath':'Chemin CLI Claude','cfg.hookguard':'Env Hook Guard',
    'cfg.cache':'Cache active','cfg.cachettl':'Cache TTL (sec)','cfg.cachemax':'Cache max entrees',
    'cfg.pattern':'Apprentissage de patterns','cfg.patterndecay':'Decroissance patterns (jours)',
    'cfg.usage':"Suivi d'utilisation",'cfg.fallback':"Repli en cas d'echec",
    'cfg.model':'Modele','cfg.timeout':'Timeout (s)','cfg.retries':'Max tentatives',
    'cfg.taskEnabled':'Active','cfg.failmode':"Mode d'echec",'cfg.language':'Langue',
    'gc.enabled':'Active','gc.maxbytes':'Taille max (octets)','gc.warnonblock':'Avertir au blocage',
    'gc.action':'Action au blocage','gc.blockedpatterns':'Patterns bloques','gc.addpattern':'+ Ajouter Pattern',
    'gc.sanitize':'Nettoyage LLM','gc.sanitizeretries':'Boucle nettoyage max',
},
es: {
    'tab.dashboard':'Panel','tab.threads':'Hilos','tab.agents':'Agentes','tab.graph':'Grafo','tab.settings':'Configuracion',
    'stab.general':'General','stab.guardian':'Guardian LLM','stab.matching':'Correspondencia',
    'stab.gossip':'Gossip','stab.engram':'Engram','stab.decay':'Decaimiento & Ciclo de vida','stab.alerts':'Alertas',
    'stab.fallback':'Respaldo','stab.guardcode':'GuardCode','stab.network':'Red','stab.updates':'Actualizaciones',
    'dash.daemon':'Daemon','dash.active':'Hilos activos','dash.suspended':'Suspendidos',
    'dash.archived':'Archivados','dash.bridges':'Puentes','dash.version':'Version',
    'dash.cpu':'CPU','dash.memory':'Memoria','dash.pool':'Pool',
    'stab.daemon':'Daemon','sec.daemonpool':'Pool de conexiones',
    'daemon.autostart':'Auto-inicio del daemon','daemon.maxconn':'Conexiones max',
    'daemon.idletimeout':'Timeout inactividad (seg)','daemon.pruneinterval':'Intervalo limpieza (seg)',
    'daemon.crossgossip':'Gossip entre proyectos',
    'cfg.mode':'Modo','cfg.theme':'Tema',
    'btn.start':'Iniciar','btn.stop':'Detener','btn.debug':'Debug','btn.search':'Buscar','btn.save':'Guardar',
    'btn.reset':'Restablecer','btn.add':'Agregar','btn.cancel':'Cancelar','btn.remove':'Eliminar',
    'btn.addagent':'+ Agregar Agente',
    'sec.global':'Configuracion global','sec.extraction':'Extraccion','sec.coherence':'Coherencia',
    'sec.reactivation':'Reactivacion','sec.synthesis':'Sintesis','sec.labels':'Sugerencia de etiquetas',
    'sec.importance':'Calificacion de importancia','sec.langdisplay':'Idioma y visualizacion',
    'sec.validatorweights':'Pesos de validadores (0.0 = desactivado, 1.0 = completo)',
    'th.title':'Titulo','th.status':'Estado','th.weight':'Peso','th.importance':'Importancia',
    'th.topics':'Temas','th.activations':'Activaciones','th.id':'ID','th.name':'Nombre',
    'th.role':'Rol','th.supervisor':'Supervisor','th.team':'Equipo','th.mode':'Modo',
    'status.checking':'verificando...','status.stopped':'Detenido',
    'settings.title':'Configuracion','settings.saved':'Configuracion guardada','settings.failed':'Error al guardar',
    'ph.search':'Buscar hilos...','ph.theme':'Opciones de tema disponibles en una version futura.',
    'ph.network.title':'Red de agentes','ph.network':'Comunicacion multi-agente y colaboracion remota.',
    'ph.network2':'Descubrimiento de agentes, enrutamiento de mensajes, sincronizacion de memoria.',
    'ph.updates.title':'Actualizaciones','ph.updates':'Preferencias de actualizacion automatica y gestion de versiones.',
    'ph.comingsoon':'Proximamente',
    'modal.addproject':'Agregar proyecto','modal.projectpath':'Ruta del proyecto','modal.projectname':'Nombre (opcional)',
    'modal.addagent':'Agregar agente','modal.agentid':'ID del agente','modal.agentname':'Nombre',
    'modal.agentrole':'Rol','modal.agentsupervisor':'Supervisor (opcional)','modal.agentteam':'Equipo (opcional)',
    'modal.issupervisor':'Es Supervisor',
    'agents.list':'Agentes registrados','agents.hierarchy':'Jerarquia',
    'agents.noagents':'Ningun agente registrado','agents.nohierarchy':'Sin jerarquia',
    'dash.projects':'Proyectos','dash.roletree':'Arbol de roles del equipo',
    'btn.addproject':'+ Agregar Proyecto','btn.edit':'Editar','btn.delete':'Eliminar',
    'th.path':'Ruta','th.provider':'Proveedor','th.hash':'Hash',
    'modal.editproject':'Editar proyecto','modal.provider':'Proveedor',
    'project.noprojects':'Ningun proyecto registrado','project.confirmdelete':'Eliminar proyecto',
    'cfg.enabled':'Guardian activado','cfg.clipath':'Ruta CLI Claude','cfg.hookguard':'Env Hook Guard',
    'cfg.cache':'Cache activado','cfg.cachettl':'Cache TTL (seg)','cfg.cachemax':'Cache max entradas',
    'cfg.pattern':'Aprendizaje de patrones','cfg.patterndecay':'Decaimiento patrones (dias)',
    'cfg.usage':'Seguimiento de uso','cfg.fallback':'Respaldo en caso de fallo',
    'cfg.model':'Modelo','cfg.timeout':'Timeout (s)','cfg.retries':'Max reintentos',
    'cfg.taskEnabled':'Activado','cfg.failmode':'Modo de fallo','cfg.language':'Idioma',
    'gc.enabled':'Activado','gc.maxbytes':'Tamano max (bytes)','gc.warnonblock':'Avisar al bloquear',
    'gc.action':'Accion al bloquear','gc.blockedpatterns':'Patrones bloqueados','gc.addpattern':'+ Agregar Patron',
    'gc.sanitize':'Sanear LLM','gc.sanitizeretries':'Bucle saneamiento max',
}
};

let currentLang = localStorage.getItem('ai_smartness_lang') || 'en';

function applyTranslations(lang) {
    currentLang = lang;
    localStorage.setItem('ai_smartness_lang', lang);
    const dict = T[lang] || T.en;
    document.querySelectorAll('[data-i18n]').forEach(el => {
        const key = el.dataset.i18n;
        const text = dict[key];
        if (!text) return;
        // For labels with child inputs/selects: replace only the first text node
        const hasInput = el.querySelector('input, select');
        if (hasInput) {
            for (const node of el.childNodes) {
                if (node.nodeType === 3 && node.textContent.trim()) {
                    node.textContent = text + ' ';
                    break;
                }
            }
        } else {
            el.textContent = text;
        }
    });
    // Placeholders
    document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
        const key = el.dataset.i18nPlaceholder;
        const text = dict[key];
        if (text) el.placeholder = text;
    });
    document.documentElement.lang = lang;
}

// ─── State ───────────────────────────────────────────────────
let projectHash = '';
let agentId = 'default';
let currentSettings = null;
let overviewAgents = [];  // per-agent metrics from get_project_overview

// ─── Init ────────────────────────────────────────────────────
loadProjects();
loadDashboard();
setInterval(loadDashboard, 30000);  // 30s — metrics change slowly
loadResources();
setInterval(loadResources, 15000);  // 15s — CPU/RAM change moderately
// Apply saved language
if (currentLang !== 'en') {
    applyTranslations(currentLang);
}
// Sync language selector
const langSel = document.getElementById('lang-select');
if (langSel) langSel.value = currentLang;

// ─── Tab navigation ──────────────────────────────────────────
document.querySelectorAll('.tab:not(.tab-debug)').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.tab:not(.tab-debug)').forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
        tab.classList.add('active');
        document.getElementById(tab.dataset.tab).classList.add('active');

        if (tab.dataset.tab === 'threads') loadThreadAgentTabs();
        if (tab.dataset.tab === 'settings') loadSettings();
        if (tab.dataset.tab === 'agents') loadAgents();
        if (tab.dataset.tab === 'graph') requestAnimationFrame(() => loadGraph());
    });
});

// ─── Settings sub-tab navigation ────────────────────────────
document.querySelectorAll('.sub-tab').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.sub-tab').forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.sub-panel').forEach(p => p.classList.remove('active'));
        tab.classList.add('active');
        document.getElementById(tab.dataset.stab).classList.add('active');
    });
});

// ─── Section toggles ─────────────────────────────────────────
document.querySelectorAll('.section-toggle').forEach(toggle => {
    toggle.addEventListener('click', () => {
        const section = document.getElementById('section-' + toggle.dataset.section);
        section.classList.toggle('open');
        toggle.classList.toggle('open');
    });
});

// ─── Language change ─────────────────────────────────────────
document.getElementById('lang-select')?.addEventListener('change', (e) => {
    applyTranslations(e.target.value);
});

// ─── Theme & Mode ───────────────────────────────────────────
const savedTheme = localStorage.getItem('ai_smartness_theme') || 'green';
const savedMode = localStorage.getItem('ai_smartness_mode') || 'dark';
document.documentElement.setAttribute('data-theme', savedTheme);
document.documentElement.setAttribute('data-mode', savedMode);
const themeSel = document.getElementById('theme-select');
const modeSel = document.getElementById('mode-select');
if (themeSel) themeSel.value = savedTheme;
if (modeSel) modeSel.value = savedMode;

themeSel?.addEventListener('change', (e) => {
    document.documentElement.setAttribute('data-theme', e.target.value);
    localStorage.setItem('ai_smartness_theme', e.target.value);
});
modeSel?.addEventListener('change', (e) => {
    document.documentElement.setAttribute('data-mode', e.target.value);
    localStorage.setItem('ai_smartness_mode', e.target.value);
});

// ─── Project selector ────────────────────────────────────────
document.getElementById('project-select').addEventListener('change', (e) => {
    projectHash = e.target.value;
    loadDashboard();
    loadResources();
    // Reload whichever tab is currently active
    const activeTab = document.querySelector('.tab:not(.tab-debug).active');
    if (activeTab) {
        const tab = activeTab.dataset.tab;
        if (tab === 'threads') { loadThreads(); loadLabelOptions(); loadTopicOptions(); }
        if (tab === 'agents') loadAgents();
        if (tab === 'settings') loadSettings();
        if (tab === 'graph') { document.getElementById('graph-agent-select').innerHTML = ''; requestAnimationFrame(() => loadGraph()); }
    }
});

// ─── Daemon controls ─────────────────────────────────────────
document.getElementById('btn-daemon-start').addEventListener('click', async () => {
    try {
        await invoke('daemon_start');
        setTimeout(loadDashboard, 1500);
    } catch (e) { console.error('Daemon start error:', e); }
});

document.getElementById('btn-daemon-stop').addEventListener('click', async () => {
    try {
        await invoke('daemon_stop');
        setTimeout(loadDashboard, 1000);
    } catch (e) { console.error('Daemon stop error:', e); }
});

// ─── Debug window ───────────────────────────────────────────
document.getElementById('btn-debug').addEventListener('click', async () => {
    if (!projectHash) { alert('Select a project first'); return; }
    try {
        await invoke('open_debug_window', { projectHash });
    } catch (e) { console.error('Debug window error:', e); }
});

// ─── Search ──────────────────────────────────────────────────
document.getElementById('btn-search')?.addEventListener('click', () => {
    const query = document.getElementById('thread-search').value.trim();
    if (query) searchThreads(query);
});
document.getElementById('thread-search')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
        const query = e.target.value.trim();
        if (query) searchThreads(query);
    }
});
document.getElementById('thread-filter')?.addEventListener('change', loadThreads);

// ─── Search sub-tabs (Text / Labels / Topics) ───────────────
document.querySelectorAll('.search-tab').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.search-tab').forEach(t => {
            t.classList.remove('active');
            t.style.borderBottomColor = 'transparent';
            t.style.color = 'var(--text-dim,#888)';
        });
        document.querySelectorAll('.search-panel').forEach(p => p.style.display = 'none');
        tab.classList.add('active');
        tab.style.borderBottomColor = 'var(--accent,#6cf)';
        tab.style.color = 'var(--text,#eee)';
        const panel = document.getElementById('search-' + tab.dataset.searchTab);
        if (panel) panel.style.display = '';
        // Auto-load labels/topics when switching tabs
        if (tab.dataset.searchTab === 'labels') loadLabelOptions();
        if (tab.dataset.searchTab === 'topics') loadTopicOptions();
    });
});

async function loadLabelOptions() {
    if (!projectHash || !threadAgentId) return;
    try {
        const labels = await invoke('list_all_labels', { projectHash, agentId: threadAgentId });
        const sel = document.getElementById('label-select');
        sel.innerHTML = '';
        for (const l of labels) {
            const opt = document.createElement('option');
            opt.value = l;
            opt.textContent = l;
            sel.appendChild(opt);
        }
    } catch (e) { console.error('Labels load error:', e); }
}

async function loadTopicOptions() {
    if (!projectHash || !threadAgentId) return;
    try {
        const topics = await invoke('list_all_topics', { projectHash, agentId: threadAgentId });
        const sel = document.getElementById('topic-select');
        sel.innerHTML = '';
        for (const t of topics) {
            const opt = document.createElement('option');
            opt.value = t;
            opt.textContent = t;
            sel.appendChild(opt);
        }
    } catch (e) { console.error('Topics load error:', e); }
}

document.getElementById('btn-search-labels')?.addEventListener('click', async () => {
    const sel = document.getElementById('label-select');
    const labels = Array.from(sel.selectedOptions).map(o => o.value);
    if (labels.length === 0) return;
    try {
        const threads = await invoke('search_threads_by_label', { projectHash, agentId: threadAgentId, labels });
        renderThreads(threads);
    } catch (e) { console.error('Label search error:', e); }
});

document.getElementById('btn-search-topics')?.addEventListener('click', async () => {
    const sel = document.getElementById('topic-select');
    const topics = Array.from(sel.selectedOptions).map(o => o.value);
    if (topics.length === 0) return;
    try {
        const threads = await invoke('search_threads_by_topic', { projectHash, agentId: threadAgentId, topics });
        renderThreads(threads);
    } catch (e) { console.error('Topic search error:', e); }
});

document.getElementById('btn-load-labels')?.addEventListener('click', loadLabelOptions);
document.getElementById('btn-load-topics')?.addEventListener('click', loadTopicOptions);

// ─── Settings buttons ────────────────────────────────────────
document.getElementById('btn-save-settings').addEventListener('click', saveSettings);
document.getElementById('btn-reset-defaults').addEventListener('click', async () => {
    if (!confirm('Reset all settings to defaults?')) return;
    currentSettings = null;
    await loadSettings();
});

// ─── Add Project modal ──────────────────────────────────────
document.getElementById('btn-add-project')?.addEventListener('click', openAddProjectModal);
document.getElementById('btn-cancel-project').addEventListener('click', () => {
    document.getElementById('modal-add-project').classList.remove('open');
});
document.getElementById('modal-add-project').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) e.currentTarget.classList.remove('open');
});
document.getElementById('btn-confirm-project').addEventListener('click', addProject);
document.getElementById('new-project-path').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') addProject();
});
document.getElementById('new-project-name').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') addProject();
});
function openAddProjectModal() {
    document.getElementById('modal-add-project').classList.add('open');
    document.getElementById('new-project-path').value = '';
    document.getElementById('new-project-name').value = '';
    document.getElementById('add-project-error').textContent = '';
    document.getElementById('new-project-path').focus();
}

// ─── Add Agent modal ────────────────────────────────────────
document.getElementById('btn-add-agent').addEventListener('click', async () => {
    document.getElementById('modal-add-agent').classList.add('open');
    document.getElementById('new-agent-id').value = '';
    document.getElementById('new-agent-name').value = '';
    document.getElementById('new-agent-team').value = '';
    document.getElementById('new-agent-supervisor-flag').checked = false;
    document.getElementById('new-agent-custom-role').value = '';
    document.getElementById('add-agent-error').textContent = '';
    // Populate supervisor and report_to dropdowns with existing agents
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
    // Toggle custom role field visibility
    const roleSelect = document.getElementById('new-agent-role');
    const crField = document.getElementById('new-agent-custom-role-field');
    roleSelect.value = 'programmer';
    crField.style.display = 'none';
    roleSelect.addEventListener('change', () => {
        crField.style.display = roleSelect.value === 'custom' ? '' : 'none';
    });
    document.getElementById('new-agent-id').focus();
});
document.getElementById('btn-cancel-agent').addEventListener('click', () => {
    document.getElementById('modal-add-agent').classList.remove('open');
});
document.getElementById('modal-add-agent').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) e.currentTarget.classList.remove('open');
});
document.getElementById('btn-confirm-agent').addEventListener('click', addAgent);

async function addProject() {
    const path = document.getElementById('new-project-path').value.trim();
    const name = document.getElementById('new-project-name').value.trim() || null;
    const errEl = document.getElementById('add-project-error');

    if (!path) { errEl.textContent = 'Path is required'; return; }

    try {
        const result = await invoke('add_project', { path, name });
        document.getElementById('modal-add-project').classList.remove('open');
        projectHash = result.hash;
        await loadProjects();
        document.getElementById('project-select').value = projectHash;
        loadDashboard();
    } catch (e) {
        errEl.textContent = String(e);
    }
}

async function addAgent() {
    const agentIdVal = document.getElementById('new-agent-id').value.trim();
    const nameVal = document.getElementById('new-agent-name').value.trim();
    const roleVal = document.getElementById('new-agent-role').value;
    const supVal = document.getElementById('new-agent-supervisor').value || null;
    const teamVal = document.getElementById('new-agent-team').value.trim() || null;
    const isSup = document.getElementById('new-agent-supervisor-flag').checked;
    const threadModeVal = document.getElementById('new-agent-thread-mode').value;
    const customRoleVal = document.getElementById('new-agent-custom-role').value.trim() || null;
    const reportToVal = document.getElementById('new-agent-report-to').value || null;
    const errEl = document.getElementById('add-agent-error');

    if (!projectHash) { errEl.textContent = 'Select a project first'; return; }
    if (!agentIdVal) { errEl.textContent = 'Agent ID is required'; return; }
    if (!nameVal) { errEl.textContent = 'Name is required'; return; }

    try {
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
        document.getElementById('modal-add-agent').classList.remove('open');
        loadAgents();
        loadDashboard();
    } catch (e) {
        errEl.textContent = String(e);
    }
}

// ═══════════════════════════════════════════════════════════════
// PROJECTS — header dropdown + gear edit
// ═══════════════════════════════════════════════════════════════

let projectsCache = [];

async function loadProjects() {
    try {
        const projects = await invoke('list_projects');
        projectsCache = projects;
        // Header dropdown
        const sel = document.getElementById('project-select');
        while (sel.options.length > 1) sel.remove(1);
        for (const p of projects) {
            const opt = document.createElement('option');
            opt.value = p.hash;
            opt.textContent = p.name || p.path || p.hash;
            sel.appendChild(opt);
        }
        if (projects.length > 0 && !projectHash) {
            projectHash = projects[0].hash;
            sel.value = projectHash;
        }
    } catch (e) { console.error('Projects error:', e); }
}

// Header gear button — edit/delete current project
document.getElementById('btn-edit-project')?.addEventListener('click', () => {
    if (!projectHash) return;
    const project = projectsCache.find(p => p.hash === projectHash);
    if (!project) return;
    openProjectEditPanel(project);
});

function openProjectEditPanel(project) {
    // Close existing panel
    const existing = document.querySelector('.project-edit-panel');
    if (existing) { existing.remove(); return; }

    const dict = T[currentLang] || T.en;
    const panel = document.createElement('div');
    panel.className = 'project-edit-panel';
    panel.innerHTML = `
        <div class="project-edit-form">
            <label>${dict['th.name'] || 'Name'}
                <input type="text" class="pe-name" value="${esc(project.name || '')}">
            </label>
            <label>${dict['th.path'] || 'Path'}
                <input type="text" class="pe-path" value="${esc(project.path || '')}">
            </label>
            <div class="project-edit-actions">
                <button class="btn-sm btn-success pe-save">${dict['btn.save'] || 'Save'}</button>
                <button class="btn-sm btn-danger pe-delete">${dict['btn.delete'] || 'Delete'}</button>
                <button class="btn-sm pe-cancel">${dict['btn.cancel'] || 'Cancel'}</button>
            </div>
            <div class="project-edit-error"></div>
        </div>
    `;
    document.querySelector('header').after(panel);
    panel.querySelector('.pe-name').focus();
    panel.querySelector('.pe-save').addEventListener('click', () => saveProjectEdit(panel, project));
    panel.querySelector('.pe-delete').addEventListener('click', () => {
        deleteProject(project.hash, project.name || project.path || project.hash);
        panel.remove();
    });
    panel.querySelector('.pe-cancel').addEventListener('click', () => panel.remove());
    panel.querySelectorAll('input').forEach(input => {
        input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') saveProjectEdit(panel, project);
            if (e.key === 'Escape') panel.remove();
        });
    });
}

async function saveProjectEdit(panel, original) {
    const name = panel.querySelector('.pe-name').value.trim() || null;
    const path = panel.querySelector('.pe-path').value.trim() || null;
    const provider = null;
    const errEl = panel.querySelector('.project-edit-error');
    const hash = original.hash;

    let daemonWasRunning = false;
    const isActiveProject = (hash === projectHash);

    try {
        if (isActiveProject) {
            const status = await invoke('daemon_status');
            daemonWasRunning = status.running;
        }

        if (daemonWasRunning) {
            errEl.textContent = 'Stopping daemon...';
            await invoke('daemon_stop');
            await new Promise(r => setTimeout(r, 1000));
        }

        errEl.textContent = '';
        await invoke('update_project', { hash, name, path, provider });
        await loadProjects();
        panel.remove();

        if (daemonWasRunning && isActiveProject) {
            await invoke('daemon_start');
            setTimeout(loadDashboard, 1500);
        }
    } catch (e) {
        errEl.textContent = String(e);
        if (daemonWasRunning && isActiveProject) {
            try { await invoke('daemon_start'); } catch (_) {}
        }
    }
}

async function deleteProject(hash, displayName) {
    const dict = T[currentLang] || T.en;
    if (!confirm(`${dict['project.confirmdelete'] || 'Delete project'} "${displayName}"?`)) return;

    try {
        // Stop daemon if running on this project
        if (hash === projectHash) {
            try {
                const status = await invoke('daemon_status');
                if (status.running) {
                    await invoke('daemon_stop');
                    await new Promise(r => setTimeout(r, 500));
                }
            } catch (_) {}
        }

        await invoke('remove_project', { hash });
        if (projectHash === hash) {
            projectHash = '';
            document.getElementById('project-select').value = '';
        }
        await loadProjects();
        loadDashboard();
    } catch (e) {
        alert('Error: ' + e);
    }
}

// ═══════════════════════════════════════════════════════════════
// DASHBOARD
// ═══════════════════════════════════════════════════════════════

async function loadDashboard() {
    if (!projectHash) return;
    try {
        const data = await invoke('get_project_overview', { projectHash });
        const ds = data.daemon_status;
        const el = document.getElementById('daemon-status');
        if (ds.running) {
            el.textContent = `Running (PID ${ds.pid || '?'})`;
            el.className = 'status running';
            document.getElementById('btn-daemon-start').disabled = true;
            document.getElementById('btn-daemon-stop').disabled = false;
        } else {
            el.textContent = T[currentLang]?.['status.stopped'] || 'Stopped';
            el.className = 'status stopped';
            document.getElementById('btn-daemon-start').disabled = false;
            document.getElementById('btn-daemon-stop').disabled = true;
        }
        document.getElementById('daemon-version').textContent = ds.version || '-';

        const tc = data.totals;
        document.getElementById('thread-active').textContent = tc.active;
        document.getElementById('thread-suspended').textContent = tc.suspended;
        document.getElementById('thread-archived').textContent = tc.archived;
        document.getElementById('bridge-count').textContent = tc.bridges;

        // Store agent metrics for tree node stats
        overviewAgents = data.agents || [];
    } catch (e) {
        console.error('Dashboard error:', e);
    }

    // Load role tree for dashboard
    try {
        const hierarchy = await invoke('get_hierarchy', { projectHash });
        renderDashboardHierarchy(hierarchy);
    } catch (e) {
        console.error('Dashboard hierarchy error:', e);
    }
}

function renderDashboardHierarchy(nodes) {
    const view = document.getElementById('dashboard-hierarchy');
    if (!view) return;
    if (!nodes || nodes.length === 0) {
        const dict = T[currentLang] || T.en;
        view.innerHTML = `<p style="color:var(--text-dim);font-size:13px">${dict['agents.noagents'] || 'No agents configured'}</p>`;
        return;
    }
    view.innerHTML = `<div class="org-tree">${nodes.map(n => renderNode(n)).join('')}</div>`;
}

// ─── Agent detail panel (click on org-card) ─────────────────

// Delegated click handler on dashboard hierarchy
document.getElementById('dashboard-hierarchy')?.addEventListener('click', (e) => {
    const card = e.target.closest('.org-card[data-agent-id]');
    if (!card) return;
    selectAgentInTree(card.dataset.agentId);
});

// Tab switching inside agent detail panel
document.querySelectorAll('.agent-tab').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.agent-tab').forEach(t => t.classList.remove('active'));
        tab.classList.add('active');
        const tabName = tab.dataset.agentTab;
        document.getElementById('agent-threads-content').style.display = tabName === 'threads' ? '' : 'none';
        document.getElementById('agent-bridges-content').style.display = tabName === 'bridges' ? '' : 'none';
    });
});

async function selectAgentInTree(selectedAgentId) {
    // 1. Highlight selected card
    document.querySelectorAll('.org-card.selected').forEach(c => c.classList.remove('selected'));
    document.querySelector(`.org-card[data-agent-id="${selectedAgentId}"]`)?.classList.add('selected');

    // 2. Show panel
    const panel = document.getElementById('agent-detail-panel');
    panel.style.display = 'block';

    // 3. Title + stats from overview
    const agent = overviewAgents.find(a => a.id === selectedAgentId);
    document.getElementById('agent-detail-title').textContent =
        `${agent?.name || selectedAgentId} (${agent?.role || ''})`;
    document.getElementById('agent-detail-stats').innerHTML =
        `Active: ${agent?.active || 0} · Suspended: ${agent?.suspended || 0} · Archived: ${agent?.archived || 0} · Bridges: ${agent?.bridges || 0}`;

    // 4. Load threads
    try {
        const threads = await invoke('get_threads', { projectHash, agentId: selectedAgentId, statusFilter: 'active' });
        renderAgentThreads(threads);
    } catch (e) {
        console.error('Agent threads error:', e);
        renderAgentThreads([]);
    }

    // 5. Load bridges
    try {
        const bridges = await invoke('get_bridges', { projectHash, agentId: selectedAgentId });
        renderAgentBridges(bridges);
    } catch (e) {
        console.error('Agent bridges error:', e);
        renderAgentBridges([]);
    }
}

function renderAgentThreads(threads) {
    const tbody = document.getElementById('agent-threads-body');
    tbody.innerHTML = '';
    for (const t of threads) {
        const tr = document.createElement('tr');
        tr.dataset.threadId = t.id;
        tr.style.cursor = 'pointer';
        tr.innerHTML = `
            <td>${esc(t.title)}</td>
            <td><span class="badge badge-${(t.status||'').toLowerCase()}">${esc(t.status)}</span></td>
            <td>${(t.weight || 0).toFixed(2)}</td>
            <td>${(t.importance || 0).toFixed(2)}</td>
            <td>${(t.topics || []).join(', ')}</td>
        `;
        tbody.appendChild(tr);
    }
    if (threads.length === 0) {
        tbody.innerHTML = '<tr><td colspan="5" style="text-align:center;color:var(--text-dim)">No threads</td></tr>';
    }
}

function renderAgentBridges(bridges) {
    const tbody = document.getElementById('agent-bridges-body');
    tbody.innerHTML = '';
    for (const b of bridges) {
        const tr = document.createElement('tr');
        tr.innerHTML = `
            <td>${esc(b.source_id || '')}</td>
            <td>${esc(b.target_id || '')}</td>
            <td>${esc(b.relation_type || '')}</td>
            <td>${(b.weight || 0).toFixed(2)}</td>
        `;
        tbody.appendChild(tr);
    }
    if (bridges.length === 0) {
        tbody.innerHTML = '<tr><td colspan="4" style="text-align:center;color:var(--text-dim)">No bridges</td></tr>';
    }
}

// ═══════════════════════════════════════════════════════════════
// THREADS — agent tabs + thread listing
// ═══════════════════════════════════════════════════════════════

let threadAgentId = '';  // Currently selected agent in Threads tab

async function loadThreadAgentTabs() {
    if (!projectHash) return;
    const container = document.getElementById('thread-agent-tabs');
    container.innerHTML = '';
    try {
        const agents = await invoke('list_agents', { projectHash });
        if (agents.length === 0) {
            container.innerHTML = '<span style="color:var(--text-dim);font-size:13px;padding:6px">No agents</span>';
            return;
        }
        for (const a of agents) {
            const btn = document.createElement('button');
            btn.textContent = `${a.name || a.id} (${a.role || '?'})`;
            btn.dataset.agentId = a.id;
            btn.style.cssText = 'padding:6px 16px;background:none;border:none;border-bottom:2px solid transparent;color:var(--text-dim,#888);cursor:pointer;font-size:13px;white-space:nowrap';
            btn.addEventListener('click', () => selectThreadAgent(a.id));
            container.appendChild(btn);
        }
        // Auto-select first agent
        selectThreadAgent(agents[0].id);
    } catch (e) {
        console.error('Thread agent tabs error:', e);
    }
}

function selectThreadAgent(selectedId) {
    threadAgentId = selectedId;
    const container = document.getElementById('thread-agent-tabs');
    container.querySelectorAll('button').forEach(btn => {
        if (btn.dataset.agentId === selectedId) {
            btn.style.borderBottomColor = 'var(--accent,#6cf)';
            btn.style.color = 'var(--text,#eee)';
        } else {
            btn.style.borderBottomColor = 'transparent';
            btn.style.color = 'var(--text-dim,#888)';
        }
    });
    loadThreads();
    loadLabelOptions();
    loadTopicOptions();
}

async function loadThreads() {
    if (!projectHash || !threadAgentId) return;
    const filter = document.getElementById('thread-filter').value;
    try {
        const threads = await invoke('get_threads', {
            projectHash, agentId: threadAgentId, statusFilter: filter
        });
        renderThreads(threads);
    } catch (e) { console.error('Threads error:', e); }
}

async function searchThreads(query) {
    if (!projectHash || !threadAgentId) return;
    try {
        const threads = await invoke('search_threads', { projectHash, agentId: threadAgentId, query });
        renderThreads(threads);
    } catch (e) { console.error('Search error:', e); }
}

function renderThreads(threads) {
    const tbody = document.getElementById('thread-body');
    tbody.innerHTML = '';
    for (const t of threads) {
        const tr = document.createElement('tr');
        tr.dataset.threadId = t.id;
        tr.style.cursor = 'pointer';
        tr.innerHTML = `
            <td>${esc(t.title)}</td>
            <td><span class="badge badge-${(t.status||'').toLowerCase()}">${esc(t.status)}</span></td>
            <td>${(t.weight || 0).toFixed(2)}</td>
            <td>${(t.importance || 0).toFixed(2)}</td>
            <td>${(t.topics || []).join(', ')}</td>
            <td>${t.message_count || 0}</td>
        `;
        tbody.appendChild(tr);
    }
    if (threads.length === 0) {
        tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;color:var(--text-dim)">No threads found</td></tr>';
    }
}

// ═══════════════════════════════════════════════════════════════
// AGENTS
// ═══════════════════════════════════════════════════════════════

async function loadAgents() {
    if (!projectHash) return;
    try {
        const agents = await invoke('list_agents', { projectHash });
        renderAgents(agents);
    } catch (e) { console.error('Agents error:', e); }
    try {
        const hierarchy = await invoke('get_hierarchy', { projectHash });
        renderHierarchy(hierarchy);
    } catch (e) { console.error('Hierarchy error:', e); }
}

let agentsCache = [];

function renderAgents(agents) {
    agentsCache = agents;
    const tbody = document.getElementById('agent-body');
    tbody.innerHTML = '';
    for (const a of agents) {
        const tr = document.createElement('tr');
        tr.dataset.agentId = a.id;
        tr.innerHTML = `
            <td class="td-toggle"><button class="btn-agent-toggle" title="Edit">\u25B6</button></td>
            <td>${esc(a.id)}</td>
            <td>${esc(a.name)}</td>
            <td>${esc(a.role)}</td>
            <td><span class="badge badge-${(a.status||'').toLowerCase()}">${esc(a.status)}</span></td>
            <td>${esc(a.supervisor_id || '-')}</td>
            <td>${esc(a.team || '-')}</td>
            <td>${esc(a.coordination_mode)}</td>
            <td><span class="badge">${esc(a.thread_mode || 'normal')} (${a.thread_quota || 50})</span></td>
            <td><button class="btn-sm btn-danger btn-agent-remove">${T[currentLang]?.['btn.remove'] || 'Remove'}</button></td>
        `;
        tbody.appendChild(tr);
    }
    if (agents.length === 0) {
        const noAgentsText = T[currentLang]?.['agents.noagents'] || 'No agents registered';
        tbody.innerHTML = `<tr><td colspan="10" style="text-align:center;color:var(--text-dim)">${noAgentsText}</td></tr>`;
    }
}

// Delegated click handler for agent table
document.getElementById('agent-body')?.addEventListener('click', (e) => {
    // Remove button
    const removeBtn = e.target.closest('.btn-agent-remove');
    if (removeBtn) {
        const tr = removeBtn.closest('tr');
        const id = tr?.dataset.agentId;
        if (id) removeAgent(id);
        return;
    }
    // Toggle edit — only on the chevron button
    const toggleBtn = e.target.closest('.btn-agent-toggle');
    if (toggleBtn) {
        const tr = toggleBtn.closest('tr[data-agent-id]');
        if (tr) {
            const agent = agentsCache.find(a => a.id === tr.dataset.agentId);
            if (agent) toggleAgentEditRow(tr, agent, toggleBtn);
        }
        return;
    }
});

function toggleAgentEditRow(tr, agent, toggleBtn) {
    const existing = document.querySelector('.agent-edit-row');
    if (existing) {
        // Restore chevron of previously open row
        const prevBtn = document.querySelector('.btn-agent-toggle.open');
        if (prevBtn) { prevBtn.textContent = '\u25B6'; prevBtn.classList.remove('open'); }
        existing.remove();
        if (existing.dataset.agentId === agent.id) return; // same row → just close
    }
    // Mark chevron as open
    if (toggleBtn) { toggleBtn.textContent = '\u25BC'; toggleBtn.classList.add('open'); }

    const dict = T[currentLang] || T.en;
    const editTr = document.createElement('tr');
    editTr.className = 'agent-edit-row';
    editTr.dataset.agentId = agent.id;

    // Build supervisor options (exclude self)
    let supOptions = '<option value="">— None —</option>';
    for (const a of agentsCache) {
        if (a.id === agent.id) continue;
        const sel = (a.id === agent.supervisor_id) ? 'selected' : '';
        supOptions += `<option value="${esc(a.id)}" ${sel}>${esc(a.name)} (${esc(a.role)})</option>`;
    }
    // Build report_to options (exclude self)
    let rtOptions = '<option value="">— None —</option>';
    for (const a of agentsCache) {
        if (a.id === agent.id) continue;
        const sel = (a.id === agent.report_to) ? 'selected' : '';
        rtOptions += `<option value="${esc(a.id)}" ${sel}>${esc(a.name)} (${esc(a.role)})</option>`;
    }

    const isSup = agent.coordination_mode === 'coordinator';
    const roles = ['programmer', 'coordinator', 'reviewer', 'researcher', 'architect', 'custom'];
    const roleOptions = roles.map(r => `<option value="${r}" ${r === agent.role ? 'selected' : ''}>${r}</option>`).join('');
    const curMode = agent.thread_mode || 'normal';
    const threadModes = [
        { value: 'light', label: 'Light (15)' },
        { value: 'normal', label: 'Normal (50)' },
        { value: 'heavy', label: 'Heavy (100)' },
        { value: 'max', label: 'Max (200)' },
    ];
    const threadModeOptions = threadModes.map(m =>
        `<option value="${m.value}" ${m.value === curMode ? 'selected' : ''}>${m.label}</option>`
    ).join('');

    editTr.innerHTML = `<td colspan="10">
        <div class="agent-edit-form">
            <div class="form-grid">
                <label>${dict['th.name'] || 'Name'}
                    <input type="text" class="ae-name" value="${esc(agent.name || '')}">
                </label>
                <label>${dict['th.role'] || 'Role'}
                    <select class="ae-role">${roleOptions}</select>
                </label>
                <label>Description
                    <input type="text" class="ae-description" value="${esc(agent.description || '')}">
                </label>
                <label class="ae-custom-role-label" style="display:${agent.role === 'custom' ? '' : 'none'}">Custom Role
                    <input type="text" class="ae-custom-role" value="${esc(agent.custom_role || '')}" placeholder="e.g. auditor/researcher">
                </label>
                <label>Report To
                    <select class="ae-report-to">${rtOptions}</select>
                </label>
                <label>${dict['modal.agentsupervisor'] || 'Supervisor'}
                    <select class="ae-supervisor">${supOptions}</select>
                </label>
                <label>${dict['modal.agentteam'] || 'Team'}
                    <input type="text" class="ae-team" value="${esc(agent.team || '')}">
                </label>
                <label>Thread Mode
                    <select class="ae-thread-mode">${threadModeOptions}</select>
                </label>
                <label class="ae-inline">
                    <input type="checkbox" class="ae-is-supervisor" ${isSup ? 'checked' : ''}>
                    ${dict['modal.issupervisor'] || 'Is Supervisor'}
                </label>
                <label>Capabilities (comma-sep)
                    <input type="text" class="ae-capabilities" value="${esc((agent.capabilities||[]).join(', '))}">
                </label>
                <label>Specializations (comma-sep)
                    <input type="text" class="ae-specializations" value="${esc((agent.specializations||[]).join(', '))}">
                </label>
            </div>
            <div class="project-edit-actions">
                <button class="btn-sm btn-success ae-save">${dict['btn.save'] || 'Save'}</button>
                <button class="btn-sm ae-cancel">${dict['btn.cancel'] || 'Cancel'}</button>
                <button class="btn-sm btn-danger ae-purge" style="margin-left:auto">Purge DB</button>
            </div>
            <div class="agent-edit-error"></div>
        </div>
    </td>`;

    tr.after(editTr);
    editTr.querySelector('.ae-name').focus();
    // Toggle custom role field when role changes
    const aeRole = editTr.querySelector('.ae-role');
    const aeCrLabel = editTr.querySelector('.ae-custom-role-label');
    aeRole.addEventListener('change', () => {
        aeCrLabel.style.display = aeRole.value === 'custom' ? '' : 'none';
    });
    editTr.querySelector('.ae-save').addEventListener('click', () => saveAgentEdit(editTr, agent));
    editTr.querySelector('.ae-cancel').addEventListener('click', () => {
        const openBtn = document.querySelector('.btn-agent-toggle.open');
        if (openBtn) { openBtn.textContent = '\u25B6'; openBtn.classList.remove('open'); }
        editTr.remove();
    });
    editTr.querySelector('.ae-purge')?.addEventListener('click', () => {
        purgeAgentDb(agent.id);
    });
}

async function saveAgentEdit(editTr, original) {
    const errEl = editTr.querySelector('.agent-edit-error');
    const name = editTr.querySelector('.ae-name').value.trim() || null;
    const role = editTr.querySelector('.ae-role').value || null;
    const description = editTr.querySelector('.ae-description').value.trim();
    const supervisorId = editTr.querySelector('.ae-supervisor').value;
    const team = editTr.querySelector('.ae-team').value.trim();
    const isSupervisor = editTr.querySelector('.ae-is-supervisor').checked;
    const capsStr = editTr.querySelector('.ae-capabilities').value.trim();
    const specsStr = editTr.querySelector('.ae-specializations').value.trim();
    const threadMode = editTr.querySelector('.ae-thread-mode')?.value || null;
    const customRole = editTr.querySelector('.ae-custom-role')?.value?.trim() ?? '';
    const reportTo = editTr.querySelector('.ae-report-to')?.value ?? '';

    const capabilities = capsStr ? capsStr.split(',').map(s => s.trim()).filter(Boolean) : [];
    const specializations = specsStr ? specsStr.split(',').map(s => s.trim()).filter(Boolean) : [];

    try {
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
        if (result.threads_suspended > 0) {
            alert(`Thread mode updated. ${result.threads_suspended} thread(s) suspended to enforce new quota.`);
        }
        editTr.remove();
        loadAgents();
        loadDashboard();
    } catch (e) {
        errEl.textContent = String(e);
    }
}

function renderHierarchy(nodes) {
    const view = document.getElementById('hierarchy-view');
    if (!nodes || nodes.length === 0) {
        const noHierText = T[currentLang]?.['agents.nohierarchy'] || 'No hierarchy data';
        view.innerHTML = `<p style="color:var(--text-dim);font-size:13px">${noHierText}</p>`;
        return;
    }
    view.innerHTML = `<div class="org-tree">${nodes.map(n => renderNode(n)).join('')}</div>`;
}

function renderNode(node) {
    const hasSubs = node.subordinates && node.subordinates.length > 0;
    const team = node.team ? `<span class="org-meta">${esc(node.team)}</span>` : '';
    const tasks = node.active_tasks > 0 ? `<span class="org-meta">${node.active_tasks} tasks</span>` : '';
    const name = esc(node.name || node.id || '?');
    // Lookup agent metrics from overview
    const agentStats = overviewAgents.find(a => a.id === node.id);
    const statsLine = agentStats
        ? `<span class="node-stats">${agentStats.active} threads · ${agentStats.bridges} bridges</span>`
        : '';
    let html = `<div class="org-node">
        <div class="org-card" data-agent-id="${esc(node.id)}">
            <span class="org-name">${name}</span>
            <span class="org-role">${esc(node.role)}</span>
            <span class="org-meta">${esc(node.mode)}</span>
            ${team}${tasks}${statsLine}
        </div>`;
    if (hasSubs) {
        html += `<div class="org-connector"></div>
        <div class="org-children">
            ${node.subordinates.map(s => renderNode(s)).join('')}
        </div>`;
    }
    html += `</div>`;
    return html;
}

async function removeAgent(agentIdToRemove) {
    if (!confirm(`Remove agent "${agentIdToRemove}"?`)) return;
    try {
        await invoke('remove_agent', { projectHash, agentId: agentIdToRemove });
        loadAgents();
    } catch (e) {
        alert('Error: ' + e);
    }
}
// Expose to onclick
window.removeAgent = removeAgent;

// ═══════════════════════════════════════════════════════════════
// SETTINGS — Full GuardianConfig binding
// ═══════════════════════════════════════════════════════════════

async function loadSettings() {
    loadDaemonSettings();
    try {
        const settings = await invoke('get_settings', { projectHash: projectHash || '' });
        currentSettings = settings;
        populateForm(settings);
        // Sync language selector with loaded config
        const langSel = document.getElementById('lang-select');
        if (langSel && settings.synthesis && settings.synthesis.language) {
            langSel.value = settings.synthesis.language;
            if (settings.synthesis.language !== currentLang) {
                applyTranslations(settings.synthesis.language);
            }
        }
    } catch (e) {
        console.error('Settings error:', e);
        showSaveStatus('Error loading settings: ' + e, true);
    }
}

function populateForm(obj) {
    document.querySelectorAll('[data-path]').forEach(el => {
        const val = getNestedValue(obj, el.dataset.path);
        if (val === undefined || val === null) return;

        if (el.dataset.array === 'lines' && Array.isArray(val)) {
            el.value = val.join('\n');
        } else if (el.dataset.array === 'keyval' && typeof val === 'object' && !Array.isArray(val)) {
            el.value = Object.entries(val).map(([k, v]) => `${k}=${v}`).join('\n');
        } else if (el.type === 'checkbox') {
            el.checked = !!val;
        } else if (el.tagName === 'SELECT') {
            const strVal = typeof val === 'object' ? Object.keys(val)[0] : String(val);
            for (const opt of el.options) {
                if (opt.value === strVal) { el.value = strVal; break; }
            }
        } else if (el.type === 'number') {
            el.value = val;
        } else {
            el.value = val || '';
        }
    });
    // Render blocked patterns list
    renderBlockedPatterns(obj.guardcode?.blocked_patterns || []);
    // Show/hide sanitize section
    updateSanitizeVisibility();
}

function collectForm() {
    const obj = currentSettings ? JSON.parse(JSON.stringify(currentSettings)) : {};

    document.querySelectorAll('[data-path]').forEach(el => {
        let val;
        if (el.dataset.array === 'lines') {
            val = el.value.split('\n').map(s => s.trim()).filter(s => s);
        } else if (el.dataset.array === 'keyval') {
            val = {};
            el.value.split('\n').forEach(line => {
                const eq = line.indexOf('=');
                if (eq > 0) val[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
            });
        } else if (el.type === 'checkbox') {
            val = el.checked;
        } else if (el.type === 'number') {
            val = el.value === '' ? 0 : Number(el.value);
        } else if (el.tagName === 'SELECT') {
            val = el.value;
        } else {
            val = el.value;
        }
        setNestedValue(obj, el.dataset.path, val);
    });

    // Collect blocked patterns from dynamic list
    if (!obj.guardcode) obj.guardcode = {};
    const patternInputs = document.querySelectorAll('#blocked-patterns-list .pattern-input');
    obj.guardcode.blocked_patterns = [];
    patternInputs.forEach(input => {
        const v = input.value.trim();
        if (v) obj.guardcode.blocked_patterns.push(v);
    });

    return obj;
}

async function saveSettings() {
    const settings = collectForm();
    try {
        const result = await invoke('save_settings', { settings });
        if (result.saved) {
            showSaveStatus(T[currentLang]?.['settings.saved'] || 'Settings saved successfully');
            currentSettings = settings;
        } else {
            showSaveStatus(T[currentLang]?.['settings.failed'] || 'Save failed', true);
        }
    } catch (e) {
        showSaveStatus('Error: ' + e, true);
    }
}

function showSaveStatus(msg, isError) {
    const el = document.getElementById('save-status');
    el.textContent = msg;
    el.className = 'save-status ' + (isError ? 'error' : 'success');
    setTimeout(() => { el.textContent = ''; el.className = 'save-status'; }, 4000);
}

// ═══════════════════════════════════════════════════════════════
// GUARDCODE — blocked patterns dynamic list
// ═══════════════════════════════════════════════════════════════

function renderBlockedPatterns(patterns) {
    const list = document.getElementById('blocked-patterns-list');
    if (!list) return;
    list.innerHTML = '';
    for (const p of patterns) {
        addPatternRow(list, p);
    }
}

function addPatternRow(list, value) {
    const row = document.createElement('div');
    row.className = 'pattern-row';
    row.innerHTML = `<input type="text" class="pattern-input" value="${esc(value || '')}" placeholder="e.g. password, secret_key">
        <button class="btn-sm btn-danger btn-remove-pattern" type="button">&times;</button>`;
    row.querySelector('.btn-remove-pattern').addEventListener('click', () => row.remove());
    list.appendChild(row);
    if (!value) row.querySelector('.pattern-input').focus();
}

document.getElementById('btn-add-pattern')?.addEventListener('click', () => {
    const list = document.getElementById('blocked-patterns-list');
    if (list) addPatternRow(list, '');
});

// Show/hide sanitize LLM section based on action_on_block
function updateSanitizeVisibility() {
    const sel = document.getElementById('gc-action-select');
    const section = document.getElementById('gc-sanitize-section');
    if (sel && section) {
        section.style.display = sel.value === 'SanitizeLlm' ? '' : 'none';
    }
}
document.getElementById('gc-action-select')?.addEventListener('change', updateSanitizeVisibility);

// ═══════════════════════════════════════════════════════════════
// UTILITY
// ═══════════════════════════════════════════════════════════════

function esc(str) {
    const d = document.createElement('div');
    d.textContent = str || '';
    return d.innerHTML;
}

function getNestedValue(obj, path) {
    return path.split('.').reduce((o, k) => (o && o[k] !== undefined) ? o[k] : undefined, obj);
}

function setNestedValue(obj, path, value) {
    const keys = path.split('.');
    let current = obj;
    for (let i = 0; i < keys.length - 1; i++) {
        if (!current[keys[i]] || typeof current[keys[i]] !== 'object') {
            current[keys[i]] = {};
        }
        current = current[keys[i]];
    }
    current[keys[keys.length - 1]] = value;
}

// ═══════════════════════════════════════════════════════════════
// RESOURCE MONITORING — CPU, Memory, Pool (polling every 5s)
// ═══════════════════════════════════════════════════════════════

async function loadResources() {
    try {
        const res = await invoke('get_system_resources');
        const cpuEl = document.getElementById('resource-cpu');
        const memEl = document.getElementById('resource-mem');
        const poolEl = document.getElementById('resource-pool');

        if (cpuEl) cpuEl.textContent = res.cpu_percent != null ? res.cpu_percent.toFixed(1) + '%' : '-';
        if (memEl && res.memory) {
            memEl.textContent = res.memory.percent != null ? res.memory.percent.toFixed(1) + '%' : '-';
            memEl.title = `${res.memory.used_mb || 0} / ${res.memory.total_mb || 0} MB`;
        }
        if (poolEl && res.daemon) {
            poolEl.textContent = `${res.daemon.pool_active || 0} active`;
        } else if (poolEl) {
            poolEl.textContent = '-';
        }
    } catch (_) {
        // silently ignore — daemon may not be running
    }
}

// ═══════════════════════════════════════════════════════════════
// DAEMON SETTINGS — load/save daemon config
// ═══════════════════════════════════════════════════════════════

async function loadDaemonSettings() {
    try {
        const cfg = await invoke('get_daemon_settings');
        const el = (id) => document.getElementById(id);
        if (el('daemon-auto-start')) el('daemon-auto-start').checked = cfg.auto_start || false;
        if (el('daemon-max-conn')) el('daemon-max-conn').value = cfg.pool_max_connections || 50;
        if (el('daemon-idle-timeout')) el('daemon-idle-timeout').value = cfg.pool_max_idle_secs || 1800;
        if (el('daemon-prune-interval')) el('daemon-prune-interval').value = cfg.prune_interval_secs || 300;
        if (el('daemon-cross-gossip')) el('daemon-cross-gossip').checked = cfg.gossip_cross_project || false;
        if (el('daemon-capture-workers')) el('daemon-capture-workers').value = cfg.capture_workers || 2;
        if (el('daemon-capture-queue')) el('daemon-capture-queue').value = cfg.capture_queue_capacity || 100;
    } catch (e) {
        console.error('Daemon settings load error:', e);
    }
}

async function saveDaemonSettings() {
    try {
        const settings = {
            auto_start: document.getElementById('daemon-auto-start')?.checked || false,
            pool_max_connections: parseInt(document.getElementById('daemon-max-conn')?.value) || 50,
            pool_max_idle_secs: parseInt(document.getElementById('daemon-idle-timeout')?.value) || 1800,
            prune_interval_secs: parseInt(document.getElementById('daemon-prune-interval')?.value) || 300,
            gossip_cross_project: document.getElementById('daemon-cross-gossip')?.checked || false,
            capture_workers: parseInt(document.getElementById('daemon-capture-workers')?.value) || 2,
            capture_queue_capacity: parseInt(document.getElementById('daemon-capture-queue')?.value) || 100,
        };
        await invoke('save_daemon_settings', { settings });
        showSaveStatus(T[currentLang]?.['settings.saved'] || 'Settings saved successfully');
    } catch (e) {
        showSaveStatus('Error: ' + e, true);
    }
}

// Load daemon settings when Settings tab opens (General includes daemon now)
document.getElementById('btn-save-daemon-settings')?.addEventListener('click', saveDaemonSettings);

// ═══════════════════════════════════════════════════════════════
// USER PROFILE TAB
// ═══════════════════════════════════════════════════════════════

let currentProfile = null;

async function loadProfile() {
    if (!projectHash || agentId === 'default') return;
    try {
        currentProfile = await invoke('get_user_profile', { projectHash, agentId });
        populateProfileForm(currentProfile);
    } catch (e) {
        console.error('Profile load error:', e);
    }
}

function populateProfileForm(p) {
    document.getElementById('profile-name').value = p.identity?.name || '';
    document.getElementById('profile-role').value = p.identity?.role || 'user';
    document.getElementById('profile-relationship').value = p.identity?.relationship || 'user';
    document.getElementById('profile-verbosity').value = p.preferences?.verbosity || 'normal';
    document.getElementById('profile-technical').value = p.preferences?.technical_level || 'intermediate';
    document.getElementById('profile-emoji').checked = p.preferences?.emoji_usage || false;
    renderProfileRules(p.context_rules || []);
}

function renderProfileRules(rules) {
    const list = document.getElementById('profile-rules-list');
    if (!list) return;
    list.innerHTML = '';
    rules.forEach((rule, idx) => {
        const item = document.createElement('div');
        item.className = 'pattern-item';
        item.style.cssText = 'display:flex;align-items:center;gap:8px;padding:4px 0';
        item.innerHTML = `<span style="flex:1;font-size:13px">${rule}</span><button class="btn-sm btn-danger" title="Remove this rule" data-idx="${idx}" style="padding:2px 8px;font-size:11px">&times;</button>`;
        item.querySelector('button').addEventListener('click', () => {
            if (currentProfile && currentProfile.context_rules) {
                currentProfile.context_rules.splice(idx, 1);
                renderProfileRules(currentProfile.context_rules);
            }
        });
        list.appendChild(item);
    });
}

function collectProfileForm() {
    return {
        created_at: currentProfile?.created_at || new Date().toISOString(),
        updated_at: new Date().toISOString(),
        identity: {
            role: document.getElementById('profile-role').value,
            relationship: document.getElementById('profile-relationship').value,
            name: document.getElementById('profile-name').value || null,
        },
        preferences: {
            language: currentProfile?.preferences?.language || 'en',
            verbosity: document.getElementById('profile-verbosity').value,
            emoji_usage: document.getElementById('profile-emoji').checked,
            technical_level: document.getElementById('profile-technical').value,
        },
        context_rules: currentProfile?.context_rules || [],
    };
}

async function saveProfile() {
    const profile = collectProfileForm();
    const statusEl = document.getElementById('profile-status');
    try {
        await invoke('save_user_profile', { projectHash, agentId, profile });
        currentProfile = profile;
        if (statusEl) { statusEl.textContent = 'Saved'; setTimeout(() => statusEl.textContent = '', 2000); }
    } catch (e) {
        if (statusEl) statusEl.textContent = 'Error: ' + e;
        console.error('Profile save error:', e);
    }
}

document.getElementById('btn-save-profile')?.addEventListener('click', saveProfile);

document.getElementById('btn-add-rule')?.addEventListener('click', () => {
    const input = document.getElementById('profile-new-rule');
    const rule = input?.value?.trim();
    if (!rule || rule.length < 5) return;
    if (!currentProfile) currentProfile = collectProfileForm();
    if (!currentProfile.context_rules) currentProfile.context_rules = [];
    if (!currentProfile.context_rules.includes(rule)) {
        currentProfile.context_rules.push(rule);
        renderProfileRules(currentProfile.context_rules);
    }
    if (input) input.value = '';
});

// Load profile when Profile sub-tab is activated
document.querySelector('[data-stab="stab-profile"]')?.addEventListener('click', () => {
    if (!currentProfile) loadProfile();
});

// ═══════════════════════════════════════════════════════════════
// BACKUP TAB
// ═══════════════════════════════════════════════════════════════

async function loadBackupSettings() {
    try {
        const config = await invoke('get_backup_settings');
        document.getElementById('backup-path').value = config.backup_path || '';
        document.getElementById('backup-schedule').value = config.schedule || 'manual';
        document.getElementById('backup-retention').value = config.retention_count || 5;
        document.getElementById('backup-hour').value = config.auto_backup_hour || 3;
    } catch (e) {
        console.error('Backup settings error:', e);
    }
}

async function saveBackupSettings() {
    const statusEl = document.getElementById('backup-settings-status');
    try {
        await invoke('save_backup_settings', {
            settings: {
                backup_path: document.getElementById('backup-path').value,
                schedule: document.getElementById('backup-schedule').value,
                retention_count: parseInt(document.getElementById('backup-retention').value) || 5,
                last_backup_at: null,
                auto_backup_hour: parseInt(document.getElementById('backup-hour').value) || 3,
            }
        });
        if (statusEl) { statusEl.textContent = 'Saved'; setTimeout(() => statusEl.textContent = '', 2000); }
    } catch (e) {
        if (statusEl) statusEl.textContent = 'Error: ' + e;
    }
}

async function loadBackupHistory() {
    try {
        const backups = await invoke('list_backups');
        const tbody = document.getElementById('backup-history-body');
        if (!tbody) return;
        tbody.innerHTML = '';
        if (!backups || backups.length === 0) {
            tbody.innerHTML = '<tr><td colspan="4" style="text-align:center;color:var(--text-secondary)">No backups found</td></tr>';
            return;
        }
        for (const b of backups) {
            const tr = document.createElement('tr');
            const sizeMB = (b.size_bytes / (1024 * 1024)).toFixed(2);
            tr.innerHTML = `<td>${b.date || '-'}</td><td>${b.agent_id}</td><td>${sizeMB} MB</td>
                <td><button class="btn-sm" onclick="restoreBackup('${b.path}','${b.agent_id}')" title="Restore this backup to the agent's database">Restore</button>
                <button class="btn-sm btn-danger" onclick="deleteBackup('${b.path}')" title="Permanently delete this backup file">Delete</button></td>`;
            tbody.appendChild(tr);
        }
    } catch (e) {
        console.error('Backup history error:', e);
    }
}

async function triggerBackup() {
    const statusEl = document.getElementById('backup-status');
    const agentSelect = document.getElementById('backup-agent-select');
    const selectedAgent = agentSelect?.value;
    if (statusEl) statusEl.textContent = 'Creating backup...';
    try {
        const result = await invoke('trigger_backup', {
            projectHash,
            agentId: selectedAgent === '__all__' ? null : selectedAgent,
        });
        if (statusEl) statusEl.textContent = `Backup complete (${result.count} agent(s))`;
        loadBackupHistory();
    } catch (e) {
        if (statusEl) statusEl.textContent = 'Error: ' + e;
    }
}

window.restoreBackup = async function(backupPath, backupAgentId) {
    if (!confirm(`Restore backup for agent "${backupAgentId}"? This will overwrite the current database.`)) return;
    try {
        await invoke('restore_backup', { backupPath, projectHash, agentId: backupAgentId });
        alert('Restore complete!');
    } catch (e) {
        alert('Restore error: ' + e);
    }
};

window.deleteBackup = async function(backupPath) {
    if (!confirm('Delete this backup permanently?')) return;
    try {
        await invoke('delete_backup', { backupPath });
        loadBackupHistory();
    } catch (e) {
        alert('Delete error: ' + e);
    }
};

document.getElementById('btn-save-backup-settings')?.addEventListener('click', saveBackupSettings);
document.getElementById('btn-trigger-backup')?.addEventListener('click', triggerBackup);

// Load backup data when Backup sub-tab is activated
document.querySelector('[data-stab="stab-backup"]')?.addEventListener('click', () => {
    loadBackupSettings();
    loadBackupHistory();
    // Populate agent selector
    const sel = document.getElementById('backup-agent-select');
    if (sel && sel.options.length <= 1 && projectHash) {
        invoke('list_agents', { projectHash }).then(agents => {
            for (const a of agents) {
                const opt = document.createElement('option');
                opt.value = a.id;
                opt.textContent = `${a.name} (${a.id})`;
                sel.appendChild(opt);
            }
        }).catch(() => {});
    }
});

// ═══════════════════════════════════════════════════════════════
// REINDEX
// ═══════════════════════════════════════════════════════════════

document.getElementById('btn-reindex')?.addEventListener('click', async () => {
    if (!confirm('Reindex all thread embeddings? This may take a moment.')) return;
    const resetWeights = document.getElementById('reindex-reset-weights')?.checked || false;
    const btn = document.getElementById('btn-reindex');
    if (btn) btn.textContent = 'Reindexing...';
    try {
        const result = await invoke('reindex_agent', { projectHash, agentId: threadAgentId || agentId, resetWeights });
        alert(`Reindex complete: ${result.reindexed}/${result.total} threads updated`);
    } catch (e) {
        alert('Reindex error: ' + e);
    }
    if (btn) btn.textContent = 'Reindex';
});

// ═══════════════════════════════════════════════════════════════
// UPDATES
// ═══════════════════════════════════════════════════════════════

document.getElementById('btn-check-update')?.addEventListener('click', async () => {
    const statusEl = document.getElementById('update-status');
    if (statusEl) statusEl.textContent = 'Checking...';
    try {
        const result = await invoke('check_update');
        document.getElementById('current-version').textContent = result.current_version;
        document.getElementById('latest-version').textContent = result.latest_version;
        if (result.update_available) {
            if (statusEl) statusEl.textContent = `Update available: v${result.latest_version} (${result.os})`;
        } else {
            if (statusEl) statusEl.textContent = 'You are up to date.';
        }
    } catch (e) {
        if (statusEl) statusEl.textContent = 'Error: ' + e;
    }
});

// ═══════════════════════════════════════════════════════════════
// THREAD DETAIL VIEWER + DELETE + PURGE
// ═══════════════════════════════════════════════════════════════

let currentDetailThreadId = null;
let currentDetailAgentId = null;

// Make thread rows clickable (delegated on both thread-body and agent-threads-body)
document.getElementById('thread-body')?.addEventListener('click', (e) => {
    const tr = e.target.closest('tr[data-thread-id]');
    if (tr) openThreadDetail(tr.dataset.threadId, threadAgentId || agentId);
});

document.getElementById('agent-threads-body')?.addEventListener('click', (e) => {
    const tr = e.target.closest('tr[data-thread-id]');
    if (tr) {
        const selectedCard = document.querySelector('.org-card.selected');
        const aid = selectedCard?.dataset.agentId || agentId;
        openThreadDetail(tr.dataset.threadId, aid);
    }
});

async function openThreadDetail(threadId, agentIdForDetail) {
    if (!projectHash || !threadId) return;
    currentDetailThreadId = threadId;
    currentDetailAgentId = agentIdForDetail;

    try {
        const data = await invoke('get_thread_detail', {
            projectHash,
            agentId: agentIdForDetail,
            threadId,
        });

        const t = data.thread;
        document.getElementById('thread-detail-title').textContent = t.title || 'Thread Detail';
        document.getElementById('thread-detail-meta').innerHTML = `
            <strong>Status:</strong> ${esc(t.status)} &nbsp;
            <strong>Weight:</strong> ${(t.weight || 0).toFixed(2)} &nbsp;
            <strong>Importance:</strong> ${(t.importance || 0).toFixed(2)} &nbsp;
            <strong>Activations:</strong> ${t.activation_count || 0}<br>
            <strong>Topics:</strong> ${(t.topics || []).join(', ') || '-'} &nbsp;
            <strong>Labels:</strong> ${(t.labels || []).join(', ') || '-'}<br>
            <strong>Created:</strong> ${t.created_at || '-'} &nbsp;
            <strong>Last active:</strong> ${t.last_active || '-'}<br>
            ${t.summary ? `<strong>Summary:</strong> ${esc(t.summary)}` : ''}
        `;

        // Messages
        const msgsEl = document.getElementById('thread-detail-messages');
        if (data.messages && data.messages.length > 0) {
            msgsEl.innerHTML = data.messages.map(m => `
                <div style="margin-bottom:8px; padding-bottom:6px; border-bottom:1px solid var(--border)">
                    <span style="color:var(--accent)">[${esc(m.source_type)}]</span>
                    <span style="color:var(--text-dim); font-size:11px">${m.timestamp || ''}</span>
                    <pre style="white-space:pre-wrap; margin:4px 0 0; font-size:11px">${esc(m.content)}</pre>
                </div>
            `).join('');
        } else {
            msgsEl.innerHTML = '<p style="color:var(--text-dim)">No messages</p>';
        }

        // Bridges
        const bridgesEl = document.getElementById('thread-detail-bridges');
        if (data.bridges && data.bridges.length > 0) {
            bridgesEl.innerHTML = `<table class="table" style="font-size:12px"><thead><tr>
                <th>Source</th><th>Target</th><th>Relation</th><th>Weight</th><th>Reason</th>
            </tr></thead><tbody>${data.bridges.map(b => `<tr>
                <td>${esc(b.source_id?.substring(0,8) || '')}</td>
                <td>${esc(b.target_id?.substring(0,8) || '')}</td>
                <td>${esc(b.relation_type || '')}</td>
                <td>${(b.weight || 0).toFixed(2)}</td>
                <td>${esc(b.reason || '')}</td>
            </tr>`).join('')}</tbody></table>`;
        } else {
            bridgesEl.innerHTML = '<p style="color:var(--text-dim)">No bridges</p>';
        }

        document.getElementById('modal-thread-detail').classList.add('open');
    } catch (e) {
        console.error('Thread detail error:', e);
        alert('Error loading thread: ' + e);
    }
}

document.getElementById('btn-close-thread-detail')?.addEventListener('click', () => {
    document.getElementById('modal-thread-detail').classList.remove('open');
});
document.getElementById('modal-thread-detail')?.addEventListener('click', (e) => {
    if (e.target === e.currentTarget) e.currentTarget.classList.remove('open');
});

document.getElementById('btn-delete-thread')?.addEventListener('click', async () => {
    if (!currentDetailThreadId || !currentDetailAgentId) return;
    if (!confirm(`Delete thread "${currentDetailThreadId.substring(0, 12)}..." and all associated bridges?`)) return;

    try {
        const result = await invoke('delete_thread', {
            projectHash,
            agentId: currentDetailAgentId,
            threadId: currentDetailThreadId,
        });
        document.getElementById('modal-thread-detail').classList.remove('open');
        alert(`Thread deleted. ${result.bridges_deleted || 0} bridge(s) removed.`);
        loadDashboard();
        loadThreads();
    } catch (e) {
        alert('Delete error: ' + e);
    }
});

// Purge agent DB — exposed as global
async function purgeAgentDb(agentIdToPurge) {
    if (!projectHash || !agentIdToPurge) return;
    if (!confirm(`PURGE all data for agent "${agentIdToPurge}"? This deletes ALL threads, messages, and bridges. This cannot be undone.`)) return;

    try {
        await invoke('purge_agent_db', { projectHash, agentId: agentIdToPurge });
        alert(`Agent "${agentIdToPurge}" DB purged.`);
        loadDashboard();
        loadAgents();
    } catch (e) {
        alert('Purge error: ' + e);
    }
}
window.purgeAgentDb = purgeAgentDb;

// ═══════════════════════════════════════════════════════════════
// GRAPH — DAG Visualization (Force-directed)
// ═══════════════════════════════════════════════════════════════

let graphNodes = [];
let graphEdges = [];
let graphTransform = { x: 0, y: 0, scale: 1 };
let graphDrag = null;
let graphHoveredNode = null;
let graphSelectedNode = null;
let graphAnimFrame = null;
let graphSearchMatches = null;  // F1: Set of matching node IDs, null = no active search
let graphColorMode = 'status';  // F2/F5: 'status' | 'topic' | 'label' | 'origin' | 'decay'
let graphFocusIndex = -1;       // F10: keyboard nav index
let graphRAF = null;            // F0: rAF throttle handle
let graphRawThreads = [];       // F4: raw data for client-side filtering
let graphRawBridges = [];       // F4: raw data for client-side filtering

const GRAPH_COLORS = {
    active: '#4caf50',
    suspended: '#ff9800',
    archived: '#666',
    edge_default: 'rgba(160,220,255,0.5)',
    edge_highlight: 'rgba(180,230,255,0.95)',
};

const RELATION_COLORS = {
    'ChildOf': '#8ad4ff',
    'SiblingOf': '#b5e86c',
    'RelatedTo': '#ffd56c',
    'Supersedes': '#d68cff',
};

async function loadGraph() {
    if (!projectHash) {
        document.getElementById('graph-stats').textContent = 'Select a project to view the memory graph.';
        return;
    }
    const agentSel = document.getElementById('graph-agent-select');
    if (!agentSel.value) {
        try {
            const agents = await invoke('list_agents', { projectHash });
            agentSel.innerHTML = '';
            for (const a of agents) {
                const opt = document.createElement('option');
                opt.value = a.id;
                opt.textContent = a.name || a.id;
                agentSel.appendChild(opt);
            }
        } catch (e) { console.error('Graph agent list:', e); }
    }
    const aid = agentSel.value;
    if (!aid) return;

    try {
        // F4: use 'all' when non-active statuses are checked
        const needAll = document.querySelector('.graph-filter-status[value="suspended"]:checked') ||
            document.querySelector('.graph-filter-status[value="archived"]:checked');
        const statusFilter = needAll ? 'all' : 'active';
        const [threads, bridges] = await Promise.all([
            invoke('get_threads', { projectHash, agentId: aid, statusFilter }),
            invoke('get_bridges', { projectHash, agentId: aid }),
        ]);
        graphRawThreads = threads;
        graphRawBridges = bridges;
        applyGraphFilters();
    } catch (e) {
        console.error('Graph load error:', e);
        document.getElementById('graph-stats').textContent = 'Error: ' + e;
    }
}

function applyGraphFilters() {
    buildGraph(graphRawThreads, graphRawBridges);
    forceLayout(150);
    centerGraph();
    drawGraph();
    renderGraphLegend();
    document.getElementById('graph-stats').textContent =
        `${graphNodes.length} threads, ${graphEdges.length} bridges`;
}

function buildGraph(threads, bridges) {
    const idSet = new Set(threads.map(t => t.id));

    // Build edges from bridges (only where both endpoints exist)
    // F8: include status and use_count from enriched backend
    graphEdges = bridges
        .filter(b => idSet.has(b.source_id) && idSet.has(b.target_id))
        .map(b => ({
            source: b.source_id,
            target: b.target_id,
            weight: b.weight || 0,
            relation: b.relation_type || 'RelatedTo',
            reason: b.reason || '',
            status: (b.status || 'Active'),
            useCount: b.use_count || 0,
        }));

    // F4: Client-side multi-criteria filtering
    const checkedStatuses = new Set();
    document.querySelectorAll('.graph-filter-status:checked').forEach(cb => checkedStatuses.add(cb.value));
    const impMin = (parseInt(document.getElementById('graph-filter-imp-min')?.value) || 0) / 100;
    const impMax = (parseInt(document.getElementById('graph-filter-imp-max')?.value) || 100) / 100;
    const checkedOrigins = new Set();
    document.querySelectorAll('.graph-filter-origin:checked').forEach(cb => checkedOrigins.add(cb.value));
    const hasOther = checkedOrigins.has('other');
    const knownOrigins = new Set(['prompt', 'file_read', 'file_write', 'task']);
    const minBridges = parseInt(document.getElementById('graph-filter-min-bridges')?.value) || 0;

    // Pre-compute bridge counts per thread
    const bridgeCounts = {};
    graphEdges.forEach(e => {
        bridgeCounts[e.source] = (bridgeCounts[e.source] || 0) + 1;
        bridgeCounts[e.target] = (bridgeCounts[e.target] || 0) + 1;
    });

    let filtered = threads.filter(t => {
        const st = (t.status || 'active').toLowerCase();
        if (!checkedStatuses.has(st)) return false;
        const imp = t.importance || 0.5;
        if (imp < impMin || imp > impMax) return false;
        const orig = (t.origin_type || 'prompt').toLowerCase();
        if (checkedOrigins.size > 0) {
            const matchesKnown = checkedOrigins.has(orig);
            const matchesOther = hasOther && !knownOrigins.has(orig);
            if (!matchesKnown && !matchesOther) return false;
        }
        if (minBridges > 0 && (bridgeCounts[t.id] || 0) < minBridges) return false;
        return true;
    });

    // Build nodes with random initial positions
    // F3: include concepts, summary, createdAt, injectionStats
    graphNodes = filtered.map(t => ({
        id: t.id,
        title: t.title || 'Untitled',
        status: (t.status || 'active').toLowerCase(),
        importance: t.importance || 0.5,
        weight: t.weight || 0.5,
        topics: t.topics || [],
        labels: t.labels || [],
        concepts: t.concepts || [],
        summary: t.summary || '',
        origin: t.origin_type || 'prompt',
        lastActive: t.last_active || null,
        createdAt: t.created_at || null,
        injectionStats: t.injection_stats || null,
        x: (Math.random() - 0.5) * 600,
        y: (Math.random() - 0.5) * 400,
        vx: 0, vy: 0,
        radius: 6 + (t.importance || 0.5) * 10,
    }));

    // Re-filter edges to match filtered nodes
    const nodeIds = new Set(graphNodes.map(n => n.id));
    graphEdges = graphEdges.filter(e => nodeIds.has(e.source) && nodeIds.has(e.target));
}

// Barnes-Hut quadtree for O(n log n) repulsion force calculation.
// For <100 nodes, falls back to brute-force O(n²) (quadtree overhead not worth it).
class QuadTree {
    constructor(x, y, w, h) {
        this.x = x; this.y = y; this.w = w; this.h = h;
        this.body = null;    // leaf: single node
        this.mass = 0;       // total mass (node count) in this quad
        this.cx = 0;         // center of mass x
        this.cy = 0;         // center of mass y
        this.children = null; // NW, NE, SW, SE (lazy allocation)
    }

    insert(node) {
        if (this.mass === 0) {
            this.body = node;
            this.mass = 1;
            this.cx = node.x;
            this.cy = node.y;
            return;
        }

        // Update center of mass
        const totalMass = this.mass + 1;
        this.cx = (this.cx * this.mass + node.x) / totalMass;
        this.cy = (this.cy * this.mass + node.y) / totalMass;
        this.mass = totalMass;

        // If leaf with existing body, subdivide
        if (this.body) {
            this._subdivide();
            this._insertChild(this.body);
            this.body = null;
        }

        this._insertChild(node);
    }

    _subdivide() {
        const hw = this.w / 2, hh = this.h / 2;
        this.children = [
            new QuadTree(this.x, this.y, hw, hh),             // NW
            new QuadTree(this.x + hw, this.y, hw, hh),        // NE
            new QuadTree(this.x, this.y + hh, hw, hh),        // SW
            new QuadTree(this.x + hw, this.y + hh, hw, hh),   // SE
        ];
    }

    _insertChild(node) {
        if (!this.children) this._subdivide();
        const mx = this.x + this.w / 2, my = this.y + this.h / 2;
        const idx = (node.x < mx ? 0 : 1) + (node.y < my ? 0 : 2);
        this.children[idx].insert(node);
    }

    computeForce(node, theta, repulsion, minDist) {
        if (this.mass === 0) return;

        const dx = this.cx - node.x;
        const dy = this.cy - node.y;
        let dist = Math.sqrt(dx * dx + dy * dy) || 1;

        // If this is a leaf with a single body (and it's not the same node)
        if (this.body) {
            if (this.body === node) return;
            if (dist < minDist) dist = minDist;
            const force = repulsion / (dist * dist);
            node.vx -= (dx / dist) * force;
            node.vy -= (dy / dist) * force;
            return;
        }

        // Barnes-Hut criterion: if quad width / distance < theta, treat as single body
        if (this.w / dist < theta) {
            if (dist < minDist) dist = minDist;
            const force = (repulsion * this.mass) / (dist * dist);
            node.vx -= (dx / dist) * force;
            node.vy -= (dy / dist) * force;
            return;
        }

        // Otherwise recurse into children
        if (this.children) {
            for (const child of this.children) {
                child.computeForce(node, theta, repulsion, minDist);
            }
        }
    }
}

function forceLayout(iterations) {
    const nodes = graphNodes;
    const edges = graphEdges;
    if (nodes.length === 0) return;

    const nodeMap = {};
    nodes.forEach(n => { nodeMap[n.id] = n; });

    const repulsion = 3000;
    const attraction = 0.01;
    const damping = 0.9;
    const minDist = 30;
    const useBarnesHut = nodes.length >= 100;
    const theta = 0.9;

    for (let iter = 0; iter < iterations; iter++) {
        if (useBarnesHut) {
            // Compute bounds with 10% margin, rebuild each iteration
            let bx0 = Infinity, by0 = Infinity, bx1 = -Infinity, by1 = -Infinity;
            for (const n of nodes) {
                if (n.x < bx0) bx0 = n.x;
                if (n.y < by0) by0 = n.y;
                if (n.x > bx1) bx1 = n.x;
                if (n.y > by1) by1 = n.y;
            }
            const margin = Math.max(bx1 - bx0, by1 - by0) * 0.1 || 100;
            const qt = new QuadTree(bx0 - margin, by0 - margin,
                (bx1 - bx0) + margin * 2, (by1 - by0) + margin * 2);
            for (const n of nodes) qt.insert(n);
            for (const n of nodes) qt.computeForce(n, theta, repulsion, minDist);
        } else {
            // Brute-force O(n²) for small graphs
            for (let i = 0; i < nodes.length; i++) {
                for (let j = i + 1; j < nodes.length; j++) {
                    const a = nodes[i], b = nodes[j];
                    let dx = b.x - a.x, dy = b.y - a.y;
                    let dist = Math.sqrt(dx * dx + dy * dy) || 1;
                    if (dist < minDist) dist = minDist;
                    const force = repulsion / (dist * dist);
                    const fx = (dx / dist) * force;
                    const fy = (dy / dist) * force;
                    a.vx -= fx; a.vy -= fy;
                    b.vx += fx; b.vy += fy;
                }
            }
        }

        // Attraction (edges) — O(E)
        for (const e of edges) {
            const a = nodeMap[e.source], b = nodeMap[e.target];
            if (!a || !b) continue;
            const dx = b.x - a.x, dy = b.y - a.y;
            const dist = Math.sqrt(dx * dx + dy * dy) || 1;
            const force = dist * attraction * (0.5 + e.weight);
            const fx = (dx / dist) * force;
            const fy = (dy / dist) * force;
            a.vx += fx; a.vy += fy;
            b.vx -= fx; b.vy -= fy;
        }

        // Center gravity — O(N)
        for (const n of nodes) {
            n.vx -= n.x * 0.001;
            n.vy -= n.y * 0.001;
        }

        // Apply velocity + damping — O(N)
        for (const n of nodes) {
            n.vx *= damping;
            n.vy *= damping;
            n.x += n.vx;
            n.y += n.vy;
        }
    }
}

function centerGraph() {
    if (graphNodes.length === 0) return;
    const container = document.getElementById('graph-container');
    const rect = container.getBoundingClientRect();
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const n of graphNodes) {
        if (n.x < minX) minX = n.x;
        if (n.y < minY) minY = n.y;
        if (n.x > maxX) maxX = n.x;
        if (n.y > maxY) maxY = n.y;
    }
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    const rangeX = maxX - minX + 100;
    const rangeY = maxY - minY + 100;
    const scale = Math.min(rect.width / rangeX, rect.height / rangeY, 2);
    graphTransform = {
        x: rect.width / 2 - cx * scale,
        y: rect.height / 2 - cy * scale,
        scale: scale,
    };
}

// ─── Canvas layer helpers (F0) ──────────────────────────────

function prepCanvas(canvasId) {
    const canvas = document.getElementById(canvasId);
    if (!canvas) return null;
    const container = document.getElementById('graph-container');
    const rect = container.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null; // Tab not visible
    const dpr = window.devicePixelRatio || 1;
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    const ctx = canvas.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return { ctx, w: rect.width, h: rect.height };
}

function isOnScreen(sx, sy, margin, w, h) {
    return sx > -margin && sx < w + margin && sy > -margin && sy < h + margin;
}

// ─── Color helpers (F2/F5) ──────────────────────────────────

function hashToHSL(str) {
    let hash = 0;
    for (let i = 0; i < str.length; i++) hash = str.charCodeAt(i) + ((hash << 5) - hash);
    return `hsl(${Math.abs(hash) % 360}, 70%, 55%)`;
}

function getNodeColor(node) {
    switch (graphColorMode) {
        case 'topic':
            return node.topics.length > 0 ? hashToHSL(node.topics[0]) : '#666';
        case 'label':
            return node.labels.length > 0 ? hashToHSL(node.labels[0]) : '#666';
        case 'origin':
            return hashToHSL(node.origin || 'prompt');
        case 'decay': {
            const days = node.lastActive
                ? (Date.now() - new Date(node.lastActive).getTime()) / 86400000
                : 30;
            const sat = Math.max(0.2, 1 - days / 30);
            return `hsl(120, ${Math.round(sat * 100)}%, 50%)`;
        }
        default: // 'status'
            return GRAPH_COLORS[node.status] || GRAPH_COLORS.active;
    }
}

// ─── Layer 0: Edges ─────────────────────────────────────────

function drawEdgesLayer(layer) {
    layer = layer || prepCanvas('graph-canvas-edges');
    if (!layer) return;
    const { ctx, w, h } = layer;
    const { x: tx, y: ty, scale } = graphTransform;
    const showLabels = document.getElementById('graph-show-labels')?.checked;
    const showWeights = document.getElementById('graph-show-weights')?.checked;

    const nodeMap = {};
    graphNodes.forEach(n => { nodeMap[n.id] = n; });

    ctx.clearRect(0, 0, w, h);

    for (const e of graphEdges) {
        const a = nodeMap[e.source], b = nodeMap[e.target];
        if (!a || !b) continue;
        const ax = a.x * scale + tx, ay = a.y * scale + ty;
        const bx = b.x * scale + tx, by = b.y * scale + ty;

        // Off-screen culling: skip if both endpoints outside viewport
        if (!isOnScreen(ax, ay, 50, w, h) && !isOnScreen(bx, by, 50, w, h)) continue;

        const isHighlight = graphSelectedNode &&
            (e.source === graphSelectedNode.id || e.target === graphSelectedNode.id);

        // F8: Bridge status indicators — line dash pattern
        if (e.status === 'Weak') {
            ctx.setLineDash([4, 4]);
        } else if (e.status === 'Invalid') {
            ctx.setLineDash([2, 2]);
        } else {
            ctx.setLineDash([]);
        }

        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.lineTo(bx, by);
        ctx.strokeStyle = isHighlight
            ? GRAPH_COLORS.edge_highlight
            : (e.status === 'Invalid' ? '#f44336' : (RELATION_COLORS[e.relation] || GRAPH_COLORS.edge_default));
        ctx.lineWidth = isHighlight ? 2.5 : Math.max(1, e.weight * 3);
        ctx.globalAlpha = isHighlight ? 1 : 0.3 + e.weight * 0.7;
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;

        if (showLabels || showWeights) {
            const mx = (ax + bx) / 2, my = (ay + by) / 2;
            if (isOnScreen(mx, my, 20, w, h)) {
                ctx.font = '9px monospace';
                ctx.fillStyle = '#aaa';
                let edgeLabel = '';
                if (showLabels) edgeLabel += e.relation;
                if (showLabels && showWeights) edgeLabel += ' ';
                if (showWeights) edgeLabel += e.weight.toFixed(2);
                ctx.fillText(edgeLabel, mx + 2, my - 2);
            }
        }
    }
}

// ─── Layer 1: Nodes ─────────────────────────────────────────

function drawNodesLayer(layer) {
    layer = layer || prepCanvas('graph-canvas-nodes');
    if (!layer) return;
    const { ctx, w, h } = layer;
    const { x: tx, y: ty, scale } = graphTransform;
    const isDimming = graphSearchMatches !== null;

    ctx.clearRect(0, 0, w, h);

    for (const n of graphNodes) {
        const nx = n.x * scale + tx, ny = n.y * scale + ty;

        // Off-screen culling
        if (!isOnScreen(nx, ny, 120, w, h)) continue;

        const label = n.title.length > 22 ? n.title.substring(0, 20) + '..' : n.title;
        const impScale = 0.8 + (n.importance || 0.5) * 0.5;
        const fontSize = Math.max(9, 11 * Math.sqrt(scale) * impScale);
        ctx.font = `${fontSize}px sans-serif`;
        const tw = ctx.measureText(label).width;
        const padX = 8 * impScale, padY = 5 * impScale;
        const rw = tw + padX * 2;
        const rh = fontSize + padY * 2;

        // Store world-space dimensions for hit detection
        n._rw = rw / scale;
        n._rh = rh / scale;

        const rx = nx - rw / 2, ry = ny - rh / 2;
        const dimmed = isDimming && !graphSearchMatches.has(n.id);

        ctx.beginPath();
        ctx.roundRect(rx, ry, rw, rh, 4);
        ctx.fillStyle = getNodeColor(n);
        ctx.globalAlpha = dimmed ? 0.15 : (0.3 + n.weight * 0.7);
        ctx.fill();
        ctx.globalAlpha = 1;

        ctx.fillStyle = dimmed ? 'rgba(255,255,255,0.2)' : '#fff';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, nx, ny);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'alphabetic';
    }
}

// ─── Layer 2: Overlay (hover/selection highlights) ──────────

function drawOverlayLayer(layer) {
    layer = layer || prepCanvas('graph-canvas-overlay');
    if (!layer) return;
    const { ctx, w, h } = layer;
    const { x: tx, y: ty, scale } = graphTransform;

    ctx.clearRect(0, 0, w, h);

    const highlights = [];
    if (graphFocusIndex >= 0 && graphFocusIndex < graphNodes.length) {
        const fn = graphNodes[graphFocusIndex];
        if (fn !== graphSelectedNode && fn !== graphHoveredNode) {
            highlights.push({ node: fn, color: '#ffd740', width: 2 });
        }
    }
    if (graphHoveredNode) highlights.push({ node: graphHoveredNode, color: '#fff', width: 2 });
    if (graphSelectedNode) highlights.push({ node: graphSelectedNode, color: '#4fc3f7', width: 2.5 });

    for (const { node, color, width } of highlights) {
        const nx = node.x * scale + tx, ny = node.y * scale + ty;
        const rw = (node._rw || node.radius * 2) * scale;
        const rh = (node._rh || node.radius * 2) * scale;
        const rx = nx - rw / 2 - 2, ry = ny - rh / 2 - 2;

        ctx.beginPath();
        ctx.roundRect(rx, ry, rw + 4, rh + 4, 5);
        ctx.strokeStyle = color;
        ctx.lineWidth = width;
        ctx.stroke();
    }
}

// ─── Unified draw ───────────────────────────────────────────

function drawGraph() {
    const edgesLayer = prepCanvas('graph-canvas-edges');
    const nodesLayer = prepCanvas('graph-canvas-nodes');
    const overlayLayer = prepCanvas('graph-canvas-overlay');
    if (!edgesLayer || !nodesLayer || !overlayLayer) return;
    drawEdgesLayer(edgesLayer);
    drawNodesLayer(nodesLayer);
    drawOverlayLayer(overlayLayer);
}

function drawOverlayOnly() {
    if (graphRAF) return;
    graphRAF = requestAnimationFrame(() => {
        graphRAF = null;
        drawOverlayLayer();
    });
}

// ─── Graph interaction ──────────────────────────────────────

function graphScreenToWorld(sx, sy) {
    const { x: tx, y: ty, scale } = graphTransform;
    return { x: (sx - tx) / scale, y: (sy - ty) / scale };
}

function graphNodeAt(wx, wy) {
    for (let i = graphNodes.length - 1; i >= 0; i--) {
        const n = graphNodes[i];
        const hw = (n._rw || n.radius * 2) / 2 + 4;
        const hh = (n._rh || n.radius * 2) / 2 + 4;
        if (Math.abs(n.x - wx) <= hw && Math.abs(n.y - wy) <= hh) return n;
    }
    return null;
}

const graphCanvas = document.getElementById('graph-canvas-overlay');
if (graphCanvas) {
    graphCanvas.addEventListener('mousedown', (e) => {
        const rect = graphCanvas.getBoundingClientRect();
        const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
        const { x: wx, y: wy } = graphScreenToWorld(sx, sy);
        const node = graphNodeAt(wx, wy);
        if (node) {
            graphDrag = { node, offsetX: node.x - wx, offsetY: node.y - wy, moved: false };
        } else {
            graphDrag = { pan: true, startX: sx, startY: sy, startTx: graphTransform.x, startTy: graphTransform.y };
        }
    });

    graphCanvas.addEventListener('mousemove', (e) => {
        const rect = graphCanvas.getBoundingClientRect();
        const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
        const { x: wx, y: wy } = graphScreenToWorld(sx, sy);

        if (graphDrag) {
            if (graphDrag.pan) {
                graphTransform.x = graphDrag.startTx + (sx - graphDrag.startX);
                graphTransform.y = graphDrag.startTy + (sy - graphDrag.startY);
            } else if (graphDrag.node) {
                graphDrag.node.x = wx + graphDrag.offsetX;
                graphDrag.node.y = wy + graphDrag.offsetY;
                graphDrag.moved = true;
            }
            drawGraph();
            return;
        }

        const node = graphNodeAt(wx, wy);
        if (node !== graphHoveredNode) {
            graphHoveredNode = node;
            graphCanvas.style.cursor = node ? 'pointer' : 'grab';
            drawOverlayOnly(); // F0: only redraw overlay layer on hover
        }

        // Tooltip on hover
        const tooltip = document.getElementById('graph-tooltip');
        if (node) {
            const bridgeCount = graphEdges.filter(e => e.source === node.id || e.target === node.id).length;
            tooltip.innerHTML =
                `<strong style="color:#fff">${esc(node.title)}</strong><br>` +
                `<span style="color:${GRAPH_COLORS[node.status] || '#6cf'}">● ${node.status}</span>` +
                ` &nbsp; Importance: <strong>${node.importance.toFixed(2)}</strong><br>` +
                `Weight: ${node.weight.toFixed(2)} &nbsp; Bridges: ${bridgeCount}` +
                (node.topics.length > 0 ? `<br><span style="color:#8ad4ff">Topics:</span> ${esc(node.topics.slice(0, 5).join(', '))}` : '');
            const containerRect = graphCanvas.parentElement.getBoundingClientRect();
            let tipX = e.clientX - containerRect.left + 14;
            let tipY = e.clientY - containerRect.top + 14;
            tooltip.style.display = 'block';
            if (tipX + 280 > containerRect.width) tipX = tipX - 300;
            if (tipY + 120 > containerRect.height) tipY = tipY - 130;
            tooltip.style.left = tipX + 'px';
            tooltip.style.top = tipY + 'px';
        } else {
            tooltip.style.display = 'none';
        }
    });

    graphCanvas.addEventListener('mouseup', (e) => {
        if (graphDrag && graphDrag.node && !graphDrag.moved) {
            graphSelectedNode = graphDrag.node;
            showGraphDetail(graphDrag.node);
            drawGraph();
        }
        graphDrag = null;
    });

    graphCanvas.addEventListener('mouseleave', () => {
        graphDrag = null;
        graphHoveredNode = null;
        document.getElementById('graph-tooltip').style.display = 'none';
        drawOverlayOnly();
    });

    graphCanvas.addEventListener('wheel', (e) => {
        e.preventDefault();
        const rect = graphCanvas.getBoundingClientRect();
        const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
        const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
        const newScale = Math.max(0.1, Math.min(5, graphTransform.scale * factor));
        graphTransform.x = sx - (sx - graphTransform.x) * (newScale / graphTransform.scale);
        graphTransform.y = sy - (sy - graphTransform.y) * (newScale / graphTransform.scale);
        graphTransform.scale = newScale;
        drawGraph();
    }, { passive: false });
}

const ORIGIN_ICONS = { prompt: '\ud83d\udcdd', file_read: '\ud83d\udcc4', file_write: '\u270f\ufe0f', task: '\u2699\ufe0f', fetch: '\ud83c\udf10', response: '\ud83d\udcac', command: '\u2328\ufe0f', split: '\u2702\ufe0f', reactivation: '\ud83d\udd04' };

function showGraphDetail(node) {
    const panel = document.getElementById('graph-detail');
    panel.style.display = 'block';
    document.getElementById('graph-detail-title').textContent = node.title;

    let html = '';

    // Status + origin
    const originIcon = ORIGIN_ICONS[node.origin] || '\ud83d\udcdd';
    html += `<span style="color:${GRAPH_COLORS[node.status] || '#6cf'}">\u25cf ${node.status}</span>`;
    html += ` &nbsp; ${originIcon} ${esc(node.origin)}<br>`;

    // Weight + importance
    html += `<strong>Weight:</strong> ${node.weight.toFixed(2)} &nbsp; `;
    html += `<strong>Importance:</strong> ${node.importance.toFixed(2)}<br>`;

    // Age
    if (node.createdAt || node.lastActive) {
        const now = Date.now();
        if (node.createdAt) {
            const daysCreated = Math.floor((now - new Date(node.createdAt).getTime()) / 86400000);
            html += `<span style="color:#888">Created ${daysCreated}d ago</span>`;
        }
        if (node.lastActive) {
            const daysActive = Math.floor((now - new Date(node.lastActive).getTime()) / 86400000);
            html += ` &nbsp; <span style="color:#888">Active ${daysActive}d ago</span>`;
        }
        html += '<br>';
    }

    // Labels as colored chips
    if (node.labels.length > 0) {
        html += '<strong>Labels:</strong> ';
        html += node.labels.map(l =>
            `<span style="display:inline-block;padding:1px 6px;border-radius:8px;font-size:10px;margin:1px 2px;background:${hashToHSL(l)};color:#fff">${esc(l)}</span>`
        ).join('');
        html += '<br>';
    }

    // Topics
    if (node.topics.length > 0) {
        html += `<strong>Topics:</strong> <span style="color:#8ad4ff">${esc(node.topics.join(', '))}</span><br>`;
    }

    // Concepts
    if (node.concepts.length > 0) {
        html += `<strong>Concepts:</strong> <span style="color:#b5e86c">${esc(node.concepts.join(', '))}</span><br>`;
    }

    // Summary
    if (node.summary) {
        html += `<div style="margin:4px 0;padding:4px 6px;background:rgba(255,255,255,0.05);border-radius:3px;font-style:italic;color:#ccc">${esc(node.summary)}</div>`;
    }

    // Injection stats
    if (node.injectionStats) {
        const is = node.injectionStats;
        const injCount = is.injection_count || 0;
        const usedCount = is.used_count || 0;
        const ratio = injCount > 0 ? ((usedCount / injCount) * 100).toFixed(0) : '0';
        html += `<strong>Injection:</strong> ${injCount} inj, ${usedCount} used (${ratio}%)<br>`;
    }

    // Bridges
    const connected = graphEdges.filter(e => e.source === node.id || e.target === node.id);
    if (connected.length > 0) {
        const nodeMap = {};
        graphNodes.forEach(n => { nodeMap[n.id] = n; });
        html += `<strong>Bridges (${connected.length}):</strong><br>`;
        html += connected.map(e => {
            const otherId = e.source === node.id ? e.target : e.source;
            const other = nodeMap[otherId];
            const otherTitle = other ? other.title.substring(0, 30) : otherId.substring(0, 8);
            const statusTag = e.status !== 'Active' ? ` <span style="color:#888;font-size:10px">[${e.status}]</span>` : '';
            return `<span style="color:${RELATION_COLORS[e.relation] || '#6cf'}">${esc(e.relation)}</span>${statusTag} ` +
                `\u2192 ${esc(otherTitle)} (${e.weight.toFixed(2)})`;
        }).join('<br>');
        html += '<br>';
    }

    // Actions
    html += '<div style="margin-top:8px;display:flex;gap:4px">';
    if (node.status === 'active') {
        html += `<button class="btn-sm" onclick="graphThreadAction('${node.id}','suspended')">Suspend</button>`;
    } else {
        html += `<button class="btn-sm" onclick="graphThreadAction('${node.id}','active')">Activate</button>`;
    }
    html += '</div>';

    document.getElementById('graph-detail-content').innerHTML = html;
}

async function graphThreadAction(threadId, newStatus) {
    try {
        const agentId = document.getElementById('graph-agent-select')?.value;
        if (!agentId) return;
        await invoke('update_thread_status', { projectHash, agentId, threadId, status: newStatus });
        loadGraph(); // Refresh after status change
    } catch (e) {
        console.error('Thread action failed:', e);
    }
}
window.graphThreadAction = graphThreadAction;

document.getElementById('btn-graph-detail-close')?.addEventListener('click', () => {
    document.getElementById('graph-detail').style.display = 'none';
    graphSelectedNode = null;
    drawGraph();
});

document.getElementById('btn-graph-refresh')?.addEventListener('click', loadGraph);
document.getElementById('graph-agent-select')?.addEventListener('change', loadGraph);
document.getElementById('graph-show-labels')?.addEventListener('change', drawGraph);
document.getElementById('graph-show-weights')?.addEventListener('change', drawGraph);

// F4: Filter panel toggle + handlers
document.getElementById('btn-graph-filters-toggle')?.addEventListener('click', () => {
    const panel = document.getElementById('graph-filters');
    if (panel) panel.style.display = panel.style.display === 'none' ? 'block' : 'none';
});

// Status filter change requires reload (may need 'all' status_filter)
document.querySelectorAll('.graph-filter-status').forEach(cb => {
    cb.addEventListener('change', () => loadGraph());
});

// Client-side filters: re-apply without reload
document.querySelectorAll('.graph-filter-origin').forEach(cb => {
    cb.addEventListener('change', () => { if (graphRawThreads.length) applyGraphFilters(); });
});

['graph-filter-imp-min', 'graph-filter-imp-max'].forEach(id => {
    document.getElementById(id)?.addEventListener('input', () => {
        const min = (parseInt(document.getElementById('graph-filter-imp-min')?.value) || 0) / 100;
        const max = (parseInt(document.getElementById('graph-filter-imp-max')?.value) || 100) / 100;
        document.getElementById('graph-filter-imp-label').textContent = `${min.toFixed(2)} \u2014 ${max.toFixed(2)}`;
        if (graphRawThreads.length) applyGraphFilters();
    });
});

document.getElementById('graph-filter-min-bridges')?.addEventListener('change', () => {
    if (graphRawThreads.length) applyGraphFilters();
});

document.getElementById('btn-graph-filter-reset')?.addEventListener('click', () => {
    document.querySelectorAll('.graph-filter-status').forEach(cb => { cb.checked = cb.value === 'active'; });
    document.querySelectorAll('.graph-filter-origin').forEach(cb => { cb.checked = true; });
    const impMin = document.getElementById('graph-filter-imp-min');
    const impMax = document.getElementById('graph-filter-imp-max');
    if (impMin) impMin.value = 0;
    if (impMax) impMax.value = 100;
    document.getElementById('graph-filter-imp-label').textContent = '0.00 \u2014 1.00';
    const minB = document.getElementById('graph-filter-min-bridges');
    if (minB) minB.value = 0;
    loadGraph();
});

// Legend — adapts to active color mode
function renderGraphLegend() {
    const legend = document.getElementById('graph-legend');
    if (!legend) return;
    const showLegend = document.getElementById('graph-show-legend')?.checked;
    if (!showLegend) { legend.style.display = 'none'; return; }
    legend.style.display = 'block';
    let html = '<strong style="color:#fff;font-size:12px">Legend</strong><br>';

    // Node coloring legend depends on active mode
    html += '<span style="color:#888">— Nodes —</span><br>';
    if (graphColorMode === 'status') {
        html += `<span style="color:${GRAPH_COLORS.active}">●</span> Active &nbsp; `;
        html += `<span style="color:${GRAPH_COLORS.suspended}">●</span> Suspended &nbsp; `;
        html += `<span style="color:${GRAPH_COLORS.archived}">●</span> Archived<br>`;
    } else if (graphColorMode === 'decay') {
        html += '<span style="color:hsl(120,100%,50%)">●</span> Recent &nbsp; ';
        html += '<span style="color:hsl(120,50%,50%)">●</span> Aging &nbsp; ';
        html += '<span style="color:hsl(120,20%,50%)">●</span> Stale<br>';
    } else {
        // Topic/Label/Origin: show unique values from current graph
        const vals = new Set();
        for (const n of graphNodes) {
            if (graphColorMode === 'topic' && n.topics.length > 0) vals.add(n.topics[0]);
            else if (graphColorMode === 'label' && n.labels.length > 0) vals.add(n.labels[0]);
            else if (graphColorMode === 'origin') vals.add(n.origin || 'prompt');
        }
        let count = 0;
        for (const v of vals) {
            if (count++ >= 8) { html += '...'; break; }
            html += `<span style="color:${hashToHSL(v)}">●</span> ${esc(v)} &nbsp; `;
        }
        html += '<br>';
    }

    html += '<span style="color:#888">— Edges —</span><br>';
    for (const [rel, color] of Object.entries(RELATION_COLORS)) {
        html += `<span style="color:${color}">━</span> ${rel} &nbsp; `;
    }
    html += '<br><span style="color:#888;font-size:10px">Node size ∝ importance</span>';
    legend.innerHTML = html;
}
document.getElementById('graph-show-legend')?.addEventListener('change', renderGraphLegend);

// F4: Zoom controls (+, -, fit)
function graphZoom(factor) {
    const container = document.getElementById('graph-container');
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const cx = rect.width / 2, cy = rect.height / 2;
    const newScale = Math.max(0.1, Math.min(5, graphTransform.scale * factor));
    graphTransform.x = cx - (cx - graphTransform.x) * (newScale / graphTransform.scale);
    graphTransform.y = cy - (cy - graphTransform.y) * (newScale / graphTransform.scale);
    graphTransform.scale = newScale;
    drawGraph();
}
document.getElementById('btn-graph-zoom-in')?.addEventListener('click', () => graphZoom(1.3));
document.getElementById('btn-graph-zoom-out')?.addEventListener('click', () => graphZoom(1 / 1.3));
document.getElementById('btn-graph-zoom-fit')?.addEventListener('click', () => {
    centerGraph();
    drawGraph();
});

// ─── F1: Search with contextual focus ───────────────────────

function graphSearchUpdate(query) {
    graphSearchQuery = query.trim().toLowerCase();
    if (!graphSearchQuery) {
        graphSearchMatches = null;
    } else {
        graphSearchMatches = new Set();
        for (const n of graphNodes) {
            const hay = (n.title + ' ' + n.topics.join(' ') + ' ' + n.labels.join(' ')).toLowerCase();
            if (hay.includes(graphSearchQuery)) graphSearchMatches.add(n.id);
        }
    }
    drawGraph();
}

function graphSearchFocusFirst() {
    if (!graphSearchMatches || graphSearchMatches.size === 0) return;
    const firstId = graphSearchMatches.values().next().value;
    const node = graphNodes.find(n => n.id === firstId);
    if (!node) return;
    // Center on the matched node
    const container = document.getElementById('graph-container');
    const rect = container.getBoundingClientRect();
    graphTransform.x = rect.width / 2 - node.x * graphTransform.scale;
    graphTransform.y = rect.height / 2 - node.y * graphTransform.scale;
    graphSelectedNode = node;
    showGraphDetail(node);
    drawGraph();
}

const graphSearchInput = document.getElementById('graph-search');
if (graphSearchInput) {
    graphSearchInput.addEventListener('input', (e) => graphSearchUpdate(e.target.value));
    graphSearchInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') { e.preventDefault(); graphSearchFocusFirst(); }
        if (e.key === 'Escape') {
            e.preventDefault();
            graphSearchInput.value = '';
            graphSearchUpdate('');
            graphSearchInput.blur();
        }
        e.stopPropagation(); // prevent keyboard nav while typing in search
    });
}

// ─── F2/F5: Color mode (radio buttons) ─────────────────────

document.querySelectorAll('input[name="graph-color-mode"]').forEach(radio => {
    radio.addEventListener('change', (e) => {
        graphColorMode = e.target.value;
        drawGraph();
        renderGraphLegend();
    });
});

// ─── F10: Keyboard navigation ───────────────────────────────

document.addEventListener('keydown', (e) => {
    // Only handle when graph panel is visible
    const graphPanel = document.getElementById('graph');
    if (!graphPanel || !graphPanel.classList.contains('active')) return;
    // Don't capture when typing in inputs (except Escape)
    const tag = document.activeElement?.tagName;
    if ((tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') && e.key !== 'Escape') return;

    switch (e.key) {
        case 'Tab': {
            e.preventDefault();
            if (graphNodes.length === 0) return;
            graphFocusIndex = e.shiftKey
                ? (graphFocusIndex <= 0 ? graphNodes.length - 1 : graphFocusIndex - 1)
                : (graphFocusIndex + 1) % graphNodes.length;
            const focusNode = graphNodes[graphFocusIndex];
            graphSelectedNode = focusNode;
            showGraphDetail(focusNode);
            // Center on focused node
            const container = document.getElementById('graph-container');
            const rect = container.getBoundingClientRect();
            const sx = focusNode.x * graphTransform.scale + graphTransform.x;
            const sy = focusNode.y * graphTransform.scale + graphTransform.y;
            if (sx < 50 || sx > rect.width - 50 || sy < 50 || sy > rect.height - 50) {
                graphTransform.x = rect.width / 2 - focusNode.x * graphTransform.scale;
                graphTransform.y = rect.height / 2 - focusNode.y * graphTransform.scale;
            }
            drawGraph();
            break;
        }
        case 'Escape':
            graphSelectedNode = null;
            graphFocusIndex = -1;
            document.getElementById('graph-detail').style.display = 'none';
            if (graphSearchInput && graphSearchInput.value) {
                graphSearchInput.value = '';
                graphSearchUpdate('');
            }
            drawGraph();
            break;
        case '+':
        case '=':
            graphZoom(1.3);
            break;
        case '-':
            graphZoom(1 / 1.3);
            break;
        case 'ArrowUp':
            e.preventDefault();
            graphTransform.y += 40;
            drawGraph();
            break;
        case 'ArrowDown':
            e.preventDefault();
            graphTransform.y -= 40;
            drawGraph();
            break;
        case 'ArrowLeft':
            graphTransform.x += 40;
            drawGraph();
            break;
        case 'ArrowRight':
            graphTransform.x -= 40;
            drawGraph();
            break;
        case 'Enter':
            if (graphSelectedNode) showGraphDetail(graphSelectedNode);
            break;
        case 'f':
        case 'F':
            if (graphSearchInput) {
                e.preventDefault();
                graphSearchInput.focus();
            }
            break;
    }
});

// ─── ResizeObserver: redraw on container resize ─────────────
if (typeof ResizeObserver !== 'undefined') {
    const _graphContainer = document.getElementById('graph-container');
    if (_graphContainer) {
        let _graphResizeTimer;
        new ResizeObserver(() => {
            clearTimeout(_graphResizeTimer);
            _graphResizeTimer = setTimeout(() => {
                if (document.getElementById('graph').classList.contains('active') && graphNodes.length > 0) {
                    drawGraph();
                }
            }, 150);
        }).observe(_graphContainer);
    }
}

// ═══════════════════════════════════════════════════════════════
// F7: Minimap — small overview canvas with viewport rectangle
// ═══════════════════════════════════════════════════════════════

let minimapEnabled = true;

function renderMinimap() {
    const mmCanvas = document.getElementById('graph-minimap');
    if (!mmCanvas) return;
    if (!minimapEnabled || graphNodes.length === 0) {
        mmCanvas.style.display = 'none';
        return;
    }
    mmCanvas.style.display = 'block';
    const ctx = mmCanvas.getContext('2d');
    const W = mmCanvas.width, H = mmCanvas.height;
    ctx.clearRect(0, 0, W, H);

    // Compute bounding box of all nodes
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const n of graphNodes) {
        if (n.x < minX) minX = n.x;
        if (n.x > maxX) maxX = n.x;
        if (n.y < minY) minY = n.y;
        if (n.y > maxY) maxY = n.y;
    }
    const pad = 40;
    minX -= pad; minY -= pad; maxX += pad; maxY += pad;
    const worldW = maxX - minX || 1;
    const worldH = maxY - minY || 1;
    const scaleX = W / worldW;
    const scaleY = H / worldH;
    const mmScale = Math.min(scaleX, scaleY);
    const offX = (W - worldW * mmScale) / 2;
    const offY = (H - worldH * mmScale) / 2;

    // Draw edges as thin lines
    ctx.globalAlpha = 0.2;
    ctx.strokeStyle = '#4af';
    ctx.lineWidth = 0.5;
    for (const e of graphEdges) {
        const src = graphNodes.find(n => n.id === e.source);
        const tgt = graphNodes.find(n => n.id === e.target);
        if (!src || !tgt) continue;
        ctx.beginPath();
        ctx.moveTo(offX + (src.x - minX) * mmScale, offY + (src.y - minY) * mmScale);
        ctx.lineTo(offX + (tgt.x - minX) * mmScale, offY + (tgt.y - minY) * mmScale);
        ctx.stroke();
    }
    ctx.globalAlpha = 1;

    // Draw nodes as colored dots
    for (const n of graphNodes) {
        const mx = offX + (n.x - minX) * mmScale;
        const my = offY + (n.y - minY) * mmScale;
        ctx.fillStyle = getNodeColor(n);
        ctx.beginPath();
        ctx.arc(mx, my, Math.max(1.5, n.radius * mmScale * 0.3), 0, Math.PI * 2);
        ctx.fill();
    }

    // Draw viewport rectangle
    const container = document.getElementById('graph-container');
    if (container) {
        const rect = container.getBoundingClientRect();
        // Visible world bounds
        const vx0 = (0 - graphTransform.x) / graphTransform.scale;
        const vy0 = (0 - graphTransform.y) / graphTransform.scale;
        const vx1 = (rect.width - graphTransform.x) / graphTransform.scale;
        const vy1 = (rect.height - graphTransform.y) / graphTransform.scale;

        const rx = offX + (vx0 - minX) * mmScale;
        const ry = offY + (vy0 - minY) * mmScale;
        const rw = (vx1 - vx0) * mmScale;
        const rh = (vy1 - vy0) * mmScale;

        ctx.strokeStyle = 'rgba(255,255,255,0.7)';
        ctx.lineWidth = 1.5;
        ctx.strokeRect(
            Math.max(0, Math.min(rx, W)),
            Math.max(0, Math.min(ry, H)),
            Math.min(rw, W - Math.max(0, rx)),
            Math.min(rh, H - Math.max(0, ry))
        );
    }

    // Store transform for click-to-navigate
    mmCanvas._mmTransform = { minX, minY, mmScale, offX, offY };
}

// Minimap click/drag to navigate main view
(function setupMinimapInteraction() {
    const mmCanvas = document.getElementById('graph-minimap');
    if (!mmCanvas) return;
    let dragging = false;

    function navigateToMinimap(e) {
        const t = mmCanvas._mmTransform;
        if (!t) return;
        const rect = mmCanvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        // Convert minimap coords to world coords
        const wx = (mx - t.offX) / t.mmScale + t.minX;
        const wy = (my - t.offY) / t.mmScale + t.minY;
        // Center the main view on this world point
        const container = document.getElementById('graph-container');
        if (!container) return;
        const cr = container.getBoundingClientRect();
        graphTransform.x = cr.width / 2 - wx * graphTransform.scale;
        graphTransform.y = cr.height / 2 - wy * graphTransform.scale;
        drawGraph();
    }

    mmCanvas.addEventListener('mousedown', (e) => {
        e.stopPropagation();
        dragging = true;
        navigateToMinimap(e);
    });
    mmCanvas.addEventListener('mousemove', (e) => {
        if (dragging) { e.stopPropagation(); navigateToMinimap(e); }
    });
    window.addEventListener('mouseup', () => { dragging = false; });
})();

// Toggle minimap
document.getElementById('graph-show-minimap')?.addEventListener('change', (e) => {
    minimapEnabled = e.target.checked;
    renderMinimap();
});

// Hook minimap into drawGraph via override (non-invasive)
{
    const _origDrawGraph = drawGraph;
    drawGraph = function() {
        _origDrawGraph();
        renderMinimap();
    };
}

// ═══════════════════════════════════════════════════════════════
// F9: Time slider — dual-range date filter on last_active
// ═══════════════════════════════════════════════════════════════

let graphTimeFilterActive = false;
let graphTimeDates = [];  // sorted epoch ms array from graphNodes

function initTimeSlider() {
    if (graphNodes.length === 0) {
        document.getElementById('graph-time-slider').style.display = 'none';
        return;
    }
    // Collect all dates (lastActive or createdAt)
    graphTimeDates = graphNodes
        .map(n => n.lastActive || n.createdAt)
        .filter(Boolean)
        .map(d => new Date(d).getTime())
        .filter(d => !isNaN(d))
        .sort((a, b) => a - b);

    if (graphTimeDates.length < 2) {
        document.getElementById('graph-time-slider').style.display = 'none';
        return;
    }
    document.getElementById('graph-time-slider').style.display = 'flex';

    const minSlider = document.getElementById('graph-time-min');
    const maxSlider = document.getElementById('graph-time-max');
    minSlider.value = 0;
    maxSlider.value = 1000;
    graphTimeFilterActive = false;
    updateTimeLabels();
    updateTimeRangeBar();
}

function getTimeRange() {
    if (graphTimeDates.length < 2) return { min: 0, max: Date.now() };
    const first = graphTimeDates[0];
    const last = graphTimeDates[graphTimeDates.length - 1];
    const minVal = parseInt(document.getElementById('graph-time-min')?.value || '0');
    const maxVal = parseInt(document.getElementById('graph-time-max')?.value || '1000');
    return {
        min: first + (last - first) * (minVal / 1000),
        max: first + (last - first) * (maxVal / 1000),
    };
}

function formatShortDate(epoch) {
    const d = new Date(epoch);
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: '2-digit' });
}

function updateTimeLabels() {
    const range = getTimeRange();
    const minLabel = document.getElementById('graph-time-min-label');
    const maxLabel = document.getElementById('graph-time-max-label');
    if (minLabel) minLabel.textContent = formatShortDate(range.min);
    if (maxLabel) maxLabel.textContent = formatShortDate(range.max);
}

function updateTimeRangeBar() {
    const bar = document.getElementById('graph-time-range');
    if (!bar) return;
    const minVal = parseInt(document.getElementById('graph-time-min')?.value || '0');
    const maxVal = parseInt(document.getElementById('graph-time-max')?.value || '1000');
    bar.style.left = (minVal / 10) + '%';
    bar.style.width = ((maxVal - minVal) / 10) + '%';
}

function applyTimeFilter() {
    const range = getTimeRange();
    const minVal = parseInt(document.getElementById('graph-time-min')?.value || '0');
    const maxVal = parseInt(document.getElementById('graph-time-max')?.value || '1000');
    graphTimeFilterActive = (minVal > 0 || maxVal < 1000);

    if (!graphTimeFilterActive) {
        for (const n of graphNodes) n._timeHidden = false;
    } else {
        for (const n of graphNodes) {
            const epoch = new Date(n.lastActive || n.createdAt || 0).getTime();
            n._timeHidden = isNaN(epoch) || epoch < range.min || epoch > range.max;
        }
    }
    drawGraph();
}

// Hook time filter into draw layers via override (non-invasive)
{
    const _origDrawEdgesLayer = drawEdgesLayer;
    const _origDrawNodesLayer = drawNodesLayer;

    drawEdgesLayer = function(layer) {
        if (!graphTimeFilterActive) return _origDrawEdgesLayer(layer);
        // Temporarily swap graphNodes/graphEdges with filtered versions
        const hiddenIds = new Set(graphNodes.filter(n => n._timeHidden).map(n => n.id));
        const savedNodes = graphNodes;
        const savedEdges = graphEdges;
        graphNodes = savedNodes.filter(n => !n._timeHidden);
        graphEdges = savedEdges.filter(e => !hiddenIds.has(e.source) && !hiddenIds.has(e.target));
        _origDrawEdgesLayer(layer);
        graphNodes = savedNodes;
        graphEdges = savedEdges;
    };

    drawNodesLayer = function(layer) {
        if (!graphTimeFilterActive) return _origDrawNodesLayer(layer);
        const savedNodes = graphNodes;
        graphNodes = savedNodes.filter(n => !n._timeHidden);
        _origDrawNodesLayer(layer);
        graphNodes = savedNodes;
    };
}

// Slider event listeners
['graph-time-min', 'graph-time-max'].forEach(id => {
    document.getElementById(id)?.addEventListener('input', () => {
        const minSlider = document.getElementById('graph-time-min');
        const maxSlider = document.getElementById('graph-time-max');
        if (parseInt(minSlider.value) > parseInt(maxSlider.value)) {
            if (id === 'graph-time-min') minSlider.value = maxSlider.value;
            else maxSlider.value = minSlider.value;
        }
        updateTimeLabels();
        updateTimeRangeBar();
        applyTimeFilter();
    });
});

document.getElementById('btn-graph-time-reset')?.addEventListener('click', () => {
    document.getElementById('graph-time-min').value = 0;
    document.getElementById('graph-time-max').value = 1000;
    updateTimeLabels();
    updateTimeRangeBar();
    applyTimeFilter();
});

// Initialize time slider when graph is loaded
{
    const _origBuildGraph = buildGraph;
    buildGraph = function(threads, bridges) {
        _origBuildGraph(threads, bridges);
        initTimeSlider();
    };
}

// ═══════════════════════════════════════════════════════════════
// F11: Export — PNG, JSON, SVG
// ═══════════════════════════════════════════════════════════════

function exportGraphPNG() {
    const container = document.getElementById('graph-container');
    if (!container || graphNodes.length === 0) return;
    const edgesC = document.getElementById('graph-canvas-edges');
    const nodesC = document.getElementById('graph-canvas-nodes');
    const overlayC = document.getElementById('graph-canvas-overlay');
    if (!edgesC || !nodesC || !overlayC) return;

    // Merge 3 layers onto a temp canvas
    const w = edgesC.width, h = edgesC.height;
    const merged = document.createElement('canvas');
    merged.width = w;
    merged.height = h;
    const ctx = merged.getContext('2d');
    ctx.fillStyle = '#0f0f23';
    ctx.fillRect(0, 0, w, h);
    ctx.drawImage(edgesC, 0, 0);
    ctx.drawImage(nodesC, 0, 0);
    ctx.drawImage(overlayC, 0, 0);

    // Download
    const link = document.createElement('a');
    link.download = 'memory-graph.png';
    link.href = merged.toDataURL('image/png');
    link.click();
}

function exportGraphJSON() {
    if (graphNodes.length === 0) return;
    const data = {
        nodes: graphNodes.map(n => ({
            id: n.id, title: n.title, status: n.status,
            importance: n.importance, weight: n.weight,
            topics: n.topics, labels: n.labels, concepts: n.concepts,
            origin: n.origin, lastActive: n.lastActive,
            x: Math.round(n.x), y: Math.round(n.y),
        })),
        edges: graphEdges.map(e => ({
            source: e.source, target: e.target,
            relation: e.relation, weight: e.weight,
            status: e.status,
        })),
        meta: {
            exported: new Date().toISOString(),
            nodeCount: graphNodes.length,
            edgeCount: graphEdges.length,
        }
    };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const link = document.createElement('a');
    link.download = 'memory-graph.json';
    link.href = URL.createObjectURL(blob);
    link.click();
    URL.revokeObjectURL(link.href);
}

function exportGraphSVG() {
    if (graphNodes.length === 0) return;
    const container = document.getElementById('graph-container');
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const w = Math.round(rect.width), h = Math.round(rect.height);
    const { x: tx, y: ty, scale } = graphTransform;

    let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">\n`;
    svg += `<rect width="${w}" height="${h}" fill="#0f0f23"/>\n`;

    // Edges
    svg += '<g class="edges">\n';
    for (const e of graphEdges) {
        const src = graphNodes.find(n => n.id === e.source);
        const tgt = graphNodes.find(n => n.id === e.target);
        if (!src || !tgt) continue;
        const x1 = tx + src.x * scale, y1 = ty + src.y * scale;
        const x2 = tx + tgt.x * scale, y2 = ty + tgt.y * scale;
        const color = typeof RELATION_COLORS !== 'undefined' && RELATION_COLORS[e.relation]
            ? RELATION_COLORS[e.relation]
            : 'rgba(160,220,255,0.5)';
        svg += `  <line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="${color}" stroke-width="1" opacity="0.6"/>\n`;
    }
    svg += '</g>\n';

    // Nodes
    svg += '<g class="nodes">\n';
    for (const n of graphNodes) {
        const cx = tx + n.x * scale, cy = ty + n.y * scale;
        const color = getNodeColor(n);
        const r = n.radius * scale;
        svg += `  <circle cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${Math.max(2, r).toFixed(1)}" fill="${color}" opacity="0.85"/>\n`;
        if (scale > 0.5) {
            const escaped = (n.title || '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
            svg += `  <text x="${(cx + r + 4).toFixed(1)}" y="${(cy + 3).toFixed(1)}" fill="#ddd" font-size="${Math.max(8, 11 * scale).toFixed(0)}" font-family="sans-serif">${escaped}</text>\n`;
        }
    }
    svg += '</g>\n';
    svg += '</svg>';

    const blob = new Blob([svg], { type: 'image/svg+xml' });
    const link = document.createElement('a');
    link.download = 'memory-graph.svg';
    link.href = URL.createObjectURL(blob);
    link.click();
    URL.revokeObjectURL(link.href);
}

// Export button event listeners
document.getElementById('btn-graph-export-png')?.addEventListener('click', exportGraphPNG);
document.getElementById('btn-graph-export-json')?.addEventListener('click', exportGraphJSON);
document.getElementById('btn-graph-export-svg')?.addEventListener('click', exportGraphSVG);

// ═══════════════════════════════════════════════════════════════
// F13: Combo nodes — group by label when threshold exceeded
// ═══════════════════════════════════════════════════════════════

const COMBO_THRESHOLD = 5;  // min threads per label to form a combo
let graphComboEnabled = false;
let graphComboGroups = new Map();  // label → { nodes: [], expanded: bool, x, y, id }
let graphComboActive = false;     // true when combos are currently applied

function computeComboGroups() {
    graphComboGroups.clear();
    if (!graphComboEnabled || graphNodes.length === 0) return;

    // Count threads per label
    const labelMap = new Map();
    for (const n of graphNodes) {
        for (const lab of (n.labels || [])) {
            if (!labelMap.has(lab)) labelMap.set(lab, []);
            labelMap.get(lab).push(n);
        }
    }

    // Only create combos for labels exceeding threshold
    for (const [label, nodes] of labelMap) {
        if (nodes.length >= COMBO_THRESHOLD) {
            // Average position of member nodes
            let cx = 0, cy = 0;
            for (const n of nodes) { cx += n.x; cy += n.y; }
            cx /= nodes.length; cy /= nodes.length;
            graphComboGroups.set(label, {
                id: '__combo__' + label,
                label,
                nodes,
                nodeIds: new Set(nodes.map(n => n.id)),
                expanded: false,
                x: cx, y: cy,
                _rw: 0, _rh: 0,
            });
        }
    }
}

function applyComboGrouping() {
    if (!graphComboEnabled || graphComboGroups.size === 0) {
        graphComboActive = false;
        return;
    }
    graphComboActive = true;

    // Collect all node IDs that belong to a collapsed combo
    const collapsedIds = new Set();
    for (const [, combo] of graphComboGroups) {
        if (!combo.expanded) {
            for (const id of combo.nodeIds) collapsedIds.add(id);
        }
    }

    // Replace collapsed nodes with combo nodes in graphNodes
    const keptNodes = graphNodes.filter(n => !collapsedIds.has(n.id));

    for (const [, combo] of graphComboGroups) {
        if (!combo.expanded) {
            keptNodes.push({
                id: combo.id,
                title: `${combo.label} (${combo.nodes.length})`,
                status: 'active',
                importance: 0.8,
                weight: 0.9,
                topics: [],
                labels: [combo.label],
                concepts: [],
                summary: '',
                origin: 'combo',
                lastActive: null,
                createdAt: null,
                injectionStats: null,
                x: combo.x, y: combo.y,
                vx: 0, vy: 0,
                radius: 12 + combo.nodes.length * 2,
                _isCombo: true,
                _comboLabel: combo.label,
                _comboCount: combo.nodes.length,
            });
        }
    }

    graphNodes = keptNodes;

    // Reroute edges: edges to/from collapsed nodes → point to combo node
    graphEdges = graphEdges.map(e => {
        let src = e.source, tgt = e.target;
        for (const [, combo] of graphComboGroups) {
            if (!combo.expanded) {
                if (combo.nodeIds.has(src)) src = combo.id;
                if (combo.nodeIds.has(tgt)) tgt = combo.id;
            }
        }
        // Skip internal edges (both endpoints in same combo)
        if (src === tgt) return null;
        return { ...e, source: src, target: tgt };
    }).filter(Boolean);

    // Deduplicate edges between same pair
    const edgeKey = e => e.source < e.target ? `${e.source}|${e.target}` : `${e.target}|${e.source}`;
    const seen = new Map();
    for (const e of graphEdges) {
        const k = edgeKey(e);
        if (!seen.has(k) || e.weight > seen.get(k).weight) seen.set(k, e);
    }
    graphEdges = [...seen.values()];
}

function toggleComboExpand(label) {
    const combo = graphComboGroups.get(label);
    if (!combo) return;
    combo.expanded = !combo.expanded;

    if (combo.expanded) {
        // Restore individual nodes with offset from combo position
        const angle = (2 * Math.PI) / combo.nodes.length;
        const radius = 30 + combo.nodes.length * 5;
        combo.nodes.forEach((n, i) => {
            n.x = combo.x + Math.cos(angle * i) * radius;
            n.y = combo.y + Math.sin(angle * i) * radius;
        });
    } else {
        // Recalculate combo position from average of members
        let cx = 0, cy = 0;
        for (const n of combo.nodes) { cx += n.x; cy += n.y; }
        combo.x = cx / combo.nodes.length;
        combo.y = cy / combo.nodes.length;
    }

    // Re-apply grouping from raw data
    if (graphRawThreads.length) applyGraphFilters();
}

// Hook combo into buildGraph via override (non-invasive)
{
    const _origBuildGraph = buildGraph;
    buildGraph = function(threads, bridges) {
        _origBuildGraph(threads, bridges);
        computeComboGroups();
        applyComboGrouping();
    };
}

// Hook combo node click into mouseup
{
    const _origGraphCanvas = document.getElementById('graph-canvas-overlay');
    if (_origGraphCanvas) {
        _origGraphCanvas.addEventListener('dblclick', (e) => {
            if (!graphComboActive) return;
            const rect = _origGraphCanvas.getBoundingClientRect();
            const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
            const { x: wx, y: wy } = graphScreenToWorld(sx, sy);
            const node = graphNodeAt(wx, wy);
            if (node && node._isCombo) {
                e.stopPropagation();
                toggleComboExpand(node._comboLabel);
            }
        });
    }
}

// Checkbox toggle
document.getElementById('graph-combo-labels')?.addEventListener('change', (e) => {
    graphComboEnabled = e.target.checked;
    if (graphRawThreads.length) applyGraphFilters();
});

// ═══════════════════════════════════════════════════════════════
// F14: Semantic zoom — LOD-based node rendering
// ═══════════════════════════════════════════════════════════════

// Override drawNodesLayer with LOD-aware version (non-invasive)
{
    const _origDrawNodesLayer = drawNodesLayer;

    drawNodesLayer = function(layer) {
        const scale = graphTransform.scale;

        // LOD 2 (0.8-1.5): existing behavior — no override needed
        if (scale >= 0.8 && scale <= 1.5) {
            return _origDrawNodesLayer(layer);
        }

        layer = layer || prepCanvas('graph-canvas-nodes');
        if (!layer) return;
        const { ctx, w, h } = layer;
        const { x: tx, y: ty } = graphTransform;
        const isDimming = graphSearchMatches !== null;

        ctx.clearRect(0, 0, w, h);

        for (const n of graphNodes) {
            const nx = n.x * scale + tx, ny = n.y * scale + ty;
            if (!isOnScreen(nx, ny, 120, w, h)) continue;

            const dimmed = isDimming && !graphSearchMatches.has(n.id);

            if (scale < 0.4) {
                // LOD 0: Simple colored circles, no text
                const r = Math.max(3, (n._isCombo ? 8 : 4) * Math.sqrt(scale));
                ctx.beginPath();
                ctx.arc(nx, ny, r, 0, Math.PI * 2);
                ctx.fillStyle = getNodeColor(n);
                ctx.globalAlpha = dimmed ? 0.15 : (0.4 + n.weight * 0.6);
                ctx.fill();
                ctx.globalAlpha = 1;
                // Store hit dimensions
                n._rw = r * 2 / scale;
                n._rh = r * 2 / scale;

            } else if (scale < 0.8) {
                // LOD 1: Rectangles with short title (8 chars)
                const label = n._isCombo
                    ? n.title
                    : (n.title.length > 8 ? n.title.substring(0, 7) + '\u2026' : n.title);
                const impScale = 0.8 + (n.importance || 0.5) * 0.5;
                const fontSize = Math.max(8, 9 * Math.sqrt(scale) * impScale);
                ctx.font = `${fontSize}px sans-serif`;
                const tw = ctx.measureText(label).width;
                const padX = 4 * impScale, padY = 3 * impScale;
                const rw = tw + padX * 2, rh = fontSize + padY * 2;
                n._rw = rw / scale;
                n._rh = rh / scale;
                const rx = nx - rw / 2, ry = ny - rh / 2;

                ctx.beginPath();
                ctx.roundRect(rx, ry, rw, rh, 3);
                ctx.fillStyle = getNodeColor(n);
                ctx.globalAlpha = dimmed ? 0.15 : (0.3 + n.weight * 0.7);
                ctx.fill();
                ctx.globalAlpha = 1;

                ctx.fillStyle = dimmed ? 'rgba(255,255,255,0.2)' : '#fff';
                ctx.textAlign = 'center';
                ctx.textBaseline = 'middle';
                ctx.fillText(label, nx, ny);
                ctx.textAlign = 'left';
                ctx.textBaseline = 'alphabetic';

            } else {
                // LOD 3 (scale > 1.5): Extended — full title + labels/topics subline
                const fullTitle = n._isCombo ? n.title : n.title;
                const impScale = 0.8 + (n.importance || 0.5) * 0.5;
                const fontSize = Math.max(10, 12 * Math.sqrt(scale) * impScale);
                const subFontSize = Math.max(8, fontSize * 0.7);
                ctx.font = `${fontSize}px sans-serif`;
                const tw = ctx.measureText(fullTitle).width;

                // Build subtitle from labels + topics
                const subs = [];
                if (n.labels && n.labels.length > 0) subs.push(n.labels.slice(0, 3).join(', '));
                if (n.topics && n.topics.length > 0) subs.push(n.topics.slice(0, 3).join(', '));
                const subtitle = subs.join(' | ');
                ctx.font = `${subFontSize}px sans-serif`;
                const stw = subtitle ? ctx.measureText(subtitle).width : 0;

                const padX = 10 * impScale, padY = 6 * impScale;
                const rw = Math.max(tw, stw) + padX * 2;
                const rh = fontSize + (subtitle ? subFontSize + 4 : 0) + padY * 2;
                n._rw = rw / scale;
                n._rh = rh / scale;
                const rx = nx - rw / 2, ry = ny - rh / 2;

                ctx.beginPath();
                ctx.roundRect(rx, ry, rw, rh, 5);
                ctx.fillStyle = getNodeColor(n);
                ctx.globalAlpha = dimmed ? 0.15 : (0.3 + n.weight * 0.7);
                ctx.fill();
                ctx.globalAlpha = 1;

                // Combo nodes: thicker border
                if (n._isCombo) {
                    ctx.strokeStyle = hashToHSL(n._comboLabel);
                    ctx.lineWidth = 2;
                    ctx.stroke();
                }

                // Title
                const textY = subtitle ? ny - subFontSize / 2 : ny;
                ctx.font = `${fontSize}px sans-serif`;
                ctx.fillStyle = dimmed ? 'rgba(255,255,255,0.2)' : '#fff';
                ctx.textAlign = 'center';
                ctx.textBaseline = 'middle';
                ctx.fillText(fullTitle, nx, textY);

                // Subtitle
                if (subtitle) {
                    ctx.font = `${subFontSize}px sans-serif`;
                    ctx.fillStyle = dimmed ? 'rgba(255,255,255,0.1)' : 'rgba(200,200,200,0.7)';
                    ctx.fillText(subtitle, nx, textY + fontSize * 0.7 + 2);
                }

                ctx.textAlign = 'left';
                ctx.textBaseline = 'alphabetic';
            }
        }
    };
}

// ═══════════════════════════════════════════════════════════════
// F15: Node glyphs — small icon badges on nodes
// ═══════════════════════════════════════════════════════════════

let glyphsEnabled = true;

function drawGlyphs() {
    if (!glyphsEnabled || graphNodes.length === 0) return;
    const canvas = document.getElementById('graph-canvas-overlay');
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const { x: tx, y: ty, scale } = graphTransform;
    const now = Date.now();

    for (const n of graphNodes) {
        if (n._timeHidden) continue;
        const sx = tx + n.x * scale;
        const sy = ty + n.y * scale;
        const r = n.radius * scale;
        if (r < 3) continue; // too small for glyphs
        const gs = Math.max(5, Math.min(8, r * 0.5)); // glyph size

        // Star (top-right): importance >= 0.8
        if (n.importance >= 0.8) {
            drawStarGlyph(ctx, sx + r - gs * 0.2, sy - r + gs * 0.2, gs);
        }
        // Clock (top-left): active in last 2 hours
        if (n.lastActive) {
            const age = now - new Date(n.lastActive).getTime();
            if (age < 7200000) { // 2 hours
                drawClockGlyph(ctx, sx - r + gs * 0.2, sy - r + gs * 0.2, gs);
            }
        }
        // Arrow in (bottom-left): injection_count > 5
        if (n.injectionStats && n.injectionStats.injection_count > 5) {
            drawArrowInGlyph(ctx, sx - r + gs * 0.2, sy + r - gs * 0.2, gs);
        }
        // Exclamation (bottom-right): no labels AND no concepts
        if ((!n.labels || n.labels.length === 0) && (!n.concepts || n.concepts.length === 0)) {
            drawExclamGlyph(ctx, sx + r - gs * 0.2, sy + r - gs * 0.2, gs);
        }
    }
}

function drawStarGlyph(ctx, cx, cy, s) {
    ctx.fillStyle = '#ffd700';
    ctx.beginPath();
    for (let i = 0; i < 5; i++) {
        const a = (i * 72 - 90) * Math.PI / 180;
        const ai = ((i * 72) + 36 - 90) * Math.PI / 180;
        const ox = cx + Math.cos(a) * s * 0.5;
        const oy = cy + Math.sin(a) * s * 0.5;
        const ix = cx + Math.cos(ai) * s * 0.2;
        const iy = cy + Math.sin(ai) * s * 0.2;
        if (i === 0) ctx.moveTo(ox, oy);
        else ctx.lineTo(ox, oy);
        ctx.lineTo(ix, iy);
    }
    ctx.closePath();
    ctx.fill();
}

function drawClockGlyph(ctx, cx, cy, s) {
    const r = s * 0.4;
    ctx.strokeStyle = '#4af';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(cx, cy - r * 0.7);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(cx + r * 0.5, cy);
    ctx.stroke();
}

function drawArrowInGlyph(ctx, cx, cy, s) {
    ctx.fillStyle = '#8f8';
    ctx.beginPath();
    const h = s * 0.5;
    ctx.moveTo(cx - h, cy - h);
    ctx.lineTo(cx + h * 0.3, cy + h * 0.3);
    ctx.lineTo(cx + h * 0.3, cy - h * 0.2);
    ctx.closePath();
    ctx.fill();
    ctx.beginPath();
    ctx.moveTo(cx + h * 0.3, cy + h * 0.3);
    ctx.lineTo(cx - h * 0.1, cy + h * 0.3);
    ctx.lineTo(cx + h * 0.3, cy - h * 0.1);
    ctx.closePath();
    ctx.fill();
}

function drawExclamGlyph(ctx, cx, cy, s) {
    ctx.fillStyle = '#f66';
    const w = s * 0.15;
    ctx.fillRect(cx - w, cy - s * 0.4, w * 2, s * 0.5);
    ctx.beginPath();
    ctx.arc(cx, cy + s * 0.3, w * 1.2, 0, Math.PI * 2);
    ctx.fill();
}

// Hook glyphs into overlay layer via override (non-invasive)
{
    const _origDrawOverlayLayer = drawOverlayLayer;
    drawOverlayLayer = function(layer) {
        _origDrawOverlayLayer(layer);
        drawGlyphs();
    };
}

document.getElementById('graph-show-glyphs')?.addEventListener('change', (e) => {
    glyphsEnabled = e.target.checked;
    drawGraph();
});

// ═══════════════════════════════════════════════════════════════
// F16: Multi-agent view — all agents in one graph
// ═══════════════════════════════════════════════════════════════

let graphMultiAgentMode = false;
let graphAgentColors = {};    // agent_id -> color
let graphMultiAgentData = []; // raw multi-agent response

const AGENT_COLOR_PALETTE = [
    '#4af', '#f4a', '#4fa', '#fa4', '#a4f', '#af4', '#f44', '#44f',
    '#4ff', '#ff4', '#f4f', '#aaf', '#faa', '#afa', '#aff', '#ffa',
];

function assignAgentColors(agentIds) {
    graphAgentColors = {};
    agentIds.forEach((id, i) => {
        graphAgentColors[id] = AGENT_COLOR_PALETTE[i % AGENT_COLOR_PALETTE.length];
    });
}

// Override loadGraph to handle multi-agent mode
{
    const _origLoadGraph = loadGraph;
    loadGraph = async function() {
        if (!graphMultiAgentMode) return _origLoadGraph();
        if (!projectHash) {
            document.getElementById('graph-stats').textContent = 'Select a project to view the memory graph.';
            return;
        }
        try {
            const needAll = document.querySelector('.graph-filter-status[value="suspended"]:checked') ||
                document.querySelector('.graph-filter-status[value="archived"]:checked');
            const statusFilter = needAll ? 'all' : 'active';
            const agentsData = await invoke('get_all_agents_threads', { projectHash, statusFilter });
            graphMultiAgentData = agentsData;

            // Flatten threads with owner_agent tag
            const allThreads = [];
            const agentIds = [];
            for (const entry of agentsData) {
                agentIds.push(entry.agent_id);
                for (const t of entry.threads) {
                    t.owner_agent = entry.agent_id;
                    allThreads.push(t);
                }
            }
            assignAgentColors(agentIds);

            // Load bridges for each agent
            let allBridges = [];
            for (const entry of agentsData) {
                try {
                    const bridges = await invoke('get_bridges', { projectHash, agentId: entry.agent_id });
                    allBridges = allBridges.concat(bridges);
                } catch (_) { /* agent may not have bridges */ }
            }

            graphRawThreads = allThreads;
            graphRawBridges = allBridges;
            applyGraphFilters();

            const agentCount = agentsData.length;
            document.getElementById('graph-stats').textContent =
                `${graphNodes.length} threads, ${graphEdges.length} bridges (${agentCount} agents)`;
        } catch (e) {
            console.error('Multi-agent graph load:', e);
            document.getElementById('graph-stats').textContent = 'Error: ' + e;
        }
    };
}

// Override drawNodesLayer to add agent-colored outlines
{
    const _origDrawNodesLayerF16 = drawNodesLayer;
    drawNodesLayer = function(layer) {
        _origDrawNodesLayerF16(layer);
        if (!graphMultiAgentMode || Object.keys(graphAgentColors).length === 0) return;
        const canvas = document.getElementById('graph-canvas-nodes');
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        const { x: tx, y: ty, scale } = graphTransform;
        for (const n of graphNodes) {
            if (n._timeHidden) continue;
            const agentColor = graphAgentColors[n.ownerAgent];
            if (!agentColor) continue;
            const sx = tx + n.x * scale;
            const sy = ty + n.y * scale;
            const r = n.radius * scale + 3;
            ctx.strokeStyle = agentColor;
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.arc(sx, sy, r, 0, Math.PI * 2);
            ctx.stroke();
        }
    };
}

// Override drawEdgesLayer to style cross-agent bridges as dashed
{
    const _origDrawEdgesLayerF16 = drawEdgesLayer;
    drawEdgesLayer = function(layer) {
        _origDrawEdgesLayerF16(layer);
        if (!graphMultiAgentMode) return;
        const canvas = document.getElementById('graph-canvas-edges');
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        const { x: tx, y: ty, scale } = graphTransform;
        for (const e of graphEdges) {
            const src = graphNodes.find(n => n.id === e.source);
            const tgt = graphNodes.find(n => n.id === e.target);
            if (!src || !tgt) continue;
            if (src.ownerAgent === tgt.ownerAgent) continue;
            const x1 = tx + src.x * scale, y1 = ty + src.y * scale;
            const x2 = tx + tgt.x * scale, y2 = ty + tgt.y * scale;
            ctx.strokeStyle = 'rgba(255,200,100,0.6)';
            ctx.lineWidth = 1.5;
            ctx.setLineDash([6, 4]);
            ctx.beginPath();
            ctx.moveTo(x1, y1);
            ctx.lineTo(x2, y2);
            ctx.stroke();
            ctx.setLineDash([]);
        }
    };
}

// Override buildGraph to pass owner_agent through to nodes
{
    const _origBuildGraphF16 = buildGraph;
    buildGraph = function(threads, bridges) {
        _origBuildGraphF16(threads, bridges);
        if (graphMultiAgentMode) {
            for (const n of graphNodes) {
                const t = threads.find(th => th.id === n.id);
                if (t && t.owner_agent) n.ownerAgent = t.owner_agent;
            }
        }
    };
}

// Override renderGraphLegend to show agent colors
{
    const _origRenderGraphLegend = renderGraphLegend;
    renderGraphLegend = function() {
        _origRenderGraphLegend();
        if (!graphMultiAgentMode || Object.keys(graphAgentColors).length === 0) return;
        const legend = document.getElementById('graph-legend');
        if (!legend || legend.style.display === 'none') return;
        let html = legend.innerHTML;
        html += '<br><span style="color:#888">— Agents —</span><br>';
        for (const [agentId, color] of Object.entries(graphAgentColors)) {
            const label = graphMultiAgentData.find(d => d.agent_id === agentId);
            const name = label ? (label.agent_name || agentId) : agentId;
            html += `<span style="color:${color}">&#9673;</span> ${esc(name)} &nbsp; `;
        }
        html += '<br><span style="color:rgba(255,200,100,0.6)">&#9477;&#9477;</span> Cross-agent bridge';
        legend.innerHTML = html;
    };
}

// Toggle multi-agent mode
document.getElementById('graph-all-agents')?.addEventListener('change', (e) => {
    graphMultiAgentMode = e.target.checked;
    loadGraph();
});

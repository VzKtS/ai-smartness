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
        if (tab.dataset.tab === 'graph') loadGraph();
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
        if (tab === 'graph') { document.getElementById('graph-agent-select').innerHTML = ''; loadGraph(); }
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
    const fullPermsVal = document.getElementById('new-agent-full-permissions').checked;
    const modelVal = document.getElementById('new-agent-model').value || null;
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
            fullPermissions: fullPermsVal,
            expectedModel: modelVal,
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
        const versionEl = document.getElementById('version');
        if (versionEl && ds.version) versionEl.textContent = `AI Smartness v${ds.version}`;

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
    const curModel = agent.expected_model || '';
    const modelOptions = [
        { value: '', label: '— Default —' },
        { value: 'haiku', label: 'Haiku' },
        { value: 'sonnet', label: 'Sonnet' },
        { value: 'opus', label: 'Opus' },
    ].map(m => `<option value="${m.value}" ${m.value === curModel ? 'selected' : ''}>${m.label}</option>`).join('');

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
                <label>Expected Model
                    <select class="ae-expected-model">${modelOptions}</select>
                </label>
                <label class="ae-inline">
                    <input type="checkbox" class="ae-is-supervisor" ${isSup ? 'checked' : ''}>
                    ${dict['modal.issupervisor'] || 'Is Supervisor'}
                </label>
                <label class="ae-inline">
                    <input type="checkbox" class="ae-full-permissions" ${agent.full_permissions ? 'checked' : ''}>
                    Full Permissions
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
    const fullPermissions = editTr.querySelector('.ae-full-permissions')?.checked ?? false;
    const expectedModel = editTr.querySelector('.ae-expected-model')?.value || null;

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
            fullPermissions,
            expectedModel,
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

const GRAPH_COLORS = {
    active: '#00cc85',
    suspended: '#e8a735',
    archived: '#3a4e46',
    edge_default: 'rgba(0,120,184,0.5)',
    edge_highlight: 'rgba(0,204,133,0.95)',
    text: '#e2ece8',
    text_dim: '#5e7a70',
    surface: '#0c1614',
    info: '#0078b8',
};

const RELATION_COLORS = {
    'ChildOf': '#40b0e8',
    'Sibling': '#40d0a0',
    'Extends': '#e8a735',
    'Depends': '#d07040',
    'Contradicts': '#ef4444',
    'Replaces': '#6090c8',
};

// Read CSS variable value from computed style (cached — call only on theme change)
function getThemeColor(varName) {
    return getComputedStyle(document.documentElement).getPropertyValue(varName).trim();
}

// Refresh graph colors from CSS variables — called on theme/mode change, NEVER in render loop
function refreshGraphColors() {
    GRAPH_COLORS.active = getThemeColor('--success') || '#00cc85';
    GRAPH_COLORS.suspended = getThemeColor('--warning') || '#e8a735';
    GRAPH_COLORS.archived = getThemeColor('--text-disabled') || '#3a4e46';
    GRAPH_COLORS.edge_default = (getThemeColor('--info') || '#0078b8') + '80';
    GRAPH_COLORS.edge_highlight = (getThemeColor('--accent') || '#00cc85') + 'F2';
    GRAPH_COLORS.text = getThemeColor('--text') || '#e2ece8';
    GRAPH_COLORS.text_dim = getThemeColor('--text-dim') || '#5e7a70';
    GRAPH_COLORS.surface = getThemeColor('--surface') || '#0c1614';
    GRAPH_COLORS.info = getThemeColor('--info') || '#0078b8';
    RELATION_COLORS['Extends'] = getThemeColor('--warning') || '#e8a735';
    RELATION_COLORS['Contradicts'] = getThemeColor('--danger') || '#ef4444';
    if (graphNodes.length > 0) {
        drawGraph();
        renderGraphLegend();
    }
}

// Watch for theme/mode changes to refresh graph colors
new MutationObserver(() => { refreshGraphColors(); }).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-theme', 'data-mode']
});

// Initial color sync
refreshGraphColors();

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
        const [activeThreads, bridges] = await Promise.all([
            invoke('get_threads', { projectHash, agentId: aid, statusFilter: 'active' }),
            invoke('get_bridges', { projectHash, agentId: aid }),
        ]);

        // Lazy-load threads referenced by bridges but not in active set
        const activeIds = new Set(activeThreads.map(t => t.id));
        const missingIds = new Set();
        for (const b of bridges) {
            if (!activeIds.has(b.source_id)) missingIds.add(b.source_id);
            if (!activeIds.has(b.target_id)) missingIds.add(b.target_id);
        }
        let threads = activeThreads;
        if (missingIds.size > 0) {
            try {
                const allThreads = await invoke('get_threads', {
                    projectHash, agentId: aid, statusFilter: 'all'
                });
                const missing = allThreads.filter(t => missingIds.has(t.id));
                threads = [...activeThreads, ...missing];
            } catch (_) { /* fallback: use active only */ }
        }

        // Filter dead bridges (weight <= 0.05)
        const liveBridges = bridges.filter(b => b.weight > 0.05);
        buildGraph(threads, liveBridges);
        forceLayout(150);
        centerGraph();
        drawGraph();
        renderGraphLegend();
        document.getElementById('graph-stats').textContent =
            `${graphNodes.length} threads, ${graphEdges.length} bridges`;
    } catch (e) {
        console.error('Graph load error:', e);
        document.getElementById('graph-stats').textContent = 'Error: ' + e;
    }
}

function buildGraph(threads, bridges) {
    const filter = document.getElementById('graph-filter').value;
    const idSet = new Set(threads.map(t => t.id));

    // Build edges from bridges (only where both endpoints exist)
    graphEdges = bridges
        .filter(b => idSet.has(b.source_id) && idSet.has(b.target_id))
        .map(b => ({
            source: b.source_id,
            target: b.target_id,
            weight: b.weight || 0,
            relation: b.relation_type || 'RelatedTo',
            reason: b.reason || '',
        }));

    // Filter nodes
    let filtered = threads;
    if (filter === 'bridged') {
        const bridgedIds = new Set();
        graphEdges.forEach(e => { bridgedIds.add(e.source); bridgedIds.add(e.target); });
        filtered = threads.filter(t => bridgedIds.has(t.id));
    }

    // Build nodes with random initial positions
    graphNodes = filtered.map(t => ({
        id: t.id,
        title: t.title || 'Untitled',
        status: (t.status || 'active').toLowerCase(),
        importance: t.importance || 0.5,
        weight: t.weight || 0.5,
        topics: t.topics || [],
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
    const canvas = document.getElementById('graph-canvas');
    const rect = canvas.getBoundingClientRect();
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

function drawGraph() {
    const canvas = document.getElementById('graph-canvas');
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * (window.devicePixelRatio || 1);
    canvas.height = rect.height * (window.devicePixelRatio || 1);
    const ctx = canvas.getContext('2d');
    ctx.setTransform(window.devicePixelRatio || 1, 0, 0, window.devicePixelRatio || 1, 0, 0);

    const { x: tx, y: ty, scale } = graphTransform;
    const showLabels = document.getElementById('graph-show-labels')?.checked;
    const showWeights = document.getElementById('graph-show-weights')?.checked;

    ctx.clearRect(0, 0, rect.width, rect.height);

    const nodeMap = {};
    graphNodes.forEach(n => { nodeMap[n.id] = n; });

    // Draw edges
    for (const e of graphEdges) {
        const a = nodeMap[e.source], b = nodeMap[e.target];
        if (!a || !b) continue;
        const ax = a.x * scale + tx, ay = a.y * scale + ty;
        const bx = b.x * scale + tx, by = b.y * scale + ty;

        const isHighlight = graphSelectedNode &&
            (e.source === graphSelectedNode.id || e.target === graphSelectedNode.id);

        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.lineTo(bx, by);
        ctx.strokeStyle = isHighlight
            ? GRAPH_COLORS.edge_highlight
            : (RELATION_COLORS[e.relation] || GRAPH_COLORS.edge_default);
        ctx.lineWidth = isHighlight ? 2.5 : Math.max(0.5, Math.sqrt(e.weight) * 3);
        ctx.globalAlpha = isHighlight ? 1 : 0.55 + Math.sqrt(e.weight) * 0.4;
        ctx.stroke();
        ctx.globalAlpha = 1;

        // F1: Edge labels (relation_type + weight)
        if (showLabels || showWeights) {
            const mx = (ax + bx) / 2, my = (ay + by) / 2;
            ctx.font = '9px monospace';
            ctx.fillStyle = GRAPH_COLORS.text_dim;
            let edgeLabel = '';
            if (showLabels) edgeLabel += e.relation;
            if (showLabels && showWeights) edgeLabel += ' ';
            if (showWeights) edgeLabel += e.weight.toFixed(2);
            ctx.fillText(edgeLabel, mx + 2, my - 2);
        }
    }

    // Draw nodes as labeled rectangles
    for (const n of graphNodes) {
        const nx = n.x * scale + tx, ny = n.y * scale + ty;
        const isHovered = graphHoveredNode === n;
        const isSelected = graphSelectedNode === n;

        const label = n.title.length > 22 ? n.title.substring(0, 20) + '..' : n.title;
        // F3: Scale font & padding with importance (range 0.8x to 1.3x)
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
        const cr = 4;

        ctx.beginPath();
        ctx.roundRect(rx, ry, rw, rh, cr);
        ctx.fillStyle = GRAPH_COLORS[n.status] || GRAPH_COLORS.active;
        ctx.globalAlpha = 0.3 + n.weight * 0.7;
        ctx.fill();
        ctx.globalAlpha = 1;

        if (isHovered || isSelected) {
            ctx.strokeStyle = GRAPH_COLORS.text;
            ctx.lineWidth = 2;
            ctx.stroke();
        }

        // Draw label inside rectangle
        ctx.fillStyle = GRAPH_COLORS.text;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, nx, ny);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'alphabetic';
    }
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

const graphCanvas = document.getElementById('graph-canvas');
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
            drawGraph();
        }

        // F0: Tooltip on hover
        const tooltip = document.getElementById('graph-tooltip');
        if (node) {
            const bridgeCount = graphEdges.filter(e => e.source === node.id || e.target === node.id).length;
            tooltip.innerHTML =
                `<strong style="color:${GRAPH_COLORS.text}">${esc(node.title)}</strong><br>` +
                `<span style="color:${GRAPH_COLORS[node.status] || GRAPH_COLORS.info}">● ${node.status}</span>` +
                ` &nbsp; Importance: <strong>${node.importance.toFixed(2)}</strong><br>` +
                `Weight: ${node.weight.toFixed(2)} &nbsp; Bridges: ${bridgeCount}` +
                (node.topics.length > 0 ? `<br><span style="color:${GRAPH_COLORS.info}">Topics:</span> ${esc(node.topics.slice(0, 5).join(', '))}` : '');
            const containerRect = graphCanvas.parentElement.getBoundingClientRect();
            const canvasRect = graphCanvas.getBoundingClientRect();
            let tipX = e.clientX - containerRect.left + 14;
            let tipY = e.clientY - containerRect.top + 14;
            // Keep tooltip within container bounds
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
        drawGraph();
    });

    graphCanvas.addEventListener('wheel', (e) => {
        e.preventDefault();
        const rect = graphCanvas.getBoundingClientRect();
        const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
        const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
        const newScale = Math.max(0.1, Math.min(5, graphTransform.scale * factor));
        // Zoom toward cursor
        graphTransform.x = sx - (sx - graphTransform.x) * (newScale / graphTransform.scale);
        graphTransform.y = sy - (sy - graphTransform.y) * (newScale / graphTransform.scale);
        graphTransform.scale = newScale;
        drawGraph();
    }, { passive: false });
}

function showGraphDetail(node) {
    const panel = document.getElementById('graph-detail');
    panel.style.display = 'block';
    document.getElementById('graph-detail-title').textContent = node.title;
    document.getElementById('graph-detail-meta').innerHTML =
        `<strong>Status:</strong> ${esc(node.status)}<br>` +
        `<strong>Weight:</strong> ${node.weight.toFixed(2)}<br>` +
        `<strong>Importance:</strong> ${node.importance.toFixed(2)}<br>` +
        `<strong>Topics:</strong> ${node.topics.join(', ') || '-'}`;

    const connected = graphEdges.filter(e => e.source === node.id || e.target === node.id);
    if (connected.length > 0) {
        const nodeMap = {};
        graphNodes.forEach(n => { nodeMap[n.id] = n; });
        document.getElementById('graph-detail-bridges').innerHTML =
            `<strong>Bridges (${connected.length}):</strong><br>` +
            connected.map(e => {
                const otherId = e.source === node.id ? e.target : e.source;
                const other = nodeMap[otherId];
                const otherTitle = other ? other.title.substring(0, 30) : otherId.substring(0, 8);
                return `<span style="color:${RELATION_COLORS[e.relation] || GRAPH_COLORS.info}">${esc(e.relation)}</span> ` +
                    `→ ${esc(otherTitle)} (${e.weight.toFixed(2)})`;
            }).join('<br>');
    } else {
        document.getElementById('graph-detail-bridges').innerHTML =
            '<span style="color:var(--text-dim)">No bridges</span>';
    }
}

document.getElementById('btn-graph-detail-close')?.addEventListener('click', () => {
    document.getElementById('graph-detail').style.display = 'none';
    graphSelectedNode = null;
    drawGraph();
});

document.getElementById('btn-graph-refresh')?.addEventListener('click', loadGraph);
document.getElementById('graph-agent-select')?.addEventListener('change', loadGraph);
document.getElementById('graph-filter')?.addEventListener('change', loadGraph);
document.getElementById('graph-show-labels')?.addEventListener('change', drawGraph);
document.getElementById('graph-show-weights')?.addEventListener('change', drawGraph);

// F2: Legend — render relation colors + status colors
function renderGraphLegend() {
    const legend = document.getElementById('graph-legend');
    if (!legend) return;
    const showLegend = document.getElementById('graph-show-legend')?.checked;
    if (!showLegend) { legend.style.display = 'none'; return; }
    legend.style.display = 'block';
    let html = `<strong style="color:${GRAPH_COLORS.text};font-size:12px">Legend</strong><br>`;
    html += `<span style="color:${GRAPH_COLORS.text_dim}">— Nodes —</span><br>`;
    html += `<span style="color:${GRAPH_COLORS.active}">●</span> Active &nbsp; `;
    html += `<span style="color:${GRAPH_COLORS.suspended}">●</span> Suspended &nbsp; `;
    html += `<span style="color:${GRAPH_COLORS.archived}">●</span> Archived<br>`;
    html += `<span style="color:${GRAPH_COLORS.text_dim}">— Edges —</span><br>`;
    const activeRelations = new Set(graphEdges.map(e => e.relation));
    for (const [rel, color] of Object.entries(RELATION_COLORS)) {
        if (!activeRelations.has(rel)) continue;
        html += `<span style="color:${color}">━</span> ${rel} &nbsp; `;
    }
    html += `<br><span style="color:${GRAPH_COLORS.text_dim};font-size:10px">Node size ∝ importance</span>`;
    legend.innerHTML = html;
}
document.getElementById('graph-show-legend')?.addEventListener('change', renderGraphLegend);

// F4: Zoom controls (+, -, fit)
function graphZoom(factor) {
    const canvas = document.getElementById('graph-canvas');
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
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

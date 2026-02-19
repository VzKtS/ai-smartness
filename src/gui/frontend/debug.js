// AI Smartness — Debug Console
// Polls daemon.log via Tauri command and renders in real-time

const { invoke } = window.__TAURI__.core;

// ─── State ───────────────────────────────────────────────────
let projectOffset = 0;
let globalOffset = 0;
let paused = false;
let autoScroll = true;
let totalLines = 0;
let projectHash = '';
let logSource = 'global'; // 'global' or 'project'
const activeLevels = new Set(['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE']);
let textFilter = '';

// Per-level counters
const levelCounts = { ERROR: 0, WARN: 0, INFO: 0, DEBUG: 0, TRACE: 0 };

// Get projectHash from URL params (passed by parent window)
const params = new URLSearchParams(window.location.search);
projectHash = params.get('project') || '';

// ─── DOM refs ────────────────────────────────────────────────
const container = document.getElementById('log-container');
const lineCountEl = document.getElementById('line-count');
const logFileEl = document.getElementById('log-file');
const statusEl = document.getElementById('stream-status');
const textFilterInput = document.getElementById('text-filter');
const logSourceSelect = document.getElementById('log-source');
const queueStatsEl = document.getElementById('queue-stats');

// ─── Log source toggle ─────────────────────────────────────
if (logSourceSelect) {
    logSourceSelect.addEventListener('change', (e) => {
        logSource = e.target.value;
        // Clear log view when switching sources
        container.innerHTML = '';
        totalLines = 0;
        logFileEl.textContent = '-';
        // Reset counters
        for (const k of Object.keys(levelCounts)) levelCounts[k] = 0;
        updateLevelCounters();
        updateFooter();
        // Immediately poll the new source
        pollLogs();
    });
}

// ─── Level filter buttons ────────────────────────────────────
document.querySelectorAll('.level-btn').forEach(btn => {
    btn.addEventListener('click', () => {
        const level = btn.dataset.level;
        btn.classList.toggle('active');
        if (activeLevels.has(level)) {
            activeLevels.delete(level);
        } else {
            activeLevels.add(level);
        }
        refilterAll();
    });
});

// ─── Text filter ─────────────────────────────────────────────
textFilterInput.addEventListener('input', (e) => {
    textFilter = e.target.value.toLowerCase();
    refilterAll();
});

// ─── Control buttons ─────────────────────────────────────────
document.getElementById('btn-pause').addEventListener('click', () => {
    paused = !paused;
    document.getElementById('btn-pause').textContent = paused ? 'Resume' : 'Pause';
    statusEl.textContent = paused ? 'PAUSED' : 'LIVE';
    statusEl.className = 'status ' + (paused ? '' : 'live');
});

document.getElementById('btn-clear').addEventListener('click', () => {
    container.innerHTML = '';
    totalLines = 0;
    for (const k of Object.keys(levelCounts)) levelCounts[k] = 0;
    updateLevelCounters();
    updateFooter();
});

document.getElementById('btn-export').addEventListener('click', () => {
    const visible = container.querySelectorAll('.log-line:not([style*="display: none"])');
    const text = Array.from(visible).map(el => el.dataset.raw).join('\n');
    navigator.clipboard.writeText(text).then(() => {
        const btn = document.getElementById('btn-export');
        const orig = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(() => { btn.textContent = orig; }, 1500);
    }).catch(() => {});
});

document.getElementById('btn-scroll-bottom').addEventListener('click', () => {
    autoScroll = true;
    container.scrollTop = container.scrollHeight;
});

// Detect manual scroll up = disable auto-scroll
container.addEventListener('scroll', () => {
    const atBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 40;
    autoScroll = atBottom;
});

// ─── Log parsing ─────────────────────────────────────────────
function detectLevel(line) {
    if (line.includes(' ERROR ') || line.includes('[ERROR]')) return 'ERROR';
    if (line.includes(' WARN ') || line.includes('[WARN]')) return 'WARN';
    if (line.includes(' INFO ') || line.includes('[INFO]')) return 'INFO';
    if (line.includes(' DEBUG ') || line.includes('[DEBUG]')) return 'DEBUG';
    if (line.includes(' TRACE ') || line.includes('[TRACE]')) return 'TRACE';
    return 'INFO'; // default
}

function levelClass(level) {
    return level.toLowerCase();
}

function shouldShow(line, level) {
    if (!activeLevels.has(level)) return false;
    if (textFilter && !line.toLowerCase().includes(textFilter)) return false;
    return true;
}

function formatLine(raw) {
    // Highlight timestamp at start (e.g. 2026-02-15T10:30:00.123Z)
    const tsMatch = raw.match(/^(\d{4}-\d{2}-\d{2}T[\d:.]+Z?\s*)/);
    let formatted = raw;
    if (tsMatch) {
        formatted = `<span class="ts">${esc(tsMatch[1])}</span>${esc(raw.slice(tsMatch[1].length))}`;
    } else {
        formatted = esc(raw);
    }

    // Highlight text filter matches
    if (textFilter && textFilter.length > 1) {
        const escaped = textFilter.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const re = new RegExp(`(${escaped})`, 'gi');
        formatted = formatted.replace(re, '<mark style="background:var(--accent);color:var(--bg);padding:0 2px;border-radius:2px">$1</mark>');
    }

    return formatted;
}

function appendLine(raw) {
    const level = detectLevel(raw);
    const div = document.createElement('div');
    div.className = `log-line ${levelClass(level)}`;
    div.dataset.level = level;
    div.dataset.raw = raw;
    div.innerHTML = formatLine(raw);

    if (!shouldShow(raw, level)) {
        div.style.display = 'none';
    }

    container.appendChild(div);
    totalLines++;

    // Update level counter
    levelCounts[level] = (levelCounts[level] || 0) + 1;
}

function refilterAll() {
    container.querySelectorAll('.log-line').forEach(div => {
        const level = div.dataset.level;
        const raw = div.dataset.raw || '';
        div.style.display = shouldShow(raw, level) ? '' : 'none';
        // Re-render with highlight if text filter changed
        if (textFilter !== undefined) {
            div.innerHTML = formatLine(raw);
        }
    });
}

function updateLevelCounters() {
    for (const level of ['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE']) {
        const el = document.getElementById(`count-${level}`);
        if (el) el.textContent = levelCounts[level] || 0;
    }
}

function updateFooter() {
    const visible = container.querySelectorAll('.log-line:not([style*="display: none"])').length;
    lineCountEl.textContent = `${visible} / ${totalLines} lines`;
}

// ─── Polling loop ────────────────────────────────────────────
async function pollLogs() {
    if (paused) return;

    try {
        let data;
        if (logSource === 'global') {
            data = await invoke('get_global_debug_logs', { offset: globalOffset });
        } else {
            if (!projectHash) return;
            data = await invoke('get_debug_logs', { projectHash, offset: projectOffset });
        }

        if (data.file && logFileEl.textContent === '-') {
            logFileEl.textContent = data.file;
        }

        if (data.lines && data.lines.length > 0) {
            for (const line of data.lines) {
                appendLine(line);
            }
            if (logSource === 'global') {
                globalOffset = data.offset;
            } else {
                projectOffset = data.offset;
            }
            updateLevelCounters();
            updateFooter();

            if (autoScroll) {
                container.scrollTop = container.scrollHeight;
            }
        }
    } catch (e) {
        // silently retry
    }
}

// ─── Queue stats polling ─────────────────────────────────────
async function pollQueueStats() {
    try {
        const status = await invoke('daemon_status');
        if (status && status.running) {
            // daemon_status IPC returns capture_queue in the status response
            // We poll via get_system_resources which includes daemon info
            const res = await invoke('get_system_resources');
            const q = res?.daemon?.capture_queue;
            if (q && queueStatsEl) {
                queueStatsEl.textContent = `Queue: ${q.pending}/${q.workers}w | Done: ${q.processed} | Err: ${q.errors}`;
            }
        }
    } catch (e) {
        // daemon offline
        if (queueStatsEl) queueStatsEl.textContent = '';
    }
}

// Poll logs every 500ms, queue stats every 3s
setInterval(pollLogs, 500);
setInterval(pollQueueStats, 3000);
// Initial fetch
pollLogs();
pollQueueStats();

// ─── Utility ─────────────────────────────────────────────────
function esc(str) {
    const d = document.createElement('div');
    d.textContent = str || '';
    return d.innerHTML;
}

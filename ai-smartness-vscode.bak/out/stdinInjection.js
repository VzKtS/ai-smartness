"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.isClaudeIdle = isClaudeIdle;
exports.ensureMonitored = ensureMonitored;
exports.discoverClaudeProcesses = discoverClaudeProcesses;
exports.tryStdinInjection = tryStdinInjection;
exports.cleanup = cleanup;
/**
 * Stdin injection engine for Claude CLI.
 * Writes JSON messages to stdin of the Claude process.
 *
 * Protocol: JSON line (JSON.stringify + '\n') with the structure:
 * {type: "user", session_id: "", message: {role: "user", content: [{type: "text", text: "..."}]}, parent_tool_use_id: null}
 */
const IDLE_THRESHOLD_MS = 3000; // 3s of no stdout = safe to inject
const MAX_RETRIES = 3;
/** Activity tracker for Claude processes. */
const processActivity = new Map(); // PID → last activity timestamp
const monitoredProcesses = new Set();
/**
 * Build the JSON stdin message for Claude CLI.
 */
function buildMessage(agentId, fromAgent, body) {
    const text = `[AI Smartness AUTO-WAKE] Message for ${agentId} from ${fromAgent}. ` +
        `Use ai_msg_ack or ai_recall to read and reply. ` +
        `This is automated — no human triggered this.\n${body}`;
    const message = {
        type: "user",
        session_id: "",
        message: {
            role: "user",
            content: [{ type: "text", text }]
        },
        parent_tool_use_id: null
    };
    return JSON.stringify(message) + '\n';
}
/**
 * Check if a Claude process is idle (no stdout for IDLE_THRESHOLD_MS).
 */
function isClaudeIdle(pid) {
    const lastActivity = processActivity.get(pid);
    if (lastActivity === undefined) {
        return true;
    } // never seen = assume idle
    return Date.now() - lastActivity >= IDLE_THRESHOLD_MS;
}
/**
 * Monitor a Claude child process for stdout activity.
 * Only stream_event and assistant message types count as activity.
 */
function ensureMonitored(proc) {
    if (!proc.pid || monitoredProcesses.has(proc.pid)) {
        return;
    }
    monitoredProcesses.add(proc.pid);
    processActivity.set(proc.pid, Date.now());
    if (proc.stdout) {
        proc.stdout.on('data', (chunk) => {
            const text = chunk.toString();
            // Only count actual LLM activity
            if (text.includes('"stream_event"') || text.includes('"assistant"')) {
                processActivity.set(proc.pid, Date.now());
            }
        });
    }
    proc.on('exit', () => {
        monitoredProcesses.delete(proc.pid);
        processActivity.delete(proc.pid);
    });
}
/**
 * Discover Claude CLI child processes from the current Node.js process.
 */
function discoverClaudeProcesses() {
    const handles = process._getActiveHandles?.();
    if (!handles) {
        return [];
    }
    const procs = [];
    for (const handle of handles) {
        if (handle?.spawnargs && handle.stdin?.writable) {
            const args = handle.spawnargs.join(' ').toLowerCase();
            if (args.includes('claude')) {
                procs.push(handle);
                ensureMonitored(handle);
            }
        }
    }
    return procs;
}
/**
 * Write injection payload to a Claude process stdin.
 */
function injectToStdin(proc, payload) {
    if (!proc.stdin?.writable) {
        return false;
    }
    try {
        proc.stdin.write(payload);
        return true;
    }
    catch {
        return false;
    }
}
/**
 * Try to inject a message to a specific Claude process.
 * Retries up to MAX_RETRIES times waiting for idle state.
 */
async function tryStdinInjection(proc, agentId, fromAgent, messageBody) {
    const payload = buildMessage(agentId, fromAgent, messageBody);
    for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
        if (proc.pid && isClaudeIdle(proc.pid)) {
            return injectToStdin(proc, payload);
        }
        await new Promise(resolve => setTimeout(resolve, IDLE_THRESHOLD_MS));
    }
    return false;
}
/**
 * Clean up all tracking state.
 */
function cleanup() {
    processActivity.clear();
    monitoredProcesses.clear();
}
//# sourceMappingURL=stdinInjection.js.map
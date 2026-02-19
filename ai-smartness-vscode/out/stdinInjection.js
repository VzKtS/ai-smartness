"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.discoverClaudeProcesses = discoverClaudeProcesses;
exports.isDebounced = isDebounced;
exports.buildPromptText = buildPromptText;
exports.tryInject = tryInject;
exports.tryInjectSync = tryInjectSync;
exports.cleanup = cleanup;
const fs = __importStar(require("fs"));
const paths = __importStar(require("./paths"));
/**
 * Stdin injection engine for Claude CLI.
 *
 * Discovers Claude processes in the extension host, tracks stdout activity,
 * and injects JSON messages into stdin when idle.
 *
 * PID-targeted injection: reads beat.json to map agent → PID, then injects
 * into the correct Claude process. Falls back to first-idle-process if
 * beat.json is unavailable.
 *
 * Protocol: JSON line (JSON.stringify + '\n') matching Claude Code's internal format:
 * {"type":"user","session_id":"","message":{"role":"user","content":[{"type":"text","text":"..."}]},"parent_tool_use_id":null}
 */
const IDLE_THRESHOLD_MS = 3000;
const MAX_RETRIES = 3;
const DEBOUNCE_MS = 10000;
// pid → last stdout activity timestamp
const processActivity = new Map();
const monitoredProcesses = new Set();
// agentId → last injection timestamp
const lastInjectionTime = new Map();
// ─── Process Discovery ───
/**
 * Scan for Claude CLI child processes in the extension host.
 * Must be called every tick to keep tracking up to date.
 */
function discoverClaudeProcesses() {
    const handles = process._getActiveHandles?.();
    if (!handles) {
        return;
    }
    for (const handle of handles) {
        if (handle?.spawnargs &&
            handle.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null) {
            const args = handle.spawnargs.join(' ').toLowerCase();
            if (args.includes('claude')) {
                ensureMonitored(handle);
            }
        }
    }
}
function ensureMonitored(proc) {
    if (!proc.pid || monitoredProcesses.has(proc.pid)) {
        return;
    }
    monitoredProcesses.add(proc.pid);
    processActivity.set(proc.pid, Date.now());
    if (proc.stdout) {
        proc.stdout.on('data', (chunk) => {
            const text = chunk.toString();
            // Only count actual LLM activity as "busy"
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
// ─── Idle & Debounce ───
function isIdle(pid) {
    const lastActivity = processActivity.get(pid);
    if (lastActivity === undefined) {
        return true;
    }
    return Date.now() - lastActivity >= IDLE_THRESHOLD_MS;
}
function isDebounced(agentId) {
    const last = lastInjectionTime.get(agentId);
    if (last === undefined) {
        return false;
    }
    return Date.now() - last < DEBOUNCE_MS;
}
// ─── PID Resolution ───
/**
 * Read agent PID from beat.json written by MCP server heartbeat thread.
 */
function readAgentPid(projHash, agentId) {
    try {
        const beatPath = paths.agentBeatPath(projHash, agentId);
        if (!fs.existsSync(beatPath)) {
            return null;
        }
        const content = fs.readFileSync(beatPath, 'utf8');
        const beat = JSON.parse(content);
        return beat.cli_pid ?? beat.pid ?? null;
    }
    catch {
        return null;
    }
}
/**
 * Find a specific process by PID among active handles.
 */
function findProcessByPid(targetPid) {
    const handles = process._getActiveHandles?.();
    if (!handles) {
        return null;
    }
    for (const handle of handles) {
        if (handle?.pid === targetPid &&
            handle?.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null) {
            return handle;
        }
    }
    return null;
}
// ─── Payload Building ───
function buildPayload(text) {
    const message = {
        type: 'user',
        session_id: '',
        message: {
            role: 'user',
            content: [{ type: 'text', text }]
        },
        parent_tool_use_id: null
    };
    return JSON.stringify(message) + '\n';
}
/**
 * Build the prompt text depending on communication mode.
 */
function buildPromptText(agentId, fromAgent, messageBody, mode) {
    if (mode === 'cognitive') {
        return (`[automated cognitive wake for ${agentId}] You have pending cognitive messages from "${fromAgent}" ` +
            `about: "${messageBody}". Check your cognitive inbox context above and respond to the message. ` +
            `Use ai_msg_ack to acknowledge after processing.`);
    }
    return (`[automated inbox wake for ${agentId}] You have a message from "${fromAgent}": "${messageBody}". ` +
        `Call msg_inbox to read your pending messages and reply.`);
}
// ─── Injection ───
/**
 * Find the first idle, writable Claude process.
 * Returns null if no process found or none are idle.
 */
function findIdleClaudeProcess() {
    const handles = process._getActiveHandles?.();
    if (!handles) {
        return null;
    }
    for (const handle of handles) {
        if (handle?.spawnargs &&
            handle.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null) {
            const args = handle.spawnargs.join(' ').toLowerCase();
            if (args.includes('claude') && handle.pid && isIdle(handle.pid)) {
                return handle;
            }
        }
    }
    return null;
}
/**
 * Find any writable Claude process (regardless of idle state).
 */
function findClaudeProcess() {
    const handles = process._getActiveHandles?.();
    if (!handles) {
        return null;
    }
    for (const handle of handles) {
        if (handle?.spawnargs &&
            handle.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null) {
            const args = handle.spawnargs.join(' ').toLowerCase();
            if (args.includes('claude')) {
                return handle;
            }
        }
    }
    return null;
}
/**
 * Try to inject a message into a Claude process stdin.
 * Respects debounce, waits for idle, retries up to MAX_RETRIES.
 */
async function tryInject(agentId, text) {
    if (isDebounced(agentId)) {
        return false;
    }
    const payload = buildPayload(text);
    for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
        const proc = findClaudeProcess();
        if (!proc?.pid) {
            return false;
        }
        if (isIdle(proc.pid)) {
            try {
                if (!proc.stdin?.writable) {
                    return false;
                }
                proc.stdin.write(payload);
                lastInjectionTime.set(agentId, Date.now());
                return true;
            }
            catch {
                return false;
            }
        }
        // Wait for idle
        await new Promise(resolve => setTimeout(resolve, IDLE_THRESHOLD_MS));
    }
    return false;
}
/**
 * Non-blocking single-attempt inject. Used by AgentController.
 * PID-targeted: reads beat.json to find the correct Claude process for this agent.
 * Falls back to first idle Claude process if PID not available.
 * Never blocks, never retries — the controller handles retry logic.
 */
function tryInjectSync(agentId, text, projHash) {
    if (isDebounced(agentId)) {
        return false;
    }
    const payload = buildPayload(text);
    // Strategy 1: PID-targeted (if projHash available)
    if (projHash) {
        const targetPid = readAgentPid(projHash, agentId);
        if (targetPid) {
            const proc = findProcessByPid(targetPid);
            if (proc?.stdin?.writable && isIdle(targetPid)) {
                try {
                    proc.stdin.write(payload);
                    lastInjectionTime.set(agentId, Date.now());
                    return true;
                }
                catch { /* fall through to strategy 2 */ }
            }
        }
    }
    // Strategy 2: Fallback — only if there's exactly ONE monitored Claude process
    // (single-agent compat). With multiple processes, PID targeting is required
    // to avoid injecting into the wrong panel.
    if (monitoredProcesses.size <= 1) {
        const proc = findIdleClaudeProcess();
        if (proc?.stdin?.writable) {
            try {
                proc.stdin.write(payload);
                lastInjectionTime.set(agentId, Date.now());
                return true;
            }
            catch {
                return false;
            }
        }
    }
    return false;
}
// ─── Cleanup ───
function cleanup() {
    processActivity.clear();
    monitoredProcesses.clear();
    lastInjectionTime.clear();
}
//# sourceMappingURL=stdinInjection.js.map
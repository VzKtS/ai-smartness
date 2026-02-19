import * as fs from 'fs';
import type { ChildProcess } from 'child_process';
import * as paths from './paths';

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
const DEBOUNCE_MS = 10_000;

// pid → last stdout activity timestamp
const processActivity: Map<number, number> = new Map();
const monitoredProcesses: Set<number> = new Set();
// agentId → last injection timestamp
const lastInjectionTime: Map<string, number> = new Map();

// ─── Process Discovery ───

/**
 * Scan for Claude CLI child processes in the extension host.
 * Must be called every tick to keep tracking up to date.
 */
export function discoverClaudeProcesses(): void {
    const handles = (process as any)._getActiveHandles?.();
    if (!handles) { return; }

    for (const handle of handles) {
        if (
            handle?.spawnargs &&
            handle.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null
        ) {
            const args = (handle.spawnargs as string[]).join(' ').toLowerCase();
            if (args.includes('claude')) {
                ensureMonitored(handle as ChildProcess);
            }
        }
    }
}

function ensureMonitored(proc: ChildProcess): void {
    if (!proc.pid || monitoredProcesses.has(proc.pid)) { return; }
    monitoredProcesses.add(proc.pid);
    processActivity.set(proc.pid, Date.now());

    if (proc.stdout) {
        proc.stdout.on('data', (chunk: Buffer) => {
            const text = chunk.toString();
            // Only count actual LLM activity as "busy"
            if (text.includes('"stream_event"') || text.includes('"assistant"')) {
                processActivity.set(proc.pid!, Date.now());
            }
        });
    }

    proc.on('exit', () => {
        monitoredProcesses.delete(proc.pid!);
        processActivity.delete(proc.pid!);
    });
}

// ─── Idle & Debounce ───

function isIdle(pid: number): boolean {
    const lastActivity = processActivity.get(pid);
    if (lastActivity === undefined) { return true; }
    return Date.now() - lastActivity >= IDLE_THRESHOLD_MS;
}

export function isDebounced(agentId: string): boolean {
    const last = lastInjectionTime.get(agentId);
    if (last === undefined) { return false; }
    return Date.now() - last < DEBOUNCE_MS;
}

// ─── PID Resolution ───

/**
 * Read agent PID from beat.json written by MCP server heartbeat thread.
 */
function readAgentPid(projHash: string, agentId: string): number | null {
    try {
        const beatPath = paths.agentBeatPath(projHash, agentId);
        if (!fs.existsSync(beatPath)) { return null; }
        const content = fs.readFileSync(beatPath, 'utf8');
        const beat = JSON.parse(content);
        return beat.cli_pid ?? beat.pid ?? null;
    } catch {
        return null;
    }
}

/**
 * Find a specific process by PID among active handles.
 */
function findProcessByPid(targetPid: number): ChildProcess | null {
    const handles = (process as any)._getActiveHandles?.();
    if (!handles) { return null; }

    for (const handle of handles) {
        if (
            handle?.pid === targetPid &&
            handle?.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null
        ) {
            return handle as ChildProcess;
        }
    }
    return null;
}

// ─── Payload Building ───

function buildPayload(text: string): string {
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
export function buildPromptText(
    agentId: string,
    fromAgent: string,
    messageBody: string,
    mode: 'cognitive' | 'inbox'
): string {
    if (mode === 'cognitive') {
        return (
            `[automated cognitive wake for ${agentId}] You have pending cognitive messages from "${fromAgent}" ` +
            `about: "${messageBody}". Check your cognitive inbox context above and respond to the message. ` +
            `Use ai_msg_ack to acknowledge after processing.`
        );
    }
    return (
        `[automated inbox wake for ${agentId}] You have a message from "${fromAgent}": "${messageBody}". ` +
        `Call msg_inbox to read your pending messages and reply.`
    );
}

// ─── Injection ───

/**
 * Find the first idle, writable Claude process.
 * Returns null if no process found or none are idle.
 */
function findIdleClaudeProcess(): ChildProcess | null {
    const handles = (process as any)._getActiveHandles?.();
    if (!handles) { return null; }

    for (const handle of handles) {
        if (
            handle?.spawnargs &&
            handle.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null
        ) {
            const args = (handle.spawnargs as string[]).join(' ').toLowerCase();
            if (args.includes('claude') && handle.pid && isIdle(handle.pid)) {
                return handle as ChildProcess;
            }
        }
    }
    return null;
}

/**
 * Find any writable Claude process (regardless of idle state).
 */
function findClaudeProcess(): ChildProcess | null {
    const handles = (process as any)._getActiveHandles?.();
    if (!handles) { return null; }

    for (const handle of handles) {
        if (
            handle?.spawnargs &&
            handle.stdin?.writable &&
            !handle.killed &&
            handle.exitCode === null
        ) {
            const args = (handle.spawnargs as string[]).join(' ').toLowerCase();
            if (args.includes('claude')) {
                return handle as ChildProcess;
            }
        }
    }
    return null;
}

/**
 * Try to inject a message into a Claude process stdin.
 * Respects debounce, waits for idle, retries up to MAX_RETRIES.
 */
export async function tryInject(agentId: string, text: string): Promise<boolean> {
    if (isDebounced(agentId)) { return false; }

    const payload = buildPayload(text);

    for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
        const proc = findClaudeProcess();
        if (!proc?.pid) { return false; }

        if (isIdle(proc.pid)) {
            try {
                if (!proc.stdin?.writable) { return false; }
                proc.stdin.write(payload);
                lastInjectionTime.set(agentId, Date.now());
                return true;
            } catch {
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
export function tryInjectSync(agentId: string, text: string, projHash?: string): boolean {
    if (isDebounced(agentId)) { return false; }

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
                } catch { /* fall through to strategy 2 */ }
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
            } catch { return false; }
        }
    }

    return false;
}

// ─── Cleanup ───

export function cleanup(): void {
    processActivity.clear();
    monitoredProcesses.clear();
    lastInjectionTime.clear();
}

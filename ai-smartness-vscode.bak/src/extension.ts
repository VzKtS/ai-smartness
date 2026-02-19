import * as vscode from 'vscode';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { execSync } from 'child_process';

import { dataDir, resolveProjectHash, readSessionAgent, daemonSocketPath } from './paths';
import * as wakeSignals from './wakeSignals';
import * as stdinInjection from './stdinInjection';
import * as statusBar from './statusBar';
import type { WakeSignal } from './wakeSignals';

// --- Constants ---
const DEBOUNCE_MS = 10_000;
const STARTUP_GRACE_MS = 3_000;
const CLEANUP_INTERVAL_MS = 60_000;

// --- State ---
let outputChannel: vscode.OutputChannel;
let pollHandle: ReturnType<typeof setInterval> | undefined;
let cleanupHandle: ReturnType<typeof setInterval> | undefined;
let activationTime: number;
let isEnabled: boolean = true;
let windowAgentId: string | null = null;
let currentProjectHash: string | null = null;

const processedSignals: Map<string, number> = new Map();
const lastPromptSent: Map<string, number> = new Map();
const injectionInFlight: Set<string> = new Set();

// =====================================================================
// Lifecycle
// =====================================================================

export function activate(context: vscode.ExtensionContext): void {
    outputChannel = vscode.window.createOutputChannel('AI Smartness');
    activationTime = Date.now();
    log('AI Smartness extension activating...');

    const config = vscode.workspace.getConfiguration('aiSmartness');
    isEnabled = config.get<boolean>('enabled', true);

    // Resolve project hash from workspace
    const folders = vscode.workspace.workspaceFolders;
    if (folders?.length) {
        currentProjectHash = resolveProjectHash(folders[0].uri.fsPath);
        if (currentProjectHash) {
            log(`Project hash: ${currentProjectHash} (${folders[0].uri.fsPath})`);
        }
    }

    statusBar.create();

    context.subscriptions.push(
        vscode.commands.registerCommand('aiSmartnessWaker.showStatus', showStatusCommand),
        vscode.commands.registerCommand('aiSmartnessWaker.checkInbox', checkInboxCommand),
        vscode.commands.registerCommand('aiSmartnessWaker.toggleAutoWake', toggleAutoWakeCommand),
        vscode.commands.registerCommand('aiSmartnessWaker.openAgentWindow', openAgentWindowCommand),
        vscode.commands.registerCommand('aiSmartnessWaker.selectAgent', selectAgentCommand),
    );

    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(e => {
            if (e.affectsConfiguration('aiSmartness')) {
                const cfg = vscode.workspace.getConfiguration('aiSmartness');
                const newEnabled = cfg.get<boolean>('enabled', true);
                if (newEnabled !== isEnabled) {
                    isEnabled = newEnabled;
                    isEnabled ? startPolling() : stopPolling();
                }
            }
        })
    );

    if (isEnabled) {
        startPolling();
    }

    cleanupHandle = setInterval(() => {
        const deleted = wakeSignals.cleanupOldSignals();
        if (deleted > 0) { log(`Cleaned up ${deleted} old signal(s)`); }
        const cutoff = Date.now() - 5 * 60 * 1000;
        for (const [key, ts] of processedSignals) {
            if (ts < cutoff) { processedSignals.delete(key); }
        }
    }, CLEANUP_INTERVAL_MS);

    log('AI Smartness extension activated');
}

export function deactivate(): void {
    stopPolling();
    if (cleanupHandle) { clearInterval(cleanupHandle); }
    stdinInjection.cleanup();
    statusBar.dispose();
    outputChannel?.dispose();
}

// =====================================================================
// Polling
// =====================================================================

function startPolling(): void {
    if (pollHandle) { return; }
    const config = vscode.workspace.getConfiguration('aiSmartness');
    const interval = config.get<number>('watchInterval', 3000);
    log(`Starting poll loop every ${interval}ms`);
    pollHandle = setInterval(tick, interval);
    tick();
}

function stopPolling(): void {
    if (pollHandle) {
        clearInterval(pollHandle);
        pollHandle = undefined;
        log('Polling stopped');
    }
}

async function tick(): Promise<void> {
    try {
        stdinInjection.discoverClaudeProcesses();

        if (!windowAgentId) {
            windowAgentId = detectAgent();
        }

        if (windowAgentId && Date.now() - activationTime > STARTUP_GRACE_MS) {
            await checkWakeSignals(windowAgentId);
        }

        const daemonAlive = isDaemonAlive();

        const pendingCount = windowAgentId
            ? wakeSignals.listPendingSignals().filter(s => s.agent_id === windowAgentId).length
            : 0;

        statusBar.update(windowAgentId, pendingCount, daemonAlive, isEnabled);

    } catch (err) {
        log(`Tick error: ${err}`);
    }
}

// =====================================================================
// Agent Detection — cascade: session file → .mcp.json → CLI query
// =====================================================================

function detectAgent(): string | null {
    // 1. Env var (highest priority, set by user)
    const envAgent = process.env.AI_SMARTNESS_AGENT;
    if (envAgent) {
        log(`Agent from env var: ${envAgent}`);
        return envAgent;
    }

    // 2. Session file (set via `ai-smartness agent select`)
    if (currentProjectHash) {
        const sessionAgent = readSessionAgent(currentProjectHash);
        if (sessionAgent) {
            log(`Agent from session file: ${sessionAgent}`);
            return sessionAgent;
        }
    }

    // 3. .mcp.json (deterministic from workspace config)
    const mcpAgent = detectAgentFromMcpJson();
    if (mcpAgent) { return mcpAgent; }

    // 4. CLI query (fallback)
    return detectAgentFromCli();
}

/**
 * Read .mcp.json from workspace and look for --agent-id in ai-smartness server args.
 */
function detectAgentFromMcpJson(): string | null {
    const folders = vscode.workspace.workspaceFolders;
    if (!folders?.length) { return null; }

    for (const folder of folders) {
        const mcpPath = path.join(folder.uri.fsPath, '.mcp.json');
        try {
            if (!fs.existsSync(mcpPath)) { continue; }
            const content = fs.readFileSync(mcpPath, 'utf8');
            const mcpConfig = JSON.parse(content);
            const servers = mcpConfig.mcpServers || mcpConfig.servers || {};
            for (const [key, serverConfig] of Object.entries(servers)) {
                if (!key.startsWith('ai-smartness')) { continue; }
                const args: string[] = (serverConfig as any)?.args || [];
                for (const arg of args) {
                    const match = arg.match(/--agent-id[=\s](\S+)/);
                    if (match) {
                        log(`Agent from .mcp.json (${key}): ${match[1]}`);
                        return match[1];
                    }
                }
            }
        } catch {
            continue;
        }
    }
    return null;
}

/**
 * Query ai-smartness agent list via CLI.
 */
function detectAgentFromCli(): string | null {
    if (!currentProjectHash) { return null; }
    try {
        const bin = resolveAiSmartnessBin();
        const result = execSync(`${bin} agent list --project-hash ${currentProjectHash} 2>/dev/null`, {
            timeout: 5000,
            encoding: 'utf8'
        });
        const lines = result.trim().split('\n');
        for (const line of lines) {
            const match = line.match(/^\s*(\S+)\s+/);
            if (match && match[1] !== 'ID' && !line.startsWith('-')) {
                log(`Agent from CLI: ${match[1]}`);
                return match[1];
            }
        }
    } catch {
        // ai-smartness not found — ignore
    }
    return null;
}

// =====================================================================
// Wake Signals
// =====================================================================

async function checkWakeSignals(agentId: string): Promise<void> {
    const signal = wakeSignals.readWakeSignal(agentId);
    if (!signal || signal.acknowledged) { return; }

    const signalKey = `${signal.agent_id}_${signal.timestamp}`;
    if (processedSignals.has(signalKey)) { return; }

    processedSignals.set(signalKey, Date.now());
    await handleWakeSignal(signal);
}

async function handleWakeSignal(signal: WakeSignal): Promise<void> {
    const agentId = signal.agent_id;

    if (injectionInFlight.has(agentId)) { return; }

    const lastSent = lastPromptSent.get(agentId) ?? 0;
    if (Date.now() - lastSent < DEBOUNCE_MS) {
        log(`Debounced wake for ${agentId}`);
        return;
    }

    log(`Wake signal: ${agentId} from ${signal.from}: "${signal.message}"`);
    vscode.window.showInformationMessage(
        `AI Smartness: Message for ${agentId} from ${signal.from}`
    );

    const autoPrompt = vscode.workspace.getConfiguration('aiSmartness').get<boolean>('autoPrompt', true);
    if (!autoPrompt) {
        wakeSignals.acknowledgeSignal(agentId);
        return;
    }

    injectionInFlight.add(agentId);
    try {
        const procs = stdinInjection.discoverClaudeProcesses();
        if (procs.length === 0) {
            log(`No Claude process found for injection to ${agentId}`);
            vscode.window.showWarningMessage(
                `AI Smartness: Cannot inject to ${agentId} — no Claude CLI process found`
            );
            return;
        }

        const success = await stdinInjection.tryStdinInjection(
            procs[0], agentId, signal.from, signal.message
        );

        if (success) {
            log(`Injected wake to ${agentId}`);
            lastPromptSent.set(agentId, Date.now());
            wakeSignals.acknowledgeSignal(agentId);
        } else {
            log(`Injection failed for ${agentId} (process busy)`);
            vscode.window.showWarningMessage(
                `AI Smartness: Could not inject to ${agentId} — process busy. ` +
                `Use "AI Smartness: Check Inbox" manually.`
            );
        }
    } finally {
        injectionInFlight.delete(agentId);
    }
}

// =====================================================================
// Daemon Status
// =====================================================================

function isDaemonAlive(): boolean {
    const sockPath = daemonSocketPath();
    if (!fs.existsSync(sockPath)) { return false; }
    try {
        const bin = resolveAiSmartnessBin();
        const result = execSync(`${bin} daemon status 2>/dev/null`, {
            timeout: 3000,
            encoding: 'utf8'
        });
        return result.includes('running') || result.includes('alive') || result.includes('pid');
    } catch {
        return fs.existsSync(sockPath);
    }
}

// =====================================================================
// Commands
// =====================================================================

function showStatusCommand(): void {
    const lines: string[] = [
        `AI Smartness Status`,
        `---`,
        `Agent: ${windowAgentId ?? 'none'}`,
        `Project: ${currentProjectHash ?? 'unknown'}`,
        `Auto-Wake: ${isEnabled ? 'enabled' : 'disabled'}`,
        `Daemon: ${isDaemonAlive() ? 'running' : 'stopped'}`,
        `Data Dir: ${dataDir()}`,
    ];

    if (windowAgentId) {
        const pending = wakeSignals.listPendingSignals()
            .filter(s => s.agent_id === windowAgentId);
        lines.push(`Pending Signals: ${pending.length}`);
        for (const s of pending) {
            lines.push(`  - from ${s.from}: ${s.message}`);
        }
    }

    vscode.window.showInformationMessage(lines.join('\n'), { modal: true });
}

function checkInboxCommand(): void {
    if (!windowAgentId) {
        vscode.window.showWarningMessage('AI Smartness: No agent detected for this window.');
        return;
    }

    const signal = wakeSignals.readWakeSignal(windowAgentId);
    if (!signal || signal.acknowledged) {
        vscode.window.showInformationMessage(`AI Smartness: No pending messages for ${windowAgentId}`);
        return;
    }

    vscode.window.showInformationMessage(
        `AI Smartness: Message from ${signal.from}: ${signal.message}`
    );
    handleWakeSignal(signal);
}

function toggleAutoWakeCommand(): void {
    isEnabled = !isEnabled;
    const config = vscode.workspace.getConfiguration('aiSmartness');
    config.update('enabled', isEnabled, vscode.ConfigurationTarget.Global);

    if (isEnabled) {
        startPolling();
        vscode.window.showInformationMessage('AI Smartness: Auto-Wake enabled');
    } else {
        stopPolling();
        vscode.window.showInformationMessage('AI Smartness: Auto-Wake disabled');
    }
}

/**
 * Select an agent profile for this session.
 * Queries the Rust registry, shows a QuickPick, then runs `ai-smartness agent select`.
 */
async function selectAgentCommand(): Promise<void> {
    if (!currentProjectHash) {
        vscode.window.showWarningMessage('AI Smartness: No project detected. Open a workspace folder first.');
        return;
    }

    const agents = listAgentsFromCli();
    if (agents.length === 0) {
        vscode.window.showWarningMessage(
            'AI Smartness: No agents registered for this project. ' +
            'Use the GUI or `ai-smartness agent add` to create one.'
        );
        return;
    }

    const items = agents.map(a => ({
        label: a.id,
        description: `${a.role} (${a.mode})`,
        detail: a.supervisor ? `Supervisor: ${a.supervisor}` : undefined,
    }));

    // Add "clear" option
    items.push({
        label: '$(circle-slash) Clear agent binding',
        description: 'Remove session agent',
        detail: undefined,
    });

    const selected = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select agent profile for this session'
    });
    if (!selected) { return; }

    try {
        const bin = resolveAiSmartnessBin();
        if (selected.label.includes('Clear')) {
            execSync(`${bin} agent select --project-hash ${currentProjectHash}`, {
                timeout: 5000,
                encoding: 'utf8'
            });
            windowAgentId = null;
            vscode.window.showInformationMessage('AI Smartness: Agent binding cleared');
            log('Agent binding cleared');
        } else {
            execSync(`${bin} agent select ${selected.label} --project-hash ${currentProjectHash}`, {
                timeout: 5000,
                encoding: 'utf8'
            });
            windowAgentId = selected.label;
            vscode.window.showInformationMessage(`AI Smartness: Agent set to ${selected.label}`);
            log(`Agent selected: ${selected.label}`);
        }
    } catch (err) {
        vscode.window.showErrorMessage(`AI Smartness: Failed to select agent: ${err}`);
    }
}

/**
 * Open a new VSCode window for a specific agent.
 * Lists agents from the registry and creates a workspace.
 */
async function openAgentWindowCommand(): Promise<void> {
    if (!currentProjectHash) {
        vscode.window.showWarningMessage('AI Smartness: No project detected.');
        return;
    }

    const agents = listAgentsFromCli();
    if (agents.length === 0) {
        vscode.window.showWarningMessage('AI Smartness: No agents registered for this project.');
        return;
    }

    const items = agents.map(a => ({
        label: a.id,
        description: `${a.role} (${a.mode})`,
    }));

    const selected = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select agent to open in new window'
    });
    if (!selected) { return; }

    const folders = vscode.workspace.workspaceFolders;
    if (!folders?.length) { return; }

    const rootPath = folders[0].uri.fsPath;

    // Create a workspace file in the project's .claude/ directory
    const claudeDir = path.join(rootPath, '.claude');
    if (!fs.existsSync(claudeDir)) {
        fs.mkdirSync(claudeDir, { recursive: true });
    }

    const workspace = {
        folders: [
            { path: '..', name: `${selected.label} — ${path.basename(rootPath)}` }
        ],
        settings: {
            'aiSmartness.defaultAgent': selected.label,
        }
    };

    const workspacePath = path.join(claudeDir, `agent-${selected.label}.code-workspace`);
    fs.writeFileSync(workspacePath, JSON.stringify(workspace, null, 2));

    // Select the agent for this workspace
    try {
        const bin = resolveAiSmartnessBin();
        execSync(`${bin} agent select ${selected.label} --project-hash ${currentProjectHash}`, {
            timeout: 5000
        });
    } catch {
        // Non-fatal
    }

    const uri = vscode.Uri.file(workspacePath);
    await vscode.commands.executeCommand('vscode.openFolder', uri, true);
}

// =====================================================================
// Helpers
// =====================================================================

interface AgentInfo {
    id: string;
    role: string;
    mode: string;
    supervisor: string | null;
}

/**
 * List agents from the ai-smartness registry via CLI.
 */
function listAgentsFromCli(): AgentInfo[] {
    if (!currentProjectHash) { return []; }
    try {
        const bin = resolveAiSmartnessBin();
        const result = execSync(`${bin} agent list --project-hash ${currentProjectHash} 2>/dev/null`, {
            timeout: 5000,
            encoding: 'utf8'
        });

        const agents: AgentInfo[] = [];
        const lines = result.trim().split('\n');
        for (const line of lines) {
            // Skip header and separator lines
            if (line.startsWith('ID') || line.startsWith('-') || line.includes('Total:') || !line.trim()) {
                continue;
            }
            const parts = line.trim().split(/\s{2,}/);
            if (parts.length >= 6) {
                agents.push({
                    id: parts[0],
                    role: parts[1],
                    supervisor: parts[3] === '-' ? null : parts[3],
                    mode: parts[5],
                });
            }
        }
        return agents;
    } catch {
        return [];
    }
}

/**
 * Resolve the path to the ai-smartness binary.
 * Priority: which ai-smartness → $HOME/.local/bin → /usr/local/bin
 */
function resolveAiSmartnessBin(): string {
    try {
        const result = execSync('which ai-smartness 2>/dev/null', {
            timeout: 2000,
            encoding: 'utf8'
        });
        const p = result.trim();
        if (p) { return p; }
    } catch {
        // Not in PATH
    }

    const candidates = [
        path.join(os.homedir(), '.local', 'bin', 'ai-smartness'),
        '/usr/local/bin/ai-smartness',
    ];
    for (const c of candidates) {
        if (fs.existsSync(c)) { return c; }
    }

    return 'ai-smartness';
}

// =====================================================================
// Logging
// =====================================================================

function log(msg: string): void {
    const ts = new Date().toISOString().substring(11, 19);
    outputChannel?.appendLine(`[${ts}] ${msg}`);
}


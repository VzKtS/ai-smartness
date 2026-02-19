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
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const paths = __importStar(require("./paths"));
const cli = __importStar(require("./cli"));
const statusBar = __importStar(require("./statusBar"));
const wakeSignals = __importStar(require("./wakeSignals"));
const stdinInjection = __importStar(require("./stdinInjection"));
const agentController_1 = require("./agentController");
// ─── Constants ───
const STARTUP_GRACE_MS = 3000;
const CLEANUP_INTERVAL_MS = 60000;
// ─── State ───
let outputChannel;
let pollHandle;
let cleanupHandle;
let activationTime;
let isEnabled = true;
let currentProjectHash = null;
/** Per-agent controllers. Keyed by agentId. Created/removed as agents appear/disappear. */
const controllers = new Map();
// =====================================================================
// Lifecycle
// =====================================================================
function activate(context) {
    outputChannel = vscode.window.createOutputChannel('AI Smartness');
    activationTime = Date.now();
    log('AI Smartness extension activating...');
    const config = vscode.workspace.getConfiguration('aiSmartness');
    isEnabled = config.get('enabled', true);
    // Resolve project hash from workspace
    const folders = vscode.workspace.workspaceFolders;
    if (folders?.length) {
        currentProjectHash = paths.resolveProjectHash(folders[0].uri.fsPath);
        if (currentProjectHash) {
            log(`Project hash: ${currentProjectHash} (${folders[0].uri.fsPath})`);
        }
    }
    statusBar.create();
    // Register commands
    context.subscriptions.push(vscode.commands.registerCommand('aiSmartness.showStatus', showStatusCommand), vscode.commands.registerCommand('aiSmartness.checkInbox', checkInboxCommand), vscode.commands.registerCommand('aiSmartness.toggleAutoWake', toggleAutoWakeCommand), vscode.commands.registerCommand('aiSmartness.openAgentWindow', openAgentWindowCommand), vscode.commands.registerCommand('aiSmartness.selectAgent', selectAgentCommand));
    // Watch config changes
    context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(e => {
        if (e.affectsConfiguration('aiSmartness')) {
            const cfg = vscode.workspace.getConfiguration('aiSmartness');
            const newEnabled = cfg.get('enabled', true);
            if (newEnabled !== isEnabled) {
                isEnabled = newEnabled;
                isEnabled ? startPolling() : stopPolling();
            }
            // Propagate communication mode change to all controllers
            const mode = cfg.get('communicationMode', 'cognitive');
            for (const ctrl of controllers.values()) {
                ctrl.setMode(mode);
            }
        }
    }));
    // Daemon auto-start
    if (currentProjectHash) {
        const dStatus = cli.daemonStatus();
        if (dStatus.status !== 'running') {
            log('Daemon not running, starting...');
            const ok = cli.daemonStart();
            log(ok ? 'Daemon started' : 'Failed to start daemon');
        }
    }
    if (isEnabled) {
        startPolling();
    }
    // Periodic cleanup
    cleanupHandle = setInterval(() => {
        const deleted = wakeSignals.cleanupOldSignals();
        if (deleted > 0) {
            log(`Cleaned up ${deleted} old signal(s)`);
        }
        for (const ctrl of controllers.values()) {
            ctrl.cleanup();
        }
    }, CLEANUP_INTERVAL_MS);
    log('AI Smartness extension activated');
}
function deactivate() {
    stopPolling();
    if (cleanupHandle) {
        clearInterval(cleanupHandle);
    }
    controllers.clear();
    stdinInjection.cleanup();
    statusBar.dispose();
    outputChannel?.dispose();
}
// =====================================================================
// Polling
// =====================================================================
function startPolling() {
    if (pollHandle) {
        return;
    }
    const config = vscode.workspace.getConfiguration('aiSmartness');
    const interval = config.get('watchInterval', 3000);
    log(`Starting poll loop every ${interval}ms`);
    pollHandle = setInterval(tick, interval);
    tick();
}
function stopPolling() {
    if (pollHandle) {
        clearInterval(pollHandle);
        pollHandle = undefined;
        log('Polling stopped');
    }
}
function tick() {
    try {
        stdinInjection.discoverClaudeProcesses();
        // Refresh agent list and sync controllers
        const agentIds = detectAllAgents();
        syncControllers(agentIds);
        // Tick all controllers — non-blocking, each manages its own state
        if (controllers.size > 0 && Date.now() - activationTime > STARTUP_GRACE_MS) {
            const config = vscode.workspace.getConfiguration('aiSmartness');
            const autoPrompt = config.get('autoPrompt', true);
            for (const ctrl of controllers.values()) {
                ctrl.tick(autoPrompt);
            }
        }
        const daemonAlive = isDaemonAlive();
        // Count pending signals across all controllers
        let pendingCount = 0;
        for (const ctrl of controllers.values()) {
            pendingCount += ctrl.pendingCount;
        }
        statusBar.update(agentIds, pendingCount, daemonAlive, isEnabled);
    }
    catch (err) {
        log(`Tick error: ${err}`);
    }
}
// =====================================================================
// Controller Lifecycle
// =====================================================================
/**
 * Sync the controller map with the current set of detected agents.
 * Creates new controllers for new agents, removes stale ones.
 */
function syncControllers(agentIds) {
    const config = vscode.workspace.getConfiguration('aiSmartness');
    const mode = config.get('communicationMode', 'cognitive');
    const currentSet = new Set(agentIds);
    // Remove controllers for agents that disappeared
    for (const [id] of controllers) {
        if (!currentSet.has(id)) {
            log(`Agent removed: ${id}`);
            controllers.delete(id);
        }
    }
    // Create controllers for new agents
    for (const id of agentIds) {
        if (!controllers.has(id)) {
            log(`Agent discovered: ${id}`);
            const ctrl = new agentController_1.AgentController(id, currentProjectHash, {
                onLog: log,
                onNotify: (msg) => vscode.window.showInformationMessage(msg),
                onWarn: (msg) => vscode.window.showWarningMessage(msg),
            });
            ctrl.setMode(mode);
            controllers.set(id, ctrl);
        }
    }
}
// =====================================================================
// Daemon Check (PID file, no CLI call)
// =====================================================================
function isDaemonAlive() {
    try {
        const pidFile = paths.daemonPidPath();
        if (!fs.existsSync(pidFile)) {
            return false;
        }
        const pid = parseInt(fs.readFileSync(pidFile, 'utf8').trim(), 10);
        if (isNaN(pid)) {
            return false;
        }
        process.kill(pid, 0);
        return true;
    }
    catch {
        return false;
    }
}
// =====================================================================
// Agent Detection — all agents from per-session files + global + env
// =====================================================================
function detectAllAgents() {
    const agents = new Set();
    // 1. Env var (applies to all sessions)
    const envAgent = process.env.AI_SMARTNESS_AGENT;
    if (envAgent) {
        agents.add(envAgent);
    }
    if (currentProjectHash) {
        // 2. Per-session agent files — one file per Claude Code panel
        const sessionAgents = paths.listSessionAgents(currentProjectHash);
        for (const a of sessionAgents) {
            agents.add(a);
        }
        // 3. Global session file (fallback for sessions without per-session binding)
        const globalAgent = paths.readSessionAgent(currentProjectHash);
        if (globalAgent) {
            agents.add(globalAgent);
        }
    }
    // 4. .mcp.json
    const mcpAgent = detectAgentFromMcpJson();
    if (mcpAgent) {
        agents.add(mcpAgent);
    }
    return [...agents];
}
function detectAgentFromMcpJson() {
    const folders = vscode.workspace.workspaceFolders;
    if (!folders?.length) {
        return null;
    }
    for (const folder of folders) {
        const mcpPath = path.join(folder.uri.fsPath, '.mcp.json');
        try {
            if (!fs.existsSync(mcpPath)) {
                continue;
            }
            const content = fs.readFileSync(mcpPath, 'utf8');
            const mcpConfig = JSON.parse(content);
            const servers = mcpConfig.mcpServers || mcpConfig.servers || {};
            for (const [key, serverConfig] of Object.entries(servers)) {
                if (!key.startsWith('ai-smartness')) {
                    continue;
                }
                const args = serverConfig?.args || [];
                for (const arg of args) {
                    const match = arg.match(/--agent-id[=\s](\S+)/);
                    if (match) {
                        return match[1];
                    }
                }
            }
        }
        catch {
            continue;
        }
    }
    return null;
}
// =====================================================================
// Commands
// =====================================================================
function showStatusCommand() {
    const config = vscode.workspace.getConfiguration('aiSmartness');
    const mode = config.get('communicationMode', 'cognitive');
    const agentIds = [...controllers.keys()];
    let pendingCount = 0;
    const stateLines = [];
    for (const [id, ctrl] of controllers) {
        pendingCount += ctrl.pendingCount;
        stateLines.push(`  ${id}: ${ctrl.currentState}`);
    }
    const lines = [
        `AI Smartness Status`,
        `---`,
        `Agents: ${agentIds.length > 0 ? agentIds.join(', ') : 'none'}`,
        ...stateLines,
        `Project: ${currentProjectHash ?? 'unknown'}`,
        `Daemon: ${isDaemonAlive() ? 'running' : 'stopped'}`,
        `Auto-Wake: ${isEnabled ? 'enabled' : 'disabled'}`,
        `Communication: ${mode}`,
        `Pending signals: ${pendingCount}`,
        `Data dir: ${paths.dataDir()}`,
    ];
    vscode.window.showInformationMessage(lines.join('\n'), { modal: true });
}
function checkInboxCommand() {
    if (controllers.size === 0) {
        vscode.window.showWarningMessage('AI Smartness: No agents detected.');
        return;
    }
    let anySuccess = false;
    for (const ctrl of controllers.values()) {
        if (ctrl.forceCheck()) {
            anySuccess = true;
        }
    }
    if (anySuccess) {
        vscode.window.showInformationMessage(`AI Smartness: Inbox check injected`);
    }
    else {
        vscode.window.showWarningMessage(`AI Smartness: No idle Claude process found`);
    }
}
function toggleAutoWakeCommand() {
    isEnabled = !isEnabled;
    const config = vscode.workspace.getConfiguration('aiSmartness');
    config.update('enabled', isEnabled, vscode.ConfigurationTarget.Global);
    if (isEnabled) {
        startPolling();
        vscode.window.showInformationMessage('AI Smartness: Auto-Wake enabled');
    }
    else {
        stopPolling();
        vscode.window.showInformationMessage('AI Smartness: Auto-Wake disabled');
    }
}
async function selectAgentCommand() {
    if (!currentProjectHash) {
        vscode.window.showWarningMessage('AI Smartness: No project detected. Open a workspace folder first.');
        return;
    }
    const agents = cli.listAgents(currentProjectHash);
    if (agents.length === 0) {
        vscode.window.showWarningMessage('AI Smartness: No agents registered. Use the GUI or `ai-smartness agent add` to create one.');
        return;
    }
    const items = agents.map(a => ({
        label: a.id,
        description: `${a.role} (${a.mode})`,
        detail: a.supervisor ? `Supervisor: ${a.supervisor}` : undefined,
    }));
    items.push({
        label: '$(circle-slash) Clear agent binding',
        description: 'Remove session agent',
        detail: undefined,
    });
    const selected = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select agent profile for this session'
    });
    if (!selected) {
        return;
    }
    if (selected.label.includes('Clear')) {
        cli.selectAgent(null, currentProjectHash);
        vscode.window.showInformationMessage('AI Smartness: Agent binding cleared');
        log('Agent binding cleared');
    }
    else {
        cli.selectAgent(selected.label, currentProjectHash);
        vscode.window.showInformationMessage(`AI Smartness: Agent set to ${selected.label}`);
        log(`Agent selected: ${selected.label}`);
    }
}
async function openAgentWindowCommand() {
    if (!currentProjectHash) {
        vscode.window.showWarningMessage('AI Smartness: No project detected.');
        return;
    }
    const agents = cli.listAgents(currentProjectHash);
    if (agents.length === 0) {
        vscode.window.showWarningMessage('AI Smartness: No agents registered.');
        return;
    }
    const items = agents.map(a => ({
        label: a.id,
        description: `${a.role} (${a.mode})`,
    }));
    const selected = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select agent to open in new window'
    });
    if (!selected) {
        return;
    }
    const folders = vscode.workspace.workspaceFolders;
    if (!folders?.length) {
        return;
    }
    const rootPath = folders[0].uri.fsPath;
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
    cli.selectAgent(selected.label, currentProjectHash);
    const uri = vscode.Uri.file(workspacePath);
    await vscode.commands.executeCommand('vscode.openFolder', uri, true);
}
// =====================================================================
// Logging
// =====================================================================
function log(msg) {
    const ts = new Date().toISOString().substring(11, 19);
    outputChannel?.appendLine(`[${ts}] ${msg}`);
}
//# sourceMappingURL=extension.js.map
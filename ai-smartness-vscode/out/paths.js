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
exports.dataDir = dataDir;
exports.projectsDir = projectsDir;
exports.projectDir = projectDir;
exports.agentBeatPath = agentBeatPath;
exports.wakeSignalsDir = wakeSignalsDir;
exports.wakeSignalPath = wakeSignalPath;
exports.daemonPidPath = daemonPidPath;
exports.agentSessionPath = agentSessionPath;
exports.projectHash = projectHash;
exports.resolveProjectHash = resolveProjectHash;
exports.sessionAgentsDir = sessionAgentsDir;
exports.readSessionAgent = readSessionAgent;
exports.listSessionAgents = listSessionAgents;
exports.listSessionAgentsByPid = listSessionAgentsByPid;
exports.readAgentCliPid = readAgentCliPid;
exports.findMostRecentActiveAgent = findMostRecentActiveAgent;
const crypto = __importStar(require("crypto"));
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const os = __importStar(require("os"));
const PROJECT_HASH_LEN = 12;
/**
 * Cross-platform data directory.
 * Must match Rust path_utils::data_dir() exactly.
 */
function dataDir() {
    const override = process.env.AI_SMARTNESS_DATA_DIR;
    if (override) {
        return override;
    }
    const home = os.homedir();
    switch (process.platform) {
        case 'darwin':
            return path.join(home, 'Library', 'Application Support', 'ai-smartness');
        case 'win32':
            return path.join(process.env.APPDATA || path.join(home, 'AppData', 'Roaming'), 'ai-smartness');
        default:
            return path.join(process.env.XDG_CONFIG_HOME || path.join(home, '.config'), 'ai-smartness');
    }
}
function projectsDir() {
    return path.join(dataDir(), 'projects');
}
function projectDir(projectHash) {
    return path.join(projectsDir(), projectHash);
}
function agentBeatPath(projHash, agentId) {
    return path.join(projectDir(projHash), 'agents', agentId, 'beat.json');
}
function wakeSignalsDir() {
    return path.join(dataDir(), 'wake_signals');
}
function wakeSignalPath(agentId) {
    return path.join(wakeSignalsDir(), `${agentId}.signal`);
}
function daemonPidPath() {
    return path.join(dataDir(), 'daemon.pid');
}
function agentSessionPath(projHash) {
    return path.join(projectDir(projHash), 'session_agent');
}
/**
 * Compute project hash from canonical path.
 * Must match Rust id_gen::hash_path_string(): SHA-256, first 12 hex chars.
 */
function projectHash(canonicalPath) {
    return crypto
        .createHash('sha256')
        .update(canonicalPath)
        .digest('hex')
        .substring(0, PROJECT_HASH_LEN);
}
/**
 * Resolve project hash from a workspace folder path.
 * Uses realpathSync to match Rust's canonicalize().
 */
function resolveProjectHash(workspacePath) {
    try {
        const canonical = fs.realpathSync(workspacePath);
        return projectHash(canonical);
    }
    catch {
        return null;
    }
}
/**
 * Per-session agents directory.
 * Each file is named by session_id and contains a plaintext agent_id.
 */
function sessionAgentsDir(projHash) {
    return path.join(projectDir(projHash), 'session_agents');
}
/**
 * Read session agent from plaintext file (global fallback).
 */
function readSessionAgent(projHash) {
    const sessionPath = agentSessionPath(projHash);
    try {
        if (!fs.existsSync(sessionPath)) {
            return null;
        }
        const content = fs.readFileSync(sessionPath, 'utf8').trim();
        return content || null;
    }
    catch {
        return null;
    }
}
/**
 * List all agents bound to active sessions (panels) for this project.
 * Reads all files in session_agents/ directory. Returns unique agent IDs.
 */
function listSessionAgents(projHash) {
    const dir = sessionAgentsDir(projHash);
    try {
        if (!fs.existsSync(dir)) {
            return [];
        }
        const files = fs.readdirSync(dir);
        const agents = new Set();
        for (const file of files) {
            try {
                const content = fs.readFileSync(path.join(dir, file), 'utf8').trim();
                if (content) {
                    agents.add(content);
                }
            }
            catch {
                continue;
            }
        }
        return [...agents];
    }
    catch {
        return [];
    }
}
/**
 * List session agents as Map<MCP_PID, agentId>.
 * Each file in session_agents/ is named by MCP server PID.
 */
function listSessionAgentsByPid(projHash) {
    const dir = sessionAgentsDir(projHash);
    const result = new Map();
    try {
        if (!fs.existsSync(dir)) {
            return result;
        }
        for (const file of fs.readdirSync(dir)) {
            const pid = parseInt(file, 10);
            if (isNaN(pid)) {
                continue;
            }
            try {
                const agentId = fs.readFileSync(path.join(dir, file), 'utf8').trim();
                if (agentId) {
                    result.set(pid, agentId);
                }
            }
            catch {
                continue;
            }
        }
    }
    catch { /* ignore */ }
    return result;
}
/**
 * Read cli_pid from beat.json for a specific agent.
 * Returns null if beat.json missing or no cli_pid field.
 */
function readAgentCliPid(projHash, agentId) {
    try {
        const beatPath = agentBeatPath(projHash, agentId);
        if (!fs.existsSync(beatPath)) {
            return null;
        }
        const beat = JSON.parse(fs.readFileSync(beatPath, 'utf8'));
        return beat.cli_pid ?? null;
    }
    catch {
        return null;
    }
}
/**
 * Find the most recently active agent (by beat.json) that has a live MCP.
 * Used when monitoredPids is empty and grace period has expired.
 * Returns the agent with the freshest last_beat_at where pid is not null.
 */
function findMostRecentActiveAgent(projHash) {
    const agentsDir = path.join(projectDir(projHash), 'agents');
    try {
        if (!fs.existsSync(agentsDir)) { return null; }
        let best = null;
        let bestTime = '';
        for (const name of fs.readdirSync(agentsDir)) {
            const beatPath = path.join(agentsDir, name, 'beat.json');
            try {
                if (!fs.existsSync(beatPath)) { continue; }
                const beat = JSON.parse(fs.readFileSync(beatPath, 'utf8'));
                if (beat.pid == null) { continue; } // MCP not running
                const beatTime = beat.last_beat_at || '';
                if (beatTime > bestTime) {
                    bestTime = beatTime;
                    best = name;
                }
            } catch { continue; }
        }
        return best;
    } catch { return null; }
}
//# sourceMappingURL=paths.js.map
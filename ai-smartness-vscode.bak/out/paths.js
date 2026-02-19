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
exports.wakeSignalsDir = wakeSignalsDir;
exports.wakeSignalPath = wakeSignalPath;
exports.daemonSocketPath = daemonSocketPath;
exports.registryDbPath = registryDbPath;
exports.agentSessionPath = agentSessionPath;
exports.projectLogPath = projectLogPath;
exports.projectHash = projectHash;
exports.resolveProjectHash = resolveProjectHash;
exports.readSessionAgent = readSessionAgent;
const crypto = __importStar(require("crypto"));
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const os = __importStar(require("os"));
/**
 * Cross-platform data directory resolution.
 * Must match exactly what the Rust path_utils::data_dir() returns:
 *   Linux:   ~/.config/ai-smartness/
 *   macOS:   ~/Library/Application Support/ai-smartness/
 *   Windows: %APPDATA%/ai-smartness/
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
        default: // linux, freebsd, etc.
            return path.join(process.env.XDG_CONFIG_HOME || path.join(home, '.config'), 'ai-smartness');
    }
}
/** {data_dir}/projects/ */
function projectsDir() {
    return path.join(dataDir(), 'projects');
}
/** {data_dir}/projects/{hash}/ */
function projectDir(projectHash) {
    return path.join(projectsDir(), projectHash);
}
/** {data_dir}/wake_signals/ */
function wakeSignalsDir() {
    return path.join(dataDir(), 'wake_signals');
}
/** {data_dir}/wake_signals/{agentId}.signal */
function wakeSignalPath(agentId) {
    return path.join(wakeSignalsDir(), `${agentId}.signal`);
}
/** {data_dir}/processor.sock (global daemon) */
function daemonSocketPath() {
    return path.join(dataDir(), 'processor.sock');
}
/** {data_dir}/registry.db */
function registryDbPath() {
    return path.join(dataDir(), 'registry.db');
}
/** {data_dir}/projects/{hash}/session_agent */
function agentSessionPath(projectHash) {
    return path.join(projectDir(projectHash), 'session_agent');
}
/** {data_dir}/projects/{hash}/daemon.log */
function projectLogPath(projectHash) {
    return path.join(projectDir(projectHash), 'daemon.log');
}
/**
 * Compute project hash from a canonical path.
 * Must match Rust id_gen::hash_path_string():
 *   SHA-256(canonical_path) → first 12 hex chars
 */
function projectHash(canonicalPath) {
    const hash = crypto.createHash('sha256').update(canonicalPath).digest('hex');
    return hash.substring(0, 12);
}
/**
 * Resolve project hash for a workspace folder.
 * Uses fs.realpathSync to match Rust's canonicalize().
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
 * Read the session agent from the session file.
 * Returns agent_id or null if not set.
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
//# sourceMappingURL=paths.js.map
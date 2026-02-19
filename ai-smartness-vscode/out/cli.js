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
exports.resolveBinary = resolveBinary;
exports.listAgents = listAgents;
exports.selectAgent = selectAgent;
exports.daemonStatus = daemonStatus;
exports.daemonStart = daemonStart;
const child_process_1 = require("child_process");
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const os = __importStar(require("os"));
// ─── Binary Resolution ───
let cachedBin = null;
/**
 * Locate the ai-smartness binary.
 * Priority: which → ~/.local/bin → /usr/local/bin → bare name.
 */
function resolveBinary() {
    if (cachedBin) {
        return cachedBin;
    }
    try {
        const result = (0, child_process_1.execSync)('which ai-smartness 2>/dev/null', {
            timeout: 2000,
            encoding: 'utf8'
        });
        const p = result.trim();
        if (p) {
            cachedBin = p;
            return p;
        }
    }
    catch {
        // Not in PATH
    }
    const candidates = [
        path.join(os.homedir(), '.local', 'bin', 'ai-smartness'),
        '/usr/local/bin/ai-smartness',
    ];
    for (const c of candidates) {
        if (fs.existsSync(c)) {
            cachedBin = c;
            return c;
        }
    }
    cachedBin = 'ai-smartness';
    return cachedBin;
}
// ─── Agent Commands ───
/**
 * List agents for a project.
 * Parses text output: ID  ROLE  STATUS  SUPERVISOR  TEAM  MODE
 */
function listAgents(projectHash) {
    try {
        const bin = resolveBinary();
        const result = (0, child_process_1.execSync)(`${bin} agent list --project-hash ${projectHash} 2>/dev/null`, { timeout: 5000, encoding: 'utf8' });
        const agents = [];
        const lines = result.trim().split('\n');
        for (const line of lines) {
            // Skip header, separator, footer, empty
            if (line.startsWith('ID') ||
                line.startsWith('-') ||
                line.includes('Total:') ||
                !line.trim()) {
                continue;
            }
            const parts = line.trim().split(/\s{2,}/);
            if (parts.length >= 6) {
                agents.push({
                    id: parts[0],
                    role: parts[1],
                    status: parts[2],
                    supervisor: parts[3] === '-' ? null : parts[3],
                    team: parts[4] === '-' ? null : parts[4],
                    mode: parts[5],
                });
            }
        }
        return agents;
    }
    catch {
        return [];
    }
}
/**
 * Select an agent for a project session.
 * Pass null to clear the binding.
 */
function selectAgent(agentId, projectHash) {
    try {
        const bin = resolveBinary();
        const cmd = agentId
            ? `${bin} agent select ${agentId} --project-hash ${projectHash}`
            : `${bin} agent select --project-hash ${projectHash}`;
        (0, child_process_1.execSync)(`${cmd} 2>/dev/null`, { timeout: 5000, encoding: 'utf8' });
        return true;
    }
    catch {
        return false;
    }
}
// ─── Daemon Commands ───
/**
 * Get daemon status by parsing CLI output.
 */
function daemonStatus() {
    try {
        const bin = resolveBinary();
        const result = (0, child_process_1.execSync)(`${bin} daemon status 2>/dev/null`, {
            timeout: 3000,
            encoding: 'utf8'
        });
        if (result.includes('not running')) {
            return { status: 'stopped', pid: null };
        }
        if (result.includes('stale')) {
            return { status: 'stale', pid: null };
        }
        const pidMatch = result.match(/PID\s+(\d+)/);
        const pid = pidMatch ? parseInt(pidMatch[1], 10) : null;
        return { status: 'running', pid };
    }
    catch {
        return { status: 'stopped', pid: null };
    }
}
/**
 * Start the daemon.
 */
function daemonStart() {
    try {
        const bin = resolveBinary();
        const result = (0, child_process_1.execSync)(`${bin} daemon start 2>/dev/null`, {
            timeout: 5000,
            encoding: 'utf8'
        });
        return !result.toLowerCase().includes('error');
    }
    catch {
        return false;
    }
}
//# sourceMappingURL=cli.js.map
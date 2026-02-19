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
exports.readWakeSignal = readWakeSignal;
exports.acknowledgeSignal = acknowledgeSignal;
exports.listPendingSignals = listPendingSignals;
exports.cleanupOldSignals = cleanupOldSignals;
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const paths_1 = require("./paths");
/**
 * Read wake signal for a specific agent.
 * File: {data_dir}/wake_signals/{agentId}.signal
 */
function readWakeSignal(agentId) {
    const signalPath = path.join((0, paths_1.wakeSignalsDir)(), `${agentId}.signal`);
    try {
        if (!fs.existsSync(signalPath)) {
            return null;
        }
        const content = fs.readFileSync(signalPath, 'utf8');
        const signal = JSON.parse(content);
        return signal;
    }
    catch {
        return null;
    }
}
/**
 * Acknowledge a wake signal by setting acknowledged=true.
 */
function acknowledgeSignal(agentId) {
    const signalPath = path.join((0, paths_1.wakeSignalsDir)(), `${agentId}.signal`);
    try {
        if (!fs.existsSync(signalPath)) {
            return;
        }
        const content = fs.readFileSync(signalPath, 'utf8');
        const signal = JSON.parse(content);
        signal.acknowledged = true;
        signal.acknowledged_at = new Date().toISOString();
        fs.writeFileSync(signalPath, JSON.stringify(signal, null, 2));
    }
    catch {
        // Ignore errors — signal might have been deleted
    }
}
/**
 * List all pending (unacknowledged) wake signals.
 */
function listPendingSignals() {
    const dir = (0, paths_1.wakeSignalsDir)();
    try {
        if (!fs.existsSync(dir)) {
            return [];
        }
        const files = fs.readdirSync(dir).filter(f => f.endsWith('.signal'));
        const signals = [];
        for (const file of files) {
            try {
                const content = fs.readFileSync(path.join(dir, file), 'utf8');
                const signal = JSON.parse(content);
                if (!signal.acknowledged) {
                    signals.push(signal);
                }
            }
            catch {
                continue;
            }
        }
        return signals;
    }
    catch {
        return [];
    }
}
/**
 * Clean up old acknowledged signals (older than 5 minutes).
 */
function cleanupOldSignals() {
    const dir = (0, paths_1.wakeSignalsDir)();
    let deleted = 0;
    try {
        if (!fs.existsSync(dir)) {
            return 0;
        }
        const files = fs.readdirSync(dir).filter(f => f.endsWith('.signal'));
        const cutoff = Date.now() - 5 * 60 * 1000;
        for (const file of files) {
            try {
                const filePath = path.join(dir, file);
                const content = fs.readFileSync(filePath, 'utf8');
                const signal = JSON.parse(content);
                if (signal.acknowledged && signal.acknowledged_at) {
                    const ackTime = new Date(signal.acknowledged_at).getTime();
                    if (ackTime < cutoff) {
                        fs.unlinkSync(filePath);
                        deleted++;
                    }
                }
            }
            catch {
                continue;
            }
        }
    }
    catch {
        // Ignore
    }
    return deleted;
}
//# sourceMappingURL=wakeSignals.js.map
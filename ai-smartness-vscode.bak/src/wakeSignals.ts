import * as fs from 'fs';
import * as path from 'path';
import { wakeSignalsDir } from './paths';

export interface WakeSignal {
    agent_id: string;
    from: string;
    message: string;
    timestamp: string;
    acknowledged: boolean;
    acknowledged_at?: string;
}

/**
 * Read wake signal for a specific agent.
 * File: {data_dir}/wake_signals/{agentId}.signal
 */
export function readWakeSignal(agentId: string): WakeSignal | null {
    const signalPath = path.join(wakeSignalsDir(), `${agentId}.signal`);
    try {
        if (!fs.existsSync(signalPath)) { return null; }
        const content = fs.readFileSync(signalPath, 'utf8');
        const signal: WakeSignal = JSON.parse(content);
        return signal;
    } catch {
        return null;
    }
}

/**
 * Acknowledge a wake signal by setting acknowledged=true.
 */
export function acknowledgeSignal(agentId: string): void {
    const signalPath = path.join(wakeSignalsDir(), `${agentId}.signal`);
    try {
        if (!fs.existsSync(signalPath)) { return; }
        const content = fs.readFileSync(signalPath, 'utf8');
        const signal: WakeSignal = JSON.parse(content);
        signal.acknowledged = true;
        signal.acknowledged_at = new Date().toISOString();
        fs.writeFileSync(signalPath, JSON.stringify(signal, null, 2));
    } catch {
        // Ignore errors — signal might have been deleted
    }
}

/**
 * List all pending (unacknowledged) wake signals.
 */
export function listPendingSignals(): WakeSignal[] {
    const dir = wakeSignalsDir();
    try {
        if (!fs.existsSync(dir)) { return []; }
        const files = fs.readdirSync(dir).filter(f => f.endsWith('.signal'));
        const signals: WakeSignal[] = [];
        for (const file of files) {
            try {
                const content = fs.readFileSync(path.join(dir, file), 'utf8');
                const signal: WakeSignal = JSON.parse(content);
                if (!signal.acknowledged) {
                    signals.push(signal);
                }
            } catch {
                continue;
            }
        }
        return signals;
    } catch {
        return [];
    }
}

/**
 * Clean up old acknowledged signals (older than 5 minutes).
 */
export function cleanupOldSignals(): number {
    const dir = wakeSignalsDir();
    let deleted = 0;
    try {
        if (!fs.existsSync(dir)) { return 0; }
        const files = fs.readdirSync(dir).filter(f => f.endsWith('.signal'));
        const cutoff = Date.now() - 5 * 60 * 1000;
        for (const file of files) {
            try {
                const filePath = path.join(dir, file);
                const content = fs.readFileSync(filePath, 'utf8');
                const signal: WakeSignal = JSON.parse(content);
                if (signal.acknowledged && signal.acknowledged_at) {
                    const ackTime = new Date(signal.acknowledged_at).getTime();
                    if (ackTime < cutoff) {
                        fs.unlinkSync(filePath);
                        deleted++;
                    }
                }
            } catch {
                continue;
            }
        }
    } catch {
        // Ignore
    }
    return deleted;
}

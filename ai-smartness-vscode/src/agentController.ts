import * as stdinInjection from './stdinInjection';
import * as wakeSignals from './wakeSignals';
import type { WakeSignal } from './wakeSignals';

/**
 * Per-agent wake controller.
 *
 * Each agent has its own independent state machine driven by tick().
 * No blocking awaits — the tick loop calls tick() on every controller
 * and returns immediately. Each controller manages its own timing.
 *
 * States:
 *   idle       → watching for new wake signals
 *   pending    → signal detected, waiting for Claude process to be idle
 *   injecting  → writing to stdin (instant, non-blocking)
 *   cooldown   → debounce period after successful injection
 *   failed     → injection failed, waiting for retry or manual action
 */

type State = 'idle' | 'pending' | 'cooldown' | 'failed';

const COOLDOWN_MS = 10_000;
const IDLE_CHECK_INTERVAL_MS = 1_000;
const MAX_ATTEMPTS = 3;

export class AgentController {
    readonly agentId: string;
    readonly projectHash: string | null;
    private state: State = 'idle';
    private currentSignal: WakeSignal | null = null;
    private attempts = 0;
    private lastAttemptTime = 0;
    private cooldownUntil = 0;
    private processedSignalKeys = new Set<string>();
    private communicationMode: 'cognitive' | 'inbox' = 'cognitive';
    private onLog: (msg: string) => void;
    private onNotify: (msg: string) => void;
    private onWarn: (msg: string) => void;

    constructor(
        agentId: string,
        projectHash: string | null,
        callbacks: {
            onLog: (msg: string) => void;
            onNotify: (msg: string) => void;
            onWarn: (msg: string) => void;
        }
    ) {
        this.agentId = agentId;
        this.projectHash = projectHash;
        this.onLog = callbacks.onLog;
        this.onNotify = callbacks.onNotify;
        this.onWarn = callbacks.onWarn;
    }

    setMode(mode: 'cognitive' | 'inbox'): void {
        this.communicationMode = mode;
    }

    /**
     * Non-blocking tick. Called every poll interval.
     * Advances the state machine one step. Never blocks.
     */
    tick(autoPrompt: boolean): void {
        switch (this.state) {
            case 'idle':
                this.checkForSignal(autoPrompt);
                break;
            case 'pending':
                this.tryInjectNow();
                break;
            case 'cooldown':
                if (Date.now() >= this.cooldownUntil) {
                    this.state = 'idle';
                }
                break;
            case 'failed':
                // Stay failed until next signal or manual reset
                this.checkForSignal(autoPrompt);
                break;
        }
    }

    /**
     * Force a manual inbox check (from command).
     * Returns true if injection succeeded immediately.
     */
    forceCheck(): boolean {
        const text = stdinInjection.buildPromptText(
            this.agentId, 'user', 'Manual inbox check requested',
            this.communicationMode
        );
        const ok = stdinInjection.tryInjectSync(this.agentId, text, this.projectHash ?? undefined);
        if (ok) {
            this.enterCooldown();
        }
        return ok;
    }

    get pendingCount(): number {
        return wakeSignals.countPending(this.agentId);
    }

    get currentState(): State {
        return this.state;
    }

    // ─── State Machine ───

    private checkForSignal(autoPrompt: boolean): void {
        const signal = wakeSignals.readWakeSignal(this.agentId);
        if (!signal || signal.acknowledged) { return; }

        const key = `${signal.agent_id}_${signal.timestamp}`;
        if (this.processedSignalKeys.has(key)) { return; }

        this.processedSignalKeys.add(key);
        this.onLog(`Wake signal: ${this.agentId} from ${signal.from}`);
        this.onNotify(`AI Smartness: Message for ${this.agentId} from ${signal.from}`);

        if (!autoPrompt) {
            wakeSignals.acknowledgeSignal(this.agentId);
            return;
        }

        this.currentSignal = signal;
        this.attempts = 0;
        this.state = 'pending';
    }

    private tryInjectNow(): void {
        if (!this.currentSignal) {
            this.state = 'idle';
            return;
        }

        // Rate-limit attempts (don't spam every tick)
        if (Date.now() - this.lastAttemptTime < IDLE_CHECK_INTERVAL_MS) {
            return;
        }
        this.lastAttemptTime = Date.now();
        this.attempts++;

        // Use signal's mode if present, otherwise fall back to global config
        const mode = this.currentSignal.mode || this.communicationMode;

        const text = stdinInjection.buildPromptText(
            this.agentId,
            this.currentSignal.from,
            this.currentSignal.message,
            mode
        );

        const ok = stdinInjection.tryInjectSync(this.agentId, text, this.projectHash ?? undefined);

        if (ok) {
            this.onLog(`Injected wake to ${this.agentId}`);
            wakeSignals.acknowledgeSignal(this.agentId);
            this.currentSignal = null;
            this.enterCooldown();
            return;
        }

        if (this.attempts >= MAX_ATTEMPTS) {
            this.onLog(`Injection failed for ${this.agentId} after ${MAX_ATTEMPTS} attempts`);
            this.onWarn(
                `AI Smartness: Could not inject to ${this.agentId} — no idle Claude process. ` +
                `Use "AI Smartness: Check Inbox" manually.`
            );
            wakeSignals.acknowledgeSignal(this.agentId);
            this.currentSignal = null;
            this.state = 'failed';
        }
        // else: stay in 'pending', will retry next tick
    }

    private enterCooldown(): void {
        this.state = 'cooldown';
        this.cooldownUntil = Date.now() + COOLDOWN_MS;
    }

    /**
     * Purge old processed signal keys (memory cleanup).
     */
    cleanup(): void {
        if (this.processedSignalKeys.size > 100) {
            this.processedSignalKeys.clear();
        }
    }
}

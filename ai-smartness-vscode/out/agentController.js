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
exports.AgentController = void 0;
const stdinInjection = __importStar(require("./stdinInjection"));
const wakeSignals = __importStar(require("./wakeSignals"));
const COOLDOWN_MS = 10000;
const RETRY_BACKOFF_MS = 15000;
const IDLE_CHECK_INTERVAL_MS = 1000;
const MAX_ATTEMPTS = 3;
const MAX_RETRIES = 5;
class AgentController {
    constructor(agentId, projectHash, callbacks) {
        this.state = 'idle';
        this.currentSignal = null;
        this.attempts = 0;
        this.retryRounds = 0;
        this.lastAttemptTime = 0;
        this.cooldownUntil = 0;
        this.processedSignalKeys = new Set();
        this.communicationMode = 'cognitive';
        this.agentId = agentId;
        this.projectHash = projectHash;
        this.onLog = callbacks.onLog;
        this.onNotify = callbacks.onNotify;
        this.onWarn = callbacks.onWarn;
    }
    setMode(mode) {
        this.communicationMode = mode;
    }
    /**
     * Non-blocking tick. Called every poll interval.
     * Advances the state machine one step. Never blocks.
     */
    tick(autoPrompt) {
        switch (this.state) {
            case 'idle':
                this.checkForSignal(autoPrompt);
                break;
            case 'pending':
                this.tryInjectNow();
                break;
            case 'cooldown':
                if (Date.now() >= this.cooldownUntil) {
                    // If there's still a pending signal to deliver, retry injection
                    if (this.currentSignal) {
                        this.state = 'pending';
                    }
                    else {
                        this.retryRounds = 0;
                        this.state = 'idle';
                    }
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
    forceCheck() {
        const text = stdinInjection.buildPromptText(this.agentId, 'user', 'Manual inbox check requested', this.communicationMode);
        const ok = stdinInjection.tryInjectSync(this.agentId, text, this.projectHash ?? undefined);
        if (ok) {
            this.enterCooldown();
        }
        return ok;
    }
    get pendingCount() {
        return wakeSignals.countPending(this.agentId);
    }
    get currentState() {
        return this.state;
    }
    // ─── State Machine ───
    checkForSignal(autoPrompt) {
        const signal = wakeSignals.readWakeSignal(this.agentId);
        if (!signal || signal.acknowledged) {
            return;
        }
        const key = `${signal.agent_id}_${signal.timestamp}`;
        if (this.processedSignalKeys.has(key)) {
            return;
        }
        this.processedSignalKeys.add(key);
        this.onLog(`Wake signal: ${this.agentId} from ${signal.from}`);
        this.onNotify(`AI Smartness: Message for ${this.agentId} from ${signal.from}`);
        if (!autoPrompt) {
            wakeSignals.acknowledgeSignal(this.agentId);
            return;
        }
        this.currentSignal = signal;
        this.attempts = 0;
        this.retryRounds = 0;
        this.state = 'pending';
    }
    tryInjectNow() {
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
        const text = stdinInjection.buildPromptText(this.agentId, this.currentSignal.from, this.currentSignal.message, mode);
        const ok = stdinInjection.tryInjectSync(this.agentId, text, this.projectHash ?? undefined);
        if (ok) {
            this.onLog(`Injected wake to ${this.agentId}`);
            wakeSignals.acknowledgeSignal(this.agentId);
            this.currentSignal = null;
            this.retryRounds = 0;
            this.enterCooldown();
            return;
        }
        if (this.attempts >= MAX_ATTEMPTS) {
            this.retryRounds++;
            if (this.retryRounds >= MAX_RETRIES) {
                // Final give-up after all retry rounds
                this.onLog(`Injection failed for ${this.agentId} after ${MAX_RETRIES} retry rounds`);
                this.onWarn(`AI Smartness: Could not inject to ${this.agentId} — no idle Claude process. ` +
                    `Use "AI Smartness: Check Inbox" manually.`);
                wakeSignals.acknowledgeSignal(this.agentId);
                this.currentSignal = null;
                this.retryRounds = 0;
                this.state = 'failed';
            }
            else {
                // Retry: backoff then re-enter pending state
                this.onLog(`Injection round ${this.retryRounds}/${MAX_RETRIES} failed for ${this.agentId}, retrying in ${RETRY_BACKOFF_MS / 1000}s`);
                this.attempts = 0;
                this.cooldownUntil = Date.now() + RETRY_BACKOFF_MS;
                this.state = 'cooldown';
            }
        }
        // else: stay in 'pending', will retry next tick
    }
    enterCooldown() {
        this.state = 'cooldown';
        this.cooldownUntil = Date.now() + COOLDOWN_MS;
    }
    /**
     * Purge old processed signal keys (memory cleanup).
     */
    cleanup() {
        if (this.processedSignalKeys.size > 100) {
            this.processedSignalKeys.clear();
        }
    }
}
exports.AgentController = AgentController;
//# sourceMappingURL=agentController.js.map
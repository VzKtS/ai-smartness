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
exports.create = create;
exports.update = update;
exports.dispose = dispose;
const vscode = __importStar(require("vscode"));
let statusBarItem;
/**
 * Create and show the status bar item.
 */
function create() {
    if (statusBarItem) {
        return statusBarItem;
    }
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    statusBarItem.command = 'aiSmartnessWaker.showStatus';
    statusBarItem.show();
    return statusBarItem;
}
/**
 * Update the status bar with current state.
 */
function update(agentId, pendingCount, daemonAlive, enabled) {
    if (!statusBarItem) {
        return;
    }
    if (!enabled) {
        statusBarItem.text = '$(circle-slash) AI Off';
        statusBarItem.tooltip = 'AI Smartness auto-wake is disabled';
        statusBarItem.backgroundColor = undefined;
        return;
    }
    if (!agentId) {
        statusBarItem.text = '$(question) AI: No Agent';
        statusBarItem.tooltip = 'No agent detected. Run `ai-smartness agent select` or use the Select Agent command.';
        statusBarItem.backgroundColor = undefined;
        return;
    }
    const daemonIcon = daemonAlive ? '$(pulse)' : '$(warning)';
    const msgBadge = pendingCount > 0 ? ` $(mail) ${pendingCount}` : '';
    statusBarItem.text = `${daemonIcon} ${agentId}${msgBadge}`;
    const parts = [`Agent: ${agentId}`];
    parts.push(`Daemon: ${daemonAlive ? 'running' : 'stopped'}`);
    if (pendingCount > 0) {
        parts.push(`Pending messages: ${pendingCount}`);
    }
    statusBarItem.tooltip = parts.join('\n');
    statusBarItem.backgroundColor = pendingCount > 0
        ? new vscode.ThemeColor('statusBarItem.warningBackground')
        : undefined;
}
/**
 * Dispose the status bar item.
 */
function dispose() {
    statusBarItem?.dispose();
    statusBarItem = undefined;
}
//# sourceMappingURL=statusBar.js.map
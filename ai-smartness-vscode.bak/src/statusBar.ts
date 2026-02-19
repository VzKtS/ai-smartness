import * as vscode from 'vscode';

let statusBarItem: vscode.StatusBarItem | undefined;

/**
 * Create and show the status bar item.
 */
export function create(): vscode.StatusBarItem {
    if (statusBarItem) { return statusBarItem; }
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    statusBarItem.command = 'aiSmartnessWaker.showStatus';
    statusBarItem.show();
    return statusBarItem;
}

/**
 * Update the status bar with current state.
 */
export function update(agentId: string | null, pendingCount: number, daemonAlive: boolean, enabled: boolean): void {
    if (!statusBarItem) { return; }

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
export function dispose(): void {
    statusBarItem?.dispose();
    statusBarItem = undefined;
}

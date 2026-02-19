import * as vscode from 'vscode';

let statusBarItem: vscode.StatusBarItem | undefined;

export function create(): vscode.StatusBarItem {
    if (statusBarItem) { return statusBarItem; }
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    statusBarItem.command = 'aiSmartness.showStatus';
    statusBarItem.show();
    return statusBarItem;
}

/**
 * Update status bar with all active agents for this window.
 * @param agents - All agent IDs bound to sessions in this window (deduplicated)
 * @param pendingCount - Total pending wake signals across all agents
 * @param daemonAlive - Whether the daemon process is running
 * @param enabled - Whether auto-wake is enabled
 */
export function update(
    agents: string[],
    pendingCount: number,
    daemonAlive: boolean,
    enabled: boolean
): void {
    if (!statusBarItem) { return; }

    if (!enabled) {
        statusBarItem.text = '$(circle-slash) AI Off';
        statusBarItem.tooltip = 'AI Smartness auto-wake disabled';
        statusBarItem.backgroundColor = undefined;
        return;
    }

    if (agents.length === 0) {
        statusBarItem.text = '$(question) AI: No Agent';
        statusBarItem.tooltip = 'No agent bound. Use "AI Smartness: Select Agent"';
        statusBarItem.backgroundColor = undefined;
        return;
    }

    const daemonIcon = daemonAlive ? '$(pulse)' : '$(warning)';
    const agentLabel = agents.join(' | ');
    const msgBadge = pendingCount > 0 ? ` $(mail) ${pendingCount}` : '';
    statusBarItem.text = `${daemonIcon} ${agentLabel}${msgBadge}`;

    const parts = [`Agents: ${agents.join(', ')}`];
    parts.push(`Daemon: ${daemonAlive ? 'running' : 'stopped'}`);
    if (pendingCount > 0) {
        parts.push(`Pending messages: ${pendingCount}`);
    }
    statusBarItem.tooltip = parts.join('\n');

    statusBarItem.backgroundColor = pendingCount > 0
        ? new vscode.ThemeColor('statusBarItem.warningBackground')
        : undefined;
}

export function dispose(): void {
    statusBarItem?.dispose();
    statusBarItem = undefined;
}

import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';

export function registerCommands(context: vscode.ExtensionContext, client: LanguageClient) {
    const askCommand = vscode.commands.registerCommand('barqcoder.ask', async () => {
        const query = await vscode.window.showInputBox({
            prompt: 'Ask BarqCoder a question or give an instruction',
            placeHolder: 'e.g., Explain this function or Add error handling'
        });

        if (query) {
            vscode.window.showInformationMessage(`BarqCoder: Processing your query - "${query}"`);
            // Here we would typically send a custom request to the LSP server
            // e.g., client.sendRequest('barqcoder/ask', { query });
        }
    });

    context.subscriptions.push(askCommand);
}

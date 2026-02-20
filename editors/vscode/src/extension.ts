import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';
import { registerCommands } from './commands';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
    console.log('BarqCoder VSCode extension is now active!');

    // Initialize BarqCoder LSP server
    // We assume the binary `barqcoder` is in the PATH or we use a defined config path
    const serverExecutable = 'barqcoder';
    const serverOptions: ServerOptions = {
        run: { command: serverExecutable, args: ['--lsp'], transport: TransportKind.stdio },
        debug: { command: serverExecutable, args: ['--lsp'], transport: TransportKind.stdio }
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'rust' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/.clientrc')
        }
    };

    client = new LanguageClient(
        'barqCoderLSP',
        'BarqCoder Language Server',
        serverOptions,
        clientOptions
    );

    client.start();

    // Register User Commands
    registerCommands(context, client);
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}

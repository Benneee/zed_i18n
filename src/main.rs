mod cursor;
mod locales;

use std::collections::BTreeMap;
use std::path::PathBuf;

use dashmap::DashMap;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: DashMap<String, String>,
    // I want to save a copy of the translation files on initialization here
    // BTreeMap ensures the copy keys are in the same sorted alphabetical order everytime I have to check
    // The structure from outside looking in is:
    // Dashmap<String, ...> => language name ("en")
    // BTreeMap<String, String> => the copy key ("app.welcome")
    // Usage: self.copies.get(lang) provides the english copies and then, the list .get(copy_key) gives the text ("Welcome")
    copies: DashMap<String, BTreeMap<String, String>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        let copies: DashMap<String, BTreeMap<String, String>> = DashMap::new();

        Self {
            client,
            documents: DashMap::new(),
            copies,
        }
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Load every locale file in the folder
        let locales_directory = "test-fixtures/locales";

        // params.workspace_folders is the user's project that Zed provides to us via the lsp's initialize lifecycle
        let workspace_locales = locales_dir_path(params.workspace_folders, locales_directory);
        if let Some(dir) = workspace_locales {
            match locales::load_files(&dir) {
                Ok(locales) => {
                    for (lang, table) in locales {
                        self.copies.insert(lang, table);
                    }

                    // eprintln!("{:?}", self.copies)
                }
                Err(e) => eprintln!("failed to load translation files: {e}"),
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        will_save: None,
                        will_save_wait_until: None,
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "zed-i18n".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "lsp server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // User opens a file in the text editor and after the extension initialises,
    // we load the current file's content into documents for quick reference
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.insert(uri, params.text_document.text);
    }

    // When the user changes any text, we want to update the text we've saved in memory
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let uri = params.text_document.uri.to_string();
            self.documents.insert(uri, change.text);
        }
    }

    // After the user closes the file, we want to remove the saved text from the server
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.remove(&uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        // Hover returns position data
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        // We want to check where the copy key captured is in the text we've saved on the server
        // First, we check that we do have text saved
        let Some(file_content) = self.documents.get(&uri.to_string()) else {
            return Ok(None);
        };

        // Nothing in the position? Nothing to show
        let Some(copy_key) = cursor::get_word_at_position(&file_content, position) else {
            return Ok(None);
        };
        // Dashmap's content always returns randomly when looping through
        // Need to sort the content but because of the shard locks typical with Dashmap,
        // I need to clone the content and use accordingly
        let mut sorted_vec_copies: Vec<(String, Option<String>)> = self
            .copies
            .iter()
            // Here, we don't want to go over all the copies to find the word for our hovered copy_key
            // so, we use get to find it and move forward
            .map(|entry| (entry.key().clone(), entry.value().get(copy_key).cloned()))
            .collect();
        // Here, the sorting is done by key, which in this case will be the language
        sorted_vec_copies.sort_unstable_by(|a, b| a.0.cmp(&b.0));

        let mut markdown = format!("**{copy_key}**\n");
        let mut found_a_copy = false;
        // Here, we assemble the copies for the key as they match our hovered copy_key
        for (lang, text) in sorted_vec_copies {
            if let Some(text) = text {
                found_a_copy = true;
                markdown.push_str(&format!("- {lang}: {text}\n"));
            }
        }

        if !found_a_copy {
            return Ok(None);
        }

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                // Now, the zed popup shows the assembled copies generated above
                value: markdown,
            }),
            range: None,
        }))
    }
}

// Builds the path to the locales folder inside the user's project.
// Takes the first workspace folder Zed sends in `initialize`, turns its
// URI (file:///...) into a normal path, and joins `locales_directory` onto it.
// Returns None if there's no workspace folder or the URI isn't a file path.
// Note: this only builds the path. It doesn't check that the folder exists;
// `locales::load_files` will report an error if it doesn't.
fn locales_dir_path(
    workspace_folders: Option<Vec<WorkspaceFolder>>,
    locales_directory: &str,
) -> Option<PathBuf> {
    let folders = workspace_folders?;
    let folder = folders.first()?;
    let root = folder.uri.to_file_path()?;
    Some(root.join(locales_directory))
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

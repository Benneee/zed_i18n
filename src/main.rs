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

// Folders where projects usually keep their translation files, checked in order.
// The first one that exists in the user's project is used.
const LOCALE_DIRS: &[&str] = &[
    "public/locales", // react-i18next
    "src/locales",    // vue-i18n
    "src/i18n",
    "src/assets/i18n", // Angular (ngx-translate)
    "translations",    // ember-intl
    "locales",
    "i18n",
    "lang",
    "test-fixtures/locales", // only for testing with scripts/test_lsp.py
];

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // params.workspace_folders is the user's project that Zed provides to us via the lsp's initialize lifecycle
        if let Some(workspace_root) = project_root(params.workspace_folders) {
            let mut locales_dir = None;

            // Find the first locale folder that exists
            for locale_dir in LOCALE_DIRS {
                let workspace_locales_dir = workspace_root.join(locale_dir);
                if workspace_locales_dir.is_dir() {
                    locales_dir = Some(workspace_locales_dir);
                    break;
                }
            }

            if let Some(dir) = locales_dir {
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
        let Some(word_context) = cursor::get_word_with_surroundings(&file_content, position) else {
            return Ok(None);
        };
        let copy_key = word_context.word;
        let before_copy_key = word_context.before;
        let after_copy_key = word_context.after;

        if !cursor::is_translation_call(&before_copy_key, &after_copy_key) {
            return Ok(None);
        }
        // Dashmap's content always returns randomly when looping through
        // Need to sort the content but because of the shard locks typical with Dashmap,
        // I need to clone the content and use accordingly
        let mut sorted_vec_copies: Vec<(String, Option<String>)> = self
            .copies
            .iter()
            // Here, we don't want to go over all the copies to find the word for our hovered copy_key
            // so, we use get to find it and move forward
            .map(|entry| (entry.key().clone(), entry.value().get(&copy_key).cloned()))
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

// Finds the user's project folder.
// Takes the first workspace folder Zed sends in `initialize` and turns its
// URI (file:///...) into a normal path, e.g. /Users/mac/my-app.
// Returns None if there's no workspace folder or the URI isn't a file path.
// `into_owned()` makes a real copy of the path, because the original lives
// inside `workspace_folders`, which is thrown away when this function ends.
// Note: this only finds the project folder. `initialize` then joins each
// name from `LOCALE_DIRS` onto it to look for the translations folder.
fn project_root(workspace_folders: Option<Vec<WorkspaceFolder>>) -> Option<PathBuf> {
    let folders = workspace_folders?;
    let folder = folders.first()?;
    let root = folder.uri.to_file_path()?;
    Some(root.into_owned())
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

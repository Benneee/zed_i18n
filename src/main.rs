use dashmap::DashMap;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: DashMap<String, String>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            // I will add a method to call the copy extraction bit here
            // copies_provider: <the name of the library for the copies here>
        }
    }

    // The functionality consumed from the copies extraction after the hover will go here
    // async fn provide_copies() {...}
}

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.insert(uri, params.text_document.text);
    }

    // When the user changes any text, we want to update the text we've saved in memory
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let uri = params.text_document.uri.to_string();
            self.documents.insert(uri, change.text.clone());
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
        let Some(word) = get_word_at_position(&file_content, position) else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{word}**"),
            }),
            range: None,
        }))
    }
}

// Characters that can be part of a translation key like dash
fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '-'
}

// LSP columns count UTF-16 code units, but Rust slices by bytes.
// This converts one into the other. Returns None if the column is
// past the end of the line or lands in the middle of a character.
fn utf16_to_byte_offset(line: &str, utf16_col: usize) -> Option<usize> {
    let mut utf16_count = 0;
    for (byte_idx, c) in line.char_indices() {
        if utf16_count == utf16_col {
            return Some(byte_idx);
        }
        utf16_count += c.len_utf16();
        if utf16_count > utf16_col {
            return None; // the column found a multi-unit character
        }
    }
    // And if the cursor is at the end of the line
    if utf16_count == utf16_col {
        Some(line.len())
    } else {
        None
    }
}

fn get_word_at_position(content: &str, position: Position) -> Option<&str> {
    // The line the user is hovering over
    let line_text = content.lines().nth(position.line as usize)?;

    // Cursor as a byte offset
    let cursor = utf16_to_byte_offset(line_text, position.character as usize)?;

    // Start: track backwards to the first non-key character
    let start = line_text[..cursor]
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_key_char(c))
        .map(|(idx, c)| idx + c.len_utf8()) // word starts after that character
        .unwrap_or(0);

    // End: walk forwards to the first non-key character
    let end = line_text[cursor..]
        .find(|c: char| !is_key_char(c))
        .map(|idx| cursor + idx)
        .unwrap_or(line_text.len());

    if start < end {
        Some(&line_text[start..end])
    } else {
        None
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

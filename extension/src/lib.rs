use zed_extension_api as zed;

struct I18nExtension;

impl zed::Extension for I18nExtension {
    fn new() -> Self {
        I18nExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let Some(path) = worktree.which("zed_i18n") else {
            return Err(
                "zed_i18n not found on PATH. Run 'cargo install --path .' in the server folder."
                    .into(),
            );
        };

        Ok(zed::Command {
            command: path,
            args: vec![],
            env: vec![],
        })
    }
}

zed::register_extension!(I18nExtension);

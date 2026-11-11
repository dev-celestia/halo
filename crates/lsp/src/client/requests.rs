//! Typed wrappers over the `textDocument/*` server requests.

use lsp_types::request::{
    CodeActionRequest, Completion, Formatting, GotoDefinition, HoverRequest, Request as _,
};
use lsp_types::*;
use serde_json::Value;

use super::LanguageClient;
use super::error::LspResult;
use super::normalize::{normalize_code_actions, normalize_definition};

impl LanguageClient {
    pub fn completion(
        &self,
        uri: Uri,
        position: Position,
        context: Option<CompletionContext>,
    ) -> LspResult<CompletionResponse> {
        self.request(
            Completion::METHOD,
            CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context,
            },
        )
    }

    pub fn hover(&self, uri: Uri, position: Position) -> LspResult<Option<Hover>> {
        self.request(
            HoverRequest::METHOD,
            HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position,
                },
                work_done_progress_params: Default::default(),
            },
        )
    }

    pub fn goto_definition(&self, uri: Uri, position: Position) -> LspResult<Vec<LocationLink>> {
        let value: Value = self.request(
            GotoDefinition::METHOD,
            GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )?;
        Ok(normalize_definition(value))
    }

    pub fn code_actions(
        &self,
        uri: Uri,
        range: Range,
        context: CodeActionContext,
    ) -> LspResult<Vec<CodeAction>> {
        let value: Value = self.request(
            CodeActionRequest::METHOD,
            CodeActionParams {
                text_document: TextDocumentIdentifier { uri },
                range,
                context,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )?;
        Ok(normalize_code_actions(value))
    }

    pub fn formatting(&self, uri: Uri, options: FormattingOptions) -> LspResult<Vec<TextEdit>> {
        let value: Option<Vec<TextEdit>> = self.request(
            Formatting::METHOD,
            DocumentFormattingParams {
                text_document: TextDocumentIdentifier { uri },
                options,
                work_done_progress_params: Default::default(),
            },
        )?;
        Ok(value.unwrap_or_default())
    }
}

use std::{
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    ops,
    path::{Component, Path, PathBuf, Prefix},
    process::{Command, Stdio},
    str::FromStr,
    sync::{atomic::AtomicU64, mpsc, Arc, Mutex},
    thread,
};

use caarr::EventChannel;
use lsp_types::{
    notification::{self, Notification, PublishDiagnostics},
    request, CompletionClientCapabilities, CompletionContext, CompletionItemCapability,
    CompletionItemCapabilityResolveSupport, CompletionItemKindCapability, CompletionItemTag,
    CompletionListCapability, CompletionParams, CompletionResponse, CompletionTriggerKind,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, InitializedParams,
    InsertTextMode, InsertTextModeSupport, MarkupKind, Position, PublishDiagnosticsParams, Range,
    SemanticToken, SemanticTokenType, SemanticTokensClientCapabilities,
    SemanticTokensClientCapabilitiesRequests, SemanticTokensDeltaParams,
    SemanticTokensFullDeltaResult, SemanticTokensFullOptions, SemanticTokensParams,
    SemanticTokensResult, SemanticTokensServerCapabilities, TagSupport,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TextDocumentSyncClientCapabilities, TokenFormat,
    VersionedTextDocumentIdentifier,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::{
    editor::{self, Diagnostic, Edit, TextPosition, TextRange},
    Event,
};

pub enum ResponseEvent {
    SemanticTokens {
        file: PathBuf,
        version: u32,
        highlights: Vec<Highlight>,
        result_id: Option<String>,
    },
    SemanticTokensDelta {
        file: PathBuf,
        version: u32,
        highlights: HighlightsDelta,
        result_id: Option<String>,
    },
    Diagnostics {
        version: u32,
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
    },
    Completion {
        file: PathBuf,
        token: u32,
        items: Vec<editor::CompletionItem>,
    },
}

pub enum RequestEvent {
    SemanticTokens {
        file: PathBuf,
        version: u32,
    },
    SemanticTokensDelta {
        file: PathBuf,
        version: u32,
        previous_result_id: String,
    },
    DidOpen {
        file: PathBuf,
        text: String,
        version: u32,
    },
    DidChange {
        file: PathBuf,
        edits: Vec<Edit<'static>>,
        version: u32,
    },
    Completion {
        file: PathBuf,
        token: u32,
        position: TextPosition,
        invoked: bool,
    },
}

#[derive(Clone)]
pub struct LspHandle {
    pub request_sender: mpsc::Sender<RequestEvent>,
    pub completion_trigger_chars: Arc<Mutex<Vec<char>>>,
}

#[derive(Serialize)]
struct Request<'r, Params> {
    #[allow(dead_code)]
    jsonrpc: &'r str,
    id: Option<u64>,
    method: &'r str,
    params: Params,
}

// #[derive(Deserialize, Serialize)]
// struct Response<'r, Result> {
//     #[allow(dead_code)]
//     jsonrpc: &'r str,
//     id: Option<u64>,
//     result: Result,
// }

#[derive(Deserialize)]
struct LspMessage<'r> {
    #[allow(dead_code)]
    jsonrpc: &'r str,
    id: Option<u64>,
    method: Option<&'r str>,
    params: Option<&'r RawValue>,
    #[serde(default)]
    result: Missing<&'r RawValue>,
}

#[derive(Deserialize)]
#[serde(from = "T")]
enum Missing<T> {
    Some(T),
    None,
}

impl<T> From<T> for Missing<T> {
    fn from(value: T) -> Self {
        Missing::Some(value)
    }
}

impl<T> Default for Missing<T> {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Deserialize)]
struct AnyResponse {
    id: u64,
    result_range: ops::Range<usize>,
    whole_message: String,
}

pub fn start_server(exe_path: PathBuf, app_channel: EventChannel<Event>) -> LspHandle {
    let (request_tx, request_rx) = mpsc::channel();
    let completion_trigger_chars = Arc::new(Mutex::new(vec![]));
    let completion_trigger_chars_out = completion_trigger_chars.clone();

    thread::spawn(move || {
        let mut server_process = Command::new(exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let reader = start_reader(server_process.stdout.take().unwrap(), app_channel.clone());
        let mut writer = BufWriter::new(server_process.stdin.take().unwrap());

        let init_response =
            send_request::<request::Initialize>(&mut writer, &reader, initialize()).unwrap();

        send_notification::<notification::Initialized>(&mut writer, InitializedParams {}).unwrap();

        let trigger_characters: Vec<_> = init_response
            .capabilities
            .completion_provider
            .and_then(|provider| provider.trigger_characters)
            .unwrap_or_default()
            .into_iter()
            .flat_map(|string| string.chars().next())
            .collect();

        *completion_trigger_chars.lock().unwrap() = trigger_characters;

        let tokens_legend = init_response
            .capabilities
            .semantic_tokens_provider
            .and_then(|provider| match provider {
                SemanticTokensServerCapabilities::SemanticTokensOptions(options) => Some(
                    options
                        .legend
                        .token_types
                        .into_iter()
                        .map(|token| &*Box::leak(token.as_str().to_string().into_boxed_str()))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            });

        for request in request_rx {
            match request {
                RequestEvent::DidOpen {
                    file,
                    text,
                    version,
                } => {
                    send_notification::<notification::DidOpenTextDocument>(
                        &mut writer,
                        DidOpenTextDocumentParams {
                            text_document: TextDocumentItem {
                                uri: path_to_uri(&file),
                                language_id: "rust".to_string(),
                                version: version as i32,
                                text,
                            },
                        },
                    )
                    .unwrap();
                }
                RequestEvent::DidChange {
                    file,
                    edits,
                    version,
                } => {
                    send_notification::<notification::DidChangeTextDocument>(
                        &mut writer,
                        DidChangeTextDocumentParams {
                            content_changes: edits
                                .into_iter()
                                .map(|edit| TextDocumentContentChangeEvent {
                                    range: Some(Range {
                                        start: Position {
                                            line: edit.start.line,
                                            character: edit.start.byte,
                                        },
                                        end: Position {
                                            line: edit.start.line,
                                            character: edit.start.byte,
                                        },
                                    }),
                                    range_length: None,
                                    text: edit.text.to_string(),
                                })
                                .collect(),
                            text_document: VersionedTextDocumentIdentifier {
                                uri: path_to_uri(&file),
                                version: version as i32,
                            },
                        },
                    )
                    .unwrap();
                }
                RequestEvent::SemanticTokens { file, version } => {
                    let response = send_request::<request::SemanticTokensFullRequest>(
                        &mut writer,
                        &reader,
                        SemanticTokensParams {
                            partial_result_params: Default::default(),
                            work_done_progress_params: Default::default(),
                            text_document: TextDocumentIdentifier {
                                uri: path_to_uri(&file),
                            },
                        },
                    )
                    .unwrap();

                    if let Some(response) = response {
                        match response {
                            SemanticTokensResult::Tokens(tokens) => {
                                if let Some(legend) = &tokens_legend {
                                    app_channel.send_event(Event::Lsp(
                                        ResponseEvent::SemanticTokens {
                                            file,
                                            highlights: decode_semantic_tokens(
                                                &tokens.data,
                                                legend,
                                            )
                                            .collect(),
                                            version,
                                            result_id: tokens.result_id,
                                        },
                                    ));
                                }
                            }
                            SemanticTokensResult::Partial(_) => {}
                        }
                    }
                }
                RequestEvent::SemanticTokensDelta {
                    file,
                    version,
                    previous_result_id,
                } => {
                    if let Some(response) = send_request::<request::SemanticTokensFullDeltaRequest>(
                        &mut writer,
                        &reader,
                        SemanticTokensDeltaParams {
                            partial_result_params: Default::default(),
                            work_done_progress_params: Default::default(),
                            text_document: TextDocumentIdentifier {
                                uri: path_to_uri(&file),
                            },
                            previous_result_id,
                        },
                    )
                    .unwrap()
                    {
                        match response {
                            SemanticTokensFullDeltaResult::Tokens(tokens) => {
                                if let Some(legend) = &tokens_legend {
                                    app_channel.send_event(Event::Lsp(
                                        ResponseEvent::SemanticTokens {
                                            file,
                                            highlights: decode_semantic_tokens(
                                                &tokens.data,
                                                legend,
                                            )
                                            .collect(),
                                            version,
                                            result_id: tokens.result_id,
                                        },
                                    ));
                                }
                            }
                            SemanticTokensFullDeltaResult::TokensDelta(tokens_delta) => {
                                if let Some(legend) = &tokens_legend {
                                    app_channel.send_event(Event::Lsp(
                                        ResponseEvent::SemanticTokensDelta {
                                            file,
                                            version,
                                            highlights: HighlightsDelta {
                                                edits: tokens_delta
                                                    .edits
                                                    .into_iter()
                                                    .map(|edit| {
                                                        assert_eq!(edit.start % 5, 0);
                                                        assert_eq!(edit.delete_count % 5, 0);
                                                        HighlightsEdit {
                                                            start: edit.start / 5,
                                                            delete_count: edit.delete_count / 5,
                                                            highligts: decode_semantic_tokens(
                                                                edit.data.as_deref().unwrap_or(&[]),
                                                                &legend,
                                                            )
                                                            .collect(),
                                                        }
                                                    })
                                                    .collect(),
                                            },
                                            result_id: tokens_delta.result_id,
                                        },
                                    ));
                                }
                            }
                            SemanticTokensFullDeltaResult::PartialTokensDelta { .. } => {}
                        }
                    }
                }
                RequestEvent::Completion {
                    file,
                    position,
                    token,
                    invoked,
                } => {
                    if let Some(response) = send_request::<request::Completion>(
                        &mut writer,
                        &reader,
                        CompletionParams {
                            partial_result_params: Default::default(),
                            work_done_progress_params: Default::default(),
                            context: Some(CompletionContext {
                                trigger_kind: match invoked {
                                    true => CompletionTriggerKind::INVOKED,
                                    false => CompletionTriggerKind::TRIGGER_CHARACTER,
                                },
                                trigger_character: None,
                            }),
                            text_document_position: TextDocumentPositionParams {
                                position: Position {
                                    line: position.line,
                                    character: position.byte,
                                },
                                text_document: TextDocumentIdentifier {
                                    uri: path_to_uri(&file),
                                },
                            },
                        },
                    )
                    .unwrap()
                    {
                        let (items, is_incomplete) = match response {
                            CompletionResponse::Array(array) => (array, false),
                            CompletionResponse::List(list) => (list.items, list.is_incomplete),
                        };

                        app_channel.send_event(Event::Lsp(ResponseEvent::Completion {
                            file,
                            token,
                            items: items
                                .into_iter()
                                .map(|item| editor::CompletionItem { label: item.label })
                                .collect(),
                        }));
                    }
                }
            }
        }
    });

    LspHandle {
        request_sender: request_tx,
        completion_trigger_chars: completion_trigger_chars_out,
    }
}

fn start_reader(
    reader: impl Read + Send + 'static,
    app_channel: EventChannel<Event>,
) -> mpsc::Receiver<AnyResponse> {
    let mut reader = BufReader::new(reader);

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || loop {
        if let Some(body) = get_body(&mut reader) {
            let message: LspMessage = match serde_json::from_str(&body) {
                Ok(message) => message,
                Err(e) => {
                    eprintln!("{body}");
                    panic!("{}", e);
                }
            };
            if let Missing::Some(result) = message.result {
                if let Some(id) = message.id {
                    tx.send(AnyResponse {
                        id,
                        result_range: {
                            let base = body.as_str().as_ptr() as usize;
                            let start = result.get().as_ptr() as usize;
                            let start_offset = start - base;
                            start_offset..(start_offset + result.get().len())
                        },
                        whole_message: body,
                    })
                    .unwrap();
                }
            } else if let Some(method) = message.method {
                match method {
                    notification::PublishDiagnostics::METHOD => {
                        let body: PublishDiagnosticsParams =
                            serde_json::from_str(message.params.unwrap().get()).unwrap();
                        if let Some((path, version)) =
                            body.uri.as_str().strip_prefix("file:///").zip(body.version)
                        {
                            match PathBuf::from(path).canonicalize() {
                                Err(err) => eprintln!("oof {err} - {path}"),
                                Ok(path) => {
                                    eprintln!("sending diagnostics");
                                    app_channel.send_event(Event::Lsp(
                                        ResponseEvent::Diagnostics {
                                            path,
                                            version: version as u32,
                                            diagnostics: body
                                                .diagnostics
                                                .into_iter()
                                                .map(|diagnostic| Diagnostic {
                                                    message: diagnostic.message,
                                                    range: TextRange {
                                                        start: TextPosition {
                                                            byte: diagnostic.range.start.character,
                                                            line: diagnostic.range.start.line,
                                                        },
                                                        end: TextPosition {
                                                            byte: diagnostic.range.end.character,
                                                            line: diagnostic.range.end.line,
                                                        },
                                                    },
                                                })
                                                .collect(),
                                        },
                                    ));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                eprintln!("unknown message: {}", body);
            }
        }
    });

    rx
}

fn decode_semantic_tokens<'a>(
    tokens: &'a [SemanticToken],
    legend: &'a [&str],
) -> impl ExactSizeIterator<Item = Highlight> + 'a {
    let mut line = 0;
    let mut byte = 0;

    tokens.iter().map(move |token| {
        line += token.delta_line;
        if token.delta_line != 0 {
            byte = 0;
        }
        byte += token.delta_start;

        Highlight {
            line,
            start_byte: byte,
            end_byte: byte + token.length,
            color: match legend[token.token_type as usize] {
                "type" => [0, 200, 0, 255],
                "class" => [0, 200, 0, 255],
                "enum" => [0, 0, 200, 255],
                "interface" => [0, 200, 0, 255],
                "struct" => [0, 200, 0, 255],
                "typeParameter" => [0, 200, 0, 255],
                "parameter" => [0, 0, 100, 255],
                "variable" => [0, 0, 100, 255],
                "property" => [0, 0, 100, 255],
                "enumMember" => [0, 0, 0, 255],
                "event" => [0, 0, 0, 255],
                "function" => [100, 100, 0, 255],
                "method" => [100, 100, 0, 255],
                "macro" => [0, 0, 0, 255],
                "keyword" => [0, 0, 0, 255],
                "modifier" => [0, 0, 0, 255],
                "comment" => [0, 0, 0, 255],
                "string" => [0, 0, 0, 255],
                "number" => [0, 0, 0, 255],
                "regexp" => [0, 0, 0, 255],
                "operator" => [0, 0, 0, 255],
                _ => [0, 0, 0, 255],
            },
        }
    })
}

#[derive(Debug)]
pub struct HighlightsDelta {
    edits: Vec<HighlightsEdit>,
}

#[derive(Debug)]
struct HighlightsEdit {
    start: u32,
    delete_count: u32,
    highligts: Vec<Highlight>,
}

impl HighlightsDelta {
    pub fn apply_to(
        &self,
        highlights: &mut Vec<Highlight>,
        mut on_highlight: impl FnMut(&Highlight),
    ) {
        for edit in &self.edits {
            eprintln!(
                "{:?} {:?} {:?} {:?}",
                highlights.len(),
                edit.start,
                edit.delete_count,
                edit.highligts.len()
            );

            let start = edit.start as usize;
            let end = start + edit.delete_count as usize;

            let (start_line, start_byte) = start
                .checked_sub(1)
                .map(|idx| {
                    let highlight = &highlights[idx];
                    (highlight.line, highlight.start_byte)
                })
                .unwrap_or((0, 0));

            highlights.splice(
                start..end,
                edit.highligts.iter().map(|relative| {
                    let start_byte = if relative.line == 0 { start_byte } else { 0 };
                    let new_highlight = Highlight {
                        line: relative.line + start_line,
                        start_byte: relative.start_byte + start_byte,
                        end_byte: relative.end_byte + start_byte,
                        color: relative.color,
                    };
                    on_highlight(&new_highlight);
                    new_highlight
                }),
            );
        }
    }
}

#[derive(Debug)]
pub struct Highlight {
    pub line: u32,
    pub start_byte: u32,
    pub end_byte: u32,
    pub color: [u8; 4],
}

fn send_request<R: request::Request>(
    writer: &mut impl Write,
    reader: &mpsc::Receiver<AnyResponse>,
    params: R::Params,
) -> io::Result<R::Result> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let request = serde_json::to_string(&Request {
        jsonrpc: "2.0",
        id: Some(id),
        method: R::METHOD,
        params,
    })
    .unwrap();
    println!("{}", &request);
    writer.write_all(format!("Content-Length: {}\r\n\r\n", request.len()).as_bytes())?;
    writer.write_all(request.as_bytes())?;
    writer.flush()?;
    loop {
        let response = reader.recv().unwrap();
        let result: R::Result =
            serde_json::from_str(&response.whole_message[response.result_range]).unwrap();
        if response.id == id {
            return Ok(result);
        }
    }
}

fn send_notification<N: notification::Notification>(
    writer: &mut impl Write,
    params: N::Params,
) -> io::Result<()> {
    let notification = serde_json::to_string(&Request {
        jsonrpc: "2.0",
        id: None,
        method: N::METHOD,
        params,
    })
    .unwrap();
    println!("{}", &notification);
    writer.write_all(format!("Content-Length: {}\r\n\r\n", notification.len()).as_bytes())?;
    writer.write_all(notification.as_bytes())?;
    writer.flush()?;
    Ok(())
}

fn get_body(reader: &mut impl BufRead) -> Option<String> {
    let mut content_type = None;
    let mut content_length = None;
    loop {
        let mut header = String::new();
        let Ok(bytes_read) = reader.read_line(&mut header) else {
            eprintln!("failed to read server response");
            return None;
        };

        if bytes_read == 0 {
            eprintln!("received incomplete request");
            return None;
        };

        let header = header.trim();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(": ") else {
            eprintln!("invalid header");
            return None;
        };

        match name {
            "Content-Type" => {
                content_type = Some(value.to_string());
            }
            "Content-Length" => {
                content_length = Some(value.to_string());
            }
            _ => {}
        }
    }

    let Some(len) = content_length else {
        eprintln!("missing content length");
        return None;
    };

    let Ok(len) = len.parse() else {
        eprintln!("invalid content length");

        return None;
    };

    if let Some(ty) = &content_type {
        if ty != "utf8" {
            eprintln!("invalid content type");
            return None;
        }
    }

    let mut body = vec![0; len];
    let Ok(_) = reader.read_exact(&mut body) else {
        eprintln!("failed to read server response");
        return None;
    };
    let Ok(body) = String::from_utf8(body) else {
        eprintln!("server response is not a valid utf-8 string");
        return None;
    };
    Some(body)
}

fn initialize() -> InitializeParams {
    lsp_types::InitializeParams {
        process_id: Some(std::process::id()),
        root_path: None,
        root_uri: Some(path_to_uri(&std::env::current_dir().unwrap())),
        initialization_options: None,
        capabilities: lsp_types::ClientCapabilities {
            workspace: None,
            // workspace: Some(lsp_types::WorkspaceClientCapabilities {
            //     apply_edit: Some(true),
            //     workspace_edit: None,
            //     did_change_configuration: None,
            //     did_change_watched_files: None,
            //     symbol: Default::default(),
            //     execute_command: None,
            //     workspace_folders: None,
            //     configuration: None,
            //     semantic_tokens: None,
            //     code_lens: None,
            //     file_operations: Some(lsp_types::WorkspaceFileOperationsClientCapabilities {
            //         dynamic_registration: Some(false),
            //         did_create: Some(true),
            //         will_create: Some(false),
            //         did_rename: Some(true),
            //         will_rename: Some(false),
            //         did_delete: Some(true),
            //         will_delete: Some(false),
            //     }),
            //     inline_value: None,
            //     inlay_hint: None,
            //     diagnostic: None,
            // }),
            text_document: Some(lsp_types::TextDocumentClientCapabilities {
                synchronization: Some(TextDocumentSyncClientCapabilities {
                    dynamic_registration: None,
                    will_save: None,
                    will_save_wait_until: None,
                    did_save: None,
                }),
                completion: Some(CompletionClientCapabilities {
                    dynamic_registration: None,
                    completion_item: Some(CompletionItemCapability {
                        snippet_support: None,
                        commit_characters_support: Some(true),
                        documentation_format: Some(vec![
                            MarkupKind::Markdown,
                            MarkupKind::PlainText,
                        ]),
                        deprecated_support: Some(true),
                        preselect_support: Some(true),
                        tag_support: Some(TagSupport {
                            value_set: vec![CompletionItemTag::DEPRECATED],
                        }),
                        insert_replace_support: Some(true),
                        resolve_support: Some(CompletionItemCapabilityResolveSupport {
                            properties: vec!["documentation".to_string()],
                        }),
                        insert_text_mode_support: Some(InsertTextModeSupport {
                            value_set: vec![
                                InsertTextMode::AS_IS,
                                InsertTextMode::ADJUST_INDENTATION,
                            ],
                        }),
                        label_details_support: Some(true),
                    }),
                    completion_item_kind: None,
                    context_support: Some(true),
                    insert_text_mode: None,
                    completion_list: Some(CompletionListCapability {
                        item_defaults: Some(vec![
                            "commitCharacters".to_string(),
                            "editRange".to_string(),
                            "insertTextFormat".to_string(),
                            "insertTextMode".to_string(),
                            "data".to_string(),
                        ]),
                    }),
                }),
                hover: None,
                signature_help: None,
                references: None,
                document_highlight: None,
                document_symbol: None,
                formatting: None,
                range_formatting: None,
                on_type_formatting: None,
                declaration: None,
                definition: None,
                type_definition: None,
                implementation: None,
                code_action: None,
                code_lens: None,
                document_link: None,
                color_provider: None,
                rename: None,
                publish_diagnostics: None,
                folding_range: None,
                selection_range: None,
                linked_editing_range: None,
                call_hierarchy: None,
                semantic_tokens: Some(SemanticTokensClientCapabilities {
                    dynamic_registration: None,
                    requests: SemanticTokensClientCapabilitiesRequests {
                        range: None,
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                    },
                    token_types: vec![
                        SemanticTokenType::NAMESPACE,
                        SemanticTokenType::TYPE,
                        SemanticTokenType::CLASS,
                        SemanticTokenType::ENUM,
                        SemanticTokenType::INTERFACE,
                        SemanticTokenType::STRUCT,
                        SemanticTokenType::TYPE_PARAMETER,
                        SemanticTokenType::PARAMETER,
                        SemanticTokenType::VARIABLE,
                        SemanticTokenType::PROPERTY,
                        SemanticTokenType::ENUM_MEMBER,
                        SemanticTokenType::EVENT,
                        SemanticTokenType::FUNCTION,
                        SemanticTokenType::METHOD,
                        SemanticTokenType::MACRO,
                        SemanticTokenType::KEYWORD,
                        SemanticTokenType::MODIFIER,
                        SemanticTokenType::COMMENT,
                        SemanticTokenType::STRING,
                        SemanticTokenType::NUMBER,
                        SemanticTokenType::REGEXP,
                        SemanticTokenType::OPERATOR,
                        SemanticTokenType::DECORATOR,
                    ],
                    token_modifiers: vec![],
                    formats: vec![TokenFormat::RELATIVE],
                    overlapping_token_support: None,
                    multiline_token_support: None,
                    server_cancel_support: None,
                    augments_syntax_tokens: None,
                }),
                moniker: None,
                type_hierarchy: None,
                inline_value: None,
                inlay_hint: None,
                diagnostic: None,
            }),
            notebook_document: None,
            window: Some(lsp_types::WindowClientCapabilities {
                work_done_progress: None,
                show_message: Some(lsp_types::ShowMessageRequestClientCapabilities {
                    message_action_item: None,
                }),
                show_document: None,
            }),
            general: None,
            experimental: None,
        },
        trace: None,
        workspace_folders: None,
        client_info: Some(lsp_types::ClientInfo {
            name: "xit".to_string(),
            version: None,
        }),
        locale: None,
        work_done_progress_params: lsp_types::WorkDoneProgressParams {
            work_done_token: None,
        },
    }
}

fn path_to_uri(path: &Path) -> lsp_types::Uri {
    let mut path_text = "file://".to_string();

    let mut previous_normal = false;
    for component in path.components() {
        let mut normal = false;
        match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Verbatim(_) => {}
                Prefix::VerbatimUNC(_, _) => panic!(),
                Prefix::DeviceNS(_) => panic!(),
                Prefix::UNC(_, _) => panic!(),
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    path_text.push('/');
                    path_text.push(char::from_u32(letter as u32).unwrap());
                    path_text.push(':');
                }
            },
            Component::RootDir => {
                path_text.push('/');
            }
            Component::CurDir => panic!(),
            Component::ParentDir => panic!(),
            Component::Normal(segment) => {
                normal = true;
                if previous_normal {
                    path_text.push('/');
                }
                path_text.push_str(segment.to_str().unwrap());
            }
        }
        previous_normal = normal;
    }

    dbg!(lsp_types::Uri::from_str(&path_text).unwrap())
}

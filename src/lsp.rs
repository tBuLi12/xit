use std::{
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    path::{Component, Path, PathBuf, Prefix},
    process::{Command, Stdio},
    str::FromStr,
    sync::{atomic::AtomicU64, mpsc},
    thread,
};

use caarr::EventChannel;
use lsp_types::{
    notification,
    request::{self},
    DidOpenTextDocumentParams, InitializeParams, InitializedParams, SemanticTokenType,
    SemanticTokensClientCapabilities, SemanticTokensClientCapabilitiesRequests,
    SemanticTokensFullOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentSyncClientCapabilities, TokenFormat,
};
use serde::{Deserialize, Serialize};

use crate::{
    editor::{TextPosition, TextRange},
    Event,
};

pub enum ResponseEvent {
    SemanticTokens {
        file: PathBuf,
        highlights: Vec<Highlight>,
    },
}

pub enum RequestEvent {
    SemanticTokens { file: PathBuf },
    DidOpen { file: PathBuf, text: String },
}

#[derive(Serialize)]
struct Request<'r, Params> {
    #[allow(dead_code)]
    jsonrpc: &'r str,
    id: Option<u64>,
    method: &'r str,
    params: Params,
}

#[derive(Deserialize)]
struct Response<'r, Result> {
    #[allow(dead_code)]
    jsonrpc: &'r str,
    id: Option<u64>,
    result: Result,
}

pub fn start_server(
    exe_path: PathBuf,
    app_channel: EventChannel<Event>,
) -> mpsc::Sender<RequestEvent> {
    let (request_tx, request_rx) = mpsc::channel();

    thread::spawn(move || {
        let mut server_process = Command::new(exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut reader = BufReader::new(server_process.stdout.take().unwrap());
        let mut writer = BufWriter::new(server_process.stdin.take().unwrap());

        let init_response =
            send_request::<request::Initialize>(&mut writer, &mut reader, initialize()).unwrap();

        send_notification::<notification::Initialized>(&mut writer, InitializedParams {}).unwrap();

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
                RequestEvent::DidOpen { file, text } => {
                    send_notification::<notification::DidOpenTextDocument>(
                        &mut writer,
                        DidOpenTextDocumentParams {
                            text_document: TextDocumentItem {
                                uri: path_to_uri(&file),
                                language_id: "rust".to_string(),
                                version: 1,
                                text,
                            },
                        },
                    )
                    .unwrap();
                }
                RequestEvent::SemanticTokens { file } => {
                    let response = send_request::<request::SemanticTokensFullRequest>(
                        &mut writer,
                        &mut reader,
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
                                    let mut line = 0;
                                    let mut byte = 0;
                                    let highlights = tokens
                                        .data
                                        .into_iter()
                                        .map(|token| {
                                            line += token.delta_line;
                                            if token.delta_line != 0 {
                                                byte = 0;
                                            }
                                            byte += token.delta_start;
                                            Highlight {
                                                range: TextRange {
                                                    start: TextPosition { line, byte },
                                                    end: TextPosition {
                                                        line,
                                                        byte: byte + token.length,
                                                    },
                                                },
                                                token_type: legend[token.token_type as usize],
                                            }
                                        })
                                        .collect();

                                    app_channel.send_event(Event::Lsp(
                                        ResponseEvent::SemanticTokens { file, highlights },
                                    ));
                                }
                            }
                            SemanticTokensResult::Partial(_) => {}
                        }
                    }
                }
            }
        }
    });

    request_tx
}

pub struct Highlight {
    pub range: TextRange,
    pub token_type: &'static str,
}

fn send_request<R: request::Request>(
    writer: &mut impl Write,
    reader: &mut impl BufRead,
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
        let body = get_body(reader).unwrap();
        eprintln!("{body}");
        let response: Response<'_, R::Result> = serde_json::from_str(&body).unwrap();
        if response.id == Some(id) {
            return Ok(response.result);
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
                completion: None,
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

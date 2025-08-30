use std::{
    borrow::Cow,
    fs::File,
    io::{BufWriter, Write},
    iter,
    path::{Path, PathBuf},
    process,
    sync::mpsc,
    thread,
    time::Instant,
};

use caarr::{text::TextLine, Key, KeyEvent, ModifiersState, NamedKey, Rect};
use unicode_segmentation::GraphemeCursor;

use crate::{
    lsp::{self, Highlight, HighlightsDelta},
    BASE_FONT, LINE_HEIGHT,
};

#[derive(Clone, Copy)]
pub enum EditorStatus {
    Unsaved,
    None,
}

enum EditorMode {
    Edit,
    Control,
}

pub enum EditorEvent {
    KeyUnhandled,
    Close,
}

pub struct Editor {
    mode: EditorMode,
    pub status: EditorStatus,
    // top_px_offset: u32,
    // left_px_offset: u32,
    lines: Vec<String>,
    line_colors: Vec<Vec<[u8; 4]>>,
    selections: Vec<TextRange>,
    edits_queue: Vec<Edit<'static>>,
    lsp_sender: Option<mpsc::Sender<lsp::RequestEvent>>,
    version: u32,
    last_semantic_tokens_result_id: Option<String>,
    last_highlights: Vec<Highlight>,
    diagnostics: Vec<Diagnostic>,
}

impl Editor {
    pub fn render(&self, width: u32, height: u32) -> Rect {
        let main_box = Rect::new();
        main_box.set_size(width, height);

        let top_px_offset = self.compute_new_top_offset(height);
        let left_px_offset = self.compute_new_left_offset(width);

        let lines_container = Rect::new();
        lines_container.set_size(i32::MAX as u32, i32::MAX as u32);

        // let cursor = lines_container.new_child();
        // cursor.set_size(CURSOR_WIDTH, LINE_HEIGHT);
        // cursor.set_bg_color([255, 0, 0, 255]);
        let first_visible_line = top_px_offset / LINE_HEIGHT;
        let last_visible_line = (top_px_offset + height)
            .div_ceil(LINE_HEIGHT)
            .min(self.lines.len() as u32);

        let visible_range = first_visible_line as usize..last_visible_line as usize;
        self.lines[visible_range.clone()]
            .iter()
            .zip(self.line_colors[visible_range].iter())
            .enumerate()
            .for_each(|(i, (line, colors))| {
                let idx = first_visible_line + i as u32;
                let mut text_line = TextLine::new();

                let mut fragments = vec![];
                {
                    for (i, color) in colors.iter().enumerate() {
                        fragments.push((&line[i..i + 1], *color));
                    }

                    // let mut highlights_i = 0;
                    // let mut i = 0;

                    // loop {
                    //     if let Some(highlight) = highlights.get(highlights_i) {
                    //         if highlight.byte_start == i {
                    //             let color = match highlight.token_type {
                    //                 "type" => [0, 200, 0, 255],
                    //                 "class" => [0, 200, 0, 255],
                    //                 "enum" => [0, 0, 200, 255],
                    //                 "interface" => [0, 200, 0, 255],
                    //                 "struct" => [0, 200, 0, 255],
                    //                 "typeParameter" => [0, 200, 0, 255],
                    //                 "parameter" => [0, 0, 100, 255],
                    //                 "variable" => [0, 0, 100, 255],
                    //                 "property" => [0, 0, 100, 255],
                    //                 "enumMember" => [0, 0, 0, 255],
                    //                 "event" => [0, 0, 0, 255],
                    //                 "function" => [100, 100, 0, 255],
                    //                 "method" => [100, 100, 0, 255],
                    //                 "macro" => [0, 0, 0, 255],
                    //                 "keyword" => [0, 0, 0, 255],
                    //                 "modifier" => [0, 0, 0, 255],
                    //                 "comment" => [0, 0, 0, 255],
                    //                 "string" => [0, 0, 0, 255],
                    //                 "number" => [0, 0, 0, 255],
                    //                 "regexp" => [0, 0, 0, 255],
                    //                 "operator" => [0, 0, 0, 255],
                    //                 _ => [0, 0, 0, 255],
                    //             };
                    //             fragments
                    //                 .push((&line[i as usize..highlight.byte_end as usize], color));
                    //             i = highlight.byte_end;
                    //             highlights_i += 1;
                    //         } else {
                    //             fragments.push((
                    //                 &line[i as usize..highlight.byte_start as usize],
                    //                 [0, 0, 0, 255],
                    //             ));
                    //             i = highlight.byte_start;
                    //         }
                    //     } else {
                    //         fragments.push((&line[i as usize..], [0, 0, 0, 255]));
                    //         break;
                    //     }
                    // }
                }

                text_line.set_text(*BASE_FONT, fragments);
                let selection_rect = Rect::new();
                selection_rect.set_bg_color([0, 0, 150, 100]);

                for selection in &self.selections {
                    if selection.start.line <= idx && selection.end.line >= idx {
                        let start = if selection.start.line == idx {
                            selection.start.byte.min(line.len() as u32)
                        } else {
                            0
                        };
                        let end = if selection.end.line == idx {
                            selection.end.byte.min(line.len() as u32)
                        } else {
                            line.len() as u32
                        };
                        let nudge = if start == end { 2 } else { 0 };
                        let start_offset =
                            get_px_offset_in_line(&text_line, start).saturating_sub(nudge);
                        let end_offset = get_px_offset_in_line(&text_line, end) + nudge;
                        selection_rect.set_pos(start_offset as i32, 0);
                        selection_rect.set_size(end_offset - start_offset, LINE_HEIGHT);
                        break;
                    }
                }

                text_line.rect.append_child(selection_rect);

                for diagnostic in &self.diagnostics {
                    let range = diagnostic.range;
                    if range.start.line <= idx && range.end.line >= idx {
                        let start = if range.start.line == idx {
                            range.start.byte.min(line.len() as u32)
                        } else {
                            0
                        };
                        let end = if range.end.line == idx {
                            range.end.byte.min(line.len() as u32)
                        } else {
                            line.len() as u32
                        };
                        let nudge = if start == end { 2 } else { 0 };
                        let start_offset =
                            get_px_offset_in_line(&text_line, start).saturating_sub(nudge);
                        let end_offset = get_px_offset_in_line(&text_line, end) + nudge;
                        let diagnostic_rect = Rect::new();
                        diagnostic_rect.set_pos(start_offset as i32, (LINE_HEIGHT / 2) as i32);
                        diagnostic_rect.set_size(end_offset - start_offset, 4);
                        diagnostic_rect.set_bg_color([255, 0, 0, 255]);
                        text_line.rect.append_child(diagnostic_rect);
                    }
                }

                text_line.rect.set_pos(30, (idx * LINE_HEIGHT) as i32);
                lines_container.append_child(text_line.rect);

                let mut number = TextLine::new();
                number.set_text(*BASE_FONT, [(&*i.to_string(), [100, 100, 100, 255])]);
                number.rect.set_pos(0, (idx * LINE_HEIGHT) as i32);
                lines_container.append_child(number.rect);
            });

        lines_container.set_pos(
            -(left_px_offset as i32),
            -((top_px_offset % LINE_HEIGHT) as i32),
        );
        main_box.append_child(lines_container);
        main_box
    }

    pub fn new(
        mut text: String,
        path: PathBuf,
        lsp_sender: Option<mpsc::Sender<lsp::RequestEvent>>,
    ) -> Self {
        if text.is_empty() {
            text.push('\n');
        }

        let lines: Vec<_> = text.lines().map(|line| line.to_string()).collect();
        let line_colors = lines
            .iter()
            .map(|line| vec![[0, 0, 0, 255]; line.len()])
            .collect();

        if let Some(lsp) = &lsp_sender {
            lsp.send(lsp::RequestEvent::DidOpen {
                file: path.clone(),
                text,
                version: 0,
            });
            lsp.send(lsp::RequestEvent::SemanticTokens {
                file: path.clone(),
                version: 0,
            });
        }

        Editor {
            mode: EditorMode::Control,
            status: EditorStatus::None,
            lines,
            line_colors,
            selections: vec![TextRange {
                start: TextPosition { line: 0, byte: 0 },
                end: TextPosition { line: 0, byte: 0 },
            }],
            edits_queue: vec![],
            lsp_sender,
            version: 0,
            last_highlights: vec![],
            last_semantic_tokens_result_id: None,
            diagnostics: vec![],
        }
    }

    pub fn apply_full_highlights(
        &mut self,
        version: u32,
        highlights: Vec<Highlight>,
        result_id: Option<String>,
    ) {
        if version != self.version {
            eprintln!("received outdated highlights: {version} {}", self.version);
            return;
        }

        eprintln!("applying highlights: {}", self.version);
        for highlight in &highlights {
            let colors = &mut self.line_colors[highlight.line as usize]
                [highlight.start_byte as usize..highlight.end_byte as usize];
            colors.fill(highlight.color);
        }

        self.last_semantic_tokens_result_id = result_id;
        self.last_highlights = highlights;
    }

    pub fn apply_highlights_delta(
        &mut self,
        version: u32,
        delta: HighlightsDelta,
        result_id: Option<String>,
    ) {
        if version != self.version {
            eprintln!("received outdated highlights: {version} {}", self.version);
            return;
        }

        eprintln!("applying highlights: {}", self.version);

        delta.apply_to(&mut self.last_highlights, |highlight| {
            let colors = &mut self.line_colors[highlight.line as usize]
                [highlight.start_byte as usize..highlight.end_byte as usize];
            colors.fill(highlight.color);
        });

        self.last_semantic_tokens_result_id = result_id;
    }

    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>, version: u32) {
        if self.version == version {
            self.diagnostics = diagnostics;
            eprintln!("set diagnostics");
        } else {
            eprintln!("discarding diagnostics {} {}", self.version, version);
        }
    }

    pub fn handle_key_event(&mut self, self_path: &Path, event: &KeyEvent) -> Option<EditorEvent> {
        let event = self.handle_key_event_inner(self_path, event);
        for edits in self.edits_queue.windows(2) {
            if edits[0].end > edits[1].start {
                panic!("queue contains invalid text edits");
            }
        }
        if !self.edits_queue.is_empty() {
            self.version += 1;
            if let Some(lsp) = &self.lsp_sender {
                lsp.send(lsp::RequestEvent::DidChange {
                    file: self_path.to_path_buf(),
                    edits: self.edits_queue.clone(),
                    version: self.version,
                });
                lsp.send(match self.last_semantic_tokens_result_id.clone() {
                    Some(result_id) => lsp::RequestEvent::SemanticTokensDelta {
                        file: self_path.to_path_buf(),
                        version: self.version,
                        previous_result_id: result_id,
                    },
                    None => lsp::RequestEvent::SemanticTokens {
                        file: self_path.to_path_buf(),
                        version: self.version,
                    },
                });
            }
        }
        while let Some(edit) = self.edits_queue.pop() {
            self.apply_edit(edit);
        }
        eprintln!("document is now at version {}", self.version);
        event
    }

    pub fn handle_key_event_inner(
        &mut self,
        self_path: &Path,
        event: &KeyEvent,
    ) -> Option<EditorEvent> {
        match self.mode {
            EditorMode::Control => match event.key {
                Key::Character("s") if event.modifiers.contains(ModifiersState::CONTROL) => {
                    let mut writer =
                        BufWriter::new(File::options().write(true).open(self_path).unwrap());
                    for line in &self.lines {
                        writer.write_all(line.as_bytes()).unwrap();
                        writer.write_all("\n".as_bytes()).unwrap();
                    }
                    writer.flush().unwrap();
                    self.status = EditorStatus::None;
                }
                Key::Character("c") => {
                    self.mode = EditorMode::Edit;

                    for &TextRange { start, end } in &self.selections {
                        self.edits_queue.push(Edit {
                            start,
                            end,
                            text: Cow::Borrowed(""),
                            affinity: Affinity::None,
                        });
                    }
                }
                Key::Character("i") => {
                    self.mode = EditorMode::Edit;
                    self.move_selections(|sel| sel.range.end = sel.range.start);
                }
                Key::Character("a") => {
                    self.mode = EditorMode::Edit;
                    self.move_selections(|sel| sel.range.start = sel.range.end);
                }
                Key::Character("I") => {
                    self.mode = EditorMode::Edit;
                    self.move_selections(|sel| {
                        sel.range.start.byte = 0;
                        sel.range.end = sel.range.start;
                    });
                }
                Key::Character("A") => {
                    self.mode = EditorMode::Edit;
                    self.move_selections(|selection| {
                        let line_end = selection.end_line().len() as u32;
                        selection.range.end.byte = line_end;
                        selection.range.start = selection.range.end;
                    });
                }
                Key::Character("o") => {
                    self.mode = EditorMode::Edit;
                    // same as above
                    self.move_selections(|selection| {
                        let line_end = selection.end_line().len() as u32;
                        selection.range.end.byte = line_end;
                        selection.range.start = selection.range.end;
                    });
                    self.insert_text_before(Cow::Borrowed("\n"));
                }
                Key::Character("O") => {
                    self.mode = EditorMode::Edit;
                    self.move_selections(|selection| {
                        selection.range.start.byte = 0;
                        selection.range.end = selection.range.start;
                    });
                    self.insert_text_after(Cow::Borrowed("\n"));
                }
                Key::Character("w") => {
                    // let line_end = self.move_to_next_line_if_at_end()?;
                    self.move_selections(|selection| {
                        let mut seen_letter = false;
                        for (pos, char) in selection.end_chars() {
                            if !char.is_whitespace() {
                                seen_letter = true;
                            } else if seen_letter {
                                selection.range.end = pos;
                                return;
                            }
                        }
                        selection.range.end = TextPosition {
                            line: selection.lines.len() as u32 - 1,
                            byte: selection.lines.last().unwrap().len() as u32,
                        }
                    });
                }
                Key::Character("e") => {
                    // let line_end = self.move_to_next_line_if_at_end()?;
                    // let line = &mut self.lines[self.cursor.line as usize];
                    // let mut seen_letters = false;
                    // let mut dst_idx = None;
                    // for (idx, char) in line[self.cursor.byte as usize..].char_indices() {
                    //     if !char.is_whitespace() {
                    //         seen_letters = true;
                    //     } else if seen_letters {
                    //         dst_idx = Some(idx as u32 + self.cursor.byte);
                    //         break;
                    //     }
                    // }
                    // self.cursor.byte = dst_idx.unwrap_or(line_end);
                    // self.handle_cursor_update();
                }
                Key::Character("b") => {
                    self.move_selections(|selection| {
                        let mut seen_letter = false;
                        for (pos, char) in selection.start_chars_back() {
                            if !char.is_whitespace() {
                                seen_letter = true;
                            } else if seen_letter {
                                selection.range.start = pos;
                                return;
                            }
                        }
                        selection.range.start = TextPosition { line: 0, byte: 0 }
                    });
                }
                Key::Character("q") => {
                    return Some(EditorEvent::Close);
                }
                Key::Character("Q") => process::exit(0),
                Key::Character("j") => self.down(event.modifiers),
                Key::Character("k") => self.up(event.modifiers),
                Key::Character("h") => self.left(event.modifiers),
                Key::Character("l") => self.right(event.modifiers),
                // Key::Character("d") => self.del(event.modifiers),
                _ => return Some(EditorEvent::KeyUnhandled),
            },
            EditorMode::Edit => match event.key {
                Key::Character(char) => {
                    self.insert_text_before(Cow::Owned(char.to_string()));
                }
                Key::Named(NamedKey::Escape) => {
                    self.mode = EditorMode::Control;
                }
                Key::Named(NamedKey::Tab) => {
                    // let line = &mut self.lines[self.cursor.line as usize];
                    // let mut graphemes = GraphemeCursor::new(0, line.len(), true);
                    // let mut current = 0;
                    // loop {
                    //     if self.cursor.byte <= current {
                    //         break;
                    //     }
                    //     let Some(idx) = graphemes.next_boundary(&line, 0).unwrap() else {
                    //         break;
                    //     };
                    //     current = idx as u32;
                    // }
                    // const TAB_WIDTH: u32 = 4;
                    // self.insert_text(&"    "[..(TAB_WIDTH - current % TAB_WIDTH) as usize]);
                    // return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                }
                Key::Named(NamedKey::Backspace) => {
                    // if self.try_delete_selection() {
                    //     return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                    // }

                    // let line = &mut self.lines[self.cursor.line as usize];
                    // let mut graphemes =
                    //     GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

                    // if let Some(prev_idx) = graphemes.prev_boundary(&line, 0).unwrap() {
                    //     self.apply_edit(Edit {
                    //         start: TextPosition {
                    //             line: self.cursor.line,
                    //             byte: prev_idx as u32,
                    //         },
                    //         end: TextPosition {
                    //             line: self.cursor.line,
                    //             byte: self.cursor.byte,
                    //         },
                    //         text: "",
                    //     });
                    //     return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                    // } else if let Some(prev_line) = self.cursor.line.checked_sub(1) {
                    //     self.apply_edit(Edit {
                    //         start: TextPosition {
                    //             line: prev_line,
                    //             byte: self.lines[prev_line as usize].len() as u32,
                    //         },
                    //         end: TextPosition {
                    //             line: self.cursor.line,
                    //             byte: self.cursor.byte,
                    //         },
                    //         text: "",
                    //     });
                    //     return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                    // }
                }
                Key::Named(NamedKey::Space) => {
                    self.insert_text_before(Cow::Borrowed(" "));
                }
                Key::Named(NamedKey::Enter) => {
                    self.insert_text_before(Cow::Borrowed("\n"));
                }
                Key::Named(NamedKey::ArrowDown) => self.down(event.modifiers),
                Key::Named(NamedKey::ArrowUp) => self.up(event.modifiers),
                Key::Named(NamedKey::ArrowLeft) => self.left(event.modifiers),
                Key::Named(NamedKey::ArrowRight) => self.right(event.modifiers),
                _ => return Some(EditorEvent::KeyUnhandled),
            },
        }

        None
    }

    fn apply_edit(&mut self, mut edit: Edit) {
        let start = Instant::now();

        self.status = EditorStatus::Unsaved;

        edit.start.byte = edit
            .start
            .byte
            .min(self.lines[edit.start.line as usize].len() as u32);
        edit.end.byte = edit
            .end
            .byte
            .min(self.lines[edit.end.line as usize].len() as u32);

        let mut lines: Vec<_> = edit.text.split('\n').map(|line| line.to_string()).collect();
        let mut line_colors: Vec<_> = edit
            .text
            .split('\n')
            .map(|line| vec![[0, 0, 0, 255]; line.len()])
            .collect();
        let front_slice = &self.lines[edit.start.line as usize][..(edit.start.byte as usize)];
        let back_slice = &self.lines[edit.end.line as usize][(edit.end.byte as usize)..];
        let front_color_slice =
            &self.line_colors[edit.start.line as usize][..(edit.start.byte as usize)];
        let back_color_slice =
            &self.line_colors[edit.end.line as usize][(edit.end.byte as usize)..];

        let shift = edit.start.line as i32 - edit.end.line as i32 - 1 + lines.len() as i32;

        lines.first_mut().unwrap().insert_str(0, front_slice);
        line_colors
            .first_mut()
            .unwrap()
            .splice(0..0, front_color_slice.iter().copied());

        let inline_shift = { lines.last().unwrap().len() as i32 - edit.end.byte as i32 };

        // cursor bussiness
        {
            for selection in &mut self.selections {
                if selection.start == edit.start
                    && selection.end == edit.end
                    && edit.affinity != Affinity::None
                {
                    match edit.affinity {
                        Affinity::None => unreachable!(),
                        Affinity::Left => {
                            selection.start = edit.start;
                            selection.end = selection.start;
                        }
                        Affinity::Right => {
                            selection.end.line =
                                selection.end.line.checked_add_signed(shift).unwrap();
                            selection.end.byte =
                                edit.end.byte.checked_add_signed(inline_shift).unwrap();
                            selection.start.line = edit.end.line;
                            selection.start = selection.end;
                        }
                    }
                } else if selection.start <= edit.start && selection.end >= edit.end {
                    // selection should still span the edit
                    selection.end.line = selection.end.line.checked_add_signed(shift).unwrap();
                    if selection.end.line == edit.end.line {
                        selection.end.byte =
                            selection.end.byte.checked_add_signed(inline_shift).unwrap();
                    }
                } else if selection.start >= edit.start && selection.end <= edit.end {
                    // selection should go past the edit
                    selection.start.byte = edit.end.byte.checked_add_signed(inline_shift).unwrap();
                    selection.start.line = edit.end.line;
                    selection.end = selection.start;
                } else if selection.start <= edit.start && selection.end >= edit.start {
                    // selection should be truncated
                    selection.end = edit.start;
                } else if selection.end >= edit.end && selection.start <= edit.end {
                    // selection should be truncated
                    selection.start.byte = edit.end.byte.checked_add_signed(inline_shift).unwrap();
                    selection.start.line = edit.end.line;
                }
            }

            self.selections.dedup_by(|prev, next| {
                if prev.end > next.start || prev.end == next.start && next.end == next.start {
                    prev.end = prev.end.max(next.end);
                    true
                } else {
                    false
                }
            });
        }

        {
            self.diagnostics.retain_mut(|diagnostic| {
                diagnostic.range.end < edit.start || diagnostic.range.start > edit.end
            });
        }

        lines.last_mut().unwrap().push_str(back_slice);
        line_colors
            .last_mut()
            .unwrap()
            .extend_from_slice(&back_color_slice);

        self.lines
            .splice(edit.start.line as usize..=edit.end.line as usize, lines);
        self.line_colors.splice(
            edit.start.line as usize..=edit.end.line as usize,
            line_colors,
        );

        eprintln!("edit took {:.2?}", start.elapsed());
    }

    fn compute_new_top_offset(&self, height: u32) -> u32 {
        let Some(last_selection) = self.selections.last() else {
            return 0;
        };

        let cursor_px_offset = last_selection.end.line * LINE_HEIGHT + LINE_HEIGHT / 2;

        cursor_px_offset.saturating_sub(height / 2)

        // let top_scroll_boundary = self.top_px_offset + height / 4;
        // if cursor_top_px_offset < top_scroll_boundary {
        //     return self
        //         .top_px_offset
        //         .saturating_sub(top_scroll_boundary - cursor_top_px_offset);
        // }

        // let cursor_bottom_px_offset = cursor_top_px_offset + LINE_HEIGHT;
        // let bottom_scroll_boundary = self.top_px_offset + height - height / 4;
        // if cursor_bottom_px_offset > bottom_scroll_boundary {
        //     return (self.top_px_offset + (cursor_bottom_px_offset - bottom_scroll_boundary))
        //         .min(self.lines.len() as u32 * LINE_HEIGHT);
        // }

        // self.top_px_offset
    }

    fn compute_new_left_offset(&self, width: u32) -> u32 {
        let Some(last_selection) = self.selections.last() else {
            return 0;
        };

        let offset = get_px_offset_in_line(
            &{
                let mut text_line = TextLine::new();
                text_line.set_text(
                    *BASE_FONT,
                    [(
                        &*self.lines[last_selection.end.line as usize],
                        [0, 0, 0, 255],
                    )],
                );
                text_line
            },
            last_selection.end.byte,
        );

        offset.saturating_sub(width / 2)

        // let left_scroll_boundary = self.left_px_offset + width / 4;
        // if cursor_left_px_offset < left_scroll_boundary {
        //     self.left_px_offset = self
        //         .left_px_offset
        //         .saturating_sub(left_scroll_boundary - cursor_left_px_offset);
        // }

        // let cursor_right_px_offset = cursor_left_px_offset + CURSOR_WIDTH;
        // let right_scroll_boundary = self.left_px_offset + width - width / 4;
        // if cursor_right_px_offset > right_scroll_boundary {
        //     self.left_px_offset =
        //         self.left_px_offset + (cursor_right_px_offset - right_scroll_boundary);
        // }
    }

    // fn compute_focus_left_px_offset(&self) -> u32 {
    //     get_px_offset_in_line(
    //         &self.visible_lines
    //             [self.cursor.line as usize - (self.top_px_offset / LINE_HEIGHT) as usize]
    //             .text_line,
    //         self.cursor.byte,
    //     )
    // }

    // fn handle_selection(&mut self, modifiers: ModifiersState) {
    //     if modifiers.contains(ModifiersState::SHIFT) {
    //         if self.selection_anchor.is_none() {
    //             self.selection_anchor = Some(TextPosition {
    //                 line: self.cursor.line,
    //                 byte: self.cursor.byte,
    //             });
    //         }
    //     } else {
    //         if self.selection_anchor.is_some() {
    //             self.selection_anchor = None;
    //         }
    //     }
    // }

    fn insert_text_before(&mut self, text: Cow<'static, str>) {
        for &TextRange { start, end } in &self.selections {
            self.edits_queue.push(Edit {
                start,
                end,
                text: text.clone(),
                affinity: Affinity::Right,
            });
        }
    }

    fn insert_text_after(&mut self, text: Cow<'static, str>) {
        for &TextRange { start, end } in &self.selections {
            self.edits_queue.push(Edit {
                start,
                end,
                text: text.clone(),
                affinity: Affinity::Left,
            });
        }
    }

    // fn try_delete_selection(&mut self, idx: usize) -> bool {
    //     let selection = &self.selections[idx];
    //     if selection.start < selection.end {
    //         let TextRange { start, end } = *selection;
    //         self.apply_edit(Edit {
    //             start,
    //             end,
    //             text: Cow::Borrowed(""),
    //             affinity: Affinity::None,
    //         });
    //         true
    //     } else {
    //         false
    //     }
    // }

    // fn move_to_prev_line_if_at_start(&mut self) -> Option<()> {
    //     if self.cursor.byte == 0 {
    //         self.cursor.line = self.cursor.line.checked_sub(1)?;
    //         self.cursor.byte = self.current_line_end();
    //     }

    //     Some(())
    // }

    // fn move_to_next_line_if_at_end(&mut self) -> Option<u32> {
    //     let mut line_end = self.current_line_end();
    //     if self.cursor.byte == line_end {
    //         if self.cursor.line == self.lines.len() as u32 - 1 {
    //             return None;
    //         }

    //         self.cursor.line += 1;
    //         self.cursor.byte = 0;
    //         line_end = self.current_line_end();
    //     }

    //     Some(line_end)
    // }

    // fn del(&mut self, modifiers: ModifiersState) {
    //     let mut i = self.selections.len();
    //     while i != 0 {
    //         i -= 1;
    //         if self.try_delete_selection(i) {
    //             continue;
    //         }

    //         let selection = &self.selections[i];
    //         let line = &self.lines[selection.end.line as usize];
    //         let mut graphemes = GraphemeCursor::new(selection.end.byte as usize, line.len(), true);

    //         if let Some(next_idx) = graphemes.next_boundary(&line, 0).unwrap() {
    //             self.apply_edit(Edit {
    //                 start: selection.end,
    //                 end: TextPosition {
    //                     line: selection.end.line,
    //                     byte: next_idx as u32,
    //                 },
    //                 text: Cow::Borrowed(""),
    //                 affinity: Affinity::None,
    //             });
    //             continue;
    //         }

    //         let next_line = selection.end.line + 1;
    //         if next_line < self.lines.len() as u32 {
    //             self.apply_edit(Edit {
    //                 start: selection.end,
    //                 end: TextPosition {
    //                     line: next_line,
    //                     byte: 0,
    //                 },
    //                 text: Cow::Borrowed(""),
    //                 affinity: Affinity::None,
    //             });
    //             continue;
    //         }
    //     }
    // }

    fn left(&mut self, modifiers: ModifiersState) {
        self.move_selections(|mut selection| {
            let line = &selection.lines[selection.range.start.line as usize];
            let mut graphemes = GraphemeCursor::new(
                selection.range.start.byte.min(line.len() as u32) as usize,
                line.len(),
                true,
            );

            if let Some(prev_idx) = graphemes.prev_boundary(line, 0).unwrap() {
                selection.range.start.byte = prev_idx as u32;
            } else if selection.try_up(modifiers) {
                selection.range.start.byte = selection.start_line().len() as u32;
            }

            selection.range.end = selection.range.start;
        });
    }

    fn right(&mut self, modifiers: ModifiersState) {
        self.move_selections(|mut selection| {
            let line = &selection.lines[selection.range.end.line as usize];
            let mut graphemes = GraphemeCursor::new(
                selection.range.end.byte.min(line.len() as u32) as usize,
                line.len(),
                true,
            );

            if let Some(next_idx) = graphemes.next_boundary(&line, 0).unwrap() {
                selection.range.end.byte = next_idx as u32;
            } else if selection.try_down(modifiers) {
                selection.range.end.byte = 0;
            }

            selection.range.start = selection.range.end;
        });
    }

    fn down(&mut self, modifiers: ModifiersState) {
        self.move_selections(|mut selection| {
            if selection.try_down(modifiers) {
                selection.range.start = selection.range.end;
            }
        });
    }

    fn up(&mut self, modifiers: ModifiersState) {
        self.move_selections(|mut selection| {
            if selection.try_up(modifiers) {
                selection.range.end = selection.range.start;
            }
        });
    }

    fn move_selections(&mut self, mut fun: impl FnMut(Selection)) {
        let mut i = self.selections.len();
        while i != 0 {
            i -= 1;
            fun(Selection {
                lines: &self.lines,
                range: &mut self.selections[i],
            })
        }

        self.selections.sort_by_key(|selection| selection.start);
        self.selections.dedup_by(|prev, next| {
            if prev.end > next.start || prev.end == next.start && next.end == next.start {
                prev.end = prev.end.max(next.end);
                true
            } else {
                false
            }
        });
    }

    // fn for_each_selection(&mut self, mut fun: impl FnMut(&mut Self, Selection)) {
    //     let mut i = self.selections.len();
    //     while i != 0 {
    //         i -= 1;
    //         let selection = self.selections[i];
    //         fun(self, selection);
    //     }
    // }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextPosition {
    pub line: u32,
    pub byte: u32,
}

#[derive(Debug, Clone)]
pub struct Edit<'text> {
    pub start: TextPosition,
    pub end: TextPosition,
    pub text: Cow<'text, str>,
    affinity: Affinity,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TextRange {
    pub start: TextPosition,
    pub end: TextPosition,
}

#[derive(Debug)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange,
}

struct Selection<'a> {
    lines: &'a [String],
    range: &'a mut TextRange,
}

impl<'a> Selection<'a> {
    pub fn start_line(&self) -> &'a str {
        &self.lines[self.range.start.line as usize]
    }

    pub fn end_line(&self) -> &'a str {
        &self.lines[self.range.end.line as usize]
    }

    pub fn start_chars(&self) -> impl Iterator<Item = (TextPosition, char)> + 'a {
        chars(self.lines, self.range.start)
    }

    pub fn start_chars_back(&self) -> impl Iterator<Item = (TextPosition, char)> + 'a {
        chars_back(self.lines, self.range.start)
    }

    pub fn end_chars(&self) -> impl Iterator<Item = (TextPosition, char)> + 'a {
        chars(self.lines, self.range.end)
    }

    fn try_down(&mut self, _modifiers: ModifiersState) -> bool {
        if self.range.end.line < self.lines.len() as u32 - 1 {
            self.range.end.line += 1;
            true
        } else {
            false
        }
    }

    fn try_up(&mut self, _modifiers: ModifiersState) -> bool {
        if self.range.start.line > 0 {
            self.range.start.line -= 1;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Affinity {
    None,
    Left,
    Right,
}

fn chars(
    lines: &[String],
    mut position: TextPosition,
) -> impl Iterator<Item = (TextPosition, char)> + '_ {
    iter::from_fn(move || {
        if position.line != lines.len() as u32 {
            let line = &lines[position.line as usize];
            let old_position = position;
            let char = line[position.byte.min(line.len() as u32) as usize..]
                .chars()
                .next();

            match char {
                Some(char) => {
                    position.byte += char.len_utf8() as u32;
                    return Some((old_position, char));
                }
                None => {
                    position.line += 1;
                    position.byte = 0;
                    if position.line != lines.len() as u32 {
                        return Some((old_position, '\n'));
                    }
                }
            }
        }
        None
    })
}

fn chars_back(
    lines: &[String],
    mut position: TextPosition,
) -> impl Iterator<Item = (TextPosition, char)> + '_ {
    iter::from_fn(move || {
        if position.line != 0 || position.byte != 0 {
            let line = &lines[position.line as usize];
            let old_position = position;
            let char = line[..position.byte.min(line.len() as u32) as usize]
                .chars()
                .rev()
                .next();

            match char {
                Some(char) => {
                    position.byte -= char.len_utf8() as u32;
                    return Some((old_position, char));
                }
                None => match position.line.checked_sub(1) {
                    Some(new_line) => {
                        position.line = new_line;
                        position.byte = lines[new_line as usize].len() as u32;
                        return Some((old_position, '\n'));
                    }
                    None => {}
                },
            }
        }
        None
    })
}

fn get_px_offset_in_line(line: &TextLine, byte: u32) -> u32 {
    let mut x_offset = 0;
    for cluster in line.clusters() {
        if cluster.start < byte {
            x_offset = cluster.px_offset;
        }
    }

    x_offset
}

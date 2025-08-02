use std::{
    collections::HashMap,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    process,
    time::Instant,
};

use caarr::{Key, ModifiersState, NamedKey, Rect, TextLine};
use unicode_segmentation::GraphemeCursor;

use crate::{sidebar::SIDEBAR_WIDTH, BASE_FONT, LINE_HEIGHT};

const CURSOR_WIDTH: u32 = 2;

struct Cursor {
    line: u32,
    byte: u32,
    ephemeral_byte: u32,
    rect: Option<Rect>,
}

impl Cursor {
    pub fn text_position(&self) -> TextPosition {
        TextPosition {
            line: self.line,
            byte: self.byte,
        }
    }
}

struct VisibleLine {
    text_line: TextLine,
    selection_rect: Rect,
    index: u32,
}

impl VisibleLine {
    pub fn new(parent: &Rect, idx: u32, text: &str) -> Self {
        let mut text_line = parent.new_text_child();
        text_line.set_text(*BASE_FONT, text);
        let selection_rect = text_line.rect.new_child();
        selection_rect.set_bg_color([0, 0, 150, 100]);
        Self {
            text_line,
            selection_rect,
            index: idx,
        }
    }

    pub fn set_selection(&mut self, selection_start: TextPosition, selection_end: TextPosition) {
        if self.index < selection_start.line || self.index > selection_end.line {
            self.selection_rect.set_size(0, 0);
            return;
        }

        let mut start = None;
        let mut end = None;

        if self.index == selection_start.line {
            start = Some(get_px_offset_in_line(&self.text_line, selection_start.byte));
        }
        if self.index == selection_end.line {
            end = Some(get_px_offset_in_line(&self.text_line, selection_end.byte));
        }

        let start = start.unwrap_or(0);
        self.selection_rect.set_pos(start as i32, 0);
        self.selection_rect.set_size(
            end.unwrap_or_else(|| {
                self.text_line
                    .clusters()
                    .last()
                    .map(|cluster| cluster.px_offset)
                    .unwrap_or(0)
            }) - start,
            LINE_HEIGHT,
        );
    }
}

pub enum EditorStatus {
    Unsaved,
    None,
}

enum EditorMode {
    Edit,
    Control,
}

pub enum EditorEvent {
    SetStatus(EditorStatus),
    KeyUnhandled,
    Close,
}

pub struct Editor {
    mode: EditorMode,
    top_px_offset: u32,
    left_px_offset: u32,
    lines: Vec<String>,
    visible_lines: Vec<VisibleLine>,
    cursor: Cursor,
    selection_anchor: Option<TextPosition>,
    main_box: Rect,
    lines_container: Rect,
}

impl Editor {
    pub fn new(root: Rect, lines: impl IntoIterator<Item = String>) -> Self {
        let main_box = root.new_child();
        main_box.set_size(
            root.get_size().0.saturating_sub(SIDEBAR_WIDTH),
            root.get_size().1,
        );
        main_box.set_pos(SIDEBAR_WIDTH as i32, 0);

        let lines_container = main_box.new_child();
        lines_container.set_size(i32::MAX as u32, i32::MAX as u32);

        let cursor = lines_container.new_child();
        cursor.set_size(CURSOR_WIDTH, LINE_HEIGHT);
        cursor.set_bg_color([255, 0, 0, 255]);

        let mut lines: Vec<_> = lines.into_iter().collect();

        if lines.is_empty() {
            lines.push(String::new());
        }

        let text_lines = lines
            .iter()
            .enumerate()
            .take(main_box.get_size().1.div_ceil(LINE_HEIGHT) as usize)
            .map(|(i, line)| {
                let visible_line = VisibleLine::new(&lines_container, i as u32, &line);
                visible_line
                    .text_line
                    .rect
                    .set_pos(0, (i as u32 * LINE_HEIGHT) as i32);
                visible_line
            })
            .collect();

        Editor {
            mode: EditorMode::Control,
            top_px_offset: 0,
            left_px_offset: 0,
            lines,
            visible_lines: text_lines,
            cursor: Cursor {
                line: 0,
                byte: 0,
                ephemeral_byte: 0,
                rect: Some(cursor),
            },
            selection_anchor: None,
            main_box: main_box.clone(),
            lines_container,
        }
    }

    pub fn handle_parent_resize(&mut self, parent: &Rect) {
        self.main_box.set_size(
            parent.get_size().0.saturating_sub(SIDEBAR_WIDTH),
            parent.get_size().1,
        );
        self.render_lines();
    }

    pub fn handle_key_event(
        &mut self,
        key: &Key<&str>,
        modifiers: ModifiersState,
        self_path: &Path,
    ) -> Option<EditorEvent> {
        match self.mode {
            EditorMode::Control => match key {
                Key::Character("s") if modifiers.contains(ModifiersState::CONTROL) => {
                    let mut writer =
                        BufWriter::new(File::options().write(true).open(self_path).unwrap());
                    for line in &self.lines {
                        writer.write_all(line.as_bytes()).unwrap();
                        writer.write_all("\n".as_bytes()).unwrap();
                    }
                    writer.flush().unwrap();
                    return Some(EditorEvent::SetStatus(EditorStatus::None));
                }
                Key::Character("i") => {
                    self.mode = EditorMode::Edit;
                    if let Some(anchor) = self.selection_anchor {
                        let cursor_position = self.cursor.text_position();
                        if cursor_position > anchor {
                            self.cursor.byte = anchor.byte;
                            self.cursor.line = anchor.line;
                            self.selection_anchor = Some(cursor_position);
                            self.handle_cursor_update();
                        }
                    }
                }
                Key::Character("a") => {
                    self.mode = EditorMode::Edit;
                    if let Some(anchor) = self.selection_anchor {
                        let cursor_position = self.cursor.text_position();
                        if cursor_position < anchor {
                            self.cursor.byte = anchor.byte;
                            self.cursor.line = anchor.line;
                            self.selection_anchor = Some(cursor_position);
                            self.handle_cursor_update();
                        }
                    }
                }
                Key::Character("I") => {
                    self.mode = EditorMode::Edit;
                    if self.cursor.byte != 0 {
                        self.cursor.byte = 0;
                        self.handle_cursor_update();
                    }
                    self.drop_selection();
                }
                Key::Character("A") => {
                    self.mode = EditorMode::Edit;
                    let line_end = self.current_line_end();
                    if self.cursor.byte != line_end {
                        self.cursor.byte = line_end;
                        self.handle_cursor_update();
                    }
                    self.drop_selection();
                }
                Key::Character("o") => {
                    self.mode = EditorMode::Edit;
                    let line_end = self.current_line_end();
                    let line = self.cursor.line;
                    let position = TextPosition {
                        line,
                        byte: line_end,
                    };
                    self.drop_selection();
                    self.apply_edit(Edit {
                        start: position,
                        end: position,
                        text: "\n",
                    });
                    self.cursor.byte = 0;
                    self.cursor.line = line + 1;
                    self.handle_cursor_update();
                    return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                }
                Key::Character("O") => {
                    self.mode = EditorMode::Edit;
                    let line = self.cursor.line;
                    let position = TextPosition { line, byte: 0 };
                    self.drop_selection();
                    self.apply_edit(Edit {
                        start: position,
                        end: position,
                        text: "\n",
                    });
                    self.cursor.byte = 0;
                    self.cursor.line = line;
                    self.handle_cursor_update();
                    return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                }
                Key::Character("w") => {
                    let line_end = self.move_to_next_line_if_at_end()?;
                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut seen_whitespace = false;
                    let mut dst_idx = None;
                    for (idx, char) in line[self.cursor.byte as usize..].char_indices() {
                        if char.is_whitespace() {
                            seen_whitespace = true;
                        } else if seen_whitespace {
                            dst_idx = Some(idx as u32 + self.cursor.byte);
                            break;
                        }
                    }
                    self.cursor.byte = dst_idx.unwrap_or(line_end);
                    self.handle_cursor_update();
                }
                Key::Character("e") => {
                    let line_end = self.move_to_next_line_if_at_end()?;
                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut seen_letters = false;
                    let mut dst_idx = None;
                    for (idx, char) in line[self.cursor.byte as usize..].char_indices() {
                        if !char.is_whitespace() {
                            seen_letters = true;
                        } else if seen_letters {
                            dst_idx = Some(idx as u32 + self.cursor.byte);
                            break;
                        }
                    }
                    self.cursor.byte = dst_idx.unwrap_or(line_end);
                    self.handle_cursor_update();
                }
                Key::Character("b") => {
                    self.move_to_prev_line_if_at_start()?;
                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut dst_idx = None;
                    for (idx, char) in line[..self.cursor.byte as usize].char_indices().rev() {
                        if !char.is_whitespace() {
                            dst_idx = Some(idx as u32);
                        } else if dst_idx.is_some() {
                            break;
                        }
                    }
                    self.cursor.byte = dst_idx.unwrap_or(0);
                    self.handle_cursor_update();
                }
                Key::Character("q") => {
                    return Some(EditorEvent::Close);
                }
                Key::Character("Q") => process::exit(0),
                Key::Character("j") => self.down(modifiers),
                Key::Character("k") => self.up(modifiers),
                Key::Character("h") => self.left(modifiers),
                Key::Character("l") => self.right(modifiers),
                Key::Character("d") => return self.del(modifiers),
                _ => return Some(EditorEvent::KeyUnhandled),
            },
            EditorMode::Edit => match key {
                Key::Character(char) => {
                    self.insert_text(&char);
                    return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                }
                Key::Named(NamedKey::Escape) => {
                    self.mode = EditorMode::Control;
                }
                Key::Named(NamedKey::Tab) => {
                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut graphemes = GraphemeCursor::new(0, line.len(), true);
                    let mut current = 0;
                    loop {
                        if self.cursor.byte <= current {
                            break;
                        }
                        let Some(idx) = graphemes.next_boundary(&line, 0).unwrap() else {
                            break;
                        };
                        current = idx as u32;
                    }
                    const TAB_WIDTH: u32 = 4;
                    self.insert_text(&"    "[..(TAB_WIDTH - current % TAB_WIDTH) as usize]);
                    return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                }
                Key::Named(NamedKey::Backspace) => {
                    if self.try_delete_selection() {
                        return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                    }

                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut graphemes =
                        GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

                    if let Some(prev_idx) = graphemes.prev_boundary(&line, 0).unwrap() {
                        self.apply_edit(Edit {
                            start: TextPosition {
                                line: self.cursor.line,
                                byte: prev_idx as u32,
                            },
                            end: TextPosition {
                                line: self.cursor.line,
                                byte: self.cursor.byte,
                            },
                            text: "",
                        });
                        return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                    } else if let Some(prev_line) = self.cursor.line.checked_sub(1) {
                        self.apply_edit(Edit {
                            start: TextPosition {
                                line: prev_line,
                                byte: self.lines[prev_line as usize].len() as u32,
                            },
                            end: TextPosition {
                                line: self.cursor.line,
                                byte: self.cursor.byte,
                            },
                            text: "",
                        });
                        return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                    }
                }
                Key::Named(NamedKey::Space) => {
                    self.insert_text(" ");
                    return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                }
                Key::Named(NamedKey::Enter) => {
                    self.insert_text("\n");
                    return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                }
                Key::Named(NamedKey::ArrowDown) => self.down(modifiers),
                Key::Named(NamedKey::ArrowUp) => self.up(modifiers),
                Key::Named(NamedKey::ArrowLeft) => self.left(modifiers),
                Key::Named(NamedKey::ArrowRight) => self.right(modifiers),
                _ => return Some(EditorEvent::KeyUnhandled),
            },
        }

        None
    }

    fn handle_selection(&mut self, modifiers: ModifiersState) -> bool {
        if modifiers.contains(ModifiersState::SHIFT) {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(TextPosition {
                    line: self.cursor.line,
                    byte: self.cursor.byte,
                });
            }
            true
        } else {
            if self.selection_anchor.is_some() {
                self.selection_anchor = None;
                true
            } else {
                false
            }
        }
    }

    fn apply_edit(&mut self, edit: Edit) {
        let start = Instant::now();

        let mut lines: Vec<_> = edit.text.split('\n').map(|line| line.to_string()).collect();
        let front_slice = &self.lines[edit.start.line as usize][..(edit.start.byte as usize)];
        let back_slice = &self.lines[edit.end.line as usize][(edit.end.byte as usize)..];

        lines.first_mut().unwrap().insert_str(0, front_slice);

        if self.cursor.line > edit.start.line
            || self.cursor.line == edit.start.line && self.cursor.byte >= edit.start.byte
        {
            if self.cursor.line > edit.end.line
                || self.cursor.line == edit.end.line && self.cursor.byte > edit.end.byte
            {
                let line_difference =
                    lines.len() as i32 - edit.end.line as i32 + edit.start.line as i32;

                self.cursor
                    .line
                    .checked_add_signed(line_difference as i32)
                    .unwrap();
            } else {
                self.cursor.line = edit.start.line + lines.len() as u32 - 1;
                self.cursor.byte = lines.last().unwrap().len() as u32;
            }
        }

        self.cursor.ephemeral_byte = self.cursor.byte;

        lines.last_mut().unwrap().push_str(back_slice);

        let shift = edit.start.line as i32 - edit.end.line as i32 - 1 + lines.len() as i32;
        self.lines
            .splice(edit.start.line as usize..=edit.end.line as usize, lines);

        self.visible_lines.retain_mut(|line| {
            if line.index > edit.end.line {
                line.index = line.index.checked_add_signed(shift).unwrap();
                true
            } else {
                line.index < edit.start.line
            }
        });

        self.compute_new_top_offset();
        self.render_lines();
        let cursor_left_px_offset = self.compute_cursor_left_px_offset();
        self.compute_new_left_offset(cursor_left_px_offset);
        self.reposition_cursor(cursor_left_px_offset);
        eprintln!("edit took {:.2?}", start.elapsed());
    }

    fn insert_text(&mut self, text: &str) {
        let (start, end) = self.get_selection_bounds();
        self.selection_anchor = None;
        self.apply_edit(Edit { start, end, text });
    }

    fn get_selection_bounds(&self) -> (TextPosition, TextPosition) {
        let cursor_position = self.cursor.text_position();
        let selection_anchor = self.selection_anchor.unwrap_or(TextPosition {
            line: self.cursor.line,
            byte: self.cursor.byte,
        });

        let selection_start = selection_anchor.min(cursor_position);
        let selection_end = selection_anchor.max(cursor_position);

        (selection_start, selection_end)
    }

    fn update_selection_rect_positions(&mut self) {
        let skipped_lines = self.top_px_offset / LINE_HEIGHT;

        let (selection_start, selection_end) = self.get_selection_bounds();

        for line in skipped_lines..(skipped_lines + self.visible_lines.len() as u32) {
            let visible_line = &mut self.visible_lines[(line - skipped_lines) as usize];
            visible_line.set_selection(selection_start, selection_end);
        }
    }

    fn try_delete_selection(&mut self) -> bool {
        if self.selection_anchor.is_some() {
            let (start, end) = self.get_selection_bounds();
            self.selection_anchor = None;
            self.apply_edit(Edit {
                start,
                end,
                text: "",
            });
            true
        } else {
            false
        }
    }

    fn drop_selection(&mut self) {
        if self.selection_anchor.is_some() {
            self.selection_anchor = None;
            self.update_selection_rect_positions();
        }
    }

    fn current_line_end(&self) -> u32 {
        self.lines[self.cursor.line as usize].len() as u32
    }

    fn move_to_prev_line_if_at_start(&mut self) -> Option<()> {
        if self.cursor.byte == 0 {
            self.cursor.line = self.cursor.line.checked_sub(1)?;
            self.cursor.byte = self.current_line_end();
        }

        Some(())
    }

    fn move_to_next_line_if_at_end(&mut self) -> Option<u32> {
        let mut line_end = self.current_line_end();
        if self.cursor.byte == line_end {
            if self.cursor.line == self.lines.len() as u32 - 1 {
                return None;
            }

            self.cursor.line += 1;
            self.cursor.byte = 0;
            line_end = self.current_line_end();
        }

        Some(line_end)
    }

    fn del(&mut self, modifiers: ModifiersState) -> Option<EditorEvent> {
        if self.try_delete_selection() {
            return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
        }

        let line = &mut self.lines[self.cursor.line as usize];
        let mut graphemes = GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

        if let Some(next_idx) = graphemes.next_boundary(&line, 0).unwrap() {
            self.apply_edit(Edit {
                start: self.cursor.text_position(),
                end: TextPosition {
                    line: self.cursor.line,
                    byte: next_idx as u32,
                },
                text: "",
            });
            return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
        }

        let next_line = self.cursor.line + 1;
        if next_line < self.lines.len() as u32 {
            self.apply_edit(Edit {
                start: self.cursor.text_position(),
                end: TextPosition {
                    line: next_line,
                    byte: 0,
                },
                text: "",
            });
            return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
        }

        None
    }

    fn up(&mut self, modifiers: ModifiersState) {
        if let Some(selection_changed) = self.try_up(modifiers) {
            self.handle_cursor_update();
            if selection_changed {
                self.update_selection_rect_positions();
            }
        }
    }

    fn down(&mut self, modifiers: ModifiersState) {
        if let Some(selection_changed) = self.try_down(modifiers) {
            self.handle_cursor_update();
            if selection_changed {
                self.update_selection_rect_positions();
            }
        }
    }

    fn left(&mut self, modifiers: ModifiersState) {
        let line = &mut self.lines[self.cursor.line as usize];
        let mut graphemes = GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

        if let Some(prev_idx) = graphemes.prev_boundary(&line, 0).unwrap() {
            let selection_changed = self.handle_selection(modifiers);
            self.cursor.byte = prev_idx as u32;
            self.cursor.ephemeral_byte = self.cursor.byte;
            self.handle_cursor_update();
            if selection_changed {
                self.update_selection_rect_positions();
            }
        } else if let Some(selection_changed) = self.try_up(modifiers) {
            self.cursor.byte = self.lines[self.cursor.line as usize].len() as u32;
            self.handle_cursor_update();
            if selection_changed {
                self.update_selection_rect_positions();
            }
        }
    }

    fn right(&mut self, modifiers: ModifiersState) {
        let line = &mut self.lines[self.cursor.line as usize];
        let mut graphemes = GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

        if let Some(next_idx) = graphemes.next_boundary(&line, 0).unwrap() {
            let selection_changed = self.handle_selection(modifiers);
            self.cursor.byte = next_idx as u32;
            self.cursor.ephemeral_byte = self.cursor.byte;
            self.handle_cursor_update();
            if selection_changed {
                self.update_selection_rect_positions();
            }
        } else if let Some(selection_changed) = self.try_down(modifiers) {
            self.cursor.byte = 0;
            self.handle_cursor_update();
            if selection_changed {
                self.update_selection_rect_positions();
            }
        }
    }

    fn try_down(&mut self, modifiers: ModifiersState) -> Option<bool> {
        if self.cursor.line < self.lines.len() as u32 - 1 {
            let selection_changed = self.handle_selection(modifiers);
            self.cursor.line += 1;
            self.cursor.ephemeral_byte = self.cursor.byte.max(self.cursor.ephemeral_byte);
            self.cursor.byte = (self.lines[self.cursor.line as usize].len() as u32)
                .min(self.cursor.ephemeral_byte);
            Some(selection_changed)
        } else {
            None
        }
    }

    fn try_up(&mut self, modifiers: ModifiersState) -> Option<bool> {
        if self.cursor.line > 0 {
            let selection_changed = self.handle_selection(modifiers);
            self.cursor.line -= 1;
            self.cursor.ephemeral_byte = self.cursor.byte.max(self.cursor.ephemeral_byte);
            self.cursor.byte = (self.lines[self.cursor.line as usize].len() as u32)
                .min(self.cursor.ephemeral_byte);
            Some(selection_changed)
        } else {
            None
        }
    }

    fn compute_cursor_left_px_offset(&self) -> u32 {
        get_px_offset_in_line(
            &self.visible_lines
                [self.cursor.line as usize - (self.top_px_offset / LINE_HEIGHT) as usize]
                .text_line,
            self.cursor.byte,
        )
    }

    fn reposition_cursor(&self, cursor_left_px_offset: u32) {
        self.cursor.rect.as_ref().unwrap().set_pos(
            cursor_left_px_offset as i32,
            (self.cursor.line as u32 * LINE_HEIGHT) as i32
                - (((self.top_px_offset / LINE_HEIGHT) * LINE_HEIGHT) as i32),
        );
    }

    fn handle_cursor_update(&mut self) {
        if self.compute_new_top_offset() {
            self.render_lines();
        }
        let x_offset = self.compute_cursor_left_px_offset();
        self.compute_new_left_offset(x_offset);
        self.reposition_cursor(x_offset);
    }

    fn compute_new_top_offset(&mut self) -> bool {
        let cursor_top_px_offset = self.cursor.line as u32 * LINE_HEIGHT;
        let (_, height) = self.main_box.get_size();

        let mut repositioned = false;

        let top_scroll_boundary = self.top_px_offset + height / 4;
        if cursor_top_px_offset < top_scroll_boundary {
            self.top_px_offset = self
                .top_px_offset
                .saturating_sub(top_scroll_boundary - cursor_top_px_offset);
            repositioned = true;
        }

        let cursor_bottom_px_offset = cursor_top_px_offset + LINE_HEIGHT;
        let bottom_scroll_boundary = self.top_px_offset + height - height / 4;
        if cursor_bottom_px_offset > bottom_scroll_boundary {
            self.top_px_offset = (self.top_px_offset
                + (cursor_bottom_px_offset - bottom_scroll_boundary))
                .min(self.lines.len() as u32 * LINE_HEIGHT);
            repositioned = true;
        }

        repositioned
    }

    fn compute_new_left_offset(&mut self, cursor_left_px_offset: u32) {
        let (width, _) = self.main_box.get_size();

        let mut repositioned = false;

        let left_scroll_boundary = self.left_px_offset + width / 4;
        if cursor_left_px_offset < left_scroll_boundary {
            self.left_px_offset = self
                .left_px_offset
                .saturating_sub(left_scroll_boundary - cursor_left_px_offset);
            repositioned = true;
        }

        let cursor_right_px_offset = cursor_left_px_offset + CURSOR_WIDTH;
        let right_scroll_boundary = self.left_px_offset + width - width / 4;
        if cursor_right_px_offset > right_scroll_boundary {
            self.left_px_offset =
                self.left_px_offset + (cursor_right_px_offset - right_scroll_boundary);
            repositioned = true;
        }

        if repositioned {
            self.update_lines_container_position();
        }
    }

    fn update_lines_container_position(&mut self) {
        self.lines_container.set_pos(
            -(self.left_px_offset as i32),
            -((self.top_px_offset % LINE_HEIGHT) as i32),
        );
    }

    fn render_lines(&mut self) {
        let mut current_lines: HashMap<_, _> = self
            .visible_lines
            .drain(..)
            .map(|line| (line.index, line))
            .collect();

        self.lines_container.clear_children();

        let height = self.main_box.get_size().1 as u32;

        let visible_lines_start = self.top_px_offset / LINE_HEIGHT;
        let visible_lines_end = (self.top_px_offset + height)
            .div_ceil(LINE_HEIGHT)
            .min(self.lines.len() as u32);

        let (selection_start, selection_end) = self.get_selection_bounds();

        for line_idx in visible_lines_start..visible_lines_end {
            let line = current_lines
                .remove(&line_idx)
                .inspect(|line| self.lines_container.append_child(&line.text_line.rect))
                .unwrap_or_else(|| {
                    let mut visible_line = VisibleLine::new(
                        &self.lines_container,
                        line_idx,
                        &self.lines[line_idx as usize],
                    );
                    visible_line.set_selection(selection_start, selection_end);
                    visible_line
                });

            line.text_line.rect.set_pos(
                0,
                ((line_idx - visible_lines_start) as u32 * LINE_HEIGHT) as i32,
            );
            self.visible_lines.push(line);
        }

        self.update_lines_container_position();
        self.lines_container
            .append_child(self.cursor.rect.as_ref().unwrap());
    }

    pub fn hide(&mut self) {
        self.main_box.remove_from_parent();
    }

    pub fn show(&mut self, parent: &Rect) {
        parent.append_child(&self.main_box);
        // this could be skipped if the parent wasn't resized, but idc
        self.handle_parent_resize(parent);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TextPosition {
    pub line: u32,
    pub byte: u32,
}

#[derive(Debug, Clone, Copy)]
struct Edit<'text> {
    start: TextPosition,
    end: TextPosition,
    text: &'text str,
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

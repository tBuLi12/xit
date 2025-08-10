use std::{collections::HashMap, time::Instant};

use caarr::{Rect, TextLine};

use crate::{
    editor::{Edit, TextPosition},
    BASE_FONT,
};

pub struct EditorView {
    top_px_offset: u32,
    left_px_offset: u32,
    lines: Vec<String>,
    visible_lines: Vec<VisibleLine>,
    selection: Selection,
    main_box: Rect,
    lines_container: Rect,
}

impl EditorView {
    pub fn new(main_box: Rect, lines: impl IntoIterator<Item = String>) -> Self {
        let lines_container = main_box.new_child();
        lines_container.set_size(i32::MAX as u32, i32::MAX as u32);

        // let cursor = lines_container.new_child();
        // cursor.set_size(CURSOR_WIDTH, LINE_HEIGHT);
        // cursor.set_bg_color([255, 0, 0, 255]);

        let mut lines: Vec<_> = lines.into_iter().collect();

        if lines.is_empty() {
            lines.push(String::new());
        }

        let visible_lines = lines
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

        EditorView {
            top_px_offset: 0,
            left_px_offset: 0,
            lines,
            visible_lines,
            selection: Selection {
                start: TextPosition { line: 0, byte: 0 },
                end: TextPosition { line: 0, byte: 0 },
            },
            main_box: main_box.clone(),
            lines_container,
        }
    }

    pub fn handle_resize(&mut self) {
        self.render_lines();
    }

    pub fn apply_edit(&mut self, edit: Edit) {
        let start = Instant::now();

        let mut lines: Vec<_> = edit.text.split('\n').map(|line| line.to_string()).collect();
        let front_slice = &self.lines[edit.start.line as usize][..(edit.start.byte as usize)];
        let back_slice = &self.lines[edit.end.line as usize][(edit.end.byte as usize)..];

        lines.first_mut().unwrap().insert_str(0, front_slice);

        // cursor bussiness
        {
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
        }

        lines.last_mut().unwrap().push_str(back_slice);

        let shift = edit.start.line as i32 - edit.end.line as i32 - 1 + lines.len() as i32;
        self.lines
            .splice(edit.start.line as usize..=edit.end.line as usize, lines);

        // remove modified lines
        self.visible_lines.retain_mut(|line| {
            if line.index > edit.end.line {
                line.index = line.index.checked_add_signed(shift).unwrap();
                true
            } else {
                line.index < edit.start.line
            }
        });

        let top_offset_dirty = self.compute_new_top_offset();
        self.render_lines();
        let cursor_left_px_offset = self.compute_cursor_left_px_offset();
        let left_offset_dirty = self.compute_new_left_offset(cursor_left_px_offset);

        if top_offset_dirty || left_offset_dirty {
            self.update_lines_container_position();
        }

        self.reposition_cursor(cursor_left_px_offset);
        eprintln!("edit took {:.2?}", start.elapsed());
    }

    fn reposition_cursor(&self, cursor_left_px_offset: u32) {
        self.cursor.rect.as_ref().unwrap().set_pos(
            cursor_left_px_offset as i32,
            (self.cursor.line as u32 * LINE_HEIGHT) as i32
                - (((self.top_px_offset / LINE_HEIGHT) * LINE_HEIGHT) as i32),
        );
    }

    fn compute_cursor_left_px_offset(&self) -> u32 {
        get_px_offset_in_line(
            &self.visible_lines
                [self.cursor.line as usize - (self.top_px_offset / LINE_HEIGHT) as usize]
                .text_line,
            self.cursor.byte,
        )
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

    fn compute_new_left_offset(&mut self, cursor_left_px_offset: u32) -> bool {
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

        repositioned
    }

    fn update_lines_container_position(&mut self) {
        self.lines_container.set_pos(
            -(self.left_px_offset as i32),
            -((self.top_px_offset % LINE_HEIGHT) as i32),
        );
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
}

const LINE_HEIGHT: u32 = 40;
const CURSOR_WIDTH: u32 = 2;

struct Selection {
    start: TextPosition,
    end: TextPosition,
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

    pub fn set_selection(&mut self, start_byte: u32, end_byte: u32) {
        let start = get_px_offset_in_line(&self.text_line, start_byte);
        let end = get_px_offset_in_line(&self.text_line, end_byte);

        self.selection_rect.set_pos(start as i32, 0);
        self.selection_rect.set_size(end - start, LINE_HEIGHT);
    }
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

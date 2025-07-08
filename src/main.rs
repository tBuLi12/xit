use std::{
    collections::{BTreeMap, HashMap},
    convert::identity,
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    ops::RangeInclusive,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    thread,
    time::Instant,
};

use caarr::{EventChannel, Font, Key, ModifiersState, NamedKey, Rect, TextLine};
use nucleo::{Nucleo, Utf32String};
use unicode_segmentation::GraphemeCursor;

use crate::sidebar::{Sidebar, SIDEBAR_WIDTH};

mod sidebar;

struct FilePicker {
    main_box: Rect,
    search_string: String,
    search_text: TextLine,
    results: Vec<TextLine>,
    current_pick: Option<PathBuf>,
    nucleo: Nucleo<PathBuf>,
    refilter: bool,
}

static BASE_FONT: LazyLock<Font<'static>> =
    LazyLock::new(|| Font::new(include_bytes!("../ARIAL.TTF"), 0, 30.0));

impl FilePicker {
    fn new(root: Rect, channel: EventChannel<Event>) -> Self {
        let main_box = root.new_child_at(0);
        {
            let (width, height) = root.get_size();
            main_box.set_size(width / 2, height / 2);
            main_box.set_pos(width as i32 / 4, height as i32 / 4);
        }
        main_box.set_bg_color([150, 150, 150, 255]);

        let search_text = main_box.new_text_child();
        search_text.rect.set_pos(10, 0);
        search_text.rect.set_bg_color([100, 100, 100, 255]);

        let nucleo = Nucleo::new(
            nucleo::Config::DEFAULT,
            Arc::new(move || channel.send_event(Event::ResultsChanged)),
            None,
            1,
        );

        let injector = nucleo.injector();
        thread::spawn(move || scan_dir(&injector));

        FilePicker {
            main_box,
            search_string: String::new(),
            search_text,
            results: vec![],
            current_pick: None,
            nucleo,
            refilter: true,
        }
    }

    fn handle_parent_resize(&mut self, parent: &Rect) {
        let (width, height) = parent.get_size();
        self.main_box.set_size(width / 2, height / 2);
        self.main_box.set_pos(width as i32 / 4, height as i32 / 4);
    }

    fn update_search(&mut self, append: bool) {
        self.search_text.set_text(*BASE_FONT, &self.search_string);

        self.nucleo.pattern.reparse(
            0,
            &self.search_string,
            nucleo::pattern::CaseMatching::Respect,
            nucleo::pattern::Normalization::Never,
            append,
        );

        let status = self.nucleo.tick(5);
        self.refilter = true;

        if status.changed {
            self.update_results();
        }
    }

    fn update_results(&mut self) {
        let status = self.nucleo.tick(5);

        if status.changed || self.refilter {
            self.refilter = false;
            let snapshot = self.nucleo.snapshot();
            self.main_box.clear_children();
            self.main_box.append_child(&self.search_text.rect);
            snapshot
                .matched_items(..snapshot.matched_item_count().min(20))
                .enumerate()
                .for_each(|(i, item)| {
                    let mut line = self.main_box.new_text_child();
                    line.set_text(*BASE_FONT, &item.data.as_os_str().to_string_lossy());
                    line.rect.set_pos(10, ((i as u32 + 1) * LINE_HEIGHT) as i32);
                });

            self.current_pick = snapshot.get_matched_item(0).map(|item| item.data.clone())
        }
    }

    fn handle_key_event(&mut self, key: Key<&str>, _modifiers: ModifiersState) -> Option<PathBuf> {
        match key {
            Key::Character(c) => {
                self.search_string.push_str(c);
                self.update_search(true);
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(cluster) = self.search_text.clusters().last() {
                    self.search_string
                        .replace_range(cluster.start as usize..cluster.end as usize, "");
                    self.update_search(false);
                }
            }
            Key::Named(NamedKey::Space) => {
                self.search_string.push(' ');
                self.update_search(true);
            }
            Key::Named(NamedKey::Enter) => return self.current_pick.clone(),
            _ => {}
        }

        None
    }
}

impl Drop for FilePicker {
    fn drop(&mut self) {
        self.main_box.remove_from_parent();
    }
}

fn scan_dir(injector: &nucleo::Injector<PathBuf>) {
    for entry in ignore::Walk::new(".") {
        if let Ok(entry) = entry {
            if entry.file_type().unwrap().is_file() {
                injector.push(entry.into_path(), |name, columns| {
                    columns[0] =
                        Utf32String::Unicode(name.as_os_str().to_string_lossy().chars().collect());
                });
            }
        }
    }
}

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

enum EditorStatus {
    Unsaved,
    None,
}

enum EditorMode {
    Edit,
    Control,
}

enum EditorEvent {
    SetStatus(EditorStatus),
    KeyUnhandled,
    Close,
}

struct Editor {
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
    fn new(root: Rect, lines: impl IntoIterator<Item = String>) -> Self {
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

    fn handle_parent_resize(&mut self, parent: &Rect) {
        self.main_box.set_size(
            parent.get_size().0.saturating_sub(SIDEBAR_WIDTH),
            parent.get_size().1,
        );
        self.render_lines();
    }

    fn handle_key_event(
        &mut self,
        key: &Key<&str>,
        modifiers: ModifiersState,
        self_path: &Path,
    ) -> Option<EditorEvent> {
        match self.mode {
            EditorMode::Control => match key {
                Key::Named(NamedKey::Control) => {
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
                }
                Key::Character("q") => {
                    return Some(EditorEvent::Close);
                }
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
                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut graphemes =
                        GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

                    if self.selection_anchor.is_some() {
                        let (start, end) = self.get_selection_bounds();
                        self.selection_anchor = None;
                        self.apply_edit(Edit {
                            start,
                            end,
                            text: "",
                        });
                        return Some(EditorEvent::SetStatus(EditorStatus::Unsaved));
                    }

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
                Key::Named(NamedKey::ArrowUp) => {
                    if let Some(selection_changed) = self.up(modifiers) {
                        self.handle_cursor_update();
                        if selection_changed {
                            self.update_selection_rect_positions();
                        }
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if let Some(selection_changed) = self.down(modifiers) {
                        self.handle_cursor_update();
                        if selection_changed {
                            self.update_selection_rect_positions();
                        }
                    }
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut graphemes =
                        GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

                    if let Some(prev_idx) = graphemes.prev_boundary(&line, 0).unwrap() {
                        let selection_changed = self.handle_selection(modifiers);
                        self.cursor.byte = prev_idx as u32;
                        self.cursor.ephemeral_byte = self.cursor.byte;
                        self.handle_cursor_update();
                        if selection_changed {
                            self.update_selection_rect_positions();
                        }
                    } else if let Some(selection_changed) = self.up(modifiers) {
                        self.cursor.byte = self.lines[self.cursor.line as usize].len() as u32;
                        self.handle_cursor_update();
                        if selection_changed {
                            self.update_selection_rect_positions();
                        }
                    }
                }
                Key::Named(NamedKey::ArrowRight) => {
                    let line = &mut self.lines[self.cursor.line as usize];
                    let mut graphemes =
                        GraphemeCursor::new(self.cursor.byte as usize, line.len(), true);

                    if let Some(next_idx) = graphemes.next_boundary(&line, 0).unwrap() {
                        let selection_changed = self.handle_selection(modifiers);
                        self.cursor.byte = next_idx as u32;
                        self.cursor.ephemeral_byte = self.cursor.byte;
                        self.handle_cursor_update();
                        if selection_changed {
                            self.update_selection_rect_positions();
                        }
                    } else if let Some(selection_changed) = self.down(modifiers) {
                        self.cursor.byte = 0;
                        self.handle_cursor_update();
                        if selection_changed {
                            self.update_selection_rect_positions();
                        }
                    }
                }
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

    fn down(&mut self, modifiers: ModifiersState) -> Option<bool> {
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

    fn up(&mut self, modifiers: ModifiersState) -> Option<bool> {
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
        let mut current_lines: BTreeMap<_, _> = self
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

struct App {
    editors: HashMap<PathBuf, Editor>,
    focused_editor: Option<PathBuf>,
    file_picker: Option<FilePicker>,
    sidebar: Option<Sidebar>,
    root: Option<Rect>,
    channel: Option<EventChannel<Event>>,
}

const LINE_HEIGHT: u32 = 40;
const CURSOR_WIDTH: u32 = 2;

enum Event {
    ResultsChanged,
}

impl caarr::App for App {
    type Event = Event;

    fn init(&mut self, root: &Rect, width: u32, height: u32, channel: EventChannel<Event>) {
        root.set_size(width, height);
        root.set_bg_color([255, 255, 255, 255]);

        self.file_picker = Some(FilePicker::new(root.clone(), channel.clone()));
        self.sidebar = Some(Sidebar::new(&root));

        self.root = Some(root.clone());
        self.channel = Some(channel);
    }

    fn on_key_event(&mut self, key: Key<&str>, modifiers: ModifiersState) {
        if let Some(file_picker) = self.file_picker.as_mut() {
            if let Some(path) = file_picker.handle_key_event(key, modifiers) {
                self.file_picker = None;

                let path = path.canonicalize().unwrap();

                self.open_editor(path);
            }
            return;
        }

        if let Some(path) = &self.focused_editor {
            let event = self
                .editors
                .get_mut(path)
                .unwrap()
                .handle_key_event(&key, modifiers, &path);

            if let Some(event) = event {
                match event {
                    EditorEvent::SetStatus(status) => {
                        self.sidebar
                            .as_mut()
                            .unwrap()
                            .set_editor_status(path, status);
                        return;
                    }
                    EditorEvent::KeyUnhandled => {}
                    EditorEvent::Close => {
                        self.editors.remove(path).unwrap().hide();
                        self.sidebar.as_mut().unwrap().remove_item(path);
                        self.focused_editor = None;
                        return;
                    }
                }
            }
        }

        match key {
            Key::Named(NamedKey::Tab) => {
                if self.file_picker.is_none() {
                    self.file_picker = Some(FilePicker::new(
                        self.root.as_ref().unwrap().clone(),
                        self.channel.as_ref().unwrap().clone(),
                    ));
                }
            }
            _ => {}
        }
    }

    fn on_resize(&mut self, width: u32, height: u32) {
        let root = self.root.as_ref().unwrap();
        root.set_size(width, height);
        if let Some(picker) = self.file_picker.as_mut() {
            picker.handle_parent_resize(root);
        }
        self.sidebar.as_mut().unwrap().handle_parent_resize(root);

        if let Some(path) = &self.focused_editor {
            self.editors
                .get_mut(path)
                .unwrap()
                .handle_parent_resize(root);
        }
    }

    fn on_event(&mut self, event: Self::Event) {
        match event {
            Event::ResultsChanged => {
                if let Some(picker) = self.file_picker.as_mut() {
                    picker.update_results();
                }
            }
        }
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

impl App {
    fn open_editor(&mut self, path: PathBuf) {
        eprintln!("opening editor {:?}", path);
        if let Some(editor) = self.editors.get_mut(&path) {
            if let Some(current_path) = &self.focused_editor {
                if current_path == &path {
                    eprintln!("no focus, already selected");
                    return;
                }
            }

            eprintln!("just focus");

            editor.show(self.root.as_ref().unwrap());
            self.set_focused_editor(path);
            return;
        }

        eprintln!("actually new");
        let new_editor = Editor::new(
            self.root.as_ref().unwrap().clone(),
            BufReader::new(File::open(&path).unwrap())
                .lines()
                .map(|line| line.unwrap()),
        );

        self.editors.insert(path.clone(), new_editor);
        self.sidebar
            .as_mut()
            .unwrap()
            .add_item(path.clone(), &path.file_name().unwrap().to_string_lossy());

        self.set_focused_editor(path);
    }

    fn set_focused_editor(&mut self, path: PathBuf) {
        let old = self.focused_editor.replace(path);
        if let Some(path) = old {
            self.editors.get_mut(&path).unwrap().hide();
        }
    }
}

fn main() {
    caarr::run_app(App {
        editors: HashMap::new(),
        focused_editor: None,
        sidebar: None,
        root: None,
        file_picker: None,
        channel: None,
    });
}

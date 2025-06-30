use std::{sync::Arc, thread};

use caarr::{EventChannel, Key, NamedKey, Rect, TextLine};
use nucleo::{Nucleo, Utf32String};

struct FilePicker {
    main_box: Rect,
    search_string: String,
    search_text: TextLine,
    results: Vec<TextLine>,
    nucleo: Nucleo<String>,
    refilter: bool,
}

impl FilePicker {
    fn new(root: Rect, channel: EventChannel<Event>) -> Self {
        let main_box = root.new_child();
        main_box.set_size(root.get_size().0 / 2, root.get_size().1 / 2);
        main_box.set_bg_color([150, 150, 150, 255]);

        let search_text = main_box.new_text_child();
        // search_text.rect.set_pos(10, 10);
        // search_text.rect.set_size(root.get_size().0 - 20, 30);
        // search_text.rect.set_bg_color([100, 100, 100, 255]);

        let nucleo = Nucleo::new(
            nucleo::Config::DEFAULT.match_paths(),
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
            nucleo,
            refilter: true,
        }
    }

    fn update_search(&mut self, append: bool) {
        self.nucleo.pattern.reparse(
            0,
            &self.search_string,
            nucleo::pattern::CaseMatching::Respect,
            nucleo::pattern::Normalization::Never,
            append,
        );

        self.nucleo.tick(0);
        self.refilter = true;
    }

    fn update_results(&mut self) {
        let status = self.nucleo.tick(0);

        if status.changed || self.refilter {
            self.refilter = false;
            let snapshot = self.nucleo.snapshot();
            snapshot
                .matched_items(..snapshot.matched_item_count().min(20))
                .enumerate()
                .for_each(|(i, item)| {
                    let mut line = self.main_box.new_text_child();
                    line.set_text(&item.data);
                    line.rect.set_pos(10, i as u32 * line_height);
                });
        }
    }
}

fn scan_dir(injector: &nucleo::Injector<String>) {
    for entry in ignore::Walk::new(".") {
        if let Ok(entry) = entry {
            if entry.file_type().unwrap().is_file() {
                injector.push(
                    entry.path().to_string_lossy().into_owned(),
                    |name, columns| {
                        columns[0] = Utf32String::Unicode(name.chars().collect());
                    },
                );
            }
        }
    }
}

struct Line {
    text_line: TextLine,
    string: String,
}

struct Cursor {
    line: usize,
    cluster: usize,
    ephemeral_cluster: usize,
    rect: Option<Rect>,
}

struct Editor {
    lines: Vec<Line>,
    cursor: Cursor,
    root: Rect,
}

impl Editor {
    fn new(root: Rect) -> Self {
        let text = root.new_text_child();

        let cursor = root.new_child();
        cursor.set_size(2, 20);
        cursor.set_bg_color([100, 100, 100, 255]);

        Editor {
            lines: vec![Line {
                string: String::new(),
                text_line: text,
            }],
            cursor: Cursor {
                line: 0,
                cluster: 0,
                ephemeral_cluster: 0,
                rect: Some(cursor),
            },
            root: root.clone(),
        }
    }

    fn handle_key_event(&mut self, key: Key<&str>) {
        match key {
            Key::Character(char) => {
                self.insert_text(&char);
            }
            Key::Named(NamedKey::Backspace) => {
                let mut line = &mut self.lines[self.cursor.line];

                if let Some(prev_idx) = self.cursor.cluster.checked_sub(1) {
                    let cluster = &line.text_line.clusters()[prev_idx];
                    line.string
                        .replace_range(cluster.start as usize..cluster.end as usize, "");
                    self.cursor.cluster = prev_idx;
                } else {
                    let removed_line = self.lines.remove(self.cursor.line);
                    removed_line.text_line.rect.remove_from_parent();

                    for i in self.cursor.line..self.lines.len() {
                        self.lines[i]
                            .text_line
                            .rect
                            .set_pos(0, i as u32 * line_height);
                    }

                    self.cursor.line -= 1;
                    line = &mut self.lines[self.cursor.line];
                    self.cursor.cluster = line.text_line.clusters().len();
                    line.string.push_str(&removed_line.string);
                }

                self.cursor.ephemeral_cluster = self.cursor.cluster;
                line.text_line.set_text(&line.string);
                self.reposition_cursor();
            }
            Key::Named(NamedKey::Space) => {
                self.insert_text(" ");
            }
            Key::Named(NamedKey::Enter) => {
                let line = &mut self.lines[self.cursor.line];
                let boundary = line
                    .text_line
                    .clusters()
                    .get(self.cursor.cluster)
                    .map(|c| c.start as usize)
                    .unwrap_or(line.string.len());

                let new_line = line.string.split_off(boundary);
                line.text_line.set_text(&line.string);
                self.cursor.line += 1;
                self.lines.insert(
                    self.cursor.line,
                    Line {
                        text_line: {
                            let mut line = self.root.new_text_child();
                            line.set_text(&new_line);
                            line
                        },
                        string: new_line,
                    },
                );
                for i in self.cursor.line..self.lines.len() {
                    self.lines[i]
                        .text_line
                        .rect
                        .set_pos(0, i as u32 * line_height);
                }
                self.cursor.cluster = 0;
                self.reposition_cursor();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.up();
                self.reposition_cursor();
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.down();
                self.reposition_cursor();
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if let Some(prev_idx) = self.cursor.cluster.checked_sub(1) {
                    self.cursor.cluster = prev_idx;
                    self.cursor.ephemeral_cluster = self.cursor.cluster;
                } else if self.up() {
                    self.cursor.cluster = self.lines[self.cursor.line].text_line.clusters().len();
                }
                self.reposition_cursor();
            }
            Key::Named(NamedKey::ArrowRight) => {
                let line = &mut self.lines[self.cursor.line];

                if let Some(_) = line.text_line.clusters().get(self.cursor.cluster) {
                    self.cursor.cluster += 1;
                    self.cursor.ephemeral_cluster = self.cursor.cluster;
                } else if self.down() {
                    self.cursor.cluster = 0;
                }
                self.reposition_cursor();
            }
            _ => {}
        }
    }

    fn insert_text(&mut self, text: &str) {
        let line = &mut self.lines[self.cursor.line];
        let insert_position = line
            .text_line
            .clusters()
            .get(self.cursor.cluster)
            .map(|c| c.start as usize)
            .unwrap_or(line.string.len());

        line.string.insert_str(insert_position, text);
        line.text_line.set_text(&line.string);
        let byte_position = insert_position + text.len();
        self.cursor.cluster = line
            .text_line
            .clusters()
            .iter()
            .position(|cluster| cluster.start as usize >= byte_position)
            .unwrap_or(line.text_line.clusters().len());
        self.reposition_cursor();
    }

    fn down(&mut self) -> bool {
        if self.cursor.line < self.lines.len() - 1 {
            self.cursor.line += 1;
            self.cursor.ephemeral_cluster = self.cursor.cluster.max(self.cursor.ephemeral_cluster);
            self.cursor.cluster = self.lines[self.cursor.line]
                .text_line
                .clusters()
                .len()
                .min(self.cursor.ephemeral_cluster);
            true
        } else {
            false
        }
    }

    fn up(&mut self) -> bool {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.ephemeral_cluster = self.cursor.cluster.max(self.cursor.ephemeral_cluster);
            self.cursor.cluster = self.lines[self.cursor.line]
                .text_line
                .clusters()
                .len()
                .min(self.cursor.ephemeral_cluster);
            true
        } else {
            false
        }
    }

    fn reposition_cursor(&self) {
        self.cursor.rect.as_ref().unwrap().set_pos(
            self.cursor
                .cluster
                .checked_sub(1)
                .map(|idx| self.lines[self.cursor.line].text_line.clusters()[idx].px_offset)
                .unwrap_or(0),
            self.cursor.line as u32 * line_height,
        );
    }
}

struct App {
    editor: Option<Editor>,
    file_picker: Option<FilePicker>,
    root: Option<Rect>,
}

const line_height: u32 = 40;

enum Event {
    ResultsChanged,
}

impl caarr::App for App {
    type Event = Event;

    fn init(&mut self, root: &Rect, width: u32, height: u32, channel: EventChannel<Event>) {
        root.set_size(width, height);
        root.set_bg_color([255, 255, 255, 255]);

        self.file_picker = Some(FilePicker::new(root.clone(), channel));
        self.editor = Some(Editor::new(root.clone()));

        self.root = Some(root.clone());
    }

    fn on_key_event(&mut self, key: Key<&str>) {
        if let Some(editor) = self.editor.as_mut() {
            editor.handle_key_event(key);
        }
    }

    fn on_resize(&mut self, width: u32, height: u32) {
        let root = self.root.as_ref().unwrap();
        root.set_size(width, height);
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

impl App {}

fn main() {
    caarr::run_app(App {
        editor: None,
        root: None,
        file_picker: None,
    });
}

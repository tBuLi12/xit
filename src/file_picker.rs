use std::{path::PathBuf, sync::Arc, thread};

use caarr::{text::TextLine, EventChannel, Key, KeyEvent, ModifiersState, NamedKey, Rect};
use nucleo::{Nucleo, Utf32String};
use unicode_segmentation::GraphemeCursor;

use crate::{Event, BASE_FONT, LINE_HEIGHT};

pub struct FilePicker {
    search_string: String,
    current_pick: Option<PathBuf>,
    nucleo: Nucleo<PathBuf>,
    refilter: bool,
}

impl FilePicker {
    pub fn new(channel: EventChannel<Event>) -> Self {
        let nucleo = Nucleo::new(
            nucleo::Config::DEFAULT,
            Arc::new(move || channel.send_event(Event::FilePickerResultsChanged)),
            None,
            1,
        );

        let injector = nucleo.injector();
        thread::spawn(move || scan_dir(&injector));

        FilePicker {
            search_string: String::new(),
            current_pick: None,
            nucleo,
            refilter: true,
        }
    }

    pub fn render(&self, width: u32, height: u32) -> Rect {
        let main_box = Rect::new();
        {
            main_box.set_size(width / 2, height / 2);
            main_box.set_pos(width as i32 / 4, height as i32 / 4);
        }
        main_box.set_bg_color([150, 150, 150, 255]);

        let mut search_text = TextLine::new();
        search_text.rect.set_pos(10, 0);
        search_text.rect.set_bg_color([100, 100, 100, 255]);
        search_text.set_text(*BASE_FONT, [(&*self.search_string, [0, 0, 0, 255])]);
        main_box.append_child(search_text.rect);

        let snapshot = self.nucleo.snapshot();
        snapshot
            .matched_items(..snapshot.matched_item_count().min(20))
            .enumerate()
            .for_each(|(i, item)| {
                let mut line = TextLine::new();
                line.set_text(
                    *BASE_FONT,
                    [(&*item.data.as_os_str().to_string_lossy(), [0, 0, 0, 255])],
                );
                line.rect.set_pos(10, ((i as u32 + 1) * LINE_HEIGHT) as i32);
                main_box.append_child(line.rect);
            });

        main_box
    }

    fn update_search(&mut self, append: bool) {
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

    pub fn update_results(&mut self) {
        let status = self.nucleo.tick(5);

        if status.changed || self.refilter {
            self.refilter = false;
            self.current_pick = self
                .nucleo
                .snapshot()
                .get_matched_item(0)
                .map(|item| item.data.clone())
        }
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) -> Option<PathBuf> {
        match event.key {
            Key::Character(c) => {
                self.search_string.push_str(c);
                self.update_search(true);
            }
            Key::Named(NamedKey::Backspace) => {
                let mut graphemes =
                    GraphemeCursor::new(self.search_string.len(), self.search_string.len(), true);
                if let Some(boundary) = graphemes.prev_boundary(&self.search_string, 0).unwrap() {
                    self.search_string.truncate(boundary);
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

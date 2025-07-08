use std::{path::PathBuf, sync::Arc, thread};

use caarr::{EventChannel, Key, ModifiersState, NamedKey, Rect, TextLine};
use nucleo::{Nucleo, Utf32String};

use crate::{Event, BASE_FONT, LINE_HEIGHT};

pub struct FilePicker {
    main_box: Rect,
    search_string: String,
    search_text: TextLine,
    current_pick: Option<PathBuf>,
    nucleo: Nucleo<PathBuf>,
    refilter: bool,
}

impl FilePicker {
    pub fn new(root: Rect, channel: EventChannel<Event>) -> Self {
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
            current_pick: None,
            nucleo,
            refilter: true,
        }
    }

    pub fn handle_parent_resize(&mut self, parent: &Rect) {
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

    pub fn update_results(&mut self) {
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

    pub fn handle_key_event(
        &mut self,
        key: Key<&str>,
        _modifiers: ModifiersState,
    ) -> Option<PathBuf> {
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

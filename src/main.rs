use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::LazyLock,
};

use caarr::{EventChannel, Font, Key, ModifiersState, NamedKey, Rect};

use crate::{
    editor::{Editor, EditorEvent},
    file_picker::FilePicker,
    sidebar::Sidebar,
};

mod editor;
mod file_picker;
mod sidebar;

static BASE_FONT: LazyLock<Font<'static>> =
    LazyLock::new(|| Font::new(include_bytes!("../ARIAL.TTF"), 0, 30.0));

struct App {
    editors: HashMap<PathBuf, Editor>,
    focused_editor: Option<PathBuf>,
    file_picker: Option<FilePicker>,
    sidebar: Option<Sidebar>,
    root: Option<Rect>,
    channel: Option<EventChannel<Event>>,
}

const LINE_HEIGHT: u32 = 40;

enum Event {
    FilePickerResultsChanged,
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
                self.open_editor(path.canonicalize().unwrap());
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
            Event::FilePickerResultsChanged => {
                if let Some(picker) = self.file_picker.as_mut() {
                    picker.update_results();
                }
            }
        }
    }
}

impl App {
    fn open_editor(&mut self, path: PathBuf) {
        if let Some(editor) = self.editors.get_mut(&path) {
            if let Some(current_path) = &self.focused_editor {
                if current_path == &path {
                    return;
                }
            }

            editor.show(self.root.as_ref().unwrap());
            self.set_focused_editor(path);
            return;
        }

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

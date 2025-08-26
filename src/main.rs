use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::{mpsc, LazyLock},
};

use caarr::{App, EventChannel, Font, Key, KeyEvent, ModifiersState, NamedKey, Rect};
use lsp::start_server;
use sidebar::{sidebar, SIDEBAR_WIDTH};

use crate::{
    editor::{Editor, EditorEvent},
    file_picker::FilePicker,
};

mod editor;
mod file_picker;
mod lsp;
mod sidebar;

static BASE_FONT: LazyLock<Font<'static>> =
    LazyLock::new(|| Font::new(include_bytes!("../ARIAL.TTF"), 0, 30.0));

struct State {
    editors: HashMap<PathBuf, Editor>,
    focused_editor: Option<PathBuf>,
    file_picker: Option<FilePicker>,
    server_tx: Option<mpsc::Sender<lsp::RequestEvent>>,
}

const LINE_HEIGHT: u32 = 40;

enum Event {
    FilePickerResultsChanged,
    Lsp(lsp::ResponseEvent),
}

impl caarr::State for State {
    type Event = Event;

    fn render(&self, width: u32, height: u32) -> Rect {
        let root = Rect::new();
        root.set_size(width, height);
        root.set_bg_color([255, 255, 255, 255]);

        root.append_child(sidebar(
            self.editors.iter().map(|(path, editor)| sidebar::Item {
                path,
                status: editor.status,
            }),
            height,
        ));

        if let Some(path) = &self.focused_editor {
            let editor = self.editors[path].render(width - SIDEBAR_WIDTH, height);
            editor.set_pos(SIDEBAR_WIDTH as i32, 0);
            root.append_child(editor);
        }

        if let Some(picker) = &self.file_picker {
            root.append_child(picker.render(width, height));
        }

        root
    }

    // fn init(&mut self, root: &Rect, width: u32, height: u32, channel: EventChannel<Event>) {
    //     self.file_picker = Some(FilePicker::new(root.clone(), channel.clone()));
    //     self.sidebar = Some(Sidebar::new(&root));
    // }

    fn on_key_event(app: &mut App<Self>, event: KeyEvent) {
        if let Some(file_picker) = app.state.file_picker.as_mut() {
            if let Some(path) = file_picker.handle_key_event(event) {
                app.state.file_picker = None;
                app.state.open_editor(path.canonicalize().unwrap());
            }
            return;
        }

        if let Some(path) = &app.state.focused_editor {
            let event = app
                .state
                .editors
                .get_mut(path)
                .unwrap()
                .handle_key_event(&path, &event);

            if let Some(event) = event {
                match event {
                    EditorEvent::KeyUnhandled => {}
                    EditorEvent::Close => {
                        app.state.editors.remove(path);
                        app.state.focused_editor = None;
                        return;
                    }
                }
            }
        }

        match event.key {
            Key::Named(NamedKey::Tab) => {
                if app.state.file_picker.is_none() {
                    app.state.server_tx =
                        Some(start_server("rust-analyzer".into(), app.channel.clone()));
                    app.state.file_picker = Some(FilePicker::new(app.channel.clone()));
                }
            }
            _ => {}
        }
    }

    fn on_event(app: &mut App<Self>, event: Self::Event) {
        match event {
            Event::FilePickerResultsChanged => {
                if let Some(picker) = app.state.file_picker.as_mut() {
                    picker.update_results();
                }
            }
            Event::Lsp(lsp::ResponseEvent::SemanticTokens { file, highlights }) => {
                if let Some(editor) = app.state.editors.get_mut(&file) {
                    editor.highlights = highlights;
                }
            }
        }
    }
}

impl State {
    fn open_editor(&mut self, path: PathBuf) {
        if let Some(_) = self.editors.get_mut(&path) {
            self.set_focused_editor(path);
            return;
        }

        let text = fs::read_to_string(&path).unwrap();

        let new_editor = Editor::new(text.lines().map(|line| line.to_string()));

        self.editors.insert(path.clone(), new_editor);

        if path.extension() == Some("rs".as_ref()) {
            eprintln!("sending semnatic tokens request to lsp thread");
            self.server_tx
                .as_ref()
                .unwrap()
                .send(lsp::RequestEvent::DidOpen {
                    file: path.clone(),
                    text,
                });
            self.server_tx
                .as_ref()
                .unwrap()
                .send(lsp::RequestEvent::SemanticTokens { file: path.clone() });
        } else {
            eprintln!(
                "not sending semnatic tokens request to lsp thread due to ending {}",
                path.display()
            );
        }

        self.set_focused_editor(path);
    }

    fn set_focused_editor(&mut self, path: PathBuf) {
        self.focused_editor = Some(path);
    }
}

fn main() {
    // for item in std::env::current_dir().unwrap().as_path() {
    //     eprintln!("{}", item.to_str().unwrap());
    // }
    // for item in std::env::current_dir()
    //     .unwrap()
    //     .canonicalize()
    //     .unwrap()
    //     .as_path()
    // {
    //     eprintln!("{}", item.to_str().unwrap());
    // }
    // for item in std::env::current_dir().unwrap().as_path().components() {
    //     eprintln!("{:?}", item);
    // }
    // for item in std::env::current_dir()
    //     .unwrap()
    //     .canonicalize()
    //     .unwrap()
    //     .as_path()
    //     .components()
    // {
    //     eprintln!("{:?}", item);
    // }
    // return;
    caarr::App::run(State {
        editors: HashMap::new(),
        focused_editor: None,
        file_picker: None,
        server_tx: None,
    });
}

mod renderer;

use std::{
    cell::RefCell,
    fs, iter,
    rc::{self, Rc},
    sync::Arc,
    thread, usize,
};

use nucleo::{Nucleo, Utf32String};
use renderer::{
    async_handler, center, on_key, pad, rows, text,
    widgets::{Center, FilledRect, SizedRect, TextLine, Widget},
    Child, Color, Fill, Offset, Rect, Size, View,
};
use winit::keyboard::{Key, NamedKey};

type RcCell<T> = Rc<RefCell<T>>;

enum EditorState {
    Selection(RcCell<SelectionState>),
    Edit(Vec<String>),
}

struct SelectionState {
    search: String,
    nucleo: Nucleo<String>,
    refilter: bool,
    filtered_files: Vec<String>,
    selected: usize,
    scan_pending: bool,
}

impl SelectionState {
    fn filter_files(&mut self, append: bool) {
        self.nucleo.pattern.reparse(
            0,
            &self.search,
            nucleo::pattern::CaseMatching::Respect,
            nucleo::pattern::Normalization::Never,
            append,
        );

        self.nucleo.tick(0);
        self.refilter = true;

        self.selected = 0;
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn down(&mut self) {
        self.selected = (self.filtered_files.len() - 1).min(self.selected + 1);
    }
}

fn main() {
    // renderer::run(
    //     || {
    //         Rc::new(RefCell::new(EditorState::Selection(Rc::new_cyclic(
    //             |this: &rc::Weak<RefCell<SelectionState>>| {
    //                 let this = this.clone();
    //                 let on_search_changed = async_handler(move |_| {
    //                     let Some(this) = this.upgrade() else {
    //                         return;
    //                     };

    //                     let mut this = this.borrow_mut();
    //                     let status = this.nucleo.tick(0);
    //                     if status.changed || this.refilter {
    //                         this.refilter = false;
    //                         let snapshot = this.nucleo.snapshot();
    //                         this.filtered_files = snapshot
    //                             .matched_items(..snapshot.matched_item_count().min(20))
    //                             .map(|item| item.data.clone())
    //                             .collect();
    //                     }
    //                 });

    //                 RefCell::new(SelectionState {
    //                     search: String::new(),
    //                     nucleo: Nucleo::new(
    //                         nucleo::Config::DEFAULT.match_paths(),
    //                         Arc::new(move || on_search_changed.run(())),
    //                         None,
    //                         1,
    //                     ),
    //                     filtered_files: vec![],
    //                     refilter: false,
    //                     selected: 0,
    //                     scan_pending: false,
    //                 })
    //             },
    //         ))))
    //     },
    //     editor,
    // );
    renderer::run(|| "Hello world!".to_string(), Dummy);
}

struct Dummy;

impl View<String> for Dummy {
    fn get(text: &String) -> impl Widget + '_ {
        FilledRect::new(Color::red()).children(Center::new(
            SizedRect::new(Size {
                height: 400,
                width: 400,
            })
            .color(Color::green())
            .children(Center::new(TextLine::new(text))),
        ))
    }
}

fn editor(state: &RcCell<EditorState>) -> Rect {
    let set_file = {
        let state = state.clone();
        move |name: String| {
            *state.borrow_mut() = EditorState::Edit(
                fs::read_to_string(name)
                    .unwrap()
                    .lines()
                    .map(|line| line.to_string())
                    .collect(),
            );
        }
    };

    let child = match &*state.borrow() {
        EditorState::Selection(state) => selection(state, set_file),
        EditorState::Edit(content) => rows(content.iter().map(|line| text(line)).collect()),
    };

    Rect::new()
        .fill_color(Color::gray(40))
        .size(Size {
            width: usize::MAX,
            height: usize::MAX,
        })
        .children(vec![Child {
            rect: child,
            position: Offset { x: 0, y: 0 },
        }])
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

fn selection(state: &Rc<RefCell<SelectionState>>, select_file: impl Fn(String) + 'static) -> Rect {
    if !state.borrow().scan_pending {
        let injector = state.borrow().nucleo.injector();

        thread::spawn(move || {
            scan_dir(&injector);
        });

        state.borrow_mut().scan_pending = true;
    }

    let items = {
        let state = state.borrow();

        iter::once((None, &state.search))
            .chain(
                state
                    .filtered_files
                    .iter()
                    .enumerate()
                    .map(|(i, str)| (Some(i), str)),
            )
            .map(|(i, str)| {
                let text = text(str);
                if i == Some(state.selected) {
                    text.fill_color(Color::gray(100))
                } else {
                    text
                }
            })
            .collect()
    };

    let panel = Rect::new()
        .size(Size {
            width: 1000,
            height: 1000,
        })
        .fill_color(Color::gray(60))
        .rounded(30.0)
        .children(vec![Child {
            rect: pad(rows(items), 40),
            position: Offset { x: 0, y: 0 },
        }])
        .on_key({
            let text = state.clone();
            move |key, _modifiers| match key {
                Key::Character(character) => {
                    text.borrow_mut().search.push_str(&character);
                    text.borrow_mut().filter_files(true);
                }
                Key::Named(named) => match named {
                    NamedKey::Enter => {
                        select_file(text.borrow().filtered_files[text.borrow().selected].clone());
                    }
                    NamedKey::Space => {
                        text.borrow_mut().search.push_str(" ");
                        text.borrow_mut().filter_files(true);
                    }
                    NamedKey::Backspace => {
                        text.borrow_mut().search.pop();
                        text.borrow_mut().filter_files(false);
                    }
                    NamedKey::ArrowDown => {
                        text.borrow_mut().down();
                    }
                    NamedKey::ArrowUp => {
                        text.borrow_mut().up();
                    }
                    _ => {}
                },
                _ => {}
            }
        });

    Rect::new()
        .size(Size {
            width: usize::MAX,
            height: usize::MAX,
        })
        .children(vec![Child {
            rect: panel,
            position: Offset { x: 0, y: 0 },
        }])
        .layout(center)
}

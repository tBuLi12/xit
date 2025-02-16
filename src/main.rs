mod renderer;

use std::{cell::RefCell, env, iter, rc::Rc, thread};

use renderer::{async_handler, center, on_key, rows, text, Child, Color, Fill, Offset, Rect, Size};
use winit::keyboard::{Key, NamedKey};

struct State {
    search: String,
    files: Vec<String>,
    selected: usize,
    scan_pending: bool,
}

fn main() {
    renderer::run(
        Rc::new(RefCell::new(State {
            search: String::new(),
            files: vec![],
            selected: 0,
            scan_pending: false,
        })),
        counter,
    );
}

fn counter(state: &Rc<RefCell<State>>) -> Rect {
    if !state.borrow().scan_pending {
        let on_scan_done = {
            let state = state.clone();
            async_handler(move |files| state.borrow_mut().files = files)
        };

        thread::spawn(move || {
            let mut files = vec![];
            for entry in std::fs::read_dir(env::current_dir().unwrap()).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    files.push(entry.file_name().into_string().unwrap());
                }
            }
            on_scan_done.run(files);
        });

        state.borrow_mut().scan_pending = true;
    }

    Rect::new()
        .fill_color(Color::gray(40))
        .on_key({
            let text = state.clone();
            move |key, _modifiers| match key {
                Key::Character(character) => {
                    text.borrow_mut().search.push_str(&character);
                }
                Key::Named(named) => match named {
                    NamedKey::Enter => {
                        // text.push("".to_string());
                    }
                    NamedKey::Space => {
                        text.borrow_mut().search.push_str(" ");
                    }
                    NamedKey::Backspace => {
                        text.borrow_mut().search.pop();
                    }
                    NamedKey::ArrowDown => {
                        text.borrow_mut().selected += 1;
                    }
                    NamedKey::ArrowUp => {
                        text.borrow_mut().selected -= 1;
                    }
                    _ => {}
                },
                _ => {}
            }
        })
        .layout(center)
        .children(vec![Child {
            rect: Rect::new()
                .fill_color(Color::gray(60))
                .rounded(5.0)
                .children(vec![Child {
                    rect: rows(
                        iter::once((None, &state.borrow().search))
                            .chain(
                                state
                                    .borrow()
                                    .files
                                    .iter()
                                    .filter(|str| str.contains(&state.borrow().search))
                                    .enumerate()
                                    .map(|(i, str)| (Some(i), str)),
                            )
                            .map(|(i, str)| {
                                let text = text(str);
                                if i == Some(state.borrow().selected) {
                                    text.fill_color(Color::gray(100))
                                } else {
                                    text
                                }
                            })
                            .collect(),
                    ),
                    position: Offset { x: 0, y: 0 },
                }]),
            position: Offset { x: 0, y: 0 },
        }])
}

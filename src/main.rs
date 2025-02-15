mod renderer;

use std::{cell::RefCell, env, iter, rc::Rc, thread};

use renderer::{async_handler, center, on_key, rows, text, Child, Color, Fill, Offset, Rect, Size};
use winit::keyboard::{Key, NamedKey};

struct State {
    search: String,
    files: Vec<String>,
    scan_pending: bool,
}

fn main() {
    renderer::run(
        Rc::new(RefCell::new(State {
            search: String::new(),
            files: vec![],
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

    let on_key = {
        let text = state.clone();
        on_key(move |key, modifiers| match key {
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
                _ => {}
            },
            _ => {}
        })
    };

    Rect {
        size: Size {
            width: 2_800,
            height: 1_650,
        },
        fill: Fill::Color(Color::gray(40)),
        on_click: None,
        radius: 0.0,
        on_key_pressed: Some(on_key),
        children: vec![Child {
            rect: Rect {
                children: vec![Child {
                    rect: rows(
                        iter::once(&state.borrow().search)
                            .chain(state.borrow().files.iter())
                            .filter(|str| str.contains(&state.borrow().search))
                            .map(|str| text(str))
                            .collect(),
                    ),
                    position: Offset { x: 0, y: 0 },
                }],
                do_layout: None,
                fill: Fill::Color(Color::gray(60)),
                on_click: None,
                on_key_pressed: None,
                radius: 5.0,
                size: Size {
                    width: 1400,
                    height: 1400,
                },
            },
            position: Offset { x: 0, y: 0 },
        }],
        do_layout: Some(center),
    }
}

mod renderer;

use renderer::{center, rows, text, Child, Color, Fill, KeyHandler, Offset, Rect, Size};
use winit::keyboard::{Key, NamedKey};

// fn dir_finder()

fn main() {
    renderer::run("".to_string(), counter);
}

fn counter(str: &String) -> Rect {
    let on_key = KeyHandler::new(str, |text, key, modifiers| match key {
        Key::Character(character) => {
            text.push_str(&character);
        }
        Key::Named(named) => match named {
            NamedKey::Enter => {
                // text.push("".to_string());
            }
            NamedKey::Space => {
                text.push_str(" ");
            }
            _ => {}
        },
        _ => {}
    });

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
            // rect: rows(str.iter().map(|str| text(str)).collect()),
            rect: Rect {
                children: vec![Child {
                    rect: text(str),
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

use std::{
    fs,
    io::{BufRead, Read},
};

use winit::keyboard;

use crate::{static_ui::text, SubpixelPosition};

use super::{CachedLine, Color, Component, Point, Runtime, Size, Visitor};

struct Line {
    text: String,
    cached: CachedLine,
    last_subpixel_position: Option<SubpixelPosition>,
}

impl Line {
    fn new(text: String) -> Self {
        Self {
            text,
            cached: CachedLine {
                glyphs: vec![],
                units: vec![],
                width: 0.0,
            },
            last_subpixel_position: None,
        }
    }

    fn layout_text(&mut self, rt: &mut dyn Runtime, subpixel_position: SubpixelPosition) {
        rt.get_text(super::TextProps {
            font_size: 44.0,
            font_id: 0,
        })
        .render_line(&self.text, subpixel_position, &mut self.cached);

        self.last_subpixel_position = Some(subpixel_position);
    }

    fn invalidate(&mut self) {
        self.last_subpixel_position = None;
    }

    fn draw(&mut self, point: Point, _: Option<Point>, rt: &mut dyn Runtime) {
        let (x, y, subpixel_position) = SubpixelPosition::from_f32(point.x + 0.5, point.y);

        if self.last_subpixel_position != Some(subpixel_position) {
            self.layout_text(rt, subpixel_position);
        }

        for (i, glyph) in self.cached.glyphs.iter().enumerate() {
            if let Some(tex_position) = glyph.tex_position {
                rt.draw_glyph(
                    x + glyph.left,
                    y - glyph.top - 20.0,
                    glyph.size,
                    tex_position,
                    Color::black().red(0.8).green(0.8).blue(0.8),
                    // Color::black().red(0.1215).green(0.1215).blue(0.1215),
                    Color::black().red(1.0),
                );
            }
        }
    }
}

impl Component for Editor {
    fn visit_children(&mut self, _: &mut impl Visitor) {}

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}

    fn handle_draw(&mut self, point: Point, _: Option<Point>, rt: &mut dyn Runtime) {
        rt.draw_rect(
            point.x,
            point.y,
            self.size.width,
            self.size.height,
            0.0,
            0.0,
            Color::black().red(0.1215).green(0.1215).blue(0.1215),
            Color::clear(),
        );

        for (i, line) in self.lines.iter_mut().enumerate() {
            line.draw(
                Point {
                    x: point.x,
                    y: point.y + (i as f32 + 1.0) * 60.0,
                },
                None,
                rt,
            );
        }

        rt.draw_rect(
            point.x + self.get_cursor_x() - 1.0,
            point.y + self.cursor.line as f32 * 60.0,
            2.0,
            60.0,
            0.0,
            0.0,
            Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 0.5,
            },
            Color::clear(),
        );
    }

    // fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
    //     if !self.size.contains(point) {
    //         return false;
    //     }

    //     self.cursor = Some(
    //         self.glyphs
    //             .line
    //             .iter()
    //             .find_map(|(glyph, (start, end))| {
    //                 if glyph.left <= point.x && point.x <= glyph.left + glyph.size[0] {
    //                     Some(*start)
    //                 } else {
    //                     None
    //                 }
    //             })
    //             .unwrap_or(self.line.text().len()),
    //     );

    //     true
    // }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = bounds;
    }

    fn key_pressed(&mut self, key: &keyboard::Key, rt: &mut dyn Runtime) {
        use keyboard::{Key, NamedKey};
        use unicode_segmentation::GraphemeCursor;

        match key {
            Key::Character(text) => {
                self.insert_text(&text);
            }
            Key::Named(NamedKey::Backspace) => {
                let line = &mut self.lines[self.cursor.line];
                let mut grapheme_cursor =
                    GraphemeCursor::new(self.cursor.byte, line.text.len(), true);
                if let Some(prev) = grapheme_cursor.prev_boundary(&line.text, 0).unwrap() {
                    line.text.replace_range(prev..self.cursor.byte, "");
                    line.invalidate();
                    self.cursor.byte = prev;
                    self.cursor.ephemeral_byte = self.cursor.byte;
                } else if self.cursor.line > 0 {
                    let text = self.lines.remove(self.cursor.line).text;
                    self.cursor.line -= 1;
                    let line = &mut self.lines[self.cursor.line];
                    self.cursor.byte = line.text.len();
                    line.text.push_str(&text);
                    self.cursor.ephemeral_byte = self.cursor.byte;
                    line.invalidate();
                }
            }
            Key::Named(NamedKey::Space) => {
                self.insert_text(" ");
            }
            Key::Named(NamedKey::Enter) => {
                let line = &mut self.lines[self.cursor.line];
                let new_line = line.text.split_off(self.cursor.byte);
                line.invalidate();
                self.lines.insert(self.cursor.line + 1, Line::new(new_line));
                self.cursor.line += 1;
                self.cursor.byte = 0;
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.up();
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.down();
            }
            Key::Named(NamedKey::ArrowLeft) => {
                let line = &mut self.lines[self.cursor.line];
                let mut grapheme_cursor =
                    GraphemeCursor::new(self.cursor.byte, line.text.len(), true);
                if let Some(prev) = grapheme_cursor.prev_boundary(&line.text, 0).unwrap() {
                    self.cursor.byte = prev;
                    self.cursor.ephemeral_byte = self.cursor.byte;
                } else if self.up() {
                    self.cursor.byte = self.lines[self.cursor.line].text.len();
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                let line = &mut self.lines[self.cursor.line];
                let mut grapheme_cursor =
                    GraphemeCursor::new(self.cursor.byte, line.text.len(), true);
                if let Some(next) = grapheme_cursor.next_boundary(&line.text, 0).unwrap() {
                    self.cursor.byte = next;
                    self.cursor.ephemeral_byte = self.cursor.byte;
                } else if self.down() {
                    self.cursor.byte = 0;
                }
            }
            _ => {}
        }
        // self.cursor = Some(cursor + key.len());
    }

    fn size(&self) -> Size {
        self.size
    }
}

pub struct Editor {
    lines: Vec<Line>,
    size: Size,
    cursor: Cursor,
}

impl Editor {
    pub fn new(path: impl AsRef<std::path::Path>) -> Self {
        let file = fs::File::open(path).unwrap();
        let mut bytes = std::io::BufReader::new(file).bytes().peekable();

        let mut buf = vec![];
        let mut lines = vec![];

        while let Some(byte) = bytes.next().transpose().unwrap() {
            if byte == b'\n' {
                lines.push(Line::new(String::from_utf8(buf.clone()).unwrap()));
                buf.clear();
            } else if byte == b'\r' {
                if let Some(Ok(b'\n')) = bytes.peek() {
                    bytes.next();
                }
                lines.push(Line::new(String::from_utf8(buf.clone()).unwrap()));
                buf.clear();
            } else {
                buf.push(byte);
            }
        }

        Self {
            lines,
            size: Size::ZERO,
            cursor: Cursor {
                line: 0,
                byte: 2,
                ephemeral_byte: 0,
            },
        }
    }

    fn get_cursor_x(&self) -> f32 {
        let line = &self.lines[self.cursor.line];
        let mut x = 0.0;
        for unit in &line.cached.units {
            if self.cursor.byte >= unit.byte_end {
                x = unit.advance;
            } else {
                break;
            }
        }
        x
    }

    fn insert_text(&mut self, text: &str) {
        let line = &mut self.lines[self.cursor.line];
        line.text.insert_str(self.cursor.byte, text);
        line.invalidate();
        self.cursor.byte += text.len();
        self.cursor.ephemeral_byte = self.cursor.byte;
    }

    fn down(&mut self) -> bool {
        if self.cursor.line < self.lines.len() - 1 {
            self.cursor.line += 1;
            self.cursor.ephemeral_byte = self.cursor.byte.max(self.cursor.ephemeral_byte);
            self.cursor.byte = self.lines[self.cursor.line]
                .text
                .len()
                .min(self.cursor.ephemeral_byte);
            true
        } else {
            false
        }
    }

    fn up(&mut self) -> bool {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.ephemeral_byte = self.cursor.byte.max(self.cursor.ephemeral_byte);
            self.cursor.byte = self.lines[self.cursor.line]
                .text
                .len()
                .min(self.cursor.ephemeral_byte);
            true
        } else {
            false
        }
    }
}

struct Cursor {
    line: usize,
    byte: usize,
    ephemeral_byte: usize,
}

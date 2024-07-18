use crate::SubpixelPosition;

use super::{CachedLine, Color, Component, Point, Runtime, Size};

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
            font_size: 26.0,
            font_id: 0,
        })
        .render_line(&self.text, subpixel_position, &mut self.cached);

        self.last_subpixel_position = Some(subpixel_position);
    }

    fn invalidate(&mut self) {
        self.last_subpixel_position = None;
    }

    fn draw(&mut self, point: Point, _: Option<Point>, rt: &mut dyn Runtime) {
        let (x, y, subpixel_position) = SubpixelPosition::from_f32(point.x, point.y);

        if self.last_subpixel_position != Some(subpixel_position) {
            self.layout_text(rt, subpixel_position);
        }

        for glyph in &self.cached.glyphs {
            if let Some(tex_position) = glyph.tex_position {
                rt.draw_glyph(
                    x + glyph.left,
                    y - glyph.top - 20.0,
                    glyph.size,
                    tex_position,
                    Color::black().red(1.0),
                    Color::black(),
                );
            }
        }
    }
}

impl Component for Editor {
    fn visit_children(&mut self, _: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {}

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}

    fn draw(&mut self, point: Point, _: Option<Point>, rt: &mut dyn Runtime) {
        for (i, line) in self.lines.iter_mut().enumerate() {
            line.draw(
                Point {
                    x: point.x,
                    y: point.y + i as f32 * 60.0,
                },
                None,
                rt,
            );
        }

        let offset = if self.cursor.byte == 0 {
            0.0
        } else {
            let advance = self.lines[self.cursor.line].cached.units[self.cursor.unit].advance;
            advance
        };

        rt.draw_rect(
            point.x + offset,
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

    fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
        let line = &mut self.lines[self.cursor.line];
        line.text.insert_str(self.cursor.byte, &key);
        line.invalidate();
        self.cursor.byte += key.len();
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
    pub fn new() -> Self {
        Self {
            lines: vec![
                Line::new("Hello".to_string()),
                Line::new("World".to_string()),
                Line::new("World g".to_string()),
                Line::new("World".to_string()),
            ],
            size: Size::ZERO,
            cursor: Cursor {
                line: 2,
                unit: 2,
                byte: 2,
            },
        }
    }
}

struct Cursor {
    line: usize,
    unit: usize,
    byte: usize,
}

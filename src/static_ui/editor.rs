use cosmic_text::{Attrs, AttrsList};

use super::{text::CachedLine, Color, Component, Point, Runtime, Size};

struct Line {
    buffer: cosmic_text::BufferLine,
    glyphs: CachedLine,
    dirty: bool,
}

impl Line {
    fn new(text: String) -> Self {
        Self {
            buffer: cosmic_text::BufferLine::new(
                text,
                cosmic_text::LineEnding::None,
                AttrsList::new(Attrs::new()),
                cosmic_text::Shaping::Advanced,
            ),
            glyphs: CachedLine {
                line: vec![],
                width: 0.0,
            },
            dirty: true,
        }
    }

    fn layout_text(&mut self, rt: &mut dyn Runtime) {
        self.glyphs.line.clear();

        let shape = self.buffer.shape(rt.font_system(), 4);
        let laid_out_line = shape
            .layout(40.0, None, cosmic_text::Wrap::None, None, None)
            .pop()
            .unwrap();

        for glyph in laid_out_line.glyphs {
            let physical_glyph = glyph.physical((0.0, 0.0), 1.0);
            let Some(mut cached_glyph) = rt.get_glyph(physical_glyph.cache_key) else {
                continue;
            };

            cached_glyph.top += physical_glyph.y as f32;
            cached_glyph.left += physical_glyph.x as f32;

            self.glyphs
                .line
                .push((cached_glyph, (glyph.start, glyph.end)));
        }

        self.glyphs.width = laid_out_line.w;
        self.dirty = false;
    }

    fn set_text(&mut self, text: String, rt: &mut dyn Runtime) {
        self.buffer.set_text(
            text,
            cosmic_text::LineEnding::None,
            AttrsList::new(Attrs::new()),
        );
        self.layout_text(rt);
    }

    fn draw(&mut self, point: Point, _: Option<Point>, rt: &mut dyn Runtime) {
        if self.dirty {
            self.layout_text(rt);
        }

        for cluster in &self.glyphs.line {
            for glyph in &cluster.glyphs {
                if let Some(tex_position) = glyph.tex_position {
                    rt.draw_glyph(
                        point.x + glyph.left,
                        point.y - glyph.top - 20.0,
                        glyph.size,
                        tex_position,
                        Color::black().red(1.0),
                    );
                }
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
            let glyph = self.lines[self.cursor.line].glyphs.line[self.cursor.character].glyphs;
            glyph.left + glyph.size[0]
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

    // fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
    //     let Some(cursor) = self.cursor else {
    //         return;
    //     };
    //     let mut text = self.line.text().to_string();
    //     text.insert_str(cursor, &key);
    //     self.cursor = Some(cursor + key.len());
    //     self.set_text(text, rt);
    //     self.layout_text(rt);
    // }

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
                Line::new("Worldg".to_string()),
                Line::new("World".to_string()),
            ],
            size: Size::ZERO,
            cursor: Cursor {
                line: 2,
                character: 2,
                byte: 2,
            },
        }
    }
}

struct Cursor {
    line: usize,
    character: usize,
    byte: usize,
}

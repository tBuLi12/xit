use crate::CachedGlyph;

use super::{AxisAlignment, Color, Component, Point, Runtime, Size};

#[derive(Clone, Debug)]
pub struct CachedCluster {
    pub glyphs: Vec<CachedGlyph>,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct CachedLine {
    pub line: Vec<CachedCluster>,
    pub width: f32,
}

#[derive(Clone, Debug)]
struct CachedTextState {
    buffer: cosmic_text::Buffer,
    glyphs: Vec<CachedLine>,
    size: Size,
}

impl CachedTextState {
    fn new(rt: &mut dyn Runtime) -> Self {
        let buffer = cosmic_text::Buffer::new(
            rt.font_system(),
            cosmic_text::Metrics {
                font_size: 40.0,
                line_height: 40.0,
            },
        );

        Self {
            buffer,
            glyphs: vec![],
            size: Size {
                width: 0.0,
                height: 0.0,
            },
        }
    }

    fn layout_text(&mut self, rt: &mut dyn Runtime) {
        self.glyphs.clear();
        self.buffer.shape_until_scroll(rt.font_system(), false);

        let mut line_count = 0;
        let mut width: f32 = 0.0;

        for run in self.buffer.layout_runs() {
            line_count += 1;
            width = width.max(run.line_w);

            let mut line = CachedLine {
                line: vec![],
                width: run.line_w,
            };

            for glyph in run.glyphs {
                let physical_glyph = glyph.physical((0.0, 0.0), 1.0);
                let Some(mut cached_glyph) = rt.get_glyph(physical_glyph.cache_key) else {
                    continue;
                };

                cached_glyph.top += physical_glyph.y as f32 - run.line_y;
                cached_glyph.left += physical_glyph.x as f32;

                line.line.push((cached_glyph, (glyph.start, glyph.end)));
            }
            self.glyphs.push(line);
        }

        self.size = Size {
            width,
            height: 40.0 * line_count as f32,
        };
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.buffer
            .set_size(rt.font_system(), Some(bounds.width), Some(bounds.height));
    }

    fn set_text(&mut self, text: &str, rt: &mut dyn Runtime) {
        self.buffer.set_text(
            rt.font_system(),
            text,
            cosmic_text::Attrs::new(),
            cosmic_text::Shaping::Advanced,
        );
    }
}

#[derive(Clone, Debug)]
enum TextState {
    New(String),
    Cached(CachedTextState),
}

#[derive(Clone, Debug)]
pub struct Text {
    inner: TextState,
    h_alignment: AxisAlignment,
    bounds: Size,
    color: Color,
}

impl Text {
    pub fn new(value: String, color: Color, h_alignment: AxisAlignment) -> Self {
        // let mut buffer = cosmic_text::Buffer::new_empty(cosmic_text::Metrics {
        //     font_size: 40.0,
        //     line_height: 40.0,
        // });

        Self {
            inner: TextState::New(value),
            bounds: Size {
                width: 0.0,
                height: 0.0,
            },
            h_alignment,
            color,
        }
    }

    pub fn set_text(&mut self, new_text: &str, rt: &mut dyn Runtime) {
        match &mut self.inner {
            TextState::Cached(cached) => {
                cached.set_text(new_text, rt);
                cached.layout_text(rt);
            }
            TextState::New(text) => {
                text.clear();
                text.push_str(new_text);
            }
        }
    }

    fn get_text_state(&mut self, rt: &mut dyn Runtime) -> &mut CachedTextState {
        let cached = match &mut self.inner {
            TextState::Cached(_) => None,
            TextState::New(text) => {
                let mut cached = CachedTextState::new(rt);
                cached.set_text(text, rt);
                Some(cached)
            }
        };

        if let Some(cached) = cached {
            self.inner = TextState::Cached(cached);
        }

        let TextState::Cached(cached) = &mut self.inner else {
            unreachable!()
        };
        cached
    }
}

impl Component for Text {
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        let TextState::Cached(cached) = &mut self.inner else {
            panic!("Cannot draw text before layout");
        };

        for line in &cached.glyphs {
            let end_x_offset = cached.size.width - line.width;
            let x_offset = match self.h_alignment {
                AxisAlignment::Start => 0.0,
                AxisAlignment::Center => end_x_offset / 2.0,
                AxisAlignment::End => end_x_offset,
            };

            for (glyph, _) in &line.line {
                if let Some(tex_position) = glyph.tex_position {
                    rt.draw_glyph(
                        point.x + glyph.left + x_offset,
                        point.y - glyph.top,
                        glyph.size,
                        tex_position,
                        self.color,
                    );
                }
            }
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        let cached = self.get_text_state(rt);
        cached.set_bounds(bounds, rt);
        cached.layout_text(rt);
    }

    fn size(&self) -> Size {
        let TextState::Cached(cached) = &self.inner else {
            panic!("Cannot get size before layout");
        };

        cached.size
    }

    fn visit_children(&mut self, _: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {}
    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}
}

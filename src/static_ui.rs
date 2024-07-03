use std::cell::RefCell;

use crate::CachedGlyph;

#[derive(Copy, Clone, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct Text {
    value: String,
    buffer: cosmic_text::Buffer,
    glyphs: Vec<CachedGlyph>,
    rect: Rect,
}

impl Text {
    fn new(value: String, bounds: Rect, rt: &mut dyn Runtime) -> Self {
        let mut buffer = cosmic_text::Buffer::new(
            rt.font_system(),
            cosmic_text::Metrics {
                font_size: 80.0,
                line_height: 80.0,
            },
        );
        buffer.set_text(
            rt.font_system(),
            &value,
            cosmic_text::Attrs::new(),
            cosmic_text::Shaping::Advanced,
        );
        buffer.set_size(rt.font_system(), Some(bounds.width), Some(bounds.height));

        buffer.shape_until_scroll(rt.font_system(), false);
        let glyphs = vec![];

        let mut this = Self {
            value,
            buffer,
            glyphs,
            rect: bounds,
        };

        this.layout_text(rt);
        this
    }

    fn layout_text(&mut self, rt: &mut dyn Runtime) {
        self.glyphs.clear();
        self.buffer.shape_until_scroll(rt.font_system(), false);
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical_glyph = glyph.physical((0.0, 0.0), 1.0);
                let Some(mut cached_glyph) = rt.get_glyph(physical_glyph.cache_key) else {
                    continue;
                };

                cached_glyph.top += physical_glyph.y;
                cached_glyph.left += physical_glyph.x;

                self.glyphs.push(cached_glyph);
            }
        }
    }

    fn set_bounds(&mut self, bounds: Rect, rt: &mut dyn Runtime) {
        self.rect = bounds;
        self.layout_text(rt);
    }

    fn draw(&self, rt: &mut dyn Runtime) {
        rt.draw_rect(
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height,
            0.0,
            0.0,
        );

        for glyph in &self.glyphs {
            rt.draw_glyph(
                self.rect.x + glyph.left as f32,
                self.rect.y - glyph.top as f32 + self.rect.height,
                glyph.tex_location.size,
                glyph.tex_location.pos,
            );
        }
    }
}

// #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
// pub struct PrimitiveID(u32);

pub trait Runtime {
    // fn next_text_id(&mut self) -> PrimitiveID;
    // fn next_rect_id(&mut self) -> PrimitiveID;
    fn draw_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        corner_radius: f32,
        border_width: f32,
    );
    fn draw_glyph(&mut self, x: f32, y: f32, size: [f32; 2], tex_coords: [f32; 2]);
    fn font_system(&mut self) -> &mut cosmic_text::FontSystem;
    fn get_glyph(&mut self, key: cosmic_text::CacheKey) -> Option<CachedGlyph>;
}

pub trait Component {
    fn draw(&mut self, rt: &mut dyn Runtime);
    fn set_bounds(&mut self, bounds: Rect, rt: &mut dyn Runtime);
    fn click(&mut self, x: f32, y: f32);
}

enum Size {
    Value(f32),
    Auto,
    Full,
}

struct Button {
    width: f32,
    height: f32,
    current_rect: Rect,
    text: Text,
}

impl Button {
    fn new(width: f32, height: f32, text: String, bounds: Rect, rt: &mut dyn Runtime) -> Self {
        Self {
            width,
            height,
            current_rect: Rect {
                height,
                width,
                x: bounds.x,
                y: bounds.y,
            },
            text: Text::new(text, bounds, rt),
        }
    }

    fn draw(&mut self, rt: &mut dyn Runtime) {
        rt.draw_rect(
            self.current_rect.x,
            self.current_rect.y,
            self.current_rect.width,
            self.current_rect.height,
            0.0,
            0.0,
        );

        self.text.draw(rt);
    }

    fn click(&mut self, x: f32, y: f32) -> bool {
        let rect = self.current_rect;
        return x > rect.x && x < rect.x + rect.width && y > rect.y && y < rect.y + rect.height;
    }

    fn set_bounds(&mut self, bounds: Rect, rt: &mut dyn Runtime) {
        self.current_rect = Rect {
            height: self.height,
            width: self.width,
            x: bounds.x,
            y: bounds.y,
        };
        self.text.set_bounds(self.current_rect, rt);
    }
}

pub struct Counter {
    plus: Button,
    minus: Button,
    value: u32,
    value_text: Text,
}

impl Component for Counter {
    fn draw(&mut self, rt: &mut dyn Runtime) {
        // self.plus.draw(rt);
        self.minus.draw(rt);
        self.value_text.draw(rt);
    }

    fn click(&mut self, x: f32, y: f32) {
        if self.plus.click(x, y) {
            self.value += 1;
            self.value_text.value = self.value.to_string();
            return;
        }

        if self.minus.click(x, y) {
            self.value -= 1;
            self.value_text.value = self.value.to_string();
            return;
        }
    }

    fn set_bounds(&mut self, bounds: Rect, rt: &mut dyn Runtime) {
        let (text_bounds, plus_bounds, minus_bounds) = Self::get_bounds(bounds);
        self.plus.set_bounds(plus_bounds, rt);
        self.minus.set_bounds(minus_bounds, rt);
        self.value_text.set_bounds(text_bounds, rt);
    }
}

impl Counter {
    pub fn new(bounds: Rect, rt: &mut dyn Runtime) -> Self {
        let (text_bounds, plus_bounds, minus_bounds) = Self::get_bounds(bounds);

        Self {
            plus: Button::new(100.0, 100.0, "+".to_string(), plus_bounds, rt),
            minus: Button::new(100.0, 100.0, "-".to_string(), minus_bounds, rt),
            value: 0,
            value_text: Text::new(
                "Some, text with maybe extra lines and stuff".to_string(),
                text_bounds,
                rt,
            ),
        }
    }

    fn get_bounds(bounds: Rect) -> (Rect, Rect, Rect) {
        let height = bounds.height / 3.0;
        (
            Rect { height, ..bounds },
            Rect {
                height,
                y: bounds.y + height,
                ..bounds
            },
            Rect {
                height,
                y: bounds.y + height * 2.0,
                ..bounds
            },
        )
    }
}

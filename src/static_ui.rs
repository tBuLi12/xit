use ash::sec;
use cosmic_text::{Attrs, AttrsList, BufferRef, Edit};
use winit::keyboard::SmolStr;

use crate::CachedGlyph;

#[derive(Copy, Clone, Debug)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub fn contains(&self, point: Point) -> bool {
        point.x >= 0.0 && point.y >= 0.0 && point.x < self.width && point.y < self.height
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl std::ops::Add<Point> for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Self::Output {
        Point {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BBox {
    pub dims: Size,
    pub pos: Point,
}

#[derive(Copy, Clone, Debug)]
pub enum AxisAlignment {
    Start,
    Center,
    End,
}

#[derive(Copy, Clone, Debug)]
pub struct Alignment {
    pub horizontal: AxisAlignment,
    pub vertical: AxisAlignment,
}

impl Alignment {
    pub fn center() -> Self {
        Self {
            horizontal: AxisAlignment::Center,
            vertical: AxisAlignment::Center,
        }
    }
}

#[derive(Clone, Debug)]
struct CachedLine {
    line: Vec<(CachedGlyph, (usize, usize))>,
    width: f32,
}

#[derive(Clone, Debug)]
pub struct Text {
    buffer: cosmic_text::Buffer,
    glyphs: Vec<CachedLine>,
    alignment: Alignment,
    size: Size,
    color: Color,
    line_count: usize,
}

impl Text {
    fn new(
        value: String,
        bounds: Size,
        color: Color,
        alignment: Alignment,
        rt: &mut dyn Runtime,
    ) -> Self {
        let mut buffer = cosmic_text::Buffer::new(
            rt.font_system(),
            cosmic_text::Metrics {
                font_size: 40.0,
                line_height: 40.0,
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
            buffer,
            glyphs,
            size: bounds,
            alignment,
            color,
            line_count: 0,
        };

        this.layout_text(rt);
        this
    }

    fn layout_text(&mut self, rt: &mut dyn Runtime) {
        self.glyphs.clear();
        self.buffer.shape_until_scroll(rt.font_system(), false);
        self.line_count = 0;
        for run in self.buffer.layout_runs() {
            self.line_count += 1;
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
    }

    fn set_text(&mut self, text: String, rt: &mut dyn Runtime) {
        self.buffer.set_text(
            rt.font_system(),
            &text,
            cosmic_text::Attrs::new(),
            cosmic_text::Shaping::Advanced,
        );
        self.layout_text(rt);
    }
}

impl Component for Text {
    fn click(&mut self, _: Point, _: &mut dyn Runtime) -> bool {
        return false;
    }

    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        let end_y_offset =
            self.size.height - self.buffer.metrics().line_height * self.line_count as f32;
        let y_offset = match self.alignment.vertical {
            AxisAlignment::Start => 0.0,
            AxisAlignment::Center => end_y_offset / 2.0,
            AxisAlignment::End => end_y_offset,
        };

        for line in &self.glyphs {
            let end_x_offset = self.size.width - line.width;
            let x_offset = match self.alignment.horizontal {
                AxisAlignment::Start => 0.0,
                AxisAlignment::Center => end_x_offset / 2.0,
                AxisAlignment::End => end_x_offset,
            };

            for (glyph, _) in &line.line {
                if let Some(tex_position) = glyph.tex_position {
                    rt.draw_glyph(
                        point.x + glyph.left + x_offset,
                        point.y - glyph.top + y_offset,
                        glyph.size,
                        tex_position,
                        self.color,
                    );
                }
            }
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.buffer
            .set_size(rt.font_system(), Some(bounds.width), Some(bounds.height));
        self.size = bounds;
        self.layout_text(rt);
    }

    fn size(&self) -> Size {
        self.size
    }
}

#[derive(Debug)]
pub struct Input {
    line: cosmic_text::BufferLine,
    glyphs: CachedLine,
    size: Size,
    color: Color,
    cursor: Option<usize>,
}

impl Input {
    fn new(value: String, bounds: Size, color: Color, rt: &mut dyn Runtime) -> Self {
        let line = cosmic_text::BufferLine::new(
            value,
            cosmic_text::LineEnding::None,
            AttrsList::new(Attrs::new()),
            cosmic_text::Shaping::Advanced,
        );

        let mut this = Self {
            line,
            glyphs: CachedLine {
                line: vec![],
                width: 0.0,
            },
            size: bounds,
            color,
            cursor: None,
        };

        this.layout_text(rt);
        this
    }

    fn layout_text(&mut self, rt: &mut dyn Runtime) {
        self.glyphs.line.clear();

        let shape = self.line.shape(rt.font_system(), 4);
        let laid_out_line = shape
            .layout(
                40.0,
                None,
                cosmic_text::Wrap::None,
                Some(cosmic_text::Align::Left),
                None,
            )
            .pop()
            .unwrap();

        for glyph in laid_out_line.glyphs {
            let physical_glyph = glyph.physical((0.0, 0.0), 1.0);
            let Some(mut cached_glyph) = rt.get_glyph(physical_glyph.cache_key) else {
                continue;
            };

            cached_glyph.top += physical_glyph.y as f32 - laid_out_line.max_ascent;
            cached_glyph.left += physical_glyph.x as f32;

            self.glyphs
                .line
                .push((cached_glyph, (glyph.start, glyph.end)));
        }

        self.glyphs.width = laid_out_line.w;
    }

    fn set_text(&mut self, text: String, rt: &mut dyn Runtime) {
        if self.line.set_text(
            text,
            cosmic_text::LineEnding::None,
            AttrsList::new(Attrs::new()),
        ) {
            self.layout_text(rt);
        }
    }
}

impl Component for Input {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        rt.draw_rect(
            point.x,
            point.y,
            self.size.width,
            self.size.height,
            0.0,
            0.0,
            Color::black().blue(1.0),
            Color::clear(),
        );

        for (glyph, _) in &self.glyphs.line {
            if let Some(tex_position) = glyph.tex_position {
                rt.draw_glyph(
                    point.x + glyph.left,
                    point.y - glyph.top,
                    glyph.size,
                    tex_position,
                    self.color,
                );
            }
        }

        if let Some(cursor) = self.cursor {
            let offset = if cursor == 0 {
                0.0
            } else {
                let glyph = self.glyphs.line[cursor - 1].0;
                glyph.left + glyph.size[0]
            };

            rt.draw_rect(
                point.x + offset,
                point.y,
                2.0,
                self.size.height,
                0.0,
                0.0,
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.5,
                },
                Color::clear(),
            );
        }
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        if !self.size.contains(point) {
            return false;
        }

        self.cursor = Some(
            self.glyphs
                .line
                .iter()
                .find_map(|(glyph, (start, end))| {
                    if glyph.left <= point.x && point.x <= glyph.left + glyph.size[0] {
                        Some(*start)
                    } else {
                        None
                    }
                })
                .unwrap_or(self.line.text().len()),
        );

        true
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = bounds;
    }

    fn key_pressed(&mut self, key: SmolStr, rt: &mut dyn Runtime) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let mut text = self.line.text().to_string();
        text.insert_str(cursor, &key);
        self.cursor = Some(cursor + key.len());
        self.set_text(text, rt);
        self.layout_text(rt);
    }

    fn size(&self) -> Size {
        self.size
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub fn clear() -> Self {
        Self {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }
    }

    pub fn black() -> Self {
        Self::clear().alpha(1.0)
    }

    pub fn alpha(self, value: f32) -> Self {
        Self { a: value, ..self }
    }

    pub fn red(self, value: f32) -> Self {
        Self { r: value, ..self }
    }

    pub fn blue(self, value: f32) -> Self {
        Self { b: value, ..self }
    }

    pub fn green(self, value: f32) -> Self {
        Self { g: value, ..self }
    }
}

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
        bg_color: Color,
        border_color: Color,
    );
    fn draw_glyph(&mut self, x: f32, y: f32, size: [f32; 2], tex_coords: [f32; 2], color: Color);
    fn font_system(&mut self) -> &mut cosmic_text::FontSystem;
    fn get_glyph(&mut self, key: cosmic_text::CacheKey) -> Option<CachedGlyph>;
}

pub trait Component {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime);
    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime);
    fn size(&self) -> Size;
    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool;
    fn key_pressed(&mut self, key: SmolStr, rt: &mut dyn Runtime) {}
}

enum Sizing {
    Value(f32),
    Auto,
    Full,
}

struct Button {
    width: f32,
    height: f32,
    size: Size,
    text: Text,
}

impl Button {
    fn new(width: f32, height: f32, text: String, bounds: Size, rt: &mut dyn Runtime) -> Self {
        let size = Size {
            width: width.min(bounds.width),
            height: height.min(bounds.height),
        };

        Self {
            width,
            height,
            size,
            text: Text::new(
                text,
                size,
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
                Alignment::center(),
                rt,
            ),
        }
    }
}

impl Component for Button {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        rt.draw_rect(
            point.x,
            point.y,
            self.size.width,
            self.size.height,
            10.0,
            2.0,
            Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
        );

        self.text.draw(point, rt);
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        self.size.contains(point)
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = Size {
            width: self.width.min(bounds.width),
            height: self.height.min(bounds.height),
        };
        self.text.set_bounds(self.size, rt);
    }

    fn size(&self) -> Size {
        self.size
    }
}

pub struct Align<C> {
    alignment: Alignment,
    size: Size,
    inner: C,
}

impl<C: Component> Align<C> {
    pub fn new(inner: C, size: Size) -> Self {
        Self {
            alignment: Alignment {
                horizontal: AxisAlignment::Start,
                vertical: AxisAlignment::Start,
            },
            size,
            inner,
        }
    }

    pub fn center(mut self) -> Self {
        self.alignment = Alignment::center();
        self
    }

    fn inner_offset(&self) -> Point {
        let x = match self.alignment.horizontal {
            AxisAlignment::Start => 0.0,
            AxisAlignment::Center => (self.size.width - self.inner.size().width) / 2.0,
            AxisAlignment::End => self.size.width - self.inner.size().width,
        };
        let y = match self.alignment.vertical {
            AxisAlignment::Start => 0.0,
            AxisAlignment::Center => (self.size.height - self.inner.size().height) / 2.0,
            AxisAlignment::End => self.size.height - self.inner.size().height,
        };

        Point { x, y }
    }
}

struct Rect<C> {
    width: Sizing,
    height: Sizing,
    size: Size,
    inner: C,
    border_width: f32,
    corner_radius: f32,
    bg_color: Color,
    border_color: Color,
}

impl<C: Component> Component for Rect<C> {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        rt.draw_rect(
            point.x,
            point.y,
            self.size.width,
            self.size.height,
            self.corner_radius,
            self.border_width,
            self.bg_color,
            self.border_color,
        );
        self.inner.draw(point, rt)
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = Size {
            width: bounds.width.min(match self.width {
                Sizing::Auto => self.inner.size().width,
                Sizing::Full => bounds.width,
                Sizing::Value(width) => width,
            }),
            height: bounds.height.min(match self.height {
                Sizing::Auto => self.inner.size().height,
                Sizing::Full => bounds.height,
                Sizing::Value(height) => height,
            }),
        }
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        self.inner.click(point, rt)
    }

    fn key_pressed(&mut self, key: SmolStr, rt: &mut dyn Runtime) {
        self.inner.key_pressed(key, rt)
    }

    fn size(&self) -> Size {
        self.size
    }
}

impl<C: Component> Component for Align<C> {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        let inner_offset = self.inner_offset();
        self.inner.draw(point + inner_offset, rt)
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.inner.set_bounds(bounds, rt);
        self.size = bounds;
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        self.size.contains(point) && {
            let inner_offset = self.inner_offset();
            self.inner.click(point + inner_offset, rt)
        }
    }

    fn key_pressed(&mut self, key: SmolStr, rt: &mut dyn Runtime) {
        self.inner.key_pressed(key, rt)
    }

    fn size(&self) -> Size {
        self.size
    }
}

pub struct App {
    plus: Align<Button>,
    minus: Align<Button>,
    value: u32,
    value_text: Text,
    input: Input,
    height: f32,
}

impl Component for App {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        self.plus.draw(point, rt);
        self.minus.draw(
            Point {
                x: point.x,
                y: point.y + self.height / 3.0,
            },
            rt,
        );
        self.input.draw(
            Point {
                x: point.x,
                y: point.y + 2.0 * self.height / 3.0,
            },
            rt,
        );
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        if self.plus.click(point, rt) {
            self.value += 1;
            self.value_text.set_text(self.value.to_string(), rt);
            return true;
        }

        if self.minus.click(
            Point {
                x: point.x,
                y: point.y - self.height / 3.0,
            },
            rt,
        ) {
            self.value -= 1;
            self.value_text.set_text(self.value.to_string(), rt);
            return true;
        }

        if self.input.click(
            Point {
                x: point.x,
                y: point.y - 2.0 * self.height / 3.0,
            },
            rt,
        ) {
            return true;
        }

        false
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        let child_bounds = Size {
            width: bounds.width,
            height: bounds.height / 3.0,
        };

        self.height = bounds.height;

        self.plus.set_bounds(child_bounds, rt);
        self.minus.set_bounds(child_bounds, rt);
        self.value_text.set_bounds(child_bounds, rt);
        self.input.set_bounds(child_bounds, rt);
    }

    fn key_pressed(&mut self, key: SmolStr, rt: &mut dyn Runtime) {
        self.input.key_pressed(key, rt);
    }

    fn size(&self) -> Size {
        Size {
            width: 0.0,
            height: 0.0,
        }
    }
}

impl App {
    pub fn new(bounds: Size, rt: &mut dyn Runtime) -> Self {
        let child_bounds = Size {
            width: bounds.width,
            height: bounds.height / 3.0,
        };

        Self {
            plus: Align::new(
                Button::new(100.0, 100.0, "+1".to_string(), child_bounds, rt),
                child_bounds,
            )
            .center(),
            minus: Align::new(
                Button::new(100.0, 100.0, "-1".to_string(), child_bounds, rt),
                child_bounds,
            )
            .center(),
            input: Input::new(
                "Input Two".to_string(),
                child_bounds,
                Color::black().red(1.0),
                rt,
            ),
            value: 0,
            value_text: Text::new(
                // "Some, text with maybe extra lines and stuff".to_string(),
                // "Some, text with maybe extra lines and stuffSome, text with maybe extra lines and stuff".to_string(),
                "0".to_string(),
                child_bounds,
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
                Alignment::center(),
                rt,
            ),
            height: bounds.height,
        }
    }
}

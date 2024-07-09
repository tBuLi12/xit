use cosmic_text::{Attrs, AttrsList};
use rows::Rows;

mod rows;

use crate::{
    signal::{OwnedSignal, Signal},
    CachedGlyph,
};

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
    h_alignment: AxisAlignment,
    size: Size,
    color: Color,
    line_count: usize,
    text: Signal<String>,
}

impl Text {
    fn new(
        value: Signal<String>,
        bounds: Size,
        color: Color,
        h_alignment: AxisAlignment,
        rt: &mut dyn Runtime,
    ) -> Self {
        let mut buffer = cosmic_text::Buffer::new(
            rt.font_system(),
            cosmic_text::Metrics {
                font_size: 40.0,
                line_height: 40.0,
            },
        );

        buffer.set_size(rt.font_system(), Some(bounds.width), Some(bounds.height));

        let mut this = Self {
            buffer,
            glyphs: vec![],
            size: bounds,
            h_alignment,
            color,
            line_count: 0,
            text: value,
        };

        this.set_text(&*this.text.borrow(), rt);
        this
    }

    fn layout_text(&mut self, rt: &mut dyn Runtime) {
        self.glyphs.clear();
        self.buffer.shape_until_scroll(rt.font_system(), false);
        self.line_count = 0;
        self.size.width = 0.0;
        for run in self.buffer.layout_runs() {
            self.line_count += 1;
            self.size.width = self.size.width.max(run.line_w);
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
        self.size.height = 40.0 * self.line_count as f32;
    }

    fn set_text(&mut self, text: &str, rt: &mut dyn Runtime) {
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
    // fn mouse_up(&mut self, _: Point, _: &mut dyn Runtime) {}
    // fn mouse_move(&mut self, _: f32, _: f32, _: &mut dyn Runtime) {}

    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        if self.text.is_dirty() {
            self.set_text(&*self.text.borrow(), rt);
        }

        for line in &self.glyphs {
            let end_x_offset = self.size.width - line.width;
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
        self.buffer
            .set_size(rt.font_system(), Some(bounds.width), Some(bounds.height));

        self.layout_text(rt);
    }

    fn size(&mut self, rt: &mut dyn Runtime) -> Size {
        if self.text.is_dirty() {
            self.set_text(&*self.text.borrow(), rt);
        }

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

    // fn mouse_up(&mut self, _: Point, _: &mut dyn Runtime) {}
    // fn mouse_move(&mut self, _: f32, _: f32, _: &mut dyn Runtime) {}

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

    fn size(&mut self, rt: &mut dyn Runtime) -> Size {
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
    fn mouse_position(&mut self) -> Signal<Point>;
}

pub trait Component {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime);
    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime);
    fn size(&mut self, rt: &mut dyn Runtime) -> Size;

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        self.size(rt).contains(point)
    }
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
    text_signal: OwnedSignal<String>,
    _on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    fn new(width: f32, height: f32, text: String, bounds: Size, rt: &mut dyn Runtime) -> Self {
        let size = Size {
            width: width.min(bounds.width),
            height: height.min(bounds.height),
        };

        let text_signal = OwnedSignal::new(text);
        let text = text_signal.get_signal();

        Self {
            width,
            height,
            size,
            text_signal,
            text: Text::new(
                text,
                size,
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
                AxisAlignment::Center,
                rt,
            ),
            _on_click: None,
        }
    }

    pub fn on_click(mut self, on_click: impl FnMut() + 'static) -> Self {
        self._on_click = Some(Box::new(on_click));
        self
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
            Color::black().red(1.0),
            Color::black().green(1.0),
        );

        self.text.draw(point, rt);
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        if self.size.contains(point) {
            if let Some(on_click) = &mut self._on_click {
                (on_click)();
            }

            true
        } else {
            false
        }
    }

    // fn mouse_up(&mut self, _: Point, _: &mut dyn Runtime) {}
    // fn mouse_move(&mut self, _: f32, _: f32, _: &mut dyn Runtime) {}

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = Size {
            width: self.width.min(bounds.width),
            height: self.height.min(bounds.height),
        };
        self.text.set_bounds(self.size, rt);
    }

    fn size(&mut self, rt: &mut dyn Runtime) -> Size {
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

    fn inner_offset(&mut self, rt: &mut dyn Runtime) -> Point {
        let x = match self.alignment.horizontal {
            AxisAlignment::Start => 0.0,
            AxisAlignment::Center => (self.size.width - self.inner.size(rt).width) / 2.0,
            AxisAlignment::End => self.size.width - self.inner.size(rt).width,
        };
        let y = match self.alignment.vertical {
            AxisAlignment::Start => 0.0,
            AxisAlignment::Center => (self.size.height - self.inner.size(rt).height) / 2.0,
            AxisAlignment::End => self.size.height - self.inner.size(rt).height,
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

impl<C: Component> Rect<C> {
    pub fn new(inner: C, bounds: Size) -> Self {
        Self {
            width: Sizing::Auto,
            height: Sizing::Auto,
            size: bounds,
            inner,
            border_width: 0.0,
            corner_radius: 0.0,
            bg_color: Color::clear(),
            border_color: Color::clear(),
        }
    }

    pub fn full_width(mut self) -> Self {
        self.width = Sizing::Full;
        self
    }

    pub fn full_height(mut self) -> Self {
        self.height = Sizing::Full;
        self
    }

    pub fn px_hight(mut self, height: f32) -> Self {
        self.height = Sizing::Value(height);
        self
    }

    pub fn px_width(mut self, width: f32) -> Self {
        self.width = Sizing::Value(width);
        self
    }
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
                Sizing::Auto => self.inner.size(rt).width,
                Sizing::Full => bounds.width,
                Sizing::Value(width) => width,
            }),
            height: bounds.height.min(match self.height {
                Sizing::Auto => self.inner.size(rt).height,
                Sizing::Full => bounds.height,
                Sizing::Value(height) => height,
            }),
        };
        self.inner.set_bounds(self.size, rt);
    }

    // fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
    //     self.inner.click(point, rt)
    // }

    // fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
    //     self.inner.mouse_up(point, rt)
    // }

    // fn mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
    //     self.inner.mouse_move(dx, dy, rt)
    // }

    // fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
    //     self.inner.key_pressed(key, rt)
    // }

    fn size(&mut self, rt: &mut dyn Runtime) -> Size {
        self.size
    }
}

impl<C: Component> Component for Align<C> {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        let inner_offset = self.inner_offset(rt);
        self.inner.draw(point + inner_offset, rt)
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.inner.set_bounds(bounds, rt);
        self.size = bounds;
    }

    // fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
    //     self.size.contains(point) && {
    //         let inner_offset = self.inner_offset();
    //         self.inner.click(point + inner_offset, rt)
    //     }
    // }

    // fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
    //     self.inner.mouse_up(point, rt)
    // }

    // fn mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
    //     self.inner.mouse_move(dx, dy, rt)
    // }

    // fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
    //     self.inner.key_pressed(key, rt)
    // }

    fn size(&mut self, rt: &mut dyn Runtime) -> Size {
        self.size
    }
}

struct ResizableCols<C1, C2> {
    col1: C1,
    col2: C2,
    spacer_width: f32,
    col1_width: f32,
    col2_width: f32,
    height: f32,
    dragging: Option<Signal<Point>>,
}

impl<C1: Component, C2: Component> ResizableCols<C1, C2> {
    fn width(&self) -> f32 {
        self.col1_width + self.col2_width + self.spacer_width
    }

    pub fn new(col1: C1, col2: C2, spacer_width: f32, bounds: Size, rt: &mut dyn Runtime) -> Self {
        let total_width = bounds.width - spacer_width;

        Self {
            col1,
            col2,
            spacer_width,
            col1_width: total_width / 2.0,
            col2_width: total_width / 2.0,
            height: bounds.height,
            dragging: None,
        }
    }
}

impl<C1: Component, C2: Component> Component for ResizableCols<C1, C2> {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        self.col1.draw(point, rt);
        rt.draw_rect(
            point.x + self.col1_width,
            point.y,
            self.spacer_width,
            self.height,
            0.0,
            0.0,
            if self.dragging.is_some() {
                Color::black().red(1.0)
            } else {
                Color::black().blue(1.0)
            },
            Color::clear(),
        );
        self.col2.draw(
            Point {
                x: point.x + self.col1_width + self.spacer_width,
                y: point.y,
            },
            rt,
        );
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        let old_total_width = self.width() - self.spacer_width;
        self.height = bounds.height;

        let new_total_width = bounds.width - self.spacer_width;

        let col1_old_fraction = self.col1_width / old_total_width;
        let col2_old_fraction = self.col2_width / old_total_width;
        self.col1_width = new_total_width * col1_old_fraction;
        self.col2_width = new_total_width * col2_old_fraction;

        self.col1.set_bounds(
            Size {
                width: self.col1_width,
                height: self.height,
            },
            rt,
        );
        self.col2.set_bounds(
            Size {
                width: self.col2_width,
                height: self.height,
            },
            rt,
        );
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        if point.x >= 0.0 && point.x <= self.col1_width {
            return self.col1.click(point, rt);
        }

        if point.x >= self.col1_width && point.x <= self.col1_width + self.spacer_width {
            self.dragging = Some(rt.mouse_position());
            return true;
        }

        if point.x >= self.col1_width + self.spacer_width
            && point.x <= self.col1_width + self.spacer_width + self.col2_width
        {
            return self.col2.click(point, rt);
        }

        false
    }

    // fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
    //     self.dragging = false;
    //     self.col1.mouse_up(point, rt);
    //     self.col2.mouse_up(point, rt);
    // }

    // fn mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
    //     let mut diff = dx;

    //     if dx < 0.0 {
    //         diff = dx.max(-self.col1_width);
    //     }

    //     if dx > 0.0 {
    //         diff = dx.min(self.col2_width);
    //     }

    //     if self.dragging {
    //         self.col1_width += diff;
    //         self.col2_width -= diff;
    //         self.col1.set_bounds(
    //             Size {
    //                 width: self.col1_width,
    //                 height: self.height,
    //             },
    //             rt,
    //         );
    //         self.col2.set_bounds(
    //             Size {
    //                 width: self.col2_width,
    //                 height: self.height,
    //             },
    //             rt,
    //         );
    //     }

    //     self.col1.mouse_move(dx, dy, rt);
    //     self.col2.mouse_move(dx, dy, rt);
    // }

    // fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
    //     self.col1.key_pressed(key, rt);
    //     self.col2.key_pressed(key, rt);
    // }

    fn size(&mut self, rt: &mut dyn Runtime) -> Size {
        Size {
            width: self.width(),
            height: self.height,
        }
    }
}

pub struct App {
    columns: ResizableCols<Button, Text>,
    count: OwnedSignal<u32>,
    text: OwnedSignal<String>,
}

impl Component for App {
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        self.columns.draw(point, rt);
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        self.columns.click(point, rt)
    }

    // fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
    //     self.columns.mouse_up(point, rt);
    // }

    // fn mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
    //     self.columns.mouse_move(dx, dy, rt);
    // }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.columns.set_bounds(bounds, rt);
    }

    // fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
    //     self.columns.key_pressed(key, rt);
    // }

    fn size(&mut self, rt: &mut dyn Runtime) -> Size {
        Size {
            width: 0.0,
            height: 0.0,
        }
    }
}

impl App {
    pub fn new(bounds: Size, rt: &mut dyn Runtime) -> Self {
        let count = OwnedSignal::new(0);
        let count_signal = count.get_signal();

        let text = count_signal.derived(|count| format!("Count: {}", count));
        let text_signal = text.get_signal();

        Self {
            columns: ResizableCols::new(
                Button::new(100.0, 50.0, "Text 1".to_string(), bounds, rt).on_click(move || {
                    println!("Text 1 clicked");
                    count_signal.update(|value| *value += 1);
                }),
                Text::new(
                    text_signal,
                    bounds,
                    Color::black().red(1.0),
                    AxisAlignment::Center,
                    rt,
                ),
                10.0,
                bounds,
                rt,
            ),
            count,
            text,
        }
    }

    pub fn set_text_list(&mut self, text_list: Vec<String>, rt: &mut dyn Runtime) {
        // self.columns.col1.inner.
    }
}

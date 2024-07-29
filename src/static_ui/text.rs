use crate::SubpixelPosition;

use super::{CachedLine, Color, Component, Point, Runtime, Size, TextProps, Visitor};

#[derive(Clone, Debug)]
pub struct TextLine {
    text: String,
    line: CachedLine,
    last_subpixel_position: Option<SubpixelPosition>,
    bounds: Size,
    color: Color,
    props: TextProps,
    height: f32,
    x_height: f32,
}

impl TextLine {
    pub fn new(value: String, color: Color) -> Self {
        Self {
            text: value,
            line: CachedLine::new(),
            last_subpixel_position: None,
            bounds: Size {
                width: 0.0,
                height: 0.0,
            },
            color,
            props: TextProps {
                font_size: 40.0,
                font_id: 1,
            },
            height: 40.0,
            x_height: 0.0,
        }
    }

    pub fn set_text(&mut self, new_text: &str, rt: &mut dyn Runtime) {
        self.text.clear();
        self.text.push_str(new_text);
        self.last_subpixel_position = None;
    }

    pub fn layout(&mut self, rt: &mut dyn Runtime, subpixel_position: SubpixelPosition) {
        self.line.glyphs.clear();

        let mut text_renderer = rt.get_text(self.props);

        text_renderer.render_line(&self.text, subpixel_position, &mut self.line);
        self.x_height = text_renderer.x_height();

        self.last_subpixel_position = Some(subpixel_position);
    }
}

impl Component for TextLine {
    type Event = ();

    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        let (x, y, subpixel_position) = SubpixelPosition::from_f32(point.x, point.y);

        if self.last_subpixel_position != Some(subpixel_position) {
            self.layout(rt, subpixel_position);
        }

        for glyph in &self.line.glyphs {
            if let Some(tex_position) = glyph.tex_position {
                rt.draw_glyph(
                    x + glyph.left,
                    y - glyph.top + (self.height + self.x_height) / 2.0,
                    glyph.size,
                    tex_position,
                    self.color,
                    Color::black(),
                );
            }
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.bounds = bounds;
        self.layout(rt, SubpixelPosition::from_f32(0.0, 0.0).2);
    }

    fn size(&self) -> Size {
        Size {
            width: self.line.width,
            height: self.height,
        }
    }

    fn visit_children(&mut self, visitor: &mut impl Visitor) {}
    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}
}

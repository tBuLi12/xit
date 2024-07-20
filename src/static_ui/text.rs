use crate::{CachedGlyph, SubpixelPosition};

use super::{
    AxisAlignment, CachedLine, Color, Component, Point, Runtime, Size, TextProps, Visitor,
};

#[derive(Clone, Debug)]
struct CachedTextState {
    glyphs: Vec<CachedLine>,
    size: Size,
    last_subpixel_position: Option<SubpixelPosition>,
}

impl CachedTextState {
    fn new() -> Self {
        Self {
            glyphs: vec![],
            size: Size {
                width: 0.0,
                height: 0.0,
            },
            last_subpixel_position: None,
        }
    }

    fn layout_text(
        &mut self,
        text: &str,
        props: TextProps,
        subpixel_position: SubpixelPosition,
        rt: &mut dyn Runtime,
    ) {
        self.glyphs.clear();

        let mut line = CachedLine {
            glyphs: vec![],
            units: vec![],
            width: 0.0,
        };

        rt.get_text(props)
            .render_line(text, subpixel_position, &mut line);

        let width = line.width;

        self.glyphs.push(line);

        self.size = Size {
            width,
            height: 40.0,
        };
        self.last_subpixel_position = Some(subpixel_position);
    }
}

#[derive(Clone, Debug)]
pub struct Text {
    text: String,
    cached: Option<CachedTextState>,
    h_alignment: AxisAlignment,
    bounds: Size,
    color: Color,
    props: TextProps,
}

impl Text {
    pub fn new(value: String, color: Color, h_alignment: AxisAlignment) -> Self {
        Self {
            text: value,
            cached: None,
            bounds: Size {
                width: 0.0,
                height: 0.0,
            },
            h_alignment,
            color,
            props: TextProps {
                font_size: 40.0,
                font_id: 1,
            },
        }
    }

    pub fn set_text(&mut self, new_text: &str, rt: &mut dyn Runtime) {
        self.text.clear();
        self.text.push_str(new_text);
        if let Some(cached) = &mut self.cached {
            cached.last_subpixel_position = None;
        }
    }

    // fn get_text_state(&mut self, rt: &mut dyn Runtime) -> &mut CachedTextState {
    //     let cached = match &mut self.cached {
    //         Some(_) => None,
    //         None => {
    //             let mut cached = CachedTextState::new();
    //             cached.layout_text(&self.text, self.props, rt);
    //             Some(cached)
    //         }
    //     };

    //     if let Some(cached) = cached {
    //         self.cached = Some(cached);
    //     }

    //     let Some(cached) = &mut self.cached else {
    //         unreachable!()
    //     };
    //     cached
    // }
}

impl Component for Text {
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        let (x, y, subpixel_position) = SubpixelPosition::from_f32(point.x, point.y);

        if self.cached.is_none() {
            self.cached = Some(CachedTextState::new());
        }

        let Some(cached) = &mut self.cached else {
            unreachable!()
        };

        if cached.last_subpixel_position != Some(subpixel_position) {
            cached.layout_text(&self.text, self.props, subpixel_position, rt);
        }

        for line in &cached.glyphs {
            let end_x_offset = cached.size.width - line.width;
            let x_offset = match self.h_alignment {
                AxisAlignment::Start => 0.0,
                AxisAlignment::Center => end_x_offset / 2.0,
                AxisAlignment::End => end_x_offset,
            };

            for glyph in &line.glyphs {
                if let Some(tex_position) = glyph.tex_position {
                    rt.draw_glyph(
                        x + glyph.left + x_offset,
                        y - glyph.top + self.bounds.height,
                        glyph.size,
                        tex_position,
                        self.color,
                        Color::black(),
                    );
                }
            }
        }
    }

    fn set_bounds(&mut self, bounds: Size) {
        if let Some(cached) = &mut self.cached {
            cached.last_subpixel_position = None;
        }
        self.bounds = bounds;
    }

    fn size(&self) -> Size {
        let Some(cached) = &self.cached else {
            return Size::ZERO;
        };

        cached.size
    }

    fn visit_children(&mut self, visitor: &mut impl Visitor) {}
    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}
}

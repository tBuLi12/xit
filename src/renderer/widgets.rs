use crate::renderer::Offset;

use super::{Child, Color, Fill, Rect, Size};

pub trait Widget {
    fn render(&self, bounds: Size) -> Rect;
}

pub trait Children {
    fn render(&self, bounds: Size) -> Vec<Child>;
}

impl Children for () {
    fn render(&self, _: Size) -> Vec<Child> {
        vec![]
    }
}

pub struct FilledRect<C> {
    color: Color,
    children: C,
}

impl FilledRect<()> {
    pub fn new(color: Color) -> Self {
        Self {
            color,
            children: (),
        }
    }
}

impl<T> FilledRect<T> {
    pub fn children<C>(self, children: C) -> FilledRect<C> {
        FilledRect {
            color: self.color,
            children,
        }
    }
}

impl<C: Children> Widget for FilledRect<C> {
    fn render(&self, bounds: Size) -> Rect {
        Rect::new()
            .size(bounds)
            .fill_color(self.color)
            .children(self.children.render(bounds))
    }
}

pub struct SizedRect<C> {
    fill: Fill,
    size: Size,
    children: C,
}

impl SizedRect<()> {
    pub fn new(size: Size) -> Self {
        Self {
            fill: Fill::None,
            size,
            children: (),
        }
    }
}

impl<T> SizedRect<T> {
    pub fn children<C>(self, children: C) -> SizedRect<C> {
        SizedRect {
            fill: self.fill,
            size: self.size,
            children,
        }
    }

    pub fn color(self, color: Color) -> Self {
        SizedRect {
            fill: Fill::Color(color),
            size: self.size,
            children: self.children,
        }
    }
}

impl<C: Children> Widget for SizedRect<C> {
    fn render(&self, bounds: Size) -> Rect {
        let size = Size {
            width: self.size.width.min(bounds.width),
            height: self.size.height.min(bounds.height),
        };

        Rect::new()
            .size(size)
            .fill(self.fill)
            .children(self.children.render(size))
    }
}

pub struct Center<W> {
    child: W,
}

impl<W: Widget> Center<W> {
    pub fn new(child: W) -> Self {
        Self { child }
    }
}

impl<W: Widget> Children for Center<W> {
    fn render(&self, bounds: Size) -> Vec<Child> {
        let rect = self.child.render(bounds);

        let child = Child {
            position: Offset {
                x: (bounds.width - rect.size.width) / 2,
                y: (bounds.height - rect.size.height) / 2,
            },
            rect,
        };

        vec![child]
    }
}

pub struct TextLine<'s> {
    text: &'s str,
}

impl<'s> TextLine<'s> {
    pub fn new(text: &'s str) -> Self {
        Self { text }
    }
}

impl<'s> Widget for TextLine<'s> {
    fn render(&self, bounds: Size) -> Rect {
        super::text(self.text)
    }
}

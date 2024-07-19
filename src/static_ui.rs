use std::{
    ffi::OsString,
    fs, iter, mem,
    path::{self, PathBuf},
};

use editor::Editor;
use rows::Rows;
use swash::shape::cluster::GlyphCluster;
use text::Text;
use winit::keyboard;

mod editor;
mod file_forest;
mod rows;
mod text;

use crate::{CachedGlyph, SubpixelPosition};

pub use file_forest::FileForest;

#[derive(Copy, Clone, Debug)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

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

// #[derive(Debug)]
// pub struct Input {
//     line: cosmic_text::BufferLine,
//     glyphs: CachedLine,
//     size: Size,
//     color: Color,
//     cursor: Option<usize>,
// }

// impl Input {
//     fn new(value: String, bounds: Size, color: Color, rt: &mut dyn Runtime) -> Self {
//         let line = cosmic_text::BufferLine::new(
//             value,
//             cosmic_text::LineEnding::None,
//             AttrsList::new(Attrs::new()),
//             cosmic_text::Shaping::Advanced,
//         );

//         let mut this = Self {
//             line,
//             glyphs: CachedLine {
//                 line: vec![],
//                 width: 0.0,
//             },
//             size: bounds,
//             color,
//             cursor: None,
//         };

//         this.layout_text(rt);
//         this
//     }

//     fn layout_text(&mut self, rt: &mut dyn Runtime) {
//         self.glyphs.line.clear();

//         let shape = self.line.shape(rt.font_system(), 4);
//         let laid_out_line = shape
//             .layout(
//                 40.0,
//                 None,
//                 cosmic_text::Wrap::None,
//                 Some(cosmic_text::Align::Left),
//                 None,
//             )
//             .pop()
//             .unwrap();

//         for glyph in laid_out_line.glyphs {
//             let physical_glyph = glyph.physical((0.0, 0.0), 1.0);
//             let Some(mut cached_glyph) = rt.get_glyph(physical_glyph.cache_key) else {
//                 continue;
//             };

//             cached_glyph.top += physical_glyph.y as f32 - laid_out_line.max_ascent;
//             cached_glyph.left += physical_glyph.x as f32;

//             self.glyphs
//                 .line
//                 .push((cached_glyph, (glyph.start, glyph.end)));
//         }

//         self.glyphs.width = laid_out_line.w;
//     }

//     fn set_text(&mut self, text: String, rt: &mut dyn Runtime) {
//         if self.line.set_text(
//             text,
//             cosmic_text::LineEnding::None,
//             AttrsList::new(Attrs::new()),
//         ) {
//             self.layout_text(rt);
//         }
//     }
// }

// impl Component for Input {
//     fn draw(&mut self, point: Point,cursor: Option<Point>, rt: &mut dyn Runtime) {
//         rt.draw_rect(
//             point.x,
//             point.y,
//             self.size.width,
//             self.size.height,
//             0.0,
//             0.0,
//             Color::black().blue(1.0),
//             Color::clear(),
//         );

//         for (glyph, _) in &self.glyphs.line {
//             if let Some(tex_position) = glyph.tex_position {
//                 rt.draw_glyph(
//                     point.x + glyph.left,
//                     point.y - glyph.top,
//                     glyph.size,
//                     tex_position,
//                     self.color,
//                 );
//             }
//         }

//         if let Some(cursor) = self.cursor {
//             let offset = if cursor == 0 {
//                 0.0
//             } else {
//                 let glyph = self.glyphs.line[cursor - 1].0;
//                 glyph.left + glyph.size[0]
//             };

//             rt.draw_rect(
//                 point.x + offset,
//                 point.y,
//                 2.0,
//                 self.size.height,
//                 0.0,
//                 0.0,
//                 Color {
//                     r: 0.0,
//                     g: 0.0,
//                     b: 0.0,
//                     a: 0.5,
//                 },
//                 Color::clear(),
//             );
//         }
//     }

//     fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
//         if !self.size.contains(point) {
//             return false;
//         }

//         self.cursor = Some(
//             self.glyphs
//                 .line
//                 .iter()
//                 .find_map(|(glyph, (start, end))| {
//                     if glyph.left <= point.x && point.x <= glyph.left + glyph.size[0] {
//                         Some(*start)
//                     } else {
//                         None
//                     }
//                 })
//                 .unwrap_or(self.line.text().len()),
//         );

//         true
//     }

//     fn mouse_up(&mut self, _: Point, _: &mut dyn Runtime) {}
//     fn mouse_move(&mut self, _: f32, _: f32, _: &mut dyn Runtime) {}

//     fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
//         self.size = bounds;
//     }

//     fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
//         let Some(cursor) = self.cursor else {
//             return;
//         };
//         let mut text = self.line.text().to_string();
//         text.insert_str(cursor, &key);
//         self.cursor = Some(cursor + key.len());
//         self.set_text(text, rt);
//         self.layout_text(rt);
//     }

//     fn size(&mut self, rt: &mut dyn Runtime) -> Size {
//         self.size
//     }
// }

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

#[derive(Clone, Copy, Debug)]
pub struct TextProps {
    pub font_size: f32,
    pub font_id: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct TextUnit {
    pub glyph_start: usize,
    pub glyph_end: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub advance: f32,
}

#[derive(Clone, Debug)]
pub struct CachedLine {
    pub glyphs: Vec<CachedGlyph>,
    pub units: Vec<TextUnit>,
    pub width: f32,
}

pub trait TextRenderer {
    fn render_line(
        &mut self,
        text: &str,
        subpixel_position: SubpixelPosition,
        cached: &mut CachedLine,
    );
    fn x_height(&self) -> f32;
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
    fn draw_glyph(
        &mut self,
        x: f32,
        y: f32,
        size: [f32; 2],
        tex_coords: [f32; 2],
        color: Color,
        bg_color: Color,
    );
    fn get_text(&mut self, props: TextProps) -> Box<dyn TextRenderer + '_>;
}

pub trait Component {
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime);
    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime);
    fn child_size_changed(&mut self, rt: &mut dyn Runtime);
    fn size(&self) -> Size;
    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool);

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        if self.size().contains(point) {
            self.handle_click(point, rt);
            self.visit_children(&mut |offset, child| {
                if child.click(point + offset, rt) {
                    return true;
                }
                false
            });
            true
        } else {
            false
        }
    }

    fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
        self.handle_mouse_up(point, rt);
        self.visit_children(&mut |offset, child| {
            child.mouse_up(point + offset, rt);
            false
        });
    }

    fn mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
        self.handle_mouse_move(dx, dy, rt);
        self.visit_children(&mut |_, child| {
            child.mouse_move(dx, dy, rt);
            false
        });
    }

    fn key_pressed(&mut self, key: &keyboard::Key, rt: &mut dyn Runtime) {
        self.handle_key_pressed(key, rt);
        self.visit_children(&mut |_, child| {
            child.key_pressed(key, rt);
            false
        });
    }

    fn scroll(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
        self.handle_scroll(dx, dy, rt);
        self.visit_children(&mut |_, child| {
            child.scroll(dx, dy, rt);
            false
        });
    }

    #[allow(unused_variables)]
    fn handle_click(&mut self, point: Point, rt: &mut dyn Runtime) {}

    #[allow(unused_variables)]
    fn handle_mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {}

    #[allow(unused_variables)]
    fn handle_mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {}

    #[allow(unused_variables)]
    fn handle_key_pressed(&mut self, key: &keyboard::Key, rt: &mut dyn Runtime) {}

    #[allow(unused_variables)]
    fn handle_scroll(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {}
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
    _on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    fn new(width: f32, height: f32, text: String) -> Self {
        let size = Size { width, height };

        Self {
            width,
            height,
            size,
            text: Text::new(
                text,
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
                AxisAlignment::Center,
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
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
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

        self.text.draw(point, cursor, rt);
    }

    fn handle_click(&mut self, _: Point, _: &mut dyn Runtime) {
        if let Some(on_click) = &mut self._on_click {
            (on_click)();
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = Size {
            width: self.width.min(bounds.width),
            height: self.height.min(bounds.height),
        };
        self.text.set_bounds(self.size, rt);
    }

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}
    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {
        f(Point { x: 0.0, y: 0.0 }, &mut self.text);
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
    pub fn new(inner: C) -> Self {
        Self {
            alignment: Alignment {
                horizontal: AxisAlignment::Start,
                vertical: AxisAlignment::Start,
            },
            size: Size::ZERO,
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

impl<C: Component> Rect<C> {
    pub fn new(inner: C) -> Self {
        Self {
            width: Sizing::Auto,
            height: Sizing::Auto,
            size: Size::ZERO,
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
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
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
        self.inner.draw(point, cursor, rt)
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
        };
        self.inner.set_bounds(self.size, rt);
    }

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}

    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {
        f(Point { x: 0.0, y: 0.0 }, &mut self.inner);
    }

    fn size(&self) -> Size {
        self.size
    }
}

impl<C: Component> Component for Align<C> {
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        let inner_offset = self.inner_offset();
        self.inner.draw(
            point + inner_offset,
            cursor.map(|cursor| cursor + inner_offset),
            rt,
        )
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.inner.set_bounds(bounds, rt);
        self.size = bounds;
    }

    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {
        f(self.inner_offset(), &mut self.inner);
    }

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}

    fn size(&self) -> Size {
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
    dragging: bool,
}

impl<C1: Component, C2: Component> ResizableCols<C1, C2> {
    fn width(&self) -> f32 {
        self.col1_width + self.col2_width + self.spacer_width
    }

    pub fn new(col1: C1, col2: C2, spacer_width: f32) -> Self {
        Self {
            col1,
            col2,
            spacer_width,
            col1_width: 1.0,
            col2_width: 1.0,
            height: 0.0,
            dragging: false,
        }
    }
}

impl<C1: Component, C2: Component> Component for ResizableCols<C1, C2> {
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        self.col1.draw(point, cursor, rt);
        rt.draw_rect(
            point.x + self.col1_width,
            point.y,
            self.spacer_width,
            self.height,
            0.0,
            0.0,
            if self.dragging {
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
            cursor.map(|cursor| Point {
                x: cursor.x - self.col1_width - self.spacer_width,
                y: cursor.y,
            }),
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
            self.dragging = true;
            return true;
        }

        if point.x >= self.col1_width + self.spacer_width
            && point.x <= self.col1_width + self.spacer_width + self.col2_width
        {
            return self.col2.click(
                Point {
                    x: point.x - self.col1_width - self.spacer_width,
                    y: point.y,
                },
                rt,
            );
        }

        false
    }

    fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
        self.dragging = false;
        self.col1.mouse_up(point, rt);
        self.col2.mouse_up(point, rt);
    }

    fn handle_mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
        let mut diff = dx;

        if dx < 0.0 {
            diff = dx.max(-self.col1_width);
        }

        if dx > 0.0 {
            diff = dx.min(self.col2_width);
        }

        if self.dragging {
            self.col1_width += diff;
            self.col2_width -= diff;
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
    }

    fn child_size_changed(&mut self, rt: &mut dyn Runtime) {}

    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {
        f(Point { x: 0.0, y: 0.0 }, &mut self.col1);
        f(
            Point {
                x: self.col1_width + self.spacer_width,
                y: 0.0,
            },
            &mut self.col2,
        );
    }

    fn size(&self) -> Size {
        Size {
            width: self.width(),
            height: self.height,
        }
    }
}

pub struct App {
    columns: ResizableCols<FileForest, Editor>,
    pending_renames: Option<Vec<PathBuf>>,
}

impl Component for App {
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        self.columns.draw(point, cursor, rt);
    }

    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {
        f(Point { x: 0.0, y: 0.0 }, &mut self.columns);
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.columns.set_bounds(bounds, rt);
    }

    fn child_size_changed(&mut self, rt: &mut dyn Runtime) {}

    fn size(&self) -> Size {
        self.columns.size()
    }
}

impl App {
    pub fn new(file_forest: FileForest) -> Self {
        Self {
            columns: ResizableCols::new(file_forest, Editor::new("Cargo.toml"), 10.0),
            pending_renames: None,
        }
    }

    pub fn add_files(&mut self, files: &[PathBuf], rt: &mut dyn Runtime) {
        self.columns.col1.add_files(files, rt);
    }

    pub fn remove_files(&mut self, files: &[PathBuf], rt: &mut dyn Runtime) {
        self.columns.col1.remove_files(files);
    }

    pub fn rename_from(&mut self, paths: Vec<PathBuf>) {
        if let Some(pending_renames) = &self.pending_renames {
            eprintln!("Dropping pending renames: {:?}", pending_renames);
        }

        self.pending_renames = Some(paths);
    }

    pub fn rename_to(&mut self, paths: Vec<PathBuf>, rt: &mut dyn Runtime) {
        let Some(pending_renames) = &mut self.pending_renames else {
            eprintln!("No pending renames");
            return;
        };

        if pending_renames.len() != paths.len() {
            eprintln!(
                "Renames have different lengths: {} != {}",
                pending_renames.len(),
                paths.len()
            );
            return;
        }

        for (old_path, new_path) in pending_renames.iter().zip(paths.iter()) {
            self.columns.col1.rename_file(old_path, new_path, rt);
        }

        self.pending_renames = None;
    }

    pub fn rename_one(
        &mut self,
        old_path: &path::Path,
        new_path: &path::Path,
        rt: &mut dyn Runtime,
    ) {
        self.columns.col1.rename_file(old_path, new_path, rt);
    }

    pub fn rescan_files(&mut self, rt: &mut dyn Runtime) {
        self.columns.col1.rescan_files(rt);
    }

    // fn rerender_file_tree(&mut self, rt: &mut dyn Runtime) {
    //     self.columns.col2.set_rows(Self::file_forest_to_rows(
    //         &self.file_forest,
    //         Size {
    //             width: 100.0,
    //             height: 100.0,
    //         },
    //         rt,
    //     ))
    // }

    // fn file_forest_to_rows(
    //     file_forest: &FileForest,
    //     size: Size,
    //     rt: &mut dyn Runtime,
    // ) -> Vec<Text> {
    //     let mut rows = vec![];
    //     for file_tree in &file_forest.file_trees {
    //         Self::insert_file_tree(file_tree, &mut rows, size, rt);
    //     }
    //     rows.truncate(35);
    //     rows
    // }

    // fn insert_file_tree(
    //     file_tree: &FileTree,
    //     rows: &mut Vec<Text>,
    //     size: Size,
    //     rt: &mut dyn Runtime,
    // ) {
    //     rows.push(Text::new(
    //         file_tree.name.to_string_lossy().to_string(),
    //         size,
    //         Color::black().red(1.0),
    //         AxisAlignment::Center,
    //         rt,
    //     ));

    //     if !file_tree.expanded {
    //         return;
    //     }

    //     for child in &file_tree.children {
    //         Self::insert_file_tree(child, rows, size, rt);
    //     }
    // }
}

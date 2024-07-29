use std::{
    marker::PhantomData,
    path::{self, PathBuf},
};

use editor_stack::EditorStack;
use text::TextLine;
use winit::keyboard;

mod editor;
mod editor_stack;
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
    None,
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

    pub fn gray(value: f32) -> Self {
        Self {
            r: value,
            g: value,
            b: value,
            a: 1.0,
        }
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

impl CachedLine {
    pub fn new() -> Self {
        Self {
            glyphs: vec![],
            units: vec![],
            width: 0.0,
        }
    }
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

pub enum AppEvent {
    OpenFile(PathBuf),
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
    fn schedule_event(&mut self, event: AppEvent);
}

pub trait Visitor {
    type Event;

    fn visit<C: Component>(
        &mut self,
        offset: Point,
        component: &mut C,
        map: impl Fn(C::Event) -> Self::Event,
    ) -> bool;
}

struct ClickVisitor<'rt, F, E> {
    rt: &'rt mut dyn Runtime,
    point: Point,
    fun: F,
    _e: PhantomData<fn(E)>,
}

impl<'rt, E, F: Fn(E)> Visitor for ClickVisitor<'rt, F, E> {
    type Event = E;

    fn visit<C: Component>(
        &mut self,
        offset: Point,
        component: &mut C,
        map: impl Fn(C::Event) -> E,
    ) -> bool {
        component.click(self.point + offset, self.rt, |e| (self.fun)(map(e)))
    }
}

struct MouseMoveVisitor<'rt, F, E> {
    rt: &'rt mut dyn Runtime,
    dx: f32,
    dy: f32,
    fun: F,
    _e: PhantomData<fn(E)>,
}

impl<'rt, F, E> Visitor for MouseMoveVisitor<'rt, F, E> {
    type Event = E;

    fn visit<C: Component>(
        &mut self,
        _: Point,
        component: &mut C,
        map: impl Fn(C::Event) -> E,
    ) -> bool {
        component.mouse_move(self.dx, self.dy, self.rt);
        false
    }
}

struct MouseUpVisitor<'rt, F, E> {
    rt: &'rt mut dyn Runtime,
    point: Point,
    fun: F,
    _e: PhantomData<fn(E)>,
}

impl<'rt, F, E> Visitor for MouseUpVisitor<'rt, F, E> {
    type Event = E;

    fn visit<C: Component>(
        &mut self,
        offset: Point,
        component: &mut C,
        map: impl Fn(C::Event) -> E,
    ) -> bool {
        component.mouse_up(self.point + offset, self.rt);
        false
    }
}

struct KeyPressedVisitor<'rt, 'k, F, E> {
    rt: &'rt mut dyn Runtime,
    key: &'k keyboard::Key,
    fun: F,
    _e: PhantomData<fn(E)>,
}

impl<'rt, 'k, F, E> Visitor for KeyPressedVisitor<'rt, 'k, F, E> {
    type Event = E;

    fn visit<C: Component>(
        &mut self,
        _: Point,
        component: &mut C,
        map: impl Fn(C::Event) -> E,
    ) -> bool {
        component.key_pressed(self.key, self.rt);
        false
    }
}

struct ScrollVisitor<'rt, F, E> {
    rt: &'rt mut dyn Runtime,
    dx: f32,
    dy: f32,
    fun: F,
    _e: PhantomData<fn(E)>,
}

impl<'rt, F, E> Visitor for ScrollVisitor<'rt, F, E> {
    type Event = E;

    fn visit<C: Component>(
        &mut self,
        _: Point,
        component: &mut C,
        map: impl Fn(C::Event) -> E,
    ) -> bool {
        component.scroll(self.dx, self.dy, self.rt);
        false
    }
}

struct DrawVisitor<'rt> {
    rt: &'rt mut dyn Runtime,
    point: Point,
    cursor: Option<Point>,
}

impl<'rt> Visitor for DrawVisitor<'rt> {
    type Event = ();

    fn visit<C: Component>(
        &mut self,
        offset: Point,
        component: &mut C,
        map: impl Fn(C::Event) -> E,
    ) -> bool {
        component.draw(
            self.point + offset,
            self.cursor.map(|cursor| cursor + offset),
            self.rt,
        );
        false
    }
}

pub trait Component {
    type Event;

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime);
    fn child_size_changed(&mut self, rt: &mut dyn Runtime);
    fn size(&self) -> Size;
    fn visit_children(&mut self, visitor: &mut impl Visitor);

    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        self.handle_draw(point, cursor, rt);
        self.visit_children(&mut DrawVisitor { rt, point, cursor });
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime, fun: impl Fn(Self::Event)) -> bool {
        if self.size().contains(point) {
            let event = self.handle_click(point, rt);
            self.visit_children(&mut ClickVisitor { rt, point });
            true
        } else {
            false
        }
    }

    fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
        self.handle_mouse_up(point, rt);
        self.visit_children(&mut MouseUpVisitor { rt, point });
    }

    fn mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
        self.handle_mouse_move(dx, dy, rt);
        self.visit_children(&mut MouseMoveVisitor { rt, dx, dy });
    }

    fn key_pressed(&mut self, key: &keyboard::Key, rt: &mut dyn Runtime) {
        self.handle_key_pressed(key, rt);
        self.visit_children(&mut KeyPressedVisitor { rt, key });
    }

    fn scroll(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
        self.handle_scroll(dx, dy, rt);
        self.visit_children(&mut ScrollVisitor { rt, dx, dy });
    }

    #[allow(unused_variables)]
    fn handle_draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {}

    #[allow(unused_variables)]
    fn handle_click(&mut self, point: Point, rt: &mut dyn Runtime) -> Option<Self::Event> {
        None
    }

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
    text: TextLine,
    _on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    fn new(width: f32, height: f32, text: String) -> Self {
        let size = Size { width, height };

        Self {
            width,
            height,
            size,
            text: TextLine::new(
                text,
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
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
    type Event = ();

    fn handle_draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
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
    }

    fn handle_click(&mut self, _: Point, _: &mut dyn Runtime) -> Option<Self::Event> {
        if let Some(on_click) = &mut self._on_click {
            (on_click)();
        }

        None
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = Size {
            width: self.width.min(bounds.width),
            height: self.height.min(bounds.height),
        };
        self.text.set_bounds(self.size, rt);
    }

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}
    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        visitor.visit(Point { x: 0.0, y: 0.0 }, &mut self.text);
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
                horizontal: AxisAlignment::None,
                vertical: AxisAlignment::None,
            },
            size: Size::ZERO,
            inner,
        }
    }

    pub fn center(mut self) -> Self {
        self.alignment = Alignment::center();
        self
    }

    pub fn vertical_center(mut self) -> Self {
        self.alignment.vertical = AxisAlignment::Center;
        self
    }

    pub fn horizontal_center(mut self) -> Self {
        self.alignment.horizontal = AxisAlignment::Center;
        self
    }

    fn inner_offset(&self) -> Point {
        let x = match self.alignment.horizontal {
            AxisAlignment::Start | AxisAlignment::None => 0.0,
            AxisAlignment::Center => (self.size.width - self.inner.size().width) / 2.0,
            AxisAlignment::End => self.size.width - self.inner.size().width,
        };
        let y = match self.alignment.vertical {
            AxisAlignment::Start | AxisAlignment::None => 0.0,
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

    pub fn bg_color(mut self, color: Color) -> Self {
        self.bg_color = color;
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
        self
    }
}

impl<C: Component> Component for Rect<C> {
    type Event = C::Event;

    fn handle_draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
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
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = Size {
            width: bounds.width.min(match self.width {
                Sizing::Auto | Sizing::Full => bounds.width,
                Sizing::Value(width) => width,
            }),
            height: bounds.height.min(match self.height {
                Sizing::Auto | Sizing::Full => bounds.height,
                Sizing::Value(height) => height,
            }),
        };

        self.inner.set_bounds(self.size, rt);
        let inner_size = self.inner.size();

        if let Sizing::Auto = self.width {
            self.size.width = inner_size.width;
        }
        if let Sizing::Auto = self.height {
            self.size.height = inner_size.height;
        }
    }

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}

    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        visitor.visit(Point { x: 0.0, y: 0.0 }, &mut self.inner);
    }

    fn size(&self) -> Size {
        self.size
    }
}

impl<C: Component> Component for Align<C> {
    type Event = C::Event;

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.inner.set_bounds(bounds, rt);
        self.size = Size {
            width: if let AxisAlignment::None = self.alignment.horizontal {
                self.inner.size().width
            } else {
                bounds.width
            },
            height: if let AxisAlignment::None = self.alignment.vertical {
                self.inner.size().height
            } else {
                bounds.height
            },
        };
    }

    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        visitor.visit(self.inner_offset(), &mut self.inner);
    }

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}

    fn size(&self) -> Size {
        self.size
    }
}

struct Padded<C> {
    padding: f32,
    inner: C,
}

impl<C: Component> Component for Padded<C> {
    type Event = C::Event;

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.inner.set_bounds(
            Size {
                width: bounds.width - self.padding * 2.0,
                height: bounds.height - self.padding * 2.0,
            },
            rt,
        );
    }

    fn child_size_changed(&mut self, _: &mut dyn Runtime) {}

    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        visitor.visit(
            Point {
                x: self.padding,
                y: self.padding,
            },
            &mut self.inner,
        );
    }

    fn size(&self) -> Size {
        let inner_size = self.inner.size();
        Size {
            width: inner_size.width + self.padding * 2.0,
            height: inner_size.height + self.padding * 2.0,
        }
    }
}

impl<C: Component> Padded<C> {
    pub fn new(inner: C, padding: f32) -> Self {
        Self { inner, padding }
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

enum ColumnsEvent<L, R> {
    Left(L),
    Right(R),
}

impl<C1: Component, C2: Component> Component for ResizableCols<C1, C2> {
    type Event = ColumnsEvent<C1::Event, C2::Event>;

    fn handle_draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
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

    fn handle_click(&mut self, point: Point, rt: &mut dyn Runtime) -> Option<Self::Event> {
        if point.x >= self.col1_width && point.x <= self.col1_width + self.spacer_width {
            self.dragging = true;
        }

        None
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

    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        visitor.visit(Point { x: 0.0, y: 0.0 }, &mut self.col1);
        visitor.visit(
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
    columns: ResizableCols<FileForest, EditorStack>,
    pending_renames: Option<Vec<PathBuf>>,
}

impl Component for App {
    type Event = ();

    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        visitor.visit(Point { x: 0.0, y: 0.0 }, &mut self.columns);
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
            columns: ResizableCols::new(file_forest, EditorStack::new(), 10.0),
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

    pub fn open_file(&mut self, path: path::PathBuf, rt: &mut dyn Runtime) {
        self.columns.col2.open(path, rt);
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

use std::{collections::HashMap, path::PathBuf};

use super::{
    editor::Editor, rows::Columns, text::TextLine, Align, Color, Component, Point, Rect, Runtime,
    Size, Visitor,
};

const TOP_BAR_HEIGHT: f32 = 70.0;

pub struct EditorStack {
    editors: Vec<Editor>,
    current: Option<usize>,
    path_to_editor: HashMap<PathBuf, usize>,
    tabs: Columns<Rect<Align<TextLine>>>,
    size: Size,
}

impl EditorStack {
    pub fn new() -> Self {
        Self {
            editors: vec![],
            current: None,
            size: Size::ZERO,
            path_to_editor: HashMap::new(),
            tabs: Columns::new(vec![]),
        }
    }

    pub fn open(&mut self, path: PathBuf, rt: &mut dyn Runtime) {
        if let Some(editor_index) = self.path_to_editor.get(&path) {
            self.current = Some(*editor_index);
            return;
        }

        let mut editor = Editor::new(&path);
        editor.set_bounds(
            Size {
                width: self.size.width,
                height: self.size.height - TOP_BAR_HEIGHT,
            },
            rt,
        );
        self.editors.push(editor);
        self.current = Some(self.editors.len() - 1);
        self.path_to_editor
            .insert(path.to_path_buf(), self.editors.len() - 1);

        self.tabs.push(
            Rect::new(
                Align::new(TextLine::new(
                    path.to_string_lossy().to_string(),
                    super::Color::black().red(1.0),
                ))
                .vertical_center(),
            )
            .bg_color(Color::gray(0.12))
            .border_color(Color::gray(0.30))
            .border_width(2.0)
            .full_height(),
            rt,
        );
    }
}

impl Component for EditorStack {
    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        visitor.visit(Point { x: 0.0, y: 0.0 }, &mut self.tabs);

        if let Some(current) = &self.current {
            visitor.visit(
                Point {
                    x: 0.0,
                    y: TOP_BAR_HEIGHT,
                },
                &mut self.editors[*current],
            );
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.size = bounds;
        if let Some(current) = &self.current {
            self.editors[*current].set_bounds(
                Size {
                    width: bounds.width,
                    height: bounds.height - TOP_BAR_HEIGHT,
                },
                rt,
            );
        }
        self.tabs.set_bounds(
            Size {
                width: bounds.width,
                height: TOP_BAR_HEIGHT,
            },
            rt,
        );
    }

    fn child_size_changed(&mut self, rt: &mut dyn Runtime) {}

    fn size(&self) -> Size {
        self.size
    }
}

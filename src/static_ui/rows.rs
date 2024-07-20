use super::{Component, Point, Runtime, Size, Visitor};

pub struct Rows<C: Component> {
    rows: Vec<C>,
}

impl<C: Component> Rows<C> {
    pub fn new(rows: Vec<C>) -> Self {
        Self { rows }
    }

    pub fn set_rows(&mut self, rows: Vec<C>) {
        self.rows = rows;
    }
}

impl<C: Component> Component for Rows<C> {
    fn size(&self) -> Size {
        Size {
            width: self
                .rows
                .iter()
                .map(|row| row.size().width)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0),
            height: self.rows.iter().map(|row| row.size().height).sum(),
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        let mut height_remaining = bounds.height;

        for row in &mut self.rows {
            row.set_bounds(
                Size {
                    width: bounds.width,
                    height: height_remaining,
                },
                rt,
            );
            height_remaining -= row.size().height;
        }
    }

    fn child_size_changed(&mut self, rt: &mut dyn Runtime) {}

    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        for row in &mut self.rows {
            visitor.visit(Point { x: 0.0, y: 0.0 }, row);
        }
    }
}

pub struct Columns<C: Component> {
    columns: Vec<C>,
    bounds: Size,
}

impl<C: Component> Columns<C> {
    pub fn new(columns: Vec<C>) -> Self {
        Self {
            columns,
            bounds: Size::ZERO,
        }
    }

    pub fn set_columns(&mut self, columns: Vec<C>) {
        self.columns = columns;
    }

    pub fn push(&mut self, mut column: C, rt: &mut dyn Runtime) {
        column.set_bounds(
            Size {
                width: self.bounds.width
                    - self
                        .columns
                        .iter()
                        .map(|column| column.size().width)
                        .sum::<f32>(),
                height: self.bounds.height,
            },
            rt,
        );
        self.columns.push(column);
    }
}

impl<C: Component> Component for Columns<C> {
    fn size(&self) -> Size {
        Size {
            height: self
                .columns
                .iter()
                .map(|column| column.size().height)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0),
            width: self.columns.iter().map(|column| column.size().width).sum(),
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.bounds = bounds;
        let mut width_remaining = bounds.width;

        for row in &mut self.columns {
            row.set_bounds(
                Size {
                    height: bounds.height,
                    width: width_remaining,
                },
                rt,
            );
            width_remaining -= row.size().width;
        }
    }

    fn child_size_changed(&mut self, rt: &mut dyn Runtime) {}

    fn visit_children(&mut self, visitor: &mut impl Visitor) {
        let mut x_offset = 0.0;
        for row in &mut self.columns {
            visitor.visit(
                Point {
                    x: x_offset,
                    y: 0.0,
                },
                row,
            );
            x_offset += row.size().width;
        }
    }
}

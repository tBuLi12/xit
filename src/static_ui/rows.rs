use super::{Component, Point, Runtime, Size};

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
    fn draw(&mut self, point: Point, rt: &mut dyn Runtime) {
        let mut y_offset = 0.0;

        for row in &mut self.rows {
            row.draw(
                Point {
                    x: point.x,
                    y: point.y + y_offset,
                },
                rt,
            );
            y_offset += row.size().height;
        }
    }

    fn click(&mut self, point: Point, rt: &mut dyn Runtime) -> bool {
        let mut y_offset = 0.0;

        for row in &mut self.rows {
            if row.click(
                Point {
                    x: point.x,
                    y: point.y + y_offset,
                },
                rt,
            ) {
                return true;
            }
            y_offset += row.size().height;
        }

        false
    }

    fn mouse_up(&mut self, point: Point, rt: &mut dyn Runtime) {
        let mut y_offset = 0.0;

        for row in &mut self.rows {
            row.mouse_up(
                Point {
                    x: point.x,
                    y: point.y + y_offset,
                },
                rt,
            );
            y_offset += row.size().height;
        }
    }

    fn mouse_move(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
        for row in &mut self.rows {
            row.mouse_move(dx, dy, rt);
        }
    }

    fn key_pressed(&mut self, key: &str, rt: &mut dyn Runtime) {
        for row in &mut self.rows {
            row.key_pressed(key, rt);
        }
    }

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

    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {
        for row in &mut self.rows {
            f(Point { x: 0.0, y: 0.0 }, row);
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct RawRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    corner_radius: f32,
    border_width: f32,
}

#[derive(Clone, Debug)]
pub struct RawText {
    text: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

// #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
// pub struct PrimitiveID(u32);

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
    );
    fn draw_text(&mut self, x: f32, y: f32, width: f32, height: f32, text: &str);
}

pub trait Component {
    fn draw(&self, rt: &mut dyn Runtime);
    fn click(&mut self, x: f32, y: f32);
}

struct Button {
    rect: RawRect,
    text: String,
}

impl Button {
    fn draw(&self, rt: &mut dyn Runtime) {
        rt.draw_rect(
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height,
            self.rect.corner_radius,
            self.rect.border_width,
        );
        rt.draw_text(
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height,
            &self.text,
        );
    }

    fn click(&mut self, x: f32, y: f32) -> bool {
        return x > self.rect.x
            && x < self.rect.x + self.rect.width
            && y > self.rect.y
            && y < self.rect.y + self.rect.height;
    }
}

pub struct Counter {
    plus: Button,
    minus: Button,
    value: u32,
}

impl Component for Counter {
    fn draw(&self, rt: &mut dyn Runtime) {
        self.plus.draw(rt);
        self.minus.draw(rt);
        rt.draw_text(0.0, 300.0, 100.0, 100.0, &self.value.to_string());
    }

    fn click(&mut self, x: f32, y: f32) {
        if self.plus.click(x, y) {
            self.value += 1;
            return;
        }

        if self.minus.click(x, y) {
            self.value -= 1;
            return;
        }
    }
}

impl Counter {
    pub fn new() -> Self {
        Self {
            plus: Button {
                rect: RawRect {
                    x: 0.0,
                    y: 100.0,
                    width: 100.0,
                    height: 100.0,
                    corner_radius: 0.0,
                    border_width: 0.0,
                },
                text: "+".to_string(),
            },
            minus: Button {
                rect: RawRect {
                    x: 0.0,
                    y: 400.0,
                    width: 100.0,
                    height: 100.0,
                    corner_radius: 0.0,
                    border_width: 0.0,
                },
                text: "-".to_string(),
            },
            value: 0,
        }
    }
}

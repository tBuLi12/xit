use caarr::{Rect, TextLine};

struct App {
    text: Option<TextLine>,
    cursor: Option<Rect>,
    str: String,
}

impl caarr::App for App {
    fn init(&mut self, root: &Rect) {
        root.set_size(200, 200);
        root.set_bg_color([255, 255, 255, 255]);
        let mut text = root.new_text_child();
        text.set_text(&self.str);

        let cursor = text.rect.new_child();
        cursor.set_size(2, 20);
        cursor.set_bg_color([100, 100, 100, 255]);

        self.text = Some(text);
        self.cursor = Some(cursor);
    }

    fn on_key_event(&mut self, str: &str) {
        self.str.push_str(str);
        let text = self.text.as_mut().unwrap();
        text.set_text(&self.str);
        let cursor = self.cursor.as_ref().unwrap();
        text.rect.append_child(cursor);
        cursor.set_pos(text.clusters(), y);
    }
}

fn main() {
    caarr::run_app(App {
        text: None,
        cursor: None,
        str: String::new(),
    });
}

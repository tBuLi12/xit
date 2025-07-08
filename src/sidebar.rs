use std::path::PathBuf;

use caarr::Rect;

use crate::{EditorStatus, BASE_FONT};

pub const SIDEBAR_WIDTH: u32 = 150;
pub const ITEM_HEIGHT: u32 = 40;

pub const BASE_COLOR: [u8; 4] = [180, 180, 180, 255];
pub const UNSAVED_COLOR: [u8; 4] = [180, 180, 100, 255];

pub struct Sidebar {
    main_box: Rect,
    items: Vec<Item>,
}

impl Sidebar {
    pub fn new(parent: &Rect) -> Self {
        let main_box = parent.new_child();
        main_box.set_size(SIDEBAR_WIDTH, parent.get_size().1);
        main_box.set_bg_color([200, 200, 200, 255]);

        Self {
            main_box,
            items: vec![],
        }
    }

    pub fn add_item(&mut self, path: PathBuf, name: &str) {
        eprintln!("adding sidebar item {name}");
        let item = self.main_box.new_child();
        item.set_size(SIDEBAR_WIDTH, ITEM_HEIGHT);
        item.set_pos(0, (self.items.len() as u32 * (ITEM_HEIGHT + 2)) as i32);
        item.set_bg_color(BASE_COLOR);
        let mut text = item.new_text_child();
        text.set_text(*BASE_FONT, name);
        self.items.push(Item { path, rect: item });
    }

    pub fn remove_item(&mut self, path: &PathBuf) {
        let Some(idx) = self.items.iter().position(|item| &item.path == path) else {
            return;
        };

        self.items.remove(idx).rect.remove_from_parent();

        for idx in idx..self.items.len() {
            self.items[idx]
                .rect
                .set_pos(0, (idx as u32 * (ITEM_HEIGHT + 2)) as i32);
        }
    }

    pub fn handle_parent_resize(&mut self, parent: &Rect) {
        let (_, height) = parent.get_size();
        self.main_box.set_size(SIDEBAR_WIDTH, height);
    }

    pub fn set_editor_status(&mut self, path: &PathBuf, status: EditorStatus) {
        let Some(idx) = self.items.iter().position(|item| &item.path == path) else {
            return;
        };

        let color = match status {
            EditorStatus::None => BASE_COLOR,
            EditorStatus::Unsaved => UNSAVED_COLOR,
        };
        self.items[idx].rect.set_bg_color(color);
    }
}

struct Item {
    path: PathBuf,
    rect: Rect,
}

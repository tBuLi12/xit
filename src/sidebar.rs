use std::path::PathBuf;

use caarr::{Rect, TextLine};

use crate::{editor::EditorStatus, BASE_FONT};

pub const SIDEBAR_WIDTH: u32 = 150;
pub const ITEM_HEIGHT: u32 = 40;

pub const BASE_COLOR: [u8; 4] = [180, 180, 180, 255];
pub const UNSAVED_COLOR: [u8; 4] = [180, 180, 100, 255];

pub fn sidebar<'p>(items: impl IntoIterator<Item = Item<'p>>, height: u32) -> Rect {
    let rect = Rect::new();
    rect.set_size(SIDEBAR_WIDTH, height);
    rect.set_bg_color([200, 200, 200, 255]);

    for (i, item) in items.into_iter().enumerate() {
        let item_rect = Rect::new();
        item_rect.set_size(SIDEBAR_WIDTH, ITEM_HEIGHT);
        item_rect.set_pos(0, (i as u32 * (ITEM_HEIGHT + 2)) as i32);
        item_rect.set_bg_color(match item.status {
            EditorStatus::None => BASE_COLOR,
            EditorStatus::Unsaved => UNSAVED_COLOR,
        });
        let mut text = TextLine::new();
        text.set_text(
            *BASE_FONT,
            [(
                &*item.path.file_name().unwrap().to_string_lossy(),
                [0, 0, 0, 255],
            )],
        );
        text.rect.set_pos(10, 0);
        item_rect.append_child(text.rect);
        rect.append_child(item_rect);
    }

    rect
}

pub struct Item<'p> {
    pub path: &'p PathBuf,
    pub status: EditorStatus,
}

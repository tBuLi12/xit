use std::{
    ffi::OsString,
    fs, iter, mem,
    path::{self, PathBuf},
};

use super::{text::Text, AxisAlignment, Color, Component, Point, Runtime, Size};

pub struct FileForest {
    file_trees: Vec<FileTree>,
    root: PathBuf,
    width: f32,
    inner_height: f32,
    scroll_offset: f32,
}

enum Children {
    None,
    Inline(Vec<FileTree>),
    Collapsed(Vec<FileTree>),
}

pub struct FileTree {
    name: OsString,
    text: Text,
    children: Children,
    depth: usize,
}

impl FileTree {
    pub fn set_name(&mut self, name: OsString, rt: &mut dyn Runtime) {
        self.text.set_text(&name.to_string_lossy(), rt);
        self.name = name;
    }
}

impl FileForest {
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Self {
        let root = path.as_ref().to_path_buf();
        let mut this = Self::from_path_inner(path, 0);
        this.root = root;
        this
    }

    pub fn from_path_inner(path: impl AsRef<std::path::Path>, depth: usize) -> Self {
        let mut roots = vec![];

        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name();

            if name != ".git" {
                roots.push(FileTree {
                    text: Text::new(
                        name.to_string_lossy().to_string(),
                        Color::black().red(1.0),
                        AxisAlignment::Start,
                    ),
                    name,
                    children: if path.is_dir() {
                        Children::Collapsed(Self::from_path_inner(path, depth + 1).file_trees)
                    } else {
                        Children::None
                    },
                    depth,
                });
            }
        }

        Self {
            file_trees: roots,
            width: 0.0,
            inner_height: 0.0,
            scroll_offset: 0.0,
            root: PathBuf::new(),
        }
    }

    pub fn add_files(&mut self, files: &[PathBuf], rt: &mut dyn Runtime) {
        for file in files {
            let meta = fs::metadata(&file);
            let is_dir = match meta {
                Ok(meta) => meta.is_dir(),
                Err(error) => {
                    eprintln!("Error getting metadata for {}: {}", file.display(), error);
                    continue;
                }
            };
            let Some(path) = self.strip_root(file) else {
                continue;
            };
            let mut components: Vec<_> = path.components().collect();
            let width = self.width;

            let Some(path::Component::Normal(name)) = components.pop() else {
                panic!("Missing file name");
            };

            let name_string = name.to_string_lossy().to_string();
            let mut text = Text::new(name_string, Color::black().red(1.0), AxisAlignment::Start);
            text.set_bounds(
                Size {
                    width,
                    height: 50.0,
                },
                rt,
            );

            let mut new_tree = FileTree {
                text,
                name: name.to_os_string(),
                children: if is_dir {
                    Children::Collapsed(vec![])
                } else {
                    Children::None
                },
                depth: 0,
            };

            if components.is_empty() {
                self.file_trees.push(new_tree);
            } else {
                let Some((start, i, trees)) = self.find_node_idx(&components) else {
                    continue;
                };

                new_tree.depth = i;

                match &mut trees[start].children {
                    Children::None => {
                        panic!("Expected children");
                    }
                    Children::Inline(_) => {
                        trees.insert(start + 1, new_tree);
                    }
                    Children::Collapsed(children) => {
                        children.push(new_tree);
                    }
                };
            }
        }
        self.clamp_scroll_offset();
    }

    pub fn remove_files(&mut self, files: &[PathBuf]) {
        for file in files {
            let Some(path) = self.strip_root(file) else {
                continue;
            };

            let components: Vec<_> = path.components().collect();
            let Some((start, _, trees)) = self.find_node_idx(&components) else {
                continue;
            };

            Self::remove_tree(trees, start);
        }
        self.clamp_scroll_offset();
    }

    pub fn rename_file(
        &mut self,
        old_file: &path::Path,
        new_file: &path::Path,
        rt: &mut dyn Runtime,
    ) {
        let Some(old_path) = self.strip_root(old_file) else {
            return;
        };
        let Some(new_path) = self.strip_root(new_file) else {
            return;
        };

        let old_parent = old_path.parent();
        let new_parent = new_path.parent();

        if old_parent != new_parent {
            eprintln!(
                "Cannot rename file {} to {}: Parents do not match",
                old_path.display(),
                new_path.display()
            );
            return;
        }

        let components: Vec<_> = old_path.components().collect();
        let Some((start, _, trees)) = self.find_node_idx(&components) else {
            return;
        };

        let Some(new_name) = new_path.file_name() else {
            eprintln!("New path has no file name");
            return;
        };

        trees[start].set_name(new_name.to_owned(), rt);
    }

    pub fn rescan_files(&mut self, rt: &mut dyn Runtime) {
        Self::rescan_directory(&mut self.file_trees, 0, &self.root, self.width, rt);
    }

    pub fn rescan_directory(
        trees: &mut Vec<FileTree>,
        mut start: usize,
        path: &path::Path,
        width: f32,
        rt: &mut dyn Runtime,
    ) {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .unwrap()
            .map(Result::unwrap)
            .map(|entry| (entry.file_name(), entry, false))
            .filter(|(name, _, _)| name != ".git")
            .collect();

        let depth = trees[start].depth;
        'trees: while start < trees.len() && trees[start].depth >= depth {
            let tree = &mut trees[start];
            if tree.depth > depth {
                start += 1;
                continue;
            }

            for (name, entry, found) in &mut entries {
                if name == &tree.name {
                    *found = true;
                    start += 1;
                    match &mut tree.children {
                        Children::None => {}
                        Children::Inline(_) => {
                            if trees[start].depth == depth + 1 {
                                Self::rescan_directory(trees, start, &entry.path(), width, rt);
                            }
                        }
                        Children::Collapsed(children) => {
                            if !children.is_empty() {
                                Self::rescan_directory(children, 0, &entry.path(), width, rt);
                            }
                        }
                    }
                    continue 'trees;
                }
            }

            Self::remove_tree(trees, start);
        }

        for (name, entry, found) in entries {
            if !found {
                let path = entry.path();
                let mut text = Text::new(
                    name.to_string_lossy().to_string(),
                    Color::black().red(1.0),
                    AxisAlignment::Start,
                );
                text.set_bounds(
                    Size {
                        width,
                        height: 50.0,
                    },
                    rt,
                );
                trees.insert(
                    start,
                    FileTree {
                        text,
                        name,
                        children: if path.is_dir() {
                            Children::Collapsed(
                                FileForest::from_path_inner(path, depth + 1).file_trees,
                            )
                        } else {
                            Children::None
                        },
                        depth,
                    },
                )
            }
        }
    }

    fn remove_tree(trees: &mut Vec<FileTree>, start: usize) {
        if let Children::Inline(_) = &trees[start].children {
            let depth = trees[start].depth;
            let child_count = trees[(start + 1)..]
                .iter()
                .take_while(|tree| tree.depth > depth)
                .count();
            trees.splice(start..(start + 1 + child_count), iter::empty());
        } else {
            trees.remove(start);
        }
    }

    fn strip_root<'p>(&self, file: &'p path::Path) -> Option<&'p path::Path> {
        if let Ok(path) = file.strip_prefix(&self.root) {
            Some(path)
        } else {
            eprintln!(
                "File {} is not in root: {}",
                file.display(),
                self.root.display()
            );
            None
        }
    }

    fn find_node_idx(
        &mut self,
        components: &[path::Component],
    ) -> Option<(usize, usize, &mut Vec<FileTree>)> {
        let mut i = 0;
        let mut start = 0;
        let mut trees = &mut self.file_trees;
        loop {
            let path::Component::Normal(name) = &components[i] else {
                panic!("Unexpected component {:?}", components[i]);
            };

            eprintln!("looking for {}", name.to_string_lossy());
            let parent_idx = trees[start..]
                .iter_mut()
                .take_while(|file_tree| file_tree.depth >= i)
                .position(|file_tree| file_tree.depth == i && file_tree.name == *name);

            if let Some(parent_idx) = parent_idx {
                start += parent_idx;
                i += 1;
                if i == components.len() {
                    dbg!((start, i));
                    return Some((start, i, trees));
                }

                let get_children = match &mut trees[start].children {
                    Children::None => {
                        panic!("Expected children");
                    }
                    Children::Inline(_) => {
                        start += 1;
                        false
                    }
                    Children::Collapsed(_) => {
                        // trees = children;
                        true
                    }
                };
                if get_children {
                    let Children::Collapsed(children) = &mut trees[start].children else {
                        unreachable!()
                    };
                    start = 0;
                    trees = children;
                }
            } else {
                eprintln!(
                    "File {:?} is not in intermediate path: {}",
                    &components,
                    self.root.display()
                );
                return None;
            }
        }
    }

    fn clamp_scroll_offset(&mut self) {
        self.scroll_offset = self
            .scroll_offset
            .max(0.0)
            .min((self.file_trees.len() as f32 * 50.0 - self.inner_height).max(0.0));
    }
}

impl Component for FileForest {
    fn draw(&mut self, point: Point, cursor: Option<Point>, rt: &mut dyn Runtime) {
        let width = self.width;
        self.visit_children(&mut |offset, child| {
            let point = offset + point;
            if let Some(cursor) = cursor {
                if cursor.x >= 0.0
                    && cursor.x <= width
                    && cursor.y - point.y >= 0.0
                    && cursor.y - point.y <= 50.0
                {
                    rt.draw_rect(
                        point.x,
                        point.y,
                        width,
                        50.0,
                        0.0,
                        0.0,
                        Color::black().red(0.4).green(0.4).blue(0.4),
                        Color::clear(),
                    );
                }
            }
            child.draw(point, cursor.map(|cursor| cursor + offset), rt);
            false
        });

        let scroll_height = self.file_trees.len() as f32 * 50.0;
        if scroll_height > self.inner_height {
            let bar_height = self.inner_height / scroll_height * self.inner_height;
            let max_scroll_offset = scroll_height - self.inner_height;
            rt.draw_rect(
                point.x + self.width - 20.0,
                point.y
                    + (self.inner_height - bar_height) * (self.scroll_offset / max_scroll_offset),
                20.0,
                bar_height,
                4.0,
                0.0,
                Color::black().green(1.0),
                Color::clear(),
            );
        }
    }

    fn visit_children(&mut self, f: &mut dyn FnMut(Point, &mut dyn Component) -> bool) {
        for (i, file_tree) in self.file_trees.iter_mut().enumerate() {
            f(
                Point {
                    x: file_tree.depth as f32 * 20.0,
                    y: i as f32 * 50.0 - self.scroll_offset,
                },
                &mut file_tree.text,
            );
        }
    }

    fn set_bounds(&mut self, bounds: Size, rt: &mut dyn Runtime) {
        self.width = bounds.width;
        self.inner_height = bounds.height;
        for file_tree in self.file_trees.iter_mut() {
            file_tree.text.set_bounds(
                Size {
                    width: bounds.width,
                    height: 50.0,
                },
                rt,
            );
        }
    }

    fn child_size_changed(&mut self, rt: &mut dyn Runtime) {}

    fn size(&self) -> Size {
        Size {
            width: self.width,
            height: self.inner_height,
        }
    }

    fn handle_click(&mut self, point: Point, rt: &mut dyn Runtime) {
        let i = ((point.y + self.scroll_offset) / 50.0).floor() as usize;
        if i < self.file_trees.len() {
            let file_tree = &mut self.file_trees[i];
            match &mut file_tree.children {
                Children::None => {}
                Children::Inline(empty) => {
                    let mut empty = mem::take(empty);
                    let depth = file_tree.depth;
                    let child_count = self.file_trees[(i + 1)..]
                        .iter()
                        .take_while(|tree| tree.depth > depth)
                        .count();

                    empty.extend(
                        self.file_trees
                            .splice((i + 1)..(i + 1 + child_count), iter::empty()),
                    );
                    self.file_trees[i].children = Children::Collapsed(empty);
                }
                Children::Collapsed(children) => {
                    let mut children = mem::take(children);
                    for child in &mut children {
                        child.text.set_bounds(
                            Size {
                                width: self.width,
                                height: 50.0,
                            },
                            rt,
                        );
                    }

                    self.file_trees.splice((i + 1)..(i + 1), children.drain(..));
                    self.file_trees[i].children = Children::Inline(children);
                }
            }
            self.clamp_scroll_offset();
        }
    }

    fn handle_scroll(&mut self, dx: f32, dy: f32, rt: &mut dyn Runtime) {
        self.scroll_offset -= dy;
        self.clamp_scroll_offset();
    }
}

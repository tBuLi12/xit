use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    f32::consts::E,
    hint::black_box,
    marker::PhantomData,
    mem,
    num::NonZero,
    ops::{self, Deref},
    ptr,
    rc::Rc,
    sync::Arc,
    time::Instant,
    u8,
};

use softbuffer::Surface;
use winit::{
    event::{Modifiers, WindowEvent},
    event_loop::{EventLoop, EventLoopProxy},
    keyboard::{self, ModifiersState, SmolStr},
    window::{Window, WindowAttributes},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Offset {
    pub x: usize,
    pub y: usize,
}

impl ops::Add for Offset {
    type Output = Offset;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl ops::Sub for Offset {
    type Output = Offset;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ops::Mul for Color {
    type Output = Color;

    fn mul(self, rhs: Self) -> Self::Output {
        Color {
            r: (self.r as u16 * rhs.r as u16 / u8::MAX as u16) as u8,
            g: (self.g as u16 * rhs.g as u16 / u8::MAX as u16) as u8,
            b: (self.b as u16 * rhs.b as u16 / u8::MAX as u16) as u8,
        }
    }
}

impl Color {
    pub fn red() -> Self {
        Color {
            r: u8::MAX,
            g: 0,
            b: 0,
        }
    }

    pub fn green() -> Self {
        Color {
            r: 0,
            g: u8::MAX,
            b: 0,
        }
    }

    pub fn gray(n: u8) -> Self {
        Color { r: n, g: n, b: n }
    }
}

fn blend(background: Color, color: Color, alphas: Color) -> Color {
    let Color {
        r: src_r,
        g: src_g,
        b: src_b,
    } = background;

    let r = (color.r as u16 * alphas.r as u16 + src_r as u16 * (u8::MAX - alphas.r) as u16)
        / u8::MAX as u16;
    let g = (color.g as u16 * alphas.g as u16 + src_g as u16 * (u8::MAX - alphas.g) as u16)
        / u8::MAX as u16;
    let b = (color.b as u16 * alphas.b as u16 + src_b as u16 * (u8::MAX - alphas.b) as u16)
        / u8::MAX as u16;

    Color {
        r: r as u8,
        g: g as u8,
        b: b as u8,
    }
}

// pub trait Widget<'state> {
//     // fn click(&self, offset: Offset) -> Option<ClickHandler>;
//     fn rect(&self, bounds: Size) -> Rect<'state>;
// }

// pub struct Text<'state> {
//     pub text: &'state str,
// }

// impl<'state> Widget<'state> for Text<'state> {
//     fn rect(&self, bounds: Size) -> Rect<'state> {
//         text(&self.text)
//     }
// }

// pub struct SizedRect<C> {
//     pub size: Size,
//     pub fill: Fill,
//     pub radius: f32,
//     pub children: C,
// }

// impl<'state, C: Children<'state>> Widget<'state> for SizedRect<C> {
//     fn rect(&self, bounds: Size) -> Rect<'state> {
//         Rect {
//             size: self.size,
//             children: self.children.children(self.size),
//             fill: self.fill,
//             radius: self.radius,
//             on_click: None,
//             on_key_pressed: None,
//         }
//     }
// }

// pub trait Children<'state> {
//     fn children(&self, bounds: Size) -> Vec<Child<'state>>;
// }

// impl<'state> Children<'state> for () {
//     fn children(&self, bounds: Size) -> Vec<Child<'state>> {
//         vec![]
//     }
// }

// impl<'state, C1: Widget<'state>> Children<'state> for (C1,) {
//     fn children(&self, bounds: Size) -> Vec<Child<'state>> {
//         vec![Child {
//             rect: self.0.rect(bounds),
//             position: Offset { x: 0, y: 0 },
//         }]
//     }
// }

// pub struct FillingRect<C> {
//     pub fill: Fill,
//     pub children: C,
// }

// impl<'state, C: Children<'state>> Widget<'state> for FillingRect<C> {
//     fn rect(&self, bounds: Size) -> Rect<'state> {
//         Rect {
//             size: bounds,
//             children: self.children.children(bounds),
//             fill: self.fill,
//             radius: 0.0,
//             on_click: None,
//             on_key_pressed: None,
//         }
//     }
// }

// pub struct OnKey<'state, W> {
//     pub on_key: KeyHandler<'state>,
//     pub widget: W,
// }

// impl<'state, W: Widget<'state>> Widget<'state> for OnKey<'state, W> {
//     fn rect(&self, bounds: Size) -> Rect<'state> {
//         let mut rect = self.widget.rect(bounds);
//         rect.on_key_pressed = Some(self.on_key);
//         rect
//     }
// }

pub struct Child<'state> {
    pub position: Offset,
    pub rect: Rect<'state>,
}

impl<'state> Child<'state> {
    fn to_inner(&self, offset: Offset) -> Option<Offset> {
        if offset.x >= self.position.x
            && offset.x < self.position.x + self.rect.size.width
            && offset.y >= self.position.y
            && offset.y < self.position.y + self.rect.size.height
        {
            return Some(offset - self.position);
        }

        None
    }

    fn damage_at(&self, offset: Offset) -> Option<softbuffer::Rect> {
        self.rect.damage_at(offset + self.position)
    }
}

#[derive(Clone, Copy)]
pub struct ClickHandler<'state> {
    ptr: *const u8,
    fun: fn(*const u8),
    _borrowed: PhantomData<&'state ()>,
}

impl<'state> ClickHandler<'state> {
    pub fn new<T>(state: &'state T, mutate: fn(&mut T)) -> Self {
        Self {
            _borrowed: PhantomData,
            fun: unsafe { mem::transmute(mutate) },
            ptr: state as *const _ as *const u8,
        }
    }
}

#[derive(Clone, Copy)]
pub struct KeyHandler<'state> {
    ptr: *const u8,
    fun: fn(*const u8, key: &keyboard::Key<SmolStr>, modifiers: ModifiersState),
    _borrowed: PhantomData<&'state ()>,
}

impl<'state> KeyHandler<'state> {
    pub fn new<T>(
        state: &'state T,
        mutate: fn(&mut T, &keyboard::Key<SmolStr>, ModifiersState),
    ) -> Self {
        Self {
            _borrowed: PhantomData,
            fun: unsafe { mem::transmute(mutate) },
            ptr: state as *const _ as *const u8,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Fill {
    Color(Color),
    Glyph(&'static [u8]),
    None,
}

pub struct Rect<'state> {
    pub size: Size,
    pub children: Vec<Child<'state>>,
    pub fill: Fill,
    pub radius: f32,
    pub on_click: Option<ClickHandler<'state>>,
    pub on_key_pressed: Option<KeyHandler<'state>>,
    pub do_layout: Option<fn(&mut Rect)>,
}

impl<'state> Rect<'state> {
    fn click(&self, offset: Offset) -> bool {
        let Self {
            size,
            children,
            on_click,
            ..
        } = &*self;

        for child in children {
            if let Some(offset) = child.to_inner(offset) {
                if child.rect.click(offset) {
                    return true;
                }
            }
        }
        if let Some(handler) = on_click {
            (handler.fun)(handler.ptr);
            return true;
        }
        false
    }

    fn key_press(&self, key: &keyboard::Key<SmolStr>, modifiers: ModifiersState) -> bool {
        for child in &self.children {
            if child.rect.key_press(key, modifiers) {
                return true;
            }
        }
        if let Some(handler) = &self.on_key_pressed {
            (handler.fun)(handler.ptr, key, modifiers);
            return true;
        }
        false
    }

    fn damage_at(&self, offset: Offset) -> Option<softbuffer::Rect> {
        Some(softbuffer::Rect {
            height: NonZero::new(self.size.height as u32)?,
            width: NonZero::new(self.size.width as u32)?,
            x: offset.x as u32,
            y: offset.y as u32,
        })
    }

    fn color_at(&self, offset: Offset, bg: Color) -> Color {
        let mut color = match self.fill {
            Fill::Color(color) => color,
            Fill::Glyph(data) => blend(
                bg,
                Color {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                Color {
                    g: data[(offset.y * self.size.width + offset.x) * 4 + 1],
                    r: data[(offset.y * self.size.width + offset.x) * 4 + 2],
                    b: data[(offset.y * self.size.width + offset.x) * 4],
                },
            ),
            Fill::None => bg,
        };

        for child in &self.children {
            if let Some(offset) = child.to_inner(offset) {
                color = child.rect.color_at(offset, color);
            }
        }

        color
    }
}

fn diff(prev: &Rect, new: &Rect, offset: Offset) -> Vec<softbuffer::Rect> {
    if prev.size != new.size || new.radius != prev.radius {
        return vec![prev.damage_at(offset), new.damage_at(offset)]
            .into_iter()
            .filter_map(|d| d)
            .collect();
    }

    match (prev.fill, new.fill) {
        (Fill::Color(prev), Fill::Color(new)) if prev == new => {}
        (Fill::Glyph(prev), Fill::Glyph(new)) if ptr::eq(prev, new) => {}
        (Fill::None, Fill::None) => {}
        _ => {
            return vec![prev.damage_at(offset), new.damage_at(offset)]
                .into_iter()
                .filter_map(|d| d)
                .collect()
        }
    }

    if prev.children.len() != new.children.len() {
        return vec![prev.damage_at(offset), new.damage_at(offset)]
            .into_iter()
            .filter_map(|d| d)
            .collect();
    }

    prev.children
        .iter()
        .zip(new.children.iter())
        .flat_map(|(prev, new)| {
            if prev.position == new.position {
                diff(&prev.rect, &new.rect, new.position + offset)
            } else {
                vec![prev.damage_at(offset), new.damage_at(offset)]
                    .into_iter()
                    .filter_map(|d| d)
                    .collect()
            }
        })
        .collect()
}

struct Canvas<'p> {
    pixels: &'p mut [u32],
    width: usize,
    height: usize,
}

impl<'p> Canvas<'p> {
    fn set(&mut self, x: usize, y: usize, color: Color) {
        let Some(pixel) = self.pixels.get_mut(y * self.width + x) else {
            return;
        };
        *pixel = u32::from_le_bytes([color.b, color.g, color.r, 0]);
    }

    fn blend(&mut self, x: usize, y: usize, color: Color, alphas: Color) {
        let Some(pixel) = self.pixels.get_mut(y * self.width + x) else {
            return;
        };

        let [src_b, src_g, src_r, _] = pixel.to_le_bytes();

        let r = (color.r as u16 * alphas.r as u16 + src_r as u16 * (u8::MAX - alphas.r) as u16)
            / u8::MAX as u16;
        let g = (color.g as u16 * alphas.g as u16 + src_g as u16 * (u8::MAX - alphas.g) as u16)
            / u8::MAX as u16;
        let b = (color.b as u16 * alphas.b as u16 + src_b as u16 * (u8::MAX - alphas.b) as u16)
            / u8::MAX as u16;

        *pixel = u32::from_le_bytes([b as u8, g as u8, r as u8, 0]);
    }
}

fn draw_at(canvas: &mut Canvas, rect: &Rect, offset: Offset, view: softbuffer::Rect) {
    let Rect {
        size,
        children,
        fill,
        on_click,
        radius,
        ..
    } = &*rect;

    let y_start = (view.y as usize).saturating_sub(offset.y);
    let x_start = (view.x as usize).saturating_sub(offset.x);
    let y_end = rect
        .size
        .height
        .min(((view.y + view.height.get()) as usize).saturating_sub(offset.y));
    let x_end = rect
        .size
        .width
        .min(((view.x + view.width.get()) as usize).saturating_sub(offset.x));

    let sdf = |x: usize, y: usize| {
        if *radius == 0.0 {
            return Color::gray(255);
        }

        let x = x as f32 + 0.5;
        let y = y as f32 + 0.5;
        let center_x = size.width as f32 / 2.0;
        let center_y = size.height as f32 / 2.0;
        let towards_corner_x = (x - center_x).abs();
        let towards_corner_y = (y - center_y).abs();
        let shrunk_corner_x = center_x - radius;
        let shrunk_corner_y = center_y - radius;
        let to_shrunk_corner_x = towards_corner_x - shrunk_corner_x;
        let to_shrunk_corner_y = towards_corner_y - shrunk_corner_y;
        let pixel_to_corner_x = to_shrunk_corner_x.max(0.0);
        let pixel_to_corner_y = to_shrunk_corner_y.max(0.0);
        let dist = (pixel_to_corner_x * pixel_to_corner_x + pixel_to_corner_y * pixel_to_corner_y)
            .sqrt()
            - radius;
        let dist_to_edge = to_shrunk_corner_x.max(to_shrunk_corner_y) - radius;

        let d = if pixel_to_corner_x > 0.0 && pixel_to_corner_y > 0.0 {
            dist
        } else {
            dist_to_edge
        };

        Color::gray((((-d).clamp(-0.5, 0.5) + 0.5) * 255.0) as u8)
    };

    match fill {
        Fill::Color(color) => {
            for y in y_start..y_end {
                for x in x_start..x_end {
                    canvas.blend(offset.x + x, offset.y + y, *color, sdf(x, y));
                }
            }
        }
        Fill::Glyph(glyph) => {
            for y in y_start..y_end {
                for x in x_start..x_end {
                    canvas.blend(
                        offset.x + x,
                        offset.y + y,
                        Color {
                            r: 255,
                            g: 255,
                            b: 255,
                        },
                        sdf(x, y)
                            * Color {
                                r: glyph[(y * size.width + x) * 4 + 2],
                                g: glyph[(y * size.width + x) * 4 + 1],
                                b: glyph[(y * size.width + x) * 4],
                            },
                    );
                }
            }
        }
        Fill::None => {}
    }

    for child in children {
        draw_at(
            canvas,
            &child.rect,
            Offset {
                x: offset.x + child.position.x,
                y: offset.y + child.position.y,
            },
            view,
        );
    }
}

fn draw(canvas: &mut Canvas, rect: &Rect, view: softbuffer::Rect) {
    draw_at(canvas, rect, Offset { x: 0, y: 0 }, view);
}

// pub struct Shared<T> {
//     inner: Rc<RefCell<T>>,
// }

// impl<T> Clone for Shared<T> {
//     fn clone(&self) -> Self {
//         Self {
//             inner: self.inner.clone(),
//         }
//     }
// }

// pub struct Task<T, R> {
//     shared: Shared<T>,
//     on_done: Box<dyn FnOnce(&mut T, R) + 'static>,
//     proxy: EventLoopProxy<T>,
// }

// unsafe impl<T: Send> Send for Task<T> {}

// impl<T> Task<T> {
//     pub fn done(self, value: T) {}
// }

// impl<T> Shared<T> {
//     pub fn after_task<R>(self, done: impl FnOnce(R) + 'static) {}
// }

struct App<T> {
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    create_fragment: fn(&T) -> Rect,
    fragment: Option<Rect<'static>>,
    state: Box<T>,
    cursor: Offset,
    size: Size,
    modifiers: ModifiersState,
    framerate_sum: u64,
    framerate_count: u64,
}

impl<T: 'static> App<T> {
    pub fn new(init_state: T, create_fragment: fn(&T) -> Rect) -> Self {
        let state = Box::new(init_state);
        // let fragment = create_fragment(&state);
        // let fragment = unsafe { mem::transmute::<Rect<'_>, Rect<'static>>(fragment) };

        Self {
            state,
            create_fragment,
            fragment: None,
            surface: None,
            modifiers: ModifiersState::empty(),
            cursor: Offset { x: 0, y: 0 },
            size: Size {
                height: 0,
                width: 0,
            },
            framerate_count: 0,
            framerate_sum: 0,
        }
    }

    fn create_fragment(&mut self, bounds: Size) -> (Option<Rect>, &Rect) {
        let mut fragment = unsafe {
            mem::transmute::<Rect<'_>, Rect<'static>>((self.create_fragment)(&self.state))
        };

        fragment.do_layout.unwrap_or(default_layout)(&mut fragment);

        let old = self.fragment.replace(fragment);
        (old, self.fragment.as_mut().unwrap())
    }
}

impl<T: 'static> winit::application::ApplicationHandler for App<T> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Rc::new(
            event_loop
                .create_window(WindowAttributes::default())
                .unwrap(),
        );
        let ctx = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&ctx, window.clone()).unwrap();
        self.surface = Some(surface);
    }

    fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, event: ()) {}

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                let start = Instant::now();
                let size = self.surface.as_mut().unwrap().window().inner_size();
                let (old, mut rect) = self.create_fragment(Size {
                    width: size.width as usize,
                    height: size.height as usize,
                });

                let mut regions = old.map(|old| diff(&old, rect, Offset { x: 0, y: 0 }));
                // dbg!(&regions);

                let surface = self.surface.as_mut().unwrap();
                if size.width != self.size.width as u32 || size.height != self.size.height as u32 {
                    surface
                        .resize(
                            NonZero::new(size.width).unwrap(),
                            NonZero::new(size.height).unwrap(),
                        )
                        .unwrap();
                    self.size.width = size.width as usize;
                    self.size.height = size.height as usize;
                    regions = None;
                }

                for region in regions.unwrap_or_else(|| {
                    vec![softbuffer::Rect {
                        width: NonZero::new(size.width).unwrap(),
                        height: NonZero::new(size.height).unwrap(),
                        x: 0,
                        y: 0,
                    }]
                }) {
                    draw(
                        &mut Canvas {
                            pixels: &mut surface.buffer_mut().unwrap(),
                            width: size.width as usize,
                            height: size.height as usize,
                        },
                        self.fragment.as_ref().unwrap(),
                        region,
                    );
                }
                self.framerate_sum += start.elapsed().as_secs_f64().recip().round() as u64;
                self.framerate_count += 1;

                surface.buffer_mut().unwrap().present().unwrap();
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::CursorMoved {
                device_id,
                position,
            } => {
                self.cursor = Offset {
                    x: position.x as usize,
                    y: position.y as usize,
                }
            }
            WindowEvent::MouseInput {
                device_id,
                state,
                button,
            } => {
                if state.is_pressed() {
                    if self.fragment.as_ref().unwrap().click(self.cursor) {
                        self.surface.as_ref().unwrap().window().request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    if self
                        .fragment
                        .as_ref()
                        .unwrap()
                        .key_press(&event.logical_key, self.modifiers)
                    {
                        self.surface.as_ref().unwrap().window().request_redraw();
                    }
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _: &winit::event_loop::ActiveEventLoop) {
        println!("{} Hz", self.framerate_sum / self.framerate_count);
    }
}

pub fn run<T: 'static>(init_state: T, create_root: fn(&T) -> Rect) {
    let event_loop = EventLoop::new().unwrap();
    event_loop.create_proxy();
    event_loop
        .run_app(&mut App::new(init_state, create_root))
        .unwrap();
}

thread_local! {
    static TEXT_RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
}

pub struct TextRenderer {
    shape_context: swash::shape::ShapeContext,
    scale_context: swash::scale::ScaleContext,
    glyph_cache: HashMap<GlyphKey, CachedGlyph>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    glyph_id: u16,
    x_sub: SubpixelPosition,
    y_sub: SubpixelPosition,
}

#[derive(Clone, Copy)]
pub struct CachedGlyph {
    left: i32,
    top: i32,
    size: Size,
    pixels: &'static [u8],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SubpixelPosition {
    inner: u16,
}

impl SubpixelPosition {
    const MUL: f32 = 10.0;

    fn new(value: f32) -> Self {
        Self {
            inner: (value * Self::MUL).round() as u16,
        }
    }

    fn as_f32(self) -> f32 {
        self.inner as f32 / Self::MUL
    }
}

fn split(num: f32) -> (i32, SubpixelPosition) {
    let fract = num.fract();
    let whole = (num - fract) as i32;
    (whole, SubpixelPosition::new(fract))
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            shape_context: swash::shape::ShapeContext::new(),
            scale_context: swash::scale::ScaleContext::new(),
            glyph_cache: HashMap::new(),
        }
    }

    pub fn get_glyphs(&mut self, text: &str) -> Vec<CachedGlyph> {
        let size = 60.0;
        let font = swash::FontRef::from_index(include_bytes!("../ARIAL.TTF"), 0).unwrap();

        let mut shaper = self.shape_context.builder(font).size(size).build();

        let mut scaler = self
            .scale_context
            .builder(font)
            .size(size)
            .hint(true)
            .build();

        shaper.add_str(text);

        let mut glyphs = vec![];

        let mut advance: f32 = 0.0;
        let mut glyph_idx = 0;

        shaper.shape_with(|cluster| {
            // let start = glyph_idx;
            for glyph in cluster.glyphs {
                glyph_idx += 1;
                let (mut cached_glyph, x, y) = 'glyph: {
                    let (x, x_sub) = split(glyph.x + advance);
                    let (y, y_sub) = split(glyph.y);

                    advance += glyph.advance;

                    let key = GlyphKey {
                        glyph_id: glyph.id,
                        x_sub,
                        y_sub,
                    };

                    if let Some(cached_glyph) = self.glyph_cache.get(&key) {
                        break 'glyph (*cached_glyph, x, y);
                    }

                    use swash::zeno::{Format, Vector};
                    let offset = Vector::new(x_sub.as_f32(), y_sub.as_f32());

                    let Some(image) = swash::scale::Render::new(&[swash::scale::Source::Outline])
                        .format(Format::Subpixel)
                        .offset(offset)
                        .render(&mut scaler, glyph.id)
                    else {
                        panic!("No glyph");
                    };

                    let mut cached_glyph = CachedGlyph {
                        left: image.placement.left,
                        top: image.placement.top,
                        size: Size {
                            width: image.placement.width as usize,
                            height: image.placement.height as usize,
                        },
                        pixels: Box::leak(image.data.into_boxed_slice()),
                    };

                    self.glyph_cache.insert(key, cached_glyph);

                    (cached_glyph, x, y)
                };
                cached_glyph.top += y;
                cached_glyph.left += x;
                glyphs.push(cached_glyph);
            }

            // cached.units.push(static_ui::TextUnit {
            //     glyph_start: start,
            //     glyph_end: glyph_idx,
            //     byte_start: cluster.source.start as usize,
            //     byte_end: cluster.source.end as usize,
            //     advance,
            // })
        });

        // cached.width = advance;
        glyphs
    }
}

pub fn text(text: &str) -> Rect<'static> {
    let glyphs = TEXT_RENDERER.with_borrow_mut(|text_renderer| text_renderer.get_glyphs(text));

    let right = glyphs
        .iter()
        .map(|glyph| glyph.left + glyph.size.width as i32)
        .max()
        .unwrap_or(0);

    let left = glyphs.iter().map(|glyph| glyph.left).min().unwrap_or(0);

    let top = glyphs.iter().map(|glyph| -glyph.top).min().unwrap_or(0);

    let bottom = glyphs
        .iter()
        .map(|glyph| glyph.size.height as i32 - glyph.top)
        .max()
        .unwrap_or(0);

    let children: Vec<_> = glyphs
        .into_iter()
        .map(|glyph| Child {
            rect: Rect {
                children: vec![],
                fill: Fill::Glyph(glyph.pixels),
                on_click: None,
                on_key_pressed: None,
                size: glyph.size,
                radius: 0.0,
                do_layout: None,
            },
            position: Offset {
                x: (glyph.left - left) as usize,
                y: -(top + glyph.top) as usize,
            },
        })
        .collect();

    Rect {
        size: Size {
            width: (right - left) as usize,
            height: (bottom - top) as usize,
        },
        radius: 0.0,
        children,
        fill: Fill::None,
        on_click: None,
        on_key_pressed: None,
        do_layout: None,
    }
}

pub fn rows(children: Vec<Rect>) -> Rect {
    let mut y = 0;
    let mut width = 0;

    let children = children
        .into_iter()
        .map(|rect| Child {
            position: Offset {
                x: 0,
                y: {
                    let this_y = y;
                    y += rect.size.height;
                    width = width.max(rect.size.width);
                    this_y
                },
            },
            rect,
        })
        .collect();

    Rect {
        size: Size { width, height: y },
        children,
        fill: Fill::None,
        radius: 0.0,
        on_click: None,
        on_key_pressed: None,
        do_layout: None,
    }
}

pub fn default_layout(rect: &mut Rect) {
    for child in &mut rect.children {
        if let Some(do_layout) = child.rect.do_layout {
            do_layout(&mut child.rect);
        } else {
            default_layout(&mut child.rect);
        }
    }
}

pub fn center(rect: &mut Rect) {
    for child in &mut rect.children {
        if let Some(do_layout) = child.rect.do_layout {
            do_layout(&mut child.rect);
        }

        child.position = Offset {
            x: (rect.size.width - child.rect.size.width) / 2,
            y: (rect.size.height - child.rect.size.height) / 2,
        };
    }
}

// thread_local! {

// }

pub fn async_work<R: Send>(
    work: impl FnOnce() -> R + Send + 'static,
    done: impl FnOnce(R) + 'static,
) {
}

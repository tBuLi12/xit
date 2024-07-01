use std::alloc;
use std::any::Any;
use std::cell::RefCell;
use std::ops::Deref;
use std::ptr::{addr_of, addr_of_mut};
use std::{collections::HashMap, marker::PhantomData, mem};

pub struct App {
    root: Component,
    rects: HashMap<PrimitiveID, RawRect>,
}

impl App {
    pub fn click(&mut self, x: f32, y: f32, rt: &mut impl Runtime) {
        for element in &self.root.elements {
            click_element(x, y, element, rt);
        }

        UPDATES.with_borrow_mut(|updates| {
            println!("{:?}", updates);
            updates.clear();
        });
    }

    pub fn new(root: impl Fn(&mut Ctx), rt: &mut impl Runtime) -> Self {
        let mut ctx = Ctx {
            elements: vec![],
            runtime: rt,
            owned_signals: vec![],
            rects: HashMap::new(),
        };
        root(&mut ctx);

        Self {
            root: Component {
                owned_signals: ctx.owned_signals,
                elements: ctx.elements,
            },
            rects: ctx.rects,
        }
    }
}
fn click_element(x: f32, y: f32, element: &Element, rt: &mut impl Runtime) {
    match element {
        Element::Rect {
            raw,
            handler,
            children,
        } => {
            if x > raw.x && x < raw.x + raw.width && y > raw.y && y < raw.y + raw.height {
                (handler)();
            }

            for child in children {
                click_element(x, y, child, rt);
            }
        }
        Element::Text(raw) => {}
        Element::Component(component) => {
            for child in &component.elements {
                click_element(x, y, child, rt);
            }
        }
    }
}

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PrimitiveID(u32);

pub trait Runtime {
    fn next_text_id(&mut self) -> PrimitiveID;
    fn next_rect_id(&mut self) -> PrimitiveID;
    fn create_rect(&mut self, rect: RawRect);
    fn create_text(&mut self, text: RawText);
    fn update_rect(&mut self, id: PrimitiveID, rect: RawRect);
    fn update_text(&mut self, id: PrimitiveID, text: RawText);
    fn delete_rect(&mut self, id: PrimitiveID);
    fn delete_text(&mut self, id: PrimitiveID);
}

pub struct TestRuntime {
    next_rect_id: u32,
    next_text_id: u32,
}

impl TestRuntime {
    pub fn new() -> Self {
        Self {
            next_rect_id: 0,
            next_text_id: 0,
        }
    }
}

impl Runtime for TestRuntime {
    fn next_rect_id(&mut self) -> PrimitiveID {
        let id = self.next_rect_id;
        self.next_rect_id += 1;
        PrimitiveID(id)
    }

    fn next_text_id(&mut self) -> PrimitiveID {
        let id = self.next_text_id;
        self.next_text_id += 1;
        PrimitiveID(id)
    }

    fn create_rect(&mut self, rect: RawRect) {
        println!("create_rect: {:?}", rect);
    }

    fn create_text(&mut self, text: RawText) {
        println!("create_text: {:?}", text);
    }

    fn update_rect(&mut self, id: PrimitiveID, rect: RawRect) {
        println!("update_rect: {:?}", rect);
    }

    fn update_text(&mut self, id: PrimitiveID, text: RawText) {
        println!("update_text: {:?}", text);
    }

    fn delete_rect(&mut self, id: PrimitiveID) {
        println!("delete_rect: {:?}", id);
    }

    fn delete_text(&mut self, id: PrimitiveID) {
        println!("delete_text: {:?}", id);
    }
}

pub struct Ctx<'r> {
    runtime: &'r mut dyn Runtime,
    elements: Vec<Element>,
    owned_signals: Vec<Box<dyn Any>>,
    rects: HashMap<PrimitiveID, RawRect>,
}

impl<'r> Ctx<'r> {
    fn signal<T: 'static>(&mut self, value: T) -> Signal<T> {
        let owned = OwnedSignal::new(value);
        let signal = owned.get_signal();
        self.owned_signals.push(Box::new(owned));
        signal
    }

    fn rect<C: Fn(&mut Ctx) + Copy, H: Fn() + Copy + 'static>(&mut self, rect: RectElem<C, H>) {
        let mut child_ctx = Ctx {
            elements: vec![],
            runtime: self.runtime,
            owned_signals: mem::take(&mut self.owned_signals),
            rects: mem::take(&mut self.rects),
        };
        (rect.children)(&mut child_ctx);
        let children = child_ctx.elements;
        self.owned_signals = child_ctx.owned_signals;
        self.rects = child_ctx.rects;

        let rect_id = self.runtime.next_rect_id();
        let raw = RawRect {
            x: rect.x.get(rect_id),
            y: rect.y.get(rect_id),
            width: rect.width.get(rect_id),
            height: rect.height.get(rect_id),
            corner_radius: rect.corner_radius.get(rect_id),
            border_width: rect.border_width.get(rect_id),
        };
        self.rects.insert(rect_id, rect);

        self.runtime.create_rect(raw);

        self.elements.push(Element::Rect {
            raw,
            handler: Box::new(rect.handler),
            children,
        });
    }

    fn text(&mut self, raw: RawText) {
        self.elements.push(Element::Text(raw.clone()));
        self.runtime.create_text(raw);
    }
}

struct Component {
    owned_signals: Vec<Box<dyn Any>>,
    elements: Vec<Element>,
}

enum Element {
    Rect {
        raw: RawRect,
        handler: Box<dyn Fn()>,
        children: Vec<Element>,
    },
    Text(RawText),
    Component(Component),
}

struct RectElem<C: Copy, H: Copy> {
    x: Signal<f32>,
    y: Signal<f32>,
    width: Signal<f32>,
    height: Signal<f32>,
    corner_radius: Signal<f32>,
    border_width: Signal<f32>,
    children: C,
    handler: H,
}

#[derive(Copy, Clone)]
struct Signal<T>(GenRef<RefCell<SignalValue<T>>>);

impl<T: Copy> Signal<T> {
    fn get(&self, primitive_id: PrimitiveID) -> T {
        let gen_ref = self.0.get();
        let mut inner = gen_ref.borrow_mut();
        if !inner.dependents.contains(&primitive_id) {
            inner.dependents.push(primitive_id);
        }
        inner.value
    }

    fn update(self, fun: impl Fn(T) -> T) {}

    fn set(self, value: T) {
        UPDATES.with_borrow_mut(|updates| {
            updates.extend(self.0.get().borrow().dependents.iter().copied());
        });
        self.0.get().borrow_mut().value = value;
        // updates
    }
}

struct OwnedSignal<T>(OwnedGenRef<RefCell<SignalValue<T>>>);

impl<T> OwnedSignal<T> {
    fn new(value: T) -> Self {
        let owned = OwnedGenRef::new(RefCell::new(SignalValue {
            value,
            dependents: vec![],
        }));

        Self(owned)
    }

    fn get_signal(&self) -> Signal<T> {
        Signal(self.0.get_ref())
    }
}

struct SignalValue<T> {
    value: T,
    dependents: Vec<PrimitiveID>,
}

struct GenRef<T> {
    ptr: *mut (u64, bool, T),
    gen: u64,
}

struct GenRefGuard<'a, T>(&'a GenRef<T>);

impl<'a, T> Deref for GenRefGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*addr_of!((*self.0.ptr).2) }
    }
}

impl<'a, T> Drop for GenRefGuard<'a, T> {
    fn drop(&mut self) {
        unsafe { addr_of_mut!((*self.0.ptr).1).write(false) };
    }
}

impl<T> GenRef<T> {
    fn get(&self) -> GenRefGuard<'_, T> {
        let current = unsafe { *addr_of!((*self.ptr).0) };
        if current == self.gen {
            unsafe { addr_of_mut!((*self.ptr).1).write(true) };
            GenRefGuard(self)
        } else {
            panic!("GenRef is out of date");
        }
    }
}

impl<T> Copy for GenRef<T> {}
impl<T> Clone for GenRef<T> {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            gen: self.gen,
        }
    }
}

struct OwnedGenRef<T> {
    ptr: *mut (u64, bool, T),
    _marker: PhantomData<T>,
}

thread_local! {
    static GEN_REF_STORAGE: RefCell<GenRefStorage> = RefCell::new(GenRefStorage {
        pools: HashMap::new(),
    });
    static UPDATES: RefCell<Vec<PrimitiveID>> = RefCell::new(vec![]);
}

impl<T> OwnedGenRef<T> {
    fn new(value: T) -> Self {
        let ptr = GEN_REF_STORAGE.with_borrow_mut(|storage| storage.alloc::<T>());
        unsafe {
            addr_of_mut!((*ptr).2).write(value);
        };

        OwnedGenRef {
            ptr,
            _marker: PhantomData,
        }
    }

    fn get_ref(&self) -> GenRef<T> {
        unsafe {
            GenRef {
                ptr: self.ptr,
                gen: *addr_of!((*self.ptr).0),
            }
        }
    }
}

impl<T> Drop for OwnedGenRef<T> {
    fn drop(&mut self) {
        if unsafe { *addr_of!((*self.ptr).1) } {
            panic!("OwnedGenRef is still in use");
        }

        GEN_REF_STORAGE.with_borrow_mut(|storage| storage.free(self.ptr));
    }
}

struct GenRefStorage {
    pools: HashMap<alloc::Layout, Vec<*mut u8>>,
}

impl GenRefStorage {
    fn alloc<T>(&mut self) -> *mut (u64, bool, T) {
        let layout = alloc::Layout::new::<(u64, bool, T)>();
        if let Some(pool) = self.pools.get_mut(&layout) {
            if let Some(ptr) = pool.pop() {
                return ptr as *mut (u64, bool, T);
            }
        }

        unsafe {
            let ptr = alloc::alloc(layout) as *mut (u64, bool, T);
            addr_of_mut!((*ptr).0).write(0);
            addr_of_mut!((*ptr).1).write(false);
            ptr
        }
    }

    fn free<T>(&mut self, ptr: *mut (u64, bool, T)) {
        let layout = alloc::Layout::new::<(u64, bool, T)>();
        unsafe {
            let count = &mut *addr_of_mut!((*ptr).0);
            if *count == u64::MAX {
                return;
            }

            *count += 1;
            self.pools.entry(layout).or_default().push(ptr as *mut u8);
        };
    }
}

pub fn draw_example(ctx: &mut Ctx) {
    let h = ctx.signal(100.0);

    let rect = RectElem {
        x: ctx.signal(100.0),
        y: ctx.signal(100.0),
        width: ctx.signal(100.0),
        height: h,
        corner_radius: ctx.signal(0.0),
        border_width: ctx.signal(0.0),
        children: |ctx: &mut Ctx| {
            ctx.text(RawText {
                text: "Hello World".to_string(),
                x: 100.0,
                y: 100.0,
                width: 100.0,
                height: 100.0,
            });
            ctx.text(RawText {
                text: "Hello World2".to_string(),
                x: 100.0,
                y: 300.0,
                width: 100.0,
                height: 100.0,
            });
        },
        handler: move || h.set(200.0),
    };

    ctx.rect(rect);
}

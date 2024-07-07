use std::{
    alloc,
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    ops::Deref,
    ptr::{addr_of, addr_of_mut},
};

pub struct GenRef<T> {
    ptr: *mut (u64, bool, T),
    gen: u64,
}

pub struct GenRefGuard<'a, T>(&'a GenRef<T>);

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
        let Some(guard) = self.try_get() else {
            panic!("GenRef is out of date");
        };

        guard
    }

    fn try_get(&self) -> Option<GenRefGuard<'_, T>> {
        let current = unsafe { *addr_of!((*self.ptr).0) };
        if current == self.gen {
            unsafe { addr_of_mut!((*self.ptr).1).write(true) };
            Some(GenRefGuard(self))
        } else {
            None
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

pub struct GenBox<T> {
    ptr: *mut (u64, bool, T),
    _marker: PhantomData<T>,
}

thread_local! {
    static GEN_REF_STORAGE: RefCell<GenRefStorage> = RefCell::new(GenRefStorage {
        pools: HashMap::new(),
    });
}

impl<T> GenBox<T> {
    pub fn new(value: T) -> Self {
        let ptr = GEN_REF_STORAGE.with_borrow_mut(|storage| storage.alloc::<T>());
        unsafe {
            addr_of_mut!((*ptr).2).write(value);
        };

        GenBox {
            ptr,
            _marker: PhantomData,
        }
    }

    pub fn get_ref(&self) -> GenRef<T> {
        unsafe {
            GenRef {
                ptr: self.ptr,
                gen: *addr_of!((*self.ptr).0),
            }
        }
    }
}

impl<T> Drop for GenBox<T> {
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

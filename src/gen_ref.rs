use std::{
    alloc,
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::{addr_of, addr_of_mut},
};

#[derive(Debug)]
pub struct GenRef<T: ?Sized> {
    ptr: *mut GenValue<T>,
    gen: u64,
}

pub struct GenRefGuard<T>(GenRef<T>);

impl<T> Deref for GenRefGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*addr_of!((*self.0.ptr).inner.value) }
    }
}

impl<T> Drop for GenRefGuard<T> {
    fn drop(&mut self) {
        unsafe { *addr_of_mut!((*self.0.ptr).inner.borrows) -= 1 };
    }
}

pub struct GenRefMutGuard<T>(GenRef<T>);

impl<T> Deref for GenRefMutGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*addr_of!((*self.0.ptr).inner.value) }
    }
}

impl<T> DerefMut for GenRefMutGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *addr_of_mut!((*self.0.ptr).inner.value) }
    }
}

impl<T> Drop for GenRefMutGuard<T> {
    fn drop(&mut self) {
        unsafe { *addr_of_mut!((*self.0.ptr).inner.mut_borrow) = false };
    }
}

impl<T> GenRef<T> {
    pub fn get(self) -> GenRefGuard<T> {
        let Some(guard) = self.try_get() else {
            panic!("GenRef is out of date");
        };

        guard
    }

    pub fn try_get(self) -> Option<GenRefGuard<T>> {
        let current = unsafe { *addr_of!((*self.ptr).gen) };
        if current == self.gen && !unsafe { *addr_of!((*self.ptr).inner.mut_borrow) } {
            unsafe { *addr_of_mut!((*self.ptr).inner.borrows) += 1 };
            Some(GenRefGuard(self))
        } else {
            None
        }
    }

    pub fn get_mut(self) -> GenRefMutGuard<T> {
        let Some(guard) = self.try_get_mut() else {
            panic!("GenRef is out of date");
        };

        guard
    }

    pub fn try_get_mut(self) -> Option<GenRefMutGuard<T>> {
        let current = unsafe { *addr_of!((*self.ptr).gen) };
        if current == self.gen
            && unsafe {
                *addr_of!((*self.ptr).inner.borrows) == 0
                    && !*addr_of!((*self.ptr).inner.mut_borrow)
            }
        {
            unsafe { *addr_of_mut!((*self.ptr).inner.mut_borrow) = true };
            Some(GenRefMutGuard(self))
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

#[derive(Debug)]
pub struct GenBox<T> {
    ptr: *mut GenValue<T>,
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
            addr_of_mut!((*ptr).inner.value).write(value);
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
                gen: *addr_of!((*self.ptr).gen),
            }
        }
    }
}

impl<T> Drop for GenBox<T> {
    fn drop(&mut self) {
        if unsafe { *addr_of!((*self.ptr).inner.borrows) } > 0
            || unsafe { *addr_of!((*self.ptr).inner.mut_borrow) }
        {
            panic!("OwnedGenRef is still in use");
        }

        unsafe {
            addr_of_mut!((*self.ptr).inner.value).drop_in_place();
        }

        GEN_REF_STORAGE.with_borrow_mut(|storage| storage.free(self.ptr));
    }
}

#[repr(C)]
struct GenValue<T: ?Sized> {
    gen: u64,
    inner: Value<T>,
}

struct Value<T: ?Sized> {
    borrows: u32,
    mut_borrow: bool,
    value: T,
}

struct GenRefStorage {
    pools: HashMap<alloc::Layout, Vec<*mut u8>>,
}

impl GenRefStorage {
    fn alloc<T>(&mut self) -> *mut GenValue<T> {
        let layout = alloc::Layout::new::<GenValue<T>>();
        if let Some(pool) = self.pools.get_mut(&layout) {
            if let Some(ptr) = pool.pop() {
                return ptr as *mut GenValue<T>;
            }
        }

        unsafe {
            let ptr = alloc::alloc(layout) as *mut GenValue<T>;
            addr_of_mut!((*ptr).gen).write(0);
            addr_of_mut!((*ptr).inner.borrows).write(0);
            addr_of_mut!((*ptr).inner.mut_borrow).write(false);
            ptr
        }
    }

    fn free<T>(&mut self, ptr: *mut GenValue<T>) {
        let layout = alloc::Layout::new::<GenValue<T>>();
        unsafe {
            let gen = &mut *addr_of_mut!((*ptr).gen);
            if *gen == u64::MAX {
                return;
            }

            *gen += 1;
            self.pools.entry(layout).or_default().push(ptr as *mut u8);
        };
    }
}

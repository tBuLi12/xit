use std::{
    alloc,
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::{addr_of, addr_of_mut},
};

pub struct GenRef<T> {
    ptr: *mut GenValue<T>,
    gen: u64,
}

pub struct GenRefGuard<T>(GenRef<T>);

impl<T> Deref for GenRefGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*addr_of!((*self.0.ptr).value) }
    }
}

impl<T> Drop for GenRefGuard<T> {
    fn drop(&mut self) {
        unsafe { *addr_of_mut!((*self.0.ptr).borrows) -= 1 };
    }
}

pub struct GenRefMutGuard<T>(GenRef<T>);

impl<T> Deref for GenRefMutGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*addr_of!((*self.0.ptr).value) }
    }
}

impl<T> DerefMut for GenRefMutGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *addr_of_mut!((*self.0.ptr).value) }
    }
}

impl<T> Drop for GenRefMutGuard<T> {
    fn drop(&mut self) {
        unsafe { *addr_of_mut!((*self.0.ptr).mut_borrows) -= 1 };
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
        if current == self.gen && unsafe { *addr_of!((*self.ptr).mut_borrows) } == 0 {
            unsafe { *addr_of_mut!((*self.ptr).borrows) += 1 };
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
                *addr_of!((*self.ptr).borrows) == 0 && *addr_of!((*self.ptr).mut_borrows) == 0
            }
        {
            unsafe { *addr_of_mut!((*self.ptr).mut_borrows) += 1 };
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
            addr_of_mut!((*ptr).value).write(value);
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
        if unsafe { *addr_of!((*self.ptr).borrows) } > 0
            || unsafe { *addr_of!((*self.ptr).mut_borrows) } > 0
        {
            panic!("OwnedGenRef is still in use");
        }

        GEN_REF_STORAGE.with_borrow_mut(|storage| storage.free(self.ptr));
    }
}

struct GenValue<T> {
    gen: u64,
    borrows: u32,
    mut_borrows: u32,
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
            addr_of_mut!((*ptr).borrows).write(0);
            addr_of_mut!((*ptr).mut_borrows).write(0);
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

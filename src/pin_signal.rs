use std::{cell::RefCell, marker::PhantomPinned, pin::Pin};

#[derive(Debug)]
pub struct Signal<T> {
    value: RefCell<T>,
    next: RefCell<Vec<*const Signal<T>>>,
    prev: RefCell<Vec<*const Signal<T>>>,
    _marker: PhantomPinned,
}

impl<T: Clone> Signal<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: RefCell::new(value),
            next: RefCell::new(vec![]),
            prev: RefCell::new(vec![]),
            _marker: PhantomPinned,
        }
    }

    pub fn link(self: Pin<&Self>, other: Pin<&Self>) {
        let this = self.get_ref();
        let other = other.get_ref();

        if this as *const _ == other as *const _ {
            panic!("Cannot link a signal to itself");
        }

        this.next.borrow_mut().push(other);
        other.prev.borrow_mut().push(this);

        other.value.borrow_mut().clone_from(&this.value.borrow());
    }

    pub fn set(&self, value: T) {
        *self.value.borrow_mut() = value.clone();

        for &next in self.next.borrow().iter() {
            let next = unsafe { &*next };
            next.set(value.clone());
        }
    }

    pub fn get(&self) -> T {
        self.value.borrow().clone()
    }
}

impl<T> Drop for Signal<T> {
    fn drop(&mut self) {
        let next = self.next.borrow_mut();
        let prev = self.prev.borrow_mut();

        for &next in next.iter() {
            let next = unsafe { &*next };
            next.prev.borrow_mut().retain(|&prev| prev != self);
        }

        for &prev in prev.iter() {
            let prev = unsafe { &*prev };
            prev.next.borrow_mut().retain(|&next| next != self);
        }
    }
}

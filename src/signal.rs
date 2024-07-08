use std::{
    cell::{self, RefCell},
    ops::{Deref, DerefMut},
};

use crate::gen_ref::{GenBox, GenRef, GenRefGuard, GenRefMutGuard};

pub struct Signal<T>(GenRef<SignalValue<T>>);

impl<T> Copy for Signal<T> {}
impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<T: Copy> Signal<T> {
    pub fn get(self) -> T {
        let this = self.0.get();
        this.value
    }

    // fn update(self, fun: impl Fn(T) -> T) {}
}

impl<T> Signal<T> {
    pub fn set(self, value: T) {
        let mut this = self.0.get_mut();
        let this = &mut *this;
        this.value = value;

        for dependent in &mut this.callbacks {
            (dependent)(&this.value);
        }
    }

    pub fn borrow(self) -> SignalGuard<T> {
        SignalGuard(self.0.get())
    }

    pub fn borrow_mut(self) -> SignalMutGuard<T> {
        SignalMutGuard(self.0.get_mut())
    }

    pub fn subscribe(self, callback: impl FnMut(&T) + 'static) {
        self.0.get_mut().callbacks.push(Box::new(callback));
    }
}

pub struct SignalGuard<T>(GenRefGuard<SignalValue<T>>);

impl<T> Deref for SignalGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0.value
    }
}

pub struct SignalMutGuard<T>(GenRefMutGuard<SignalValue<T>>);

impl<T> Deref for SignalMutGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0.value
    }
}

impl<T> DerefMut for SignalMutGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.value
    }
}

pub struct OwnedSignal<T>(GenBox<SignalValue<T>>);

impl<T> OwnedSignal<T> {
    pub fn new(value: T) -> Self {
        let owned = GenBox::new(SignalValue {
            value,
            callbacks: vec![],
        });

        Self(owned)
    }

    pub fn get_signal(&self) -> Signal<T> {
        Signal(self.0.get_ref())
    }
}

struct SignalValue<T> {
    value: T,
    callbacks: Vec<Box<dyn FnMut(&T)>>,
}

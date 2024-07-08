use std::{
    cell::Cell,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use crate::gen_ref::{GenBox, GenRef, GenRefGuard, GenRefMutGuard};

#[derive(Debug)]
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
}

impl<T> Signal<T> {
    pub fn with<R>(self, fun: impl FnOnce(&T) -> R) -> R {
        let this = self.0.get();
        fun(&this.value)
    }

    pub fn with_mut<R>(self, fun: impl FnOnce(&mut T) -> R) -> R {
        let mut this = self.0.get_mut();
        fun(&mut this.value)
    }
}

impl<T: 'static> Signal<T> {
    pub fn set(self, new_value: T) {
        self.update(|value| *value = new_value);
    }

    pub fn update(self, fun: impl FnOnce(&mut T)) {
        if !self.try_update(fun) {
            panic!("Cannot set signal after it has been dropped");
        }
    }

    fn try_update(self, fun: impl FnOnce(&mut T)) -> bool {
        let Some(mut this) = self.0.try_get_mut() else {
            return false;
        };
        let this = &mut *this;
        (fun)(&mut this.value);
        this.dirty.set(true);

        this.callbacks.retain(|callback| callback(&this.value));

        true
    }

    pub fn borrow(self) -> SignalGuard<T> {
        let this = self.0.get();
        this.dirty.set(false);
        SignalGuard(this)
    }

    pub fn borrow_mut(self) -> SignalMutGuard<T> {
        let this = self.0.get_mut();
        this.dirty.set(false);
        SignalMutGuard(this)
    }

    pub fn derived<R: 'static>(self, fun: fn(&T) -> R) -> OwnedSignal<R> {
        let derived = OwnedSignal::new(self.with(fun));
        let derived_signal = derived.get_signal();
        self.0.get_mut().callbacks.push(Box::new(move |new_value| {
            derived_signal.try_update(move |value| *value = fun(new_value))
        }));
        derived
    }

    pub fn is_dirty(&self) -> bool {
        self.0.get().dirty.get()
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

// pub struct DerivedSignal<F, T>(GenBox<DerivedSignalValue<F, T>>);

// impl<T, F> DerivedSignal<F, T> {
//     pub fn new<S>(value: T, fun: F) -> Self
//     where
//         F: Fn(&S) -> T + 'static,
//     {
//         let owned = GenBox::new(DerivedSignalValue {
//             value,
//             dirty: false,
//             callbacks: vec![],
//             fun,
//         });

//         Self(owned)
//     }

//     pub fn get_signal(&self) -> Signal<T> {
//         Signal(self.0.get_ref())
//     }
// }

// struct DerivedSignalValue<F, T> {
//     value: T,
//     fun: F,
//     dirty: bool,
//     callbacks: Vec<(*const u8, *const u8)>,
// }

pub struct OwnedSignal<T>(GenBox<SignalValue<T>>);

impl<T> OwnedSignal<T> {
    pub fn new(value: T) -> Self {
        let owned = GenBox::new(SignalValue {
            value,
            dirty: Cell::new(false),
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
    dirty: Cell<bool>,
    callbacks: Vec<Box<dyn Fn(&T) -> bool>>,
}

impl<T: Debug> Debug for SignalValue<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalValue")
            .field("value", &self.value)
            .field("dirty", &self.dirty.get())
            .finish()
    }
}

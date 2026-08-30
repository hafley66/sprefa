use crate::dog::Dog;
use crate::traits::{Speak, Talk};

pub fn default_call(d: &Dog) {
    d.greet();
}

pub fn dyn_call(s: &dyn Talk) {
    s.chat();
}

pub fn generic_call<T: Talk>(t: T) {
    t.chat();
}

pub fn trait_assoc_call() -> u32 {
    Dog::helper()
}

pub fn bare_trait_call() -> u32 {
    Talk::level()
}

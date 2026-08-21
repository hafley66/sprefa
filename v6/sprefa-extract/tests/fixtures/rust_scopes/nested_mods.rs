// Callables at every inline-mod depth: file root, one mod down, two mods down,
// an impl inside a mod, and a cfg-gated test mod.

fn top_level() {
    helper_a();
}

mod inner {
    fn nested_fn() {
        super::top_level();
    }

    pub mod deeper {
        fn deep_fn() {
            let closure = |x: i32| x + 1;
            closure(1);
        }
    }

    struct Inner;

    impl Inner {
        fn inner_method(&self) {}
    }

    trait Nested {
        fn nested_trait_method(&self);
    }
}

#[cfg(test)]
mod tests {
    fn setup() {}
}

const ROOT_CONST: &str = "root";

mod scoped {
    const MOD_CONST: &str = "scoped";
}

fn helper_a() {}

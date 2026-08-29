fn included_fn() {}
macro_rules! inc_macro { () => { included_fn(); } }

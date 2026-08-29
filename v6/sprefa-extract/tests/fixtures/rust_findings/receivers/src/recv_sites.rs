use crate::recv_alpha::Alpha;

pub fn tuple_pattern_init(a: &Alpha) -> u32 {
    let (ticked, extra) = (a.tick(), 2);
    ticked + extra
}

pub fn let_else_init(a: &Alpha) -> u32 {
    let Some(ticked) = Some(a.tick()) else {
        return 0;
    };
    ticked
}

pub fn shadowed_by_own_init(a: Alpha) -> u32 {
    let a = a.tick();
    a
}

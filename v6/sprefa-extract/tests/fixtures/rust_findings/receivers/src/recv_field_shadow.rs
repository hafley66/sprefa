use crate::recv_alpha::Alpha;

pub struct Frame {
    pub Alpha: u32,
    pub inner: Alpha,
}

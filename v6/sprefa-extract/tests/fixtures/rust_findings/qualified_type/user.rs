use crate::inner;

pub struct Holder {
    pub marker: inner::Marker,
    pub slot: inner::Slot<inner::Marker>,
}

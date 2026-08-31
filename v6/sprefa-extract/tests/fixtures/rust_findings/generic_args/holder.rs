use crate::parts::Carrier;
use crate::parts::Other;
use crate::parts::Payload;
use crate::parts::Plain;

pub struct Holder<T: Carrier<Payload>> {
    pub item: T,
}

pub struct Boxed<T> {
    pub item: T,
}

impl Plain for Boxed<Other> {}

impl Carrier<Payload> for Boxed<Other> {
    fn carry(&self) -> Payload {
        Payload { value: 0 }
    }
}

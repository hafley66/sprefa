mod util;

use crate::util::Helper;

pub fn build() -> Helper {
    Helper::new(4)
}

pub fn width(helper: &Helper) -> u32 {
    helper.size
}

pub fn label() -> String {
    format!("Helper")
}

mod other {
    pub struct Helper;
}

#[cfg(test)]
mod tests {
    use crate::util::Helper as H;

    #[test]
    fn builds() {
        assert_eq!(H::new(2).size, 2);
    }
}

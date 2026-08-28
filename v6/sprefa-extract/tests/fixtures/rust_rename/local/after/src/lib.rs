mod util;

use crate::util::Tool;

pub fn build() -> Tool {
    Tool::new(4)
}

pub fn width(helper: &Tool) -> u32 {
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
    use crate::util::Tool as H;

    #[test]
    fn builds() {
        assert_eq!(H::new(2).size, 2);
    }
}

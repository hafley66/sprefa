use crate::traits::Speak;

pub struct Robot;

impl Speak for Robot {
    fn speak(&self) {}

    fn helper() -> u32 {
        11
    }
}

pub fn robot_volume() -> u32 {
    Robot::helper()
}

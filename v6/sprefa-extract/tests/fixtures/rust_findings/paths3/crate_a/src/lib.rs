mod alpha;
mod gadget;
mod gem;
mod helpers;
mod other;
mod traits_mod;
mod widget;

use crate::gem::Gem;
use crate::helpers as aide;
use helpers::*;

pub fn lib_user(g: &gadget::Gadget) -> u32 {
    let via_glob = util_fn();
    let via_module = helpers::util_fn();
    let via_alias = aide::util_fn();
    let via_crate = crate::helpers::util_fn();
    let mut number = 4;
    std::mem::take(&mut number);
    mem::replace(&mut number, 1);
    let made = Gem::from(3);
    via_glob + via_module + via_alias + via_crate + made.grade + other::other_user(g)
}

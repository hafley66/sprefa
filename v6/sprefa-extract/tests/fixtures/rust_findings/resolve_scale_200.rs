// resolve_scale_200.rs: `--resolve --family call` costs about the cube of the
// number of named defs in one file, so one large generated file can exceed any
// wall budget on its own while a plain parse of it stays in milliseconds.
//
// MEASURED at cec3d5c1d, one file, n defs each called once from one driver fn:
//   n_defs   bytes   resolve ms   edges
//      100    3409           40     100
//      200    7009           54     200
//      400   14209          162     400
//      800   28609         1184     800
//     1600   58609        10488    1600
// 400 -> 800 is 7.3x and 800 -> 1600 is 8.9x for 2x the input: n^2.9.
//
// EXPECTED: resolve time grows with the corpus, not with the cube of one file's
// def count. A COUNT test on this shape belongs on the fix.
//
// Owner: RustSource::call_name_match, src/lang/rust.rs:901 rebuilds own_spans
// over every named def on EVERY call site, then own_file_blob,
// src/lang/rust.rs:951, scans the whole DefIndex once per span in that set.
//
// Corpus: crates/syntax/src/ast/generated/nodes.rs (352,112 B, 2,508 defs,
// 3,320 sites) parses in 130 ms and resolves in 12.12 s, so `--resolve` over
// crates/syntax exceeds 10 s and the whole crate has to be split, which then
// loses 373 intra-crate edges that had nothing wrong with them.

pub fn f0() -> u32 { 1 }
pub fn f1() -> u32 { 1 }
pub fn f2() -> u32 { 1 }
pub fn f3() -> u32 { 1 }
pub fn f4() -> u32 { 1 }
pub fn f5() -> u32 { 1 }
pub fn f6() -> u32 { 1 }
pub fn f7() -> u32 { 1 }
pub fn f8() -> u32 { 1 }
pub fn f9() -> u32 { 1 }
pub fn f10() -> u32 { 1 }
pub fn f11() -> u32 { 1 }
pub fn f12() -> u32 { 1 }
pub fn f13() -> u32 { 1 }
pub fn f14() -> u32 { 1 }
pub fn f15() -> u32 { 1 }
pub fn f16() -> u32 { 1 }
pub fn f17() -> u32 { 1 }
pub fn f18() -> u32 { 1 }
pub fn f19() -> u32 { 1 }
pub fn f20() -> u32 { 1 }
pub fn f21() -> u32 { 1 }
pub fn f22() -> u32 { 1 }
pub fn f23() -> u32 { 1 }
pub fn f24() -> u32 { 1 }
pub fn f25() -> u32 { 1 }
pub fn f26() -> u32 { 1 }
pub fn f27() -> u32 { 1 }
pub fn f28() -> u32 { 1 }
pub fn f29() -> u32 { 1 }
pub fn f30() -> u32 { 1 }
pub fn f31() -> u32 { 1 }
pub fn f32() -> u32 { 1 }
pub fn f33() -> u32 { 1 }
pub fn f34() -> u32 { 1 }
pub fn f35() -> u32 { 1 }
pub fn f36() -> u32 { 1 }
pub fn f37() -> u32 { 1 }
pub fn f38() -> u32 { 1 }
pub fn f39() -> u32 { 1 }
pub fn f40() -> u32 { 1 }
pub fn f41() -> u32 { 1 }
pub fn f42() -> u32 { 1 }
pub fn f43() -> u32 { 1 }
pub fn f44() -> u32 { 1 }
pub fn f45() -> u32 { 1 }
pub fn f46() -> u32 { 1 }
pub fn f47() -> u32 { 1 }
pub fn f48() -> u32 { 1 }
pub fn f49() -> u32 { 1 }
pub fn f50() -> u32 { 1 }
pub fn f51() -> u32 { 1 }
pub fn f52() -> u32 { 1 }
pub fn f53() -> u32 { 1 }
pub fn f54() -> u32 { 1 }
pub fn f55() -> u32 { 1 }
pub fn f56() -> u32 { 1 }
pub fn f57() -> u32 { 1 }
pub fn f58() -> u32 { 1 }
pub fn f59() -> u32 { 1 }
pub fn f60() -> u32 { 1 }
pub fn f61() -> u32 { 1 }
pub fn f62() -> u32 { 1 }
pub fn f63() -> u32 { 1 }
pub fn f64() -> u32 { 1 }
pub fn f65() -> u32 { 1 }
pub fn f66() -> u32 { 1 }
pub fn f67() -> u32 { 1 }
pub fn f68() -> u32 { 1 }
pub fn f69() -> u32 { 1 }
pub fn f70() -> u32 { 1 }
pub fn f71() -> u32 { 1 }
pub fn f72() -> u32 { 1 }
pub fn f73() -> u32 { 1 }
pub fn f74() -> u32 { 1 }
pub fn f75() -> u32 { 1 }
pub fn f76() -> u32 { 1 }
pub fn f77() -> u32 { 1 }
pub fn f78() -> u32 { 1 }
pub fn f79() -> u32 { 1 }
pub fn f80() -> u32 { 1 }
pub fn f81() -> u32 { 1 }
pub fn f82() -> u32 { 1 }
pub fn f83() -> u32 { 1 }
pub fn f84() -> u32 { 1 }
pub fn f85() -> u32 { 1 }
pub fn f86() -> u32 { 1 }
pub fn f87() -> u32 { 1 }
pub fn f88() -> u32 { 1 }
pub fn f89() -> u32 { 1 }
pub fn f90() -> u32 { 1 }
pub fn f91() -> u32 { 1 }
pub fn f92() -> u32 { 1 }
pub fn f93() -> u32 { 1 }
pub fn f94() -> u32 { 1 }
pub fn f95() -> u32 { 1 }
pub fn f96() -> u32 { 1 }
pub fn f97() -> u32 { 1 }
pub fn f98() -> u32 { 1 }
pub fn f99() -> u32 { 1 }
pub fn f100() -> u32 { 1 }
pub fn f101() -> u32 { 1 }
pub fn f102() -> u32 { 1 }
pub fn f103() -> u32 { 1 }
pub fn f104() -> u32 { 1 }
pub fn f105() -> u32 { 1 }
pub fn f106() -> u32 { 1 }
pub fn f107() -> u32 { 1 }
pub fn f108() -> u32 { 1 }
pub fn f109() -> u32 { 1 }
pub fn f110() -> u32 { 1 }
pub fn f111() -> u32 { 1 }
pub fn f112() -> u32 { 1 }
pub fn f113() -> u32 { 1 }
pub fn f114() -> u32 { 1 }
pub fn f115() -> u32 { 1 }
pub fn f116() -> u32 { 1 }
pub fn f117() -> u32 { 1 }
pub fn f118() -> u32 { 1 }
pub fn f119() -> u32 { 1 }
pub fn f120() -> u32 { 1 }
pub fn f121() -> u32 { 1 }
pub fn f122() -> u32 { 1 }
pub fn f123() -> u32 { 1 }
pub fn f124() -> u32 { 1 }
pub fn f125() -> u32 { 1 }
pub fn f126() -> u32 { 1 }
pub fn f127() -> u32 { 1 }
pub fn f128() -> u32 { 1 }
pub fn f129() -> u32 { 1 }
pub fn f130() -> u32 { 1 }
pub fn f131() -> u32 { 1 }
pub fn f132() -> u32 { 1 }
pub fn f133() -> u32 { 1 }
pub fn f134() -> u32 { 1 }
pub fn f135() -> u32 { 1 }
pub fn f136() -> u32 { 1 }
pub fn f137() -> u32 { 1 }
pub fn f138() -> u32 { 1 }
pub fn f139() -> u32 { 1 }
pub fn f140() -> u32 { 1 }
pub fn f141() -> u32 { 1 }
pub fn f142() -> u32 { 1 }
pub fn f143() -> u32 { 1 }
pub fn f144() -> u32 { 1 }
pub fn f145() -> u32 { 1 }
pub fn f146() -> u32 { 1 }
pub fn f147() -> u32 { 1 }
pub fn f148() -> u32 { 1 }
pub fn f149() -> u32 { 1 }
pub fn f150() -> u32 { 1 }
pub fn f151() -> u32 { 1 }
pub fn f152() -> u32 { 1 }
pub fn f153() -> u32 { 1 }
pub fn f154() -> u32 { 1 }
pub fn f155() -> u32 { 1 }
pub fn f156() -> u32 { 1 }
pub fn f157() -> u32 { 1 }
pub fn f158() -> u32 { 1 }
pub fn f159() -> u32 { 1 }
pub fn f160() -> u32 { 1 }
pub fn f161() -> u32 { 1 }
pub fn f162() -> u32 { 1 }
pub fn f163() -> u32 { 1 }
pub fn f164() -> u32 { 1 }
pub fn f165() -> u32 { 1 }
pub fn f166() -> u32 { 1 }
pub fn f167() -> u32 { 1 }
pub fn f168() -> u32 { 1 }
pub fn f169() -> u32 { 1 }
pub fn f170() -> u32 { 1 }
pub fn f171() -> u32 { 1 }
pub fn f172() -> u32 { 1 }
pub fn f173() -> u32 { 1 }
pub fn f174() -> u32 { 1 }
pub fn f175() -> u32 { 1 }
pub fn f176() -> u32 { 1 }
pub fn f177() -> u32 { 1 }
pub fn f178() -> u32 { 1 }
pub fn f179() -> u32 { 1 }
pub fn f180() -> u32 { 1 }
pub fn f181() -> u32 { 1 }
pub fn f182() -> u32 { 1 }
pub fn f183() -> u32 { 1 }
pub fn f184() -> u32 { 1 }
pub fn f185() -> u32 { 1 }
pub fn f186() -> u32 { 1 }
pub fn f187() -> u32 { 1 }
pub fn f188() -> u32 { 1 }
pub fn f189() -> u32 { 1 }
pub fn f190() -> u32 { 1 }
pub fn f191() -> u32 { 1 }
pub fn f192() -> u32 { 1 }
pub fn f193() -> u32 { 1 }
pub fn f194() -> u32 { 1 }
pub fn f195() -> u32 { 1 }
pub fn f196() -> u32 { 1 }
pub fn f197() -> u32 { 1 }
pub fn f198() -> u32 { 1 }
pub fn f199() -> u32 { 1 }
pub fn driver() -> u32 { 0 + f0() + f1() + f2() + f3() + f4() + f5() + f6() + f7() + f8() + f9() + f10() + f11() + f12() + f13() + f14() + f15() + f16() + f17() + f18() + f19() + f20() + f21() + f22() + f23() + f24() + f25() + f26() + f27() + f28() + f29() + f30() + f31() + f32() + f33() + f34() + f35() + f36() + f37() + f38() + f39() + f40() + f41() + f42() + f43() + f44() + f45() + f46() + f47() + f48() + f49() + f50() + f51() + f52() + f53() + f54() + f55() + f56() + f57() + f58() + f59() + f60() + f61() + f62() + f63() + f64() + f65() + f66() + f67() + f68() + f69() + f70() + f71() + f72() + f73() + f74() + f75() + f76() + f77() + f78() + f79() + f80() + f81() + f82() + f83() + f84() + f85() + f86() + f87() + f88() + f89() + f90() + f91() + f92() + f93() + f94() + f95() + f96() + f97() + f98() + f99() + f100() + f101() + f102() + f103() + f104() + f105() + f106() + f107() + f108() + f109() + f110() + f111() + f112() + f113() + f114() + f115() + f116() + f117() + f118() + f119() + f120() + f121() + f122() + f123() + f124() + f125() + f126() + f127() + f128() + f129() + f130() + f131() + f132() + f133() + f134() + f135() + f136() + f137() + f138() + f139() + f140() + f141() + f142() + f143() + f144() + f145() + f146() + f147() + f148() + f149() + f150() + f151() + f152() + f153() + f154() + f155() + f156() + f157() + f158() + f159() + f160() + f161() + f162() + f163() + f164() + f165() + f166() + f167() + f168() + f169() + f170() + f171() + f172() + f173() + f174() + f175() + f176() + f177() + f178() + f179() + f180() + f181() + f182() + f183() + f184() + f185() + f186() + f187() + f188() + f189() + f190() + f191() + f192() + f193() + f194() + f195() + f196() + f197() + f198() + f199() }

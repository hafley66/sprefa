use sprefa_extract::{FamilyMask, PythonSource, Source};

#[test]
fn dump_ret_diff() {
    let base = "../../plans/extract-bench-2026-08-29/python-oracle/suite";
    for case in ["decorators/return_different_func", "decorators/nested_decorators"] {
        let path = format!("{base}/{case}/main.py", case = case);
        let src = std::fs::read(&path).unwrap();
        let output = PythonSource.extract(&path, &src, FamilyMask::ALL);
        let call = output.call.as_ref().unwrap();
        let s = &output.strings;
        println!("== {case}");
        for d in &call.aux.py_decorators {
            println!("DECOR span={:?} callee={:?} decorated={:?} call={}", d.span, s.lookup(d.callee), s.lookup(d.decorated), d.call_expr);
        }
        for r in &call.aux.py_returns {
            println!("RET def={:?} value={:?}", r.def, s.lookup(r.value));
        }
        for e in &call.aux.py_ret_calls {
            println!("RETCALL span={:?} inner={:?}", e.span, e.inner);
        }
        for b in &call.aux.py_binds {
            println!("BIND target={:?} key={:?} value={:?}", s.lookup(b.target), b.key.map(|i| s.lookup(i)), b.value.map(|i| s.lookup(i)));
        }
        for p in &call.aux.py_params {
            println!("PARAM def={:?} name={:?}", p.def, s.lookup(p.name));
        }
        for st in &call.aux.sites {
            println!("SITE {:?} callee={:?}", st.span, s.lookup(st.callee));
        }
    }
}

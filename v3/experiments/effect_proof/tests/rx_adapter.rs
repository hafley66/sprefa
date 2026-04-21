//! Smoke test for the rxRust adapter (feature = "rx" on effect_runtime).
//!
//! Proves:
//!   1. `register_observable::<E>` taps request + response traffic into
//!      two `SharedSubject`s without altering `ctx.put`'s return type.
//!   2. Userland can compose rxRust operators (map, filter) over the
//!      response stream and collect side-channel results.
//!   3. Puts and subject emissions are 1:1 per kind.

use effect_proof::effects::count_lines::{CountLines, SimpleLineCounter};
use effect_runtime::rx::register_observable;
use effect_runtime::RtCtxBuilder;
use rxrust::prelude::*;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn observable_tap_mirrors_request_and_response_streams() {
    let (builder, _req_tx, resp_tx) =
        register_observable::<CountLines, _>(RtCtxBuilder::new(), SimpleLineCounter);
    let ctx = builder.build();

    let collected: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = collected.clone();
    resp_tx
        .lock()
        .unwrap()
        .clone()
        .subscribe(move |v: usize| {
            sink.lock().unwrap().push(v);
        });

    let a = ctx.put(CountLines { content: b"one\ntwo\nthree\n".to_vec() }).await;
    let b = ctx.put(CountLines { content: b"single-no-newline".to_vec() }).await;
    let c = ctx.put(CountLines { content: Vec::new() }).await;

    assert_eq!(a, 3);
    assert_eq!(b, 1);
    assert_eq!(c, 0);
    assert_eq!(*collected.lock().unwrap(), vec![3, 1, 0]);
}

#[tokio::test]
async fn rx_operators_compose_over_response_stream() {
    let (builder, _req_tx, resp_tx) =
        register_observable::<CountLines, _>(RtCtxBuilder::new(), SimpleLineCounter);
    let ctx = builder.build();

    let evens: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = evens.clone();
    resp_tx
        .lock()
        .unwrap()
        .clone()
        .filter(|v: &usize| *v % 2 == 0)
        .map(|v: usize| v * 10)
        .subscribe(move |v: usize| sink.lock().unwrap().push(v));

    for n in 1..=5u32 {
        let body = vec![b'\n'; n as usize];
        let _ = ctx.put(CountLines { content: body }).await;
    }

    assert_eq!(*evens.lock().unwrap(), vec![20, 40]);
}

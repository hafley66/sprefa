//! Targets consuming the shared IVM program.

use crate::_6_eval::ir_bundle::Bundle;

pub struct IrEmitter {
    pub name: &'static str,
    pub extension: &'static str,
    pub emit: fn(&Bundle) -> Result<String, String>,
}

fn emit_rxjs(bundle: &Bundle) -> Result<String, String> {
    ivm_rxjs::emit(&bundle.program)
}

fn emit_json(bundle: &Bundle) -> Result<String, String> {
    serde_json::to_string_pretty(bundle).map_err(|error| error.to_string())
}

pub const IR_TARGETS: [IrEmitter; 2] = [
    IrEmitter {
        name: "ir-rxjs",
        extension: "ts",
        emit: emit_rxjs,
    },
    IrEmitter {
        name: "ir-json",
        extension: "json",
        emit: emit_json,
    },
];

//! Targets consuming the shared IVM program.

use ivm_ir::Program;

pub struct IrEmitter {
    pub name: &'static str,
    pub extension: &'static str,
    pub emit: fn(&Program) -> Result<String, String>,
}

fn emit_json(program: &Program) -> Result<String, String> {
    serde_json::to_string_pretty(program).map_err(|error| error.to_string())
}

pub const IR_TARGETS: [IrEmitter; 2] = [
    IrEmitter {
        name: "ir-rxjs",
        extension: "ts",
        emit: ivm_rxjs::emit,
    },
    IrEmitter {
        name: "ir-json",
        extension: "json",
        emit: emit_json,
    },
];

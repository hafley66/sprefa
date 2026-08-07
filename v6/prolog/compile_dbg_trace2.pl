:- module(dbg_trace2, []).
:- use_module(lower, [lower_program/2, boot_statements/5]).
:- use_module(emit_ts, [emit_program/5]).
:- initialization(dbg_trace2:main, main).

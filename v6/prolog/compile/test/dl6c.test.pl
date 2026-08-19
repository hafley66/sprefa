% dl6c.test.pl : the dl6c CLI's option table, output naming, and exit-code map.
%
% The end-to-end claim (a SAVED state, copied to a temp dir with no v6/prolog
% on any path, emits bytes identical to compile_dl6/3's) is not answerable
% in-process and lives in compile/scripts/dl6c_roundtrip.sh. This unit owns
% everything that decides WHICH bytes and WHICH exit code.
%
% Several units below print the compiler's own stderr rendering of a named
% unsupported construct while they run; that text is the thing under test, not
% a fault.
%
% SABOTAGE RECEIPT (run at authoring time, reverted): swapping
% target_emitter/3's two rows so `rust` maps to emit_ts:emit_program reds
% dl6c_target_rust_selects_the_rust_emitter here and all four `--target rust`
% byte-diffs in dl6c_roundtrip.sh; putting the rows back turns both green.
%
% SECOND SABOTAGE RECEIPT (reverted): deleting the not_stratified row from
% dl6c.pl's named_reason_functor/1 reds
% dl6c_exit_two_vocabulary_matches_bop_check against bop_check.pl's own list.
% That drift, silently exiting 1 where `bop check` exits 2, is why the unit
% compares the two lists instead of pinning a hand-written expected list.

:- begin_tests(dl6c).

:- use_module(library(filesex), [delete_directory_and_contents/1]).
:- use_module('../../dl6c', [dl6c_version/1, named_reason_functor/1]).
:- use_module('../scripts/bop_check', []).

:- dynamic(dl6c_test_dir/1).
:- prolog_load_context(directory, DirHere), assertz(dl6c_test_dir(DirHere)).

dl6c_fixture(Base, File) :-
    dl6c_test_dir(Here),
    atomic_list_concat([Here, '/../../../dl/fixtures/', Base], File).

quiet_exit_code(Argv, Code) :-
    with_output_to(string(_), dl6c:exit_code(Argv, Code)).

% ── the emitter each --target picks ──────────────────────────────────────────

test(dl6c_target_ts_selects_the_ts_emitter) :-
    dl6c:target_emitter(ts, Emitter, Extension),
    Emitter == emit_ts:emit_program,
    Extension == ts.

test(dl6c_target_rust_selects_the_rust_emitter) :-
    dl6c:target_emitter(rust, Emitter, Extension),
    Emitter == emit_rust:emit_program,
    Extension == rs.

test(dl6c_target_table_is_exactly_two_rows) :-
    findall(Target, dl6c:target_emitter(Target, _, _), Targets),
    msort(Targets, Sorted),
    Sorted == [rust, ts].

% ── output naming: the input base name, the target's extension ───────────────

test(dl6c_output_file_takes_the_input_base_name) :-
    tmp_file(dl6c_out, OutDir),
    dl6c:output_file('/somewhere/else/resident-coroutine.dl6', OutDir, rs, OutFile),
    file_base_name(OutFile, OutName),
    OutName == 'resident-coroutine.rs',
    delete_directory_and_contents(OutDir).

test(dl6c_output_file_creates_a_missing_directory) :-
    tmp_file(dl6c_out, Root),
    atomic_list_concat([Root, '/nested/deeper'], OutDir),
    dl6c:output_file('/somewhere/else/probe.dl6', OutDir, ts, OutFile),
    exists_directory(OutDir),
    file_base_name(OutFile, OutName),
    OutName == 'probe.ts',
    delete_directory_and_contents(Root).

% ── exit codes, the contract bop_check.pl owns ───────────────────────────────

test(dl6c_exit_two_vocabulary_matches_bop_check) :-
    findall(Functor, named_reason_functor(Functor), Ours),
    findall(Functor, bop_check:named_reason_functor(Functor), Theirs),
    msort(Ours, Sorted),
    msort(Theirs, Sorted).

test(dl6c_compiles_a_fixture_and_exits_zero) :-
    dl6c_fixture('anonymous-type-syntax.dl6', Source),
    tmp_file(dl6c_out, OutDir),
    quiet_exit_code([Source, '--target', ts, '--out', OutDir], Code),
    Code == 0,
    atom_concat(OutDir, '/anonymous-type-syntax.ts', OutFile),
    exists_file(OutFile),
    delete_directory_and_contents(OutDir).

test(dl6c_rust_target_writes_a_rs_file) :-
    dl6c_fixture('anonymous-type-syntax.dl6', Source),
    tmp_file(dl6c_out, OutDir),
    quiet_exit_code([Source, '--target', rust, '--out', OutDir], Code),
    Code == 0,
    atom_concat(OutDir, '/anonymous-type-syntax.rs', OutFile),
    exists_file(OutFile),
    delete_directory_and_contents(OutDir).

test(dl6c_named_unsupported_construct_exits_two) :-
    tmp_file(dl6c_src, SourceDir),
    make_directory(SourceDir),
    atom_concat(SourceDir, '/unknown-column-type.dl6', Source),
    setup_call_cleanup(
        open(Source, write, Stream),
        format(Stream, 'rel person(name: text, age: treee).~n', []),
        close(Stream)),
    tmp_file(dl6c_out, OutDir),
    quiet_exit_code([Source, '--target', ts, '--out', OutDir], Code),
    Code == 2,
    delete_directory_and_contents(SourceDir),
    delete_directory_and_contents(OutDir).

test(dl6c_missing_input_file_exits_one) :-
    tmp_file(dl6c_out, OutDir),
    quiet_exit_code(['/no/such/file.dl6', '--target', ts, '--out', OutDir], Code),
    Code == 1,
    delete_directory_and_contents(OutDir).

test(dl6c_no_target_exits_one) :-
    dl6c_fixture('anonymous-type-syntax.dl6', Source),
    quiet_exit_code([Source, '--out', '/tmp'], Code),
    Code == 1.

test(dl6c_no_positional_input_exits_one) :-
    quiet_exit_code(['--target', ts, '--out', '/tmp'], Code),
    Code == 1.

% ── the version stamp ────────────────────────────────────────────────────────

test(dl6c_version_is_unknown_until_the_state_is_saved) :-
    dl6c_version(Version),
    Version == unknown.

test(dl6c_version_reports_the_stamped_sha) :-
    setup_call_cleanup(
        assertz(dl6c:dl6c_build_sha(cafef00d)),
        dl6c_version(Version),
        retractall(dl6c:dl6c_build_sha(cafef00d))),
    Version == cafef00d.

:- end_tests(dl6c).

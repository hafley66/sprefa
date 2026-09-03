:- use_module(library(filesex),
              [ copy_file/2,
                delete_directory_and_contents/1
              ]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../src/3_emit/3_rust_type_region_mainer',
              [refresh_rust_type_region/7]).

:- initialization(main, main).

main :-
    temporary_directory(dl7_rust_type_target, TargetRoot),
    temporary_directory(dl7_rust_type_state, StateRoot),
    setup_call_cleanup(
        true,
        run_e2e(TargetRoot, StateRoot),
        ( delete_directory_and_contents(TargetRoot),
          delete_directory_and_contents(StateRoot)
        )),
    writeln('rust type region e2e passed').

run_e2e(TargetRoot, StateRoot) :-
    repo_path('v6/sprefa-extract/target/debug/extract', Extract),
    repo_path('v6/sprefa-extract/tests/fixtures/tsi/rust_probe/src/lib.rs',
              Rust),
    repo_path('v7/test/fixtures/7_rust_types_region.dl7', Fixture),
    directory_file_path(TargetRoot, 'generated.dl7', Target),
    copy_file(Fixture, Target),
    refresh_rust_type_region(
        Extract, Rust, Target, 'rust-types', check,
        drift, []),
    refresh_rust_type_region(
        Extract, Rust, Target, 'rust-types', apply(StateRoot),
        applied, []),
    refresh_rust_type_region(
        Extract, Rust, Target, 'rust-types', check,
        current, []),
    read_file_to_string(Target, Text, [encoding(utf8)]),
    sub_string(Text, 0, _, _,
               "(: AuthoredBefore\n   (* (: id u64)))\n\n; sprefa:auto-begin rust-types\n"),
    sub_string(Text, _, _, 0,
               "; sprefa:auto-end rust-types\n\n(: AuthoredAfter\n   (* (: name string)))\n"),
    \+ sub_string(Text, _, _, _, "stale_generated_type"),
    sub_string(Text, _, _, _, "(: User"),
    sub_string(Text, _, _, _, "(: tsi_parameter"),
    sub_string(Text, _, _, _, "(: rust_impl"),
    compile_dl7(Target, _, _, []).

temporary_directory(Label, Directory) :-
    tmp_file(Label, Directory),
    make_directory(Directory).

repo_path(Relative, Absolute) :-
    source_file(main, ThisFile),
    file_directory_name(ThisFile, TestDirectory),
    directory_file_path(TestDirectory, '../..', RelativeRoot),
    absolute_file_name(RelativeRoot, Root,
                       [file_type(directory), access(read)]),
    directory_file_path(Root, Relative, Absolute).

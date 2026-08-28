%% Cross-file arm of the metacall fixture: `cross/0` calls `target/0`, which
%% lives in the other file, through `once/1`.
:- module(metacall_caller, [cross/0]).

cross :-
    once(target).

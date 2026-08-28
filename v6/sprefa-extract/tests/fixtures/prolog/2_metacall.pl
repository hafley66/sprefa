%% Prolog metacall resolution fixture. `helper/0` and `helper/1` appear once
%% per call shape; `once/1` argument 1 and `catch/3` arguments 1 and 3 are
%% statically callable, every other argument stays data.
:- module(metacall, [direct/0, nested_once/0, catch_protect/0, catch_recover/0,
                     data_arg/0]).

helper.

direct :-
    helper.

nested_once :-
    once(once(helper)).

catch_protect :-
    catch(helper, _, true).

catch_recover :-
    catch(fail, _, helper).

data_arg :-
    process(helper(X)).

% run.pl : load the lab modules and drive the pipeline.
%
%   swipl -q -l run.pl -g run -g halt
%
% A codegen_refused/1 failure is printed (not a bare stack) and exits 1, after
% which NO text has been emitted -- check runs before render.

:- use_module('3_render').

run :-
    catch(alloy_render:run_all,
          codegen_refused(Reason),
          refuse(Reason)).

refuse(Reason) :-
    format('codegen_refused(~w)~n', [Reason]),
    halt(1).

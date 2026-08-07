:- use_module('../compile', []).
main :-
    catch(compile_dl6('/Users/chrishafley/projects/sprefa-lanes/snakecase/v6/dl/fixtures/door-handwritten.dl6', '/tmp/door2.ts'), E,
          (writeln('ERR='), print_term(E, [portray(true)]), nl)),
    halt.

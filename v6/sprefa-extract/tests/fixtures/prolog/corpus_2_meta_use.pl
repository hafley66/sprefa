% Use file for the metacall resolve test: go/1 calls double/2 through
% maplist/3, call/3, forall/2, and findall/3.
go(Doubled) :-
    maplist(double, [1, 2], Doubled),
    call(double, 1, _),
    forall(member(Element, Doubled), double(Element, _)),
    findall(Twice, double(3, Twice), _).

:- module(dl7_embedded_fixture, [dl7_unit/5]).

:- use_module('../../3_quasi', [dl7/4]).

{|dl7||
; shared frontend fixture
(: User
   (* (: id int)
      (: name text)
      (: note "hello\nworld")))
(<- (copy ?Value ?Value ?_ ?_)
    (source ?Value))
()
; empty form, nested forms, bare atoms, symbol data
(: Wrapper
   (* (: inner
         (* (: tag 'kind)))
      (: bare atom)))
'spot
|}.

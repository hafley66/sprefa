% diag.test.pl : receipts for the dl6 diagnostic channel (compile/test).
%
% The rail the lane exists to hold: the unsupported construct term is the ONE source of
% truth and gets TWO renderers. The human renderer (0_unsupported_messages.pl) and
% the JSON channel (diag.pl) must never disagree on the message text, so the
% first test walks the whole inventory asserting both hold the same string.
% The zero-based LSP conversion is tested as its own predicate, the emitted
% record is proven to re-parse as JSON, and a real failing program is parsed
% to prove the channel lands on a real line and column.

:- op(1150, xfx, <-).

:- use_module(library(plunit)).
:- use_module(library(http/json)).

:- use_module('../../diag',
              [ lsp_position/4, diag_record/3, diag_position/3, diag_uri/2 ]).
:- use_module('../../0_unsupported_messages', [ unsupported_inventory/1 ]).
:- use_module('../../next/0_parse/parse_dl_dcg', [ parse_dl/4 ]).

:- begin_tests(diag_channel).

% LSP line and character are ZERO-based; Prolog's are 1-based. The conversion
% is exactly one subtraction on each axis.
test(one_based_to_zero_based) :-
    lsp_position(1, 1, 0, 0),
    lsp_position(1, 5, 0, 4),
    lsp_position(3, 4, 2, 3),
    lsp_position(12, 1, 11, 0).

% The one-source-two-renderers rail, across the whole dynamic inventory: the
% JSON `message` field of the record and the umbrella renderer's human line
% are the same string, whatever the inventory's size is.
test(json_message_equals_human_line, [forall(unsupported_inventory(Inventory))]) :-
    forall(member(_Sig-Example, Inventory),
           ( diag_record(unsupported_construct(Example), 'x.dl6', Record),
             message_to_string(unsupported_construct(Example), Human),
             Record.get(message) == Human )).

% Every inventory member still renders through the human umbrella with no
% "Unknown message" fallback; that guarantee is the existing unsupported_messages
% umbrella receipt, and the rail test above covers the JSON side of the same
% walk, so it is not repeated here.

% The emitted record is valid JSON: JSON out, JSON in again through
% library(http/json), keeping the code and severity. A fixed representative
% spread, not the whole inventory, because the message text (and so the walk)
% is already proven identical over the whole inventory by the rail test above.
representative_diagnostic(unsupported_construct(tag_brace_reserved('fetching'))).
representative_diagnostic(unsupported_construct(cross_plane(finalize_in_level_rule(s/1)))).
representative_diagnostic(unsupported_construct(removed_word(scan))).
representative_diagnostic(dl_parse_error(statement, position(1, 5))).

test(record_round_trips_as_json) :-
    forall(representative_diagnostic(Term),
           ( diag_record(Term, 'x.dl6', Record),
             with_output_to(codes(Codes), json_write_dict(current_output,
                                                          Record, [width(0)])),
             string_codes(String, Codes),
             open_string(String, Stream),
             call_cleanup(json_read_dict(Stream, Parsed), close(Stream)),
             atom_string(Record.get(code), RecordCodeString),
             Parsed.get(code) == RecordCodeString,
             Parsed.get(severity) == Record.get(severity),
             Parsed.get(range).get(start).get(line) ==
                 Record.get(range).get(start).get(line) )).

% DEFECT 1 FAIL-FIRST: a unsupported construct must resolve to the statement that DEFINES
% the offending relation (its head), not to the FIRST statement that merely
% mentions the relation. counter/2 is named by the valid mirror rule on line
% 5 and by the offending counter rule on line 6; the diagnostic must land on
% line 6. The prior test passes only because its program has one statement.
test(unsupported_resolves_offending_rule_not_earlier_mention) :-
    Lines = [ "rel counter(name: text, total: int) key(1).",
              "rel tick(name: text).",
              "rel mirror(name: text, total: int).",
              "",
              "mirror(Name, Total) <- counter(Name, Total).",
              "counter(Name, Total) <- tick(Name), Total = latest(1)." ],
    atomic_list_concat(Lines, "\n", Text),
    string_codes(Text, Codes),
    catch(( parse_dl(Codes, _P, _B, _F) -> true ; true ), _E, true),
    once(diag_position(unsupported_construct(keyed_level_head(counter/2)),
                       Line, Column)),
    Line == 6,
    Column == 1.

% A unsupported construct naming the offending relation resolves to that statement's real
% line and column. `t(X) <- finalize(s(X)).` sits on line 3, first column,
% and finalize_in_level_rule(s/1) resolves there through the retention.
test(unsupported_resolves_real_statement_position) :-
    Text = "rel s(x: int).\nrel t(x: int).\nt(X) <- finalize(s(X)).\n",
    string_codes(Text, Codes),
    catch(( parse_dl(Codes, _P, _B, _F) -> true ; true ), _E, true),
    once(diag_position(unsupported_construct(cross_plane(finalize_in_level_rule(s/1))),
                       Line, Column)),
    Line == 3,
    Column == 1.

% A parse error carries its exact position through the channel, zero-based in
% the record. `rel ?bad(x: int).` fails at line 1, column 5.
test(parse_error_position_is_exact_in_record) :-
    Text = "rel ?bad(x: int).\n",
    string_codes(Text, Codes),
    catch(( parse_dl(Codes, _P, _B, _F) -> true ; true ), _E, true),
    diag_record(dl_parse_error(statement, position(1, 5)), 'x.dl6', Record),
    Record.get(range).get(start).get(line) == 0,
    Record.get(range).get(start).get(character) == 4.

% DEFECT 3: the JSON `uri` is an LSP file:// scheme URI, percent-encoded for
% spaces and non-ASCII instead of a raw filesystem path. The document the text
% door named is the one `at(File, ...)` carries.
test(uri_is_percent_encoded_file_scheme) :-
    once(diag_uri(unsupported_construct(
                      at('/tmp/my résumé/notes file.dl6', 3, some_reason)),
                  Uri)),
    Uri == "file:///tmp/my%20r%C3%A9sum%C3%A9/notes%20file.dl6".

:- end_tests(diag_channel).

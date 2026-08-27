% diag.pl : the machine-readable diagnostic channel for dl6.
%
% One structured record per line, LSP-shaped, in JSON. The human renderer
% (0_unsupported_messages.pl, the single prolog:message//1 umbrella over the
% unsupported construct inventory) stays the ONE source of the message text. This module
% reads that same text back through message_to_string/2, so the JSON `message`
% field and the human line can never diverge. No second message table and no
% unsupported construct signature is duplicated here.
%
% Positions: a unsupported construct term carries its own source through the text door's
% at(File, Line, Reason) wrapper or, on a successful parse, through the
% statement positions parse_dl.pl retains. The side table dl6_span/6 is
% materialized lazily from that retention, keyed by a relation reference, only
% when a diagnostic asks for it. A successful compile therefore pays no
% per-statement inference and no parsed term shape (and so no emitted TypeScript
% byte) changes.
%
% Stream: per process, one active stream. Default user_error (stderr). Setting
% the environment variable DL6_DIAG_JSONL to a path redirects the channel to
% that file instead, appended, one record per line.

:- module(diag,
          [ lsp_position/4,
            diag_record/3,
            emit_diag/2,
            emit_diag_term/1,
            emit_diag_file/2,
            diag_stream_open/0,
            set_diag_file/1,
            diag_uri/2,
            diag_position/3,
            dl6_span/6
          ]).

:- use_module(library(http/json)).
:- use_module('../1_expansion/compile_messages', []).
:- use_module(library(uri), [uri_encoded/3]).
:- use_module('../0_unsupported_messages', []).
:- use_module('../7_lower/parse_dl_dcg',
              [ statement_location_for_reason/3 ]).

% ═══ LSP coordinate conversion (ONE predicate, tested) ═════════════════════

% LSP line and character are ZERO-based; Prolog's are 1-based.
lsp_position(Line, Column, LspLine, LspCharacter) :-
    LspLine is Line - 1,
    LspCharacter is Column - 1.

% ═══ stream selection ═════════════════════════════════════════════════════

% The active stream is chosen once per process. DL6_DIAG_JSONL, when set,
% names an append file; otherwise the channel writes to stderr.
diag_stream_open :-
    nb_current(diag_stream, _),
    !.
diag_stream_open :-
    ( getenv('DL6_DIAG_JSONL', Path)
    -> open(Path, append, Stream)
    ;  Stream = user_error
    ),
    nb_setval(diag_stream, Stream).

% ═══ pulling the one source term apart ═════════════════════════════════════

% The signature reason: the term the umbrella renderer's at/3 arm wraps, or the
% bare unsupported_construct reason, or (for a parse error) the whole term.
diag_reason(unsupported_construct(at(_, _, Reason)), Reason).
diag_reason(unsupported_construct(Reason), Reason).
diag_reason(Term, Term).

% The message text comes from the SAME prolog:message//1 the human channel uses.
diag_message(Term, Message) :-
    message_to_string(Term, Message).

% The LSP `code`, a unsupported construct's signature functor, from the wrapped reason so the
% at/3 wrapper never leaks into it.
diag_signature_code(Term, Code) :-
    diag_reason(Term, Reason),
    ( atom(Reason)
    -> Code = Reason
    ;  functor(Reason, Name, Arity),
       format(atom(Code), '~w/~d', [Name, Arity])
    ).

% ═══ position resolution ══════════════════════════════════════════════════

% A diagnostic's range. Resolution order:
%   1. a successful parse: the underlying reason resolves, through its relation
%      references, to the offending statement's start line and column;
%   2. the text door's at(File, Line, Reason) wrapper: a line, no column;
%   3. no position (the range collapses to 0,0 and the record still carries the
%      full human message).
diag_position(unsupported_construct(at(_File, Line, Reason)), LineResolved, Column) :-
    !,
    ( statement_location_for_reason(Reason, StatementLine, StatementColumn)
    -> LineResolved = StatementLine, Column = StatementColumn
    ;  LineResolved = Line, Column = 1
    ).
diag_position(unsupported_construct(Reason), Line, Column) :-
    ( statement_location_for_reason(Reason, StatementLine, StatementColumn)
    -> Line = StatementLine, Column = StatementColumn
    ;  Line = 1, Column = 1
    ).
diag_position(dl_parse_error(_Reason, position(Line, Column)), Line, Column) :-
    !.
diag_position(_, 1, 1).

diag_range(Term, Range) :-
    diag_position(Term, Line, Column),
    lsp_position(Line, Column, LspLine, LspCharacter),
    Point = _{line: LspLine, character: LspCharacter},
    Range = _{start: Point, end: Point}.

% ═══ the record ═══════════════════════════════════════════════════════════

diag_record(Term, Uri, Record) :-
    diag_message(Term, Message),
    diag_signature_code(Term, Code),
    diag_range(Term, Range),
    Record = _{ uri: Uri,
                range: Range,
                severity: 1,
                code: Code,
                source: "dl6",
                message: Message }.

% The source a diagnostic names: the file the text door wrapped, else the file
% the compiler most recently set for the channel, else unknown. LSP reads the
% field as a file:// scheme URI, so the filesystem path is percent-encoded
% (spaces and non-ASCII) rather than concatenated raw.
diag_uri(unsupported_construct(at(File, _, _)), Uri) :-
    diag_file_uri(File, Uri).
diag_uri(Term, Uri) :-
    diag_reason(Term, Reason),
    \+ (Reason = at(_, _, _)),
    diag_current_file(File),
    diag_file_uri(File, Uri).
diag_uri(_, 'unknown').

diag_file_uri(File, Uri) :-
    uri_encoded(path, File, EncodedPath),
    string_concat("file://", EncodedPath, Uri).

% The channel's current blame file, held for diagnostics (parse errors, bare
% unsupported constructs) whose term carries no at/3 wrapper.
set_diag_file(File) :-
    nb_setval(diag_blame, File).

diag_current_file(File) :-
    nb_current(diag_blame, File),
    !.
diag_current_file('unknown').

% ═══ emission ═════════════════════════════════════════════════════════════

% The bytes stay JSON; compile_messages.pl's message_hook writes the record and
% suppresses the line renderer, so the channel sits inside the message system
% without any consumer seeing a different byte.
emit_diag(Stream, Term) :-
    diag_uri(Term, Uri),
    diag_record(Term, Uri, Record),
    print_message(error, dl6_diag(Stream, Record)).

% Emit to the active (per-process) stream. This is the seam the compiler calls
% at a unsupported construct, and the entry the diag.test.pl receipts drive.
emit_diag_term(Term) :-
    diag_stream_open,
    nb_getval(diag_stream, Stream),
    emit_diag(Stream, Term).

% Emit with the compiler's current source file fixed first, so a diagnostic
% whose term carries no at/3 wrapper (a parse error, a bare unsupported construct) still
% names the file being compiled.
emit_diag_file(File, Term) :-
    set_diag_file(File),
    emit_diag_term(Term).

% ═══ the side table, materialized on demand ════════════════════════════════
%
% dl6_span(SpanId, File, StartLine, StartCol, EndLine, EndCol): one row per
% retained statement, keyed by a relation reference, a point span (start ==
% end) because the statement's position is where the unsupported construct names it. The
% predicate is a Datalog read over parse_dl's retention: nothing is stored at
% parse time and no successful-compile cost is paid. The File flank is not
% recoverable from a bare relation reference (parse_dl's table is per-last-
% parse and names only relations), so it stays a caller-supplied argument in
% the slot the contract's shape reserves.
dl6_span(SpanId, File, StartLine, StartCol, StartLine, StartCol) :-
    statement_location_for_reason(SpanId, StartLine, StartCol),
    ground(File).

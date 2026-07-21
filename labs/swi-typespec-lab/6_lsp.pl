:- module(soup_lsp, [main/0]).

:- use_module(library(http/json)).
:- use_module('5_documents').

main :-
    set_stream(user_input, encoding(octet)),
    set_stream(user_output, encoding(octet)),
    server_loop(running).

server_loop(exit) :- !.
server_loop(State0) :-
    (   read_message(Message)
    ->  catch(dispatch(Message, State0, State), Error, handle_error(Message, Error, State0, State)),
        server_loop(State)
    ;   true
    ).

dispatch(Message, State, State) :-
    Method = Message.get(method),
    request_result(Method, Message, Result),
    !,
    send_response(Message.id, Result).
dispatch(Message, State0, State) :-
    Method = Message.get(method),
    notification(Method, Message, State0, State),
    !.
dispatch(Message, State, State) :-
    get_dict(id, Message, Id),
    send_error(Id, -32601, "Method not found").
dispatch(_, State, State).

request_result("initialize", _, _{
    capabilities:_{
        textDocumentSync:1,
        hoverProvider:true,
        definitionProvider:true,
        referencesProvider:true,
        completionProvider:_{resolveProvider:false, triggerCharacters:["{", ":", "<"]},
        documentSymbolProvider:true
    },
    serverInfo:_{name:"soup-lsp", version:"0.1.0"}
}).
request_result("shutdown", _, @(null)).
request_result("textDocument/hover", Message, Result) :-
    position_params(Message, Uri, Line, Character),
    (documents:hover_at(Uri, Line, Character, Result) -> true ; Result = @(null)).
request_result("textDocument/definition", Message, Result) :-
    position_params(Message, Uri, Line, Character),
    (documents:definition_at(Uri, Line, Character, Result) -> true ; Result = @(null)).
request_result("textDocument/references", Message, Result) :-
    position_params(Message, Uri, Line, Character),
    (documents:references_at(Uri, Line, Character, Result) -> true ; Result = []).
request_result("textDocument/completion", Message, Result) :-
    Uri = Message.params.textDocument.uri,
    documents:completions(Uri, Result).
request_result("textDocument/documentSymbol", Message, Result) :-
    Uri = Message.params.textDocument.uri,
    documents:document_symbols(Uri, Result).

notification("initialized", _, State, State).
notification("textDocument/didOpen", Message, State, State) :-
    Document = Message.params.textDocument,
    documents:open_document(Document.uri, Document.version, Document.text, _),
    publish_diagnostics(Document.uri).
notification("textDocument/didChange", Message, State, State) :-
    Uri = Message.params.textDocument.uri,
    Version = Message.params.textDocument.version,
    [Change|_] = Message.params.contentChanges,
    documents:change_document(Uri, Version, Change.text, _),
    publish_diagnostics(Uri).
notification("textDocument/didClose", Message, State, State) :-
    Uri = Message.params.textDocument.uri,
    documents:close_document(Uri),
    send_notification("textDocument/publishDiagnostics", _{uri:Uri, diagnostics:[]}).
notification("exit", _, _, exit).

position_params(Message, Uri, Line, Character) :-
    Uri = Message.params.textDocument.uri,
    Position = Message.params.position,
    Line = Position.line,
    Character = Position.character.

publish_diagnostics(Uri) :-
    documents:document_diagnostics(Uri, Diagnostics),
    send_notification("textDocument/publishDiagnostics", _{uri:Uri, diagnostics:Diagnostics}).

handle_error(Message, Error, State, State) :-
    message_to_string(Error, Text),
    (get_dict(id, Message, Id) -> send_error(Id, -32603, Text) ; format(user_error, "~s~n", [Text])).

read_message(Message) :-
    read_headers(ContentLength),
    read_n_bytes(ContentLength, Bytes),
    string_bytes(Json, Bytes, utf8),
    atom_string(Atom, Json),
    atom_json_dict(Atom, Message, []).

read_headers(ContentLength) :-
    read_header_lines(Lines),
    member(Line, Lines),
    split_string(Line, ":", " ", [Name, Value]),
    string_lower(Name, "content-length"),
    number_string(ContentLength, Value).

read_header_lines(Lines) :-
    read_line_to_string(user_input, Line),
    Line \== end_of_file,
    (Line == "" -> Lines = [] ; Lines = [Line|Rest], read_header_lines(Rest)).

read_n_bytes(0, []) :- !.
read_n_bytes(Count, [Byte|Bytes]) :-
    get_byte(user_input, Byte),
    Byte \== -1,
    Next is Count - 1,
    read_n_bytes(Next, Bytes).

send_response(Id, Result) :-
    send_json(_{jsonrpc:"2.0", id:Id, result:Result}).

send_error(Id, Code, Message) :-
    send_json(_{jsonrpc:"2.0", id:Id, error:_{code:Code, message:Message}}).

send_notification(Method, Params) :-
    send_json(_{jsonrpc:"2.0", method:Method, params:Params}).

send_json(Dict) :-
    atom_json_dict(Atom, Dict, [as(string)]),
    atom_string(Atom, Json),
    string_bytes(Json, Bytes, utf8),
    length(Bytes, Length),
    format(user_output, "Content-Length: ~d\r\n\r\n", [Length]),
    maplist(put_byte(user_output), Bytes),
    flush_output(user_output).

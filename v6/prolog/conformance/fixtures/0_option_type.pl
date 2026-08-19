:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% option(text) desugars to the '__opt_text' enum instance; the column holds
% the instance id and the tag join reads it (ruling option_surface).
fixture(option_text_column_reads_through_tag_join,
    prog(
        [col_type(user_profile/2, user_id, int),
         col_type(user_profile/2, email, option(text)),
         keyed(user_profile/2, [1]),
         col_type(email_state/2, user_id, int),
         col_type(email_state/2, state, text)],
        [(email_state(UserId, State) <-
             user_profile(UserId, EmailOption),
             '__opt_text_tag'(EmailOption, State))]),
    [],
    [
        [+'__opt_text_some'(501, "chris@example.com")],
        [+user_profile(1, 501)],
        [+'__opt_text_none'(502)],
        [+user_profile(2, 502)],
        [-user_profile(2, 502)]
    ],
    [
        final(user_profile/2, [user_profile(1, 501)]),
        final(email_state/2, [email_state(1, some)]),
        deltas(email_state/2, [
            [],
            [+email_state(1, some)],
            [],
            [+email_state(2, none)],
            [-email_state(2, none)]
        ]),
        ticks(5)
    ]).

% One enum per ELEMENT TYPE, never per column site: option(text) and
% option(int) columns in one rel land on two distinct enum instances.
fixture(option_scalar_enums_mint_per_element_type,
    prog(
        [col_type(measurement/3, sensor_id, int),
         col_type(measurement/3, label, option(text)),
         col_type(measurement/3, reading, option(int)),
         keyed(measurement/3, [1])],
        []),
    [],
    [
        [+'__opt_text_some'(601, "warm"), +'__opt_int_none'(701)],
        [+measurement(1, 601, 701)]
    ],
    [
        final('__opt_text_tag'/2, ['__opt_text_tag'(601, some)]),
        final('__opt_int_tag'/2, ['__opt_int_tag'(701, none)]),
        final(measurement/3, [measurement(1, 601, 701)]),
        ticks(2)
    ]).

% option(<rel-ref>) desugars to the companion keyed split rel: the parent
% shrinks one arity, absence is a missing row, presence retracts row-wise.
fixture(option_rel_ref_desugars_to_companion_split_rel,
    prog(
        [col_type(person/2, id, int),
         col_type(person/2, name, text),
         keyed(person/2, [1]),
         col_type(commit/2, id, int),
         col_type(commit/2, reviewed_by, option(person)),
         keyed(commit/2, [1]),
         col_type(reviewed/2, commit_id, int),
         col_type(reviewed/2, reviewer_name, text)],
        [(reviewed(CommitId, ReviewerName) <-
             commit__reviewed_by(CommitId, PersonId),
             person(PersonId, ReviewerName))]),
    [],
    [
        [+person(7, "ada")],
        [+commit(101), +commit(102)],
        [+commit__reviewed_by(101, 7)],
        [-commit__reviewed_by(101, 7)]
    ],
    [
        final(commit/1, [commit(101), commit(102)]),
        final(commit__reviewed_by/2, []),
        final(reviewed/2, []),
        deltas(reviewed/2, [
            [],
            [],
            [+reviewed(101, "ada")],
            [-reviewed(101, "ada")]
        ]),
        ticks(4)
    ]).

% Option enum ids are ordinary non-NULL key values: none and some(value)
% participate in the owner's keyed replacement path.
fixture(option_in_key_column_normalizes,
    prog(
        [col_type(session/2, token, option(text)),
         col_type(session/2, user_id, int),
         keyed(session/2, [1])],
        []),
    [],
    [],
    [ticks(0)]).

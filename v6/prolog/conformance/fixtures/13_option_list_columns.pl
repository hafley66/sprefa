:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% option(list(int)) / option(list(text)) spell and lower: the option desugars
% to a companion split rel whose element is the minted list rel, list members
% carry the scalar values, and an absent (null) list is a missing companion
% row. The derived rel joins companion + list member to reconstruct the values
% of present lists only.
fixture(option_list_column_roundtrips_null_and_present,
    prog(
        [col_type(tags_holder/3, id, int),
         col_type(tags_holder/3, tags, option(list(int))),
         col_type(tags_holder/3, names, option(list(text))),
         keyed(tags_holder/3, [1]),
         col_type(tagged/2, id, int),
         col_type(tagged/2, tag, int),
         col_type(named/2, id, int),
         col_type(named/2, name, text)],
        [(tagged(Id, Tag) <-
              tags_holder__tags(Id, ListId),
              '__gen__list_int_798e673312e7575f__member'(ListId, _, Tag)),
         (named(Id, Name) <-
              tags_holder__names(Id, ListId),
              '__gen__list_text_df210f232c1299bd__member'(ListId, _, Name))]),
    [],
    [[+tags_holder(1),
      +tags_holder__tags(1, 100),
      +'__gen__list_int_798e673312e7575f'('[7,9]'),
      +'__gen__list_int_798e673312e7575f__member'(100, 0, 7),
      +'__gen__list_int_798e673312e7575f__member'(100, 1, 9),
      +tags_holder__names(1, 200),
      +'__gen__list_text_df210f232c1299bd'('["ada","bo"]'),
      +'__gen__list_text_df210f232c1299bd__member'(200, 0, "ada"),
      +'__gen__list_text_df210f232c1299bd__member'(200, 1, "bo")],
     [+tags_holder(2),
      +tags_holder__tags(2, 300),
      +'__gen__list_int_798e673312e7575f'('[11]'),
      +'__gen__list_int_798e673312e7575f__member'(300, 0, 11),
      +tags_holder__names(2, 400),
      +'__gen__list_text_df210f232c1299bd'('["cat"]'),
      +'__gen__list_text_df210f232c1299bd__member'(400, 0, "cat")],
     [+tags_holder(3)]],
    [final(tagged/2, [tagged(1, 7), tagged(1, 9), tagged(2, 11)]),
     final(named/2, [named(1, "ada"), named(1, "bo"), named(2, "cat")]),
     final(tags_holder/1, [tags_holder(1), tags_holder(2), tags_holder(3)]),
     final(tags_holder__tags/2,
           [tags_holder__tags(1, 100), tags_holder__tags(2, 300)]),
     final(tags_holder__names/2,
           [tags_holder__names(1, 200), tags_holder__names(2, 400)]),
     ticks(3)]).

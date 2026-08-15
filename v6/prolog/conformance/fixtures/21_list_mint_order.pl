% 21_list_mint_order.pl : list(T) entities mint by content TEXT, never by
% derivation order.
%
% Rows arrive alpha, bravo, charlie; store_rows/2 (engine.pl) msorts the world
% by the WHOLE row term, so a single-producer rule's derivation order is the
% NAME order alpha < bravo < charlie -- not the arrival order and not the
% split parts' content order. The three source bodies are chosen so those two
% orders disagree: alpha splits to ["z","y"], bravo to ["a","b"], charlie to
% ["m","n"]. Content-TEXT order is bravo < charlie < alpha, so both doors must
% mint bravo=1, charlie=2, alpha=3 -- the opposite of the id order a
% derivation-order mint (or a source-rowid-order emitted INSERT) would give.

:- op(1150, xfx, <-).
:- op(700,  xfx, :=).

fixture(list_mint_order_follows_content_text_not_derivation_order,
  prog([col_type(fruit_text/2, name, text),
        col_type(fruit_text/2, body, text),
        col_type(fruit_parts/2, name, text),
        col_type(fruit_parts/2, parts, list(text))],
       [ (fruit_parts(Name, Parts) <- fruit_text(Name, Body), Parts := split(Body, '/')) ]),
  [ fruit_text(alpha, 'z/y'), fruit_text(bravo, 'a/b'), fruit_text(charlie, 'm/n') ],
  [],
  [ final(fruit_parts/2,
          [ fruit_parts(alpha, 3),
            fruit_parts(bravo, 1),
            fruit_parts(charlie, 2) ]),
    final('__gen__list_text_df210f232c1299bd'/1,
          [ '__gen__list_text_df210f232c1299bd'('["a","b"]'),
            '__gen__list_text_df210f232c1299bd'('["m","n"]'),
            '__gen__list_text_df210f232c1299bd'('["z","y"]') ]),
    final('__gen__list_text_df210f232c1299bd__member'/3,
          [ '__gen__list_text_df210f232c1299bd__member'(1, 0, a),
            '__gen__list_text_df210f232c1299bd__member'(1, 1, b),
            '__gen__list_text_df210f232c1299bd__member'(2, 0, m),
            '__gen__list_text_df210f232c1299bd__member'(2, 1, n),
            '__gen__list_text_df210f232c1299bd__member'(3, 0, z),
            '__gen__list_text_df210f232c1299bd__member'(3, 1, y) ]) ]).

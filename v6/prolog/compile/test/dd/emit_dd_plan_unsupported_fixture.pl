fixture(emit_dd_plan_edge_rule,
        prog([kind(input/1, log), keep(input/1, all),
              kind(output/1, log), keep(output/1, all)],
             [(output(Item) <+ input(Item))]),
        [], [], []).

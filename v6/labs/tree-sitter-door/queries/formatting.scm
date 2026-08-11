[(string) (quoted_atom) (comment)] @leaf

":" @append_space
"rel" @append_space

(relation_declaration "," @append_space)
(atom "," @append_space)
(object_pattern "," @append_space)

(rule
  ["<-" "<+"] @prepend_space @append_hardline @append_indent_start
  (goal_list "," @append_hardline)
  "." @prepend_indent_end @append_hardline)

[
  (relation_declaration "." @append_hardline)
  (fact "." @append_hardline)
]

This explains v0 => v1 => v2 => v3 =(we are here)=> v4 

# v0 -> Can string fts5 get me far for refactoring and code queries
v0 had the goals of asking how can i take 500+ polyglot repos, make generic and custom parsers that extract all strings in high normalization into sql, with 
strings(id, value, norm, norm2), then having physical bytes in a 
refs(id, byte_range, string_id, file_id) and 
file(id, path, content_hash), and 
file_repo(file_id, repo_id, rev_id).

then dynamic rules tables made from all the declarative syntax of a language looking like:

rule xuz > {
  repo("a") > rev("a") > fs("a") > json({special json syntax})/ast({extension of ast-grep pattern lang})
}

(THIS SYNTAX CHANGED IN LATER VERSIIN TO MAKE RULE PARTICIPATE AS OP ITSELF. I have put great time into unifying the data models any chance i can see it. I want all expr language, not stmts + exprs)

query(x) {
  // it looked like datalog, but then we stopped this part bc we focused on parse -> sqlite first in this version and i had you query the fuck out of this db.
}

and it was inspired from codeql/semgrep/ast-grep/biome/oxc study, and wantiung to use rust to be fast as fuck.

was in rust, and i indexed all the strings using normalization algorithm to get rid of punc/numb and the theory was using sqlite fs5 indexes on strings to be able to query norms for cross linking across codebases

The queries were VERY intense, and worked but it was jsut so much sqlite and context. I wanted to program a rules engine that captured those relations in a forward pattern matching pipe syntax a'la my rxjs mastery and using it to understand go and then rust async stuff. 

I also wanted to study the effectiveness of this index all strings idea WITHOUT using rag indexes etc. How much could a user take that without machine learning tech, but just normal stats and some mildl'y programmable deterministic pattern matching like datalog/prolog being "declarative"/rules based.

I barely know prolog, but i wanted super ast-grep.

Then i also wanted super refactor, hence sprefa, where all the codebases are linked with these cross repo links/import/exports/deps/refs across obundaries. like with openapi.json, i wanna see the codegen in realtime of what i would be breaking. so i used oxc/syn for prototyping how hard it was to detect file/folder moves, and automatic refactoring fasteer than light. we got far thanks to oxc and syn for both ts and rs, but that was jsut 2 langs. instead i was indexing every string and drivingt that speed to its max. rust is fast as fuck, and we did have to ignore massive staging/test files etc. so every repo we saw needed an ignore pattern, and norm2 was programmable across all repos. The config, the storage, the way to run it (we did lots of clap) were all crude and highly connected. parser was manual, lsp manual, giant fucking soup mess, but it was mildly useful and performant.

So, that is story of v0, i was in a db study mood, a lang study mood, and i wanted the perf of rust i saw in new js tools but for codebase analysis techniques i had in my head forever as hypotheticals, so i used ai for experimenting. we got so far. Okay, that is the story now understand that every version number was 1 big study or change (using ai to lab out the rust world). I learned hella rust ecosystem/perf tuning thru this.

I also wanted a parallel fact engine that was not rules, but called kinds, then tags, idea was taking any 2 captures ($X, $Y) to become arbitrarily linked so i could hve a table to do recursive queries on.

also, we had file watching for git on disk. then we had lsp that said "this string/file/import/export is referenced in XYZ files within %% certainty" and it was noisy but fucking dope and it worked. we are crawling back there with every considersation layer by layer, version by version we will lab new thing to make a layered cake.

Also this lsp had capture ($X) hover support, it was grotesque but worked, and it was driven from the sqlite db as a cache.

Also json pattern lang was growing.

We also had initialized concepts of operators being repo/rev/fs/json/line

They all took globbish and re: syntax polymoprhically, was retarded but worked somehow.

I realized i need to be able to handle my link/matching for cross repo ref linking that i wanted programmable, to be more robust.

We had a way to say `repo($X)` to say "this is another repo i want and its rev($REV) tha ti want to dynamically pull/walk and apply to other rules". but i had repo op filter list from config db file. so that wasnt right, we need way to dynamically create new captures that were new repo/rev/fs possibilities to parse. Think git tags pointing to git tags, there is depedency graph. main is just current work, not prod, i want to auto crawl ish from programmable rules engine with stats behind when wanted, to program the patterns of pointers across repos and tell me when the go stale (coworker changes key scheme/name/json path of something).

files loaded via cli not config.

# v1: tree sitter and lsp and ops and ast-grep, forking, sprfpath, lowering, trait/DI design init. Fixed point running, trying to maintain old super refactor function.

Okay, for v1, i took it to new level of researching how to make it pluggable, and how to make it _hella_ pluggable, so learned about generic logic in rust, made op, but wanted to do side effects like writing to disk. so there was a bunch of trait considerations here, then learned about tree sitter, and then had some more ideas about operators. i think that was it? couldn't remember, v1 is still on disk here tho.

LSP has memory buffer of file, suspend any writes from this kind of sprf file eval, eval lives where? on server always or some lsp? did both was messy for daemon state on same db writes. 

Store trait design, sqlite one day, memory the next. How do i invalidate rules.

# v2: Tad bit more, learn http server daemon vs lsp vs cli and more trait context design. Introduced "cursor" operating mini syntax, open up dots in op name or other chars to for op to match on, anothjer slot of dsl'ability. Scan pointers were cool till they weren't.
for v2, i lab'd out more trait designs and honestly i forget the most valuable lessons in this one. tons of language evolution, lowering evolution, and runtime logic. performance was fucking awful, so we go into v3. Oh we also tried making lsp a true part of language. we have tree sitter down, and we made more ops. sqlite rule storage was unique case. but i wanted even rule op to be generic/userland. i was going thru a fuck load of langauge ideas and unification after this one into v3, so... I believe we also figured out http vs lsp vs cli and how to reconcile a few more pieces of that. No sqlite this time tho, i dont know if we ever got there.

# v3: Pop off on perf and lang design, get valuable operators, explore semantics. Lock in perf, explore programmable lsp operator, unlocks need for state and flows. Rules and parametric rule flow exploration. Ops move to stream of vecs to stream of vecs of cursors so u can opt into batching at any point. Trying pipes as values. COMMENT operators that i will want to turbo leverage later in my life.
for v3, we studied perofmrance bc perf _choked_ in v2. so v2 has more ops, and we made the effects engine in spirit of redux-saga technique, for future durability etc. we learned limits of mimic'ing perf baseline of ast-grep with rayon etc. using linux kernel as testing ground. we also did actual competent op trait design and set stage for durable execution. We are stuck on relational/reactivity logic and correctness. But we have a lot of useful things. but language semantics is still wonky, bc i want 1 type of value and 1 type of non value (a callable aka an operator in sprf lang terms). we really had to come to terms with parametric rules, idea of empty rules or tag channels, and how to do this with sprf operators we had. we got better at lsp 100% with effects and labbed a bunch more there. we also have mildly programmable lsp logic. i want to take it further. Also i leared idk how to design a language but we tried to commonize the fuck out of things in v3. Also consolidating server runtime. so i am trying to make evrerything in 1 place and yudnerstanding ram ultilization/storage ramifications for running somehting like an ast-grep query over ALL repos (like we did in v0) and look for only string nodes in every language we care about and store it into the string table. in v0 we had built in tables for OG sprefa refactoring ideas, then i opened into making a lanauge. the languge was hosted alongside the import/export analayis, for v4 i want this back but integated as custom op but in part of language. Where we put that script in the folder of all the repos u carea bout.

We added ? to TERM syntax, made ${T} and ${T?} required, weird looking. ${} is called "carvoute" as our version of language splicing

walking git blobs of linux checkout was not free, so learned some checking out perf tests with git2 and jsut raw git shell outs.

improved the shit out of general lsp hover outputs across the board. changes to ALL ops painful, learning progressive trait changes. pipe vs pipe_merge_map trait methds.

# v4: Prime time, master 2 queue system, pure render and effects and pausing, unlock lsp workflows, unlock userland sprf string node querying woith ast-grep ops, bolster LSP trait support, fully understand lang lowering and runtime semantics. Explore sqlite store techniques with DD, improve language design of nesting story, dsls, parens, terms, and yielding/nexting/facting/ruling. needs branching, need to figure formality of the whole ${} shenangians. nested correlated expr chains that can switch to another repo/rev within 1 already matching in scope and further dig to see if some X is mentioned there?

for v4 we are studying dateflow/timely + salsa + lsp helpers so i can make lsp state deterministic, rule row storage and fact (was tag)

Following working groups for v4
1. How to use something like DD/timely and Salsa, this is currently in research phase with claude code, learning how/when/where they are used and work, and why to use it and perf ramifications. DD for push, Salsa for pull it looks like. Ensure bulk effect engine perf does not degrade, simplify reactive joining logic immensely for ourselves tho which would be sick as fuck.

2. operator changes: rule -> rule, tag -> fact, pause points (lazy dispatch, way to send and await receive to continue, for stateful lsp flows at first as goal)
    - We want to be able to say "this is wrong and here is why"
    - literal next and next?() for yield like semantics baked in

3. Further LSP study, how can we make any rules or programmable lsp rules and stateful flows, inspiration is https://oxide.md/Configuration, looks sick, great idea. So designing trait surfaces relative to ops in a resuable manner like effects runtime code. Like how can we make lsp smoother brained. 

4. Lifting cursor and pipes into formalization outside of pipeline and into own crate, then ops plugin to these lower primitives. but we want certain subsystems to be verifiably sound.

5. Language liftoff
    1. Formalize sugar theory of language: Make terms less dumb, ALL_CAPS is shorthand for term(:ALL_CAPS). 
        - This is op that just reads from new cursor design of caps list/map which ever we decide.
    2. Cursor is just content + bytes, no more special repo/rev/fs, all content, next op interprets/sources how it wants with args and cursor's bag of bound variables.
    3.

6. SQLITE and co
    1. dd and relational store, how does that scale with strings/refs table of v0

7. How far can we take programmable lsp op? if we grant stateful pause/conditional/varmatching reference logic in the lang, we can orchestrate workflows, we could have ops that query the state of github-prs /jira/query extenral db/sh was meant to do alot of heavu lifting for doing own flows. if i had workflow state semantics then i could run sh and have revert sh commands etc. Still need to deal with multi value (object and array) container built in value types, pipes types, str is pipe and is type, same as re etc.

8. I want the comment lsp language of my dre4ams. I saw smidge of https://oxide.md/Configuration and im in love for what lsp could really be.

9. Imagine making a way to say "when i say this in claude code chats and you see it on disk, send lsp warning or something dynamically." 

10. Violation checking, hoping dd unlocks negation joins etc. for checking/verifying things.

11. Ensuring store trait in the pipeline running is still versatile enough for us to make memory sink in ci mode fine and we still fast as fuck.

12. git syncing is done by another tool, we just have file watcher events.

13. ID all input events:
  1. LSP Events
  2. Cold start events
  3. Warm start events
  4. git/fs change events
  5. timeouts/ dd events on close.

14. Able to disintuish between worktree folder/porcelain staging area and still handle both.

15. understanding how to query rules and otehr built in operators.


And language:
-- This is a comment
-- This is rule op in its decl form.
-- Rules affect a global symbol table in `RtCtx.rules[symbol] = RuleOp(...)` as part of their contract as an op.
-- operator accept 
-- `rule(:symbol, ...maybeUnboundTerms) BRACE_BODY`
-- calling it is just without body and all bound, querying it is having any unbounds + bounds, sql semijoin etc. (select + where)

rule(:a, X?, Y?) {  
  
}

a()

# Planning-morning notes — 2026-07-18 (verbatim)

The author's messages from the 2026-07-18 session, transcribed unchanged and
in order (voice-to-text included, typos preserved by request). Companion
docs: `docs/vision-auto-architect.md` (the synthesized version),
`chat_log/` (full session context).

---

1. we have a big enough ledger i want the fixes getting done with opus/sonnet/kimi delegation

2. ladder residue, 1. dataflow yes, 2. yes, thought we had a job system lol wtf happened to that, 3. make 2 fix 3, 4. yes, 5. yes, 6. never eprintln i want correct logging/tracing

3. explan what you need from me simplt

4. explain more of 2 and 3, like i just woke up and read only 1 message

5. i mean 1 family could still hog, we really should partition the work total into chunks that are reasonable, instead of arbitrarily slicing an open set

6. like total megabyes of files or folders or something idk. i mean that is a reasonable correlation for ast parsing i guess. also scip or indexing should be slotted a certain way too i ghuess

7. for graph measures, i mean we like all of them its for the research

8. Don't forget you have Kimmy

9. did we ever get the chaos dl file where we force ourselves to use every op

10. also did our n+1 calls actually provably find pending n+1?

11. do we have every kind of callable inevery lang? are we usong scip for dataflow at all internally?

12. call it EntityKind::Lambda, its only appropriate. its a nested non top level func, therefore its inner, k, post this table up and if we can verify it then please stamp it with comment_node jutsu and make it possible to tell when we do have this support for each. we need we _need_ ctors bc ctors are philosophically and always a func call, behind the parens

    if we dont have this then taint tracking is ass for df/scip/call fams

13. i mean also rust everything as well, like, we want it all for both diet and actual scip

14. are there any shared things in df/call/scip/type families?

15. type family covers a lot of ctor relations, although perhaps the ctor tree and assignment tree would help us understand the relations between ctors

16. and entrypoints inthe app

17. what is current SOTA for static analysis lineage that i know im not inventing, is this what people do with glean and codeql

18. and then, how does the setimtoue/promise/domevents queues work in chromium, like we are doing a job/task scheduler so i mean understanding that generically/mathematically would be useful and how does async queuing work in asyncio and tokio. okay make a fable planning agent go to town on how we can make our jobqueing scheduler tasker whatever be physically correct, i dont think that doing it by type in an order is good enough lmfao. in a frontier tick, if we _know what tasks point to what files_, then we have a list of potential people that need a single file, or a repo,  by some scoping possibility, but idea is that we need smarter partitioning and ordering in a way that doesnt betray throughput and space requirements. i mean i guess we really do just have to use k8s or idk, im at a certain point ready to sue postgres for this shit lmfao, i really dont want to pull out celery into a fucking ruist project but idk how rust eco system works and IM TIRED OF PROMPTING BESPOKE CODE WHILE IM PROTOTYPINGH

19. okay and if we dont have to make our own graph algos, whats the cost/negatives of using petgraph  Or some other popular graph theory, topology theory library in Rust. Like, I absolutely would not mind having one of those just because it probably does its job really well. The only problem with that is we have a hard time marking side effects in this repo where it's either an HTTP call, a syscall, a socket call, like any kind of, like even a print line, like any loop, like, I don't know. I'm trying to keep this code base fucking correct. Man, is it hard

20. and then, what are the biggest files bc its time for a refactor/code decomposition/code normalization plan arc

21. And regarding the effects, I mean, I don't know. Effects are also like any usage of a library. Like, go find all of the code files that use the libraries. In fact, some of the refactoring should be based on what is the affinity for the library to this code versus its instantiation or data flow cardinality and locality and stuff like that. We typically use object-oriented programming to do that. We don't use that in Rust a lot, so that's a bit of a boner. But we should probably use struck more. I know DI sucks and all that stuff, but it's like it is a nice way to organize the code. You do get the idea that all of these functions only matter for as long as this thing's alive. But I digressDon't over-index on that. I don't want you to take that last statement. That's not the one I want to wait. The real stuff is like, yeah, like, I want to know generically how can we support any code base to be able to say, oh, these are common effects. Like, this is a SQL call. This is something that could, you know, this is something that technically is just an effect, you know, like from the Haskell sense. So it could actually pause the fucking program or break it.This includes locks. Like, I don't know. There's a lot of locks that we've had to fight in this. And if we knew the data flow analysis from the entry point, that one of those locks would be held. And if it was longer than another, like basically, we need to be able to know the left-right parentheses over time of locks, like in the sense of literally expressing it as RxJS throughout the code, like as a way of saying, when this thing happens, okay, now we listen for the right parenthesis, but the other one may not come for long, and then we get hit with an exhaust map, and it's like, shit.Or worse, a concat map, and then now we're fucked because now we have to wait. And that's basically like, don't over-index on the RXGS part, but like, that's you know, locks are hard. So, if there was a way for us to do lock analysis, that's also effects. That's also honestly super duper important inside of Rust alone, like uniquely. And then, you know, for something like Go, we could have channel analysis where it's like, yeah, these channels could fucking bork. Like, you didn't pad or bound any of these, you know, that would be sick. So, the idea is: how do we build up to that, getting all of these little subsystem and subgraphs and sub-analysis tools to that level? That's a loftier goal. Sorry, this is a huge message. I'm having a very intense planning morning, and I want to. The day is youngAnd you do really need to start using Kimmy because we are using the fuck out of Claude and I just got my weekly usage back up. So I would prefer not to just dump it all in one day

22. Also, when it comes to something like Airflow, not something I used a lot, but basically a DAG, right? So we have the directed acyclic graph, and the problem is there's a lot of things you can do with that. Like here we have like the entire program is indicating its own DAG, either purely or with impure async next time operators, right? Like we did add some impurity flow to this language. So yeah, I don't know. The idea is that basically I want to be able to have this tool that auto-architectures and auto localizes or finds potential refactored targets, like saying, like, oh, hey, this folder, you know, it really loves this library, or this folder really loves this folder that's all the way the fuck over here by like a lot. Or really, you know, some of those things may not matter at all, and it could just be up to the fucking user's whim. So the idea is: I don't know, this is a tool I want to use in a fuckload of repos, including itself. That's actually where we validate all of it. So sick. Okay. I do want a lot of this. I want everything I just said written down into a human doc of some sort. I don't know. This place is a mess. This whole code base is a mess because I keep not looking at it. So try to put it somewhere sensible that I can find it later. Probably in the docs

23. Because the DAG system, etc., like I'm gonna use this at like hundreds of repos scale. So the problem is I need this graph of relationships, this reactivity graph, the memo graph, that has to be saved into the SQLite, which I know is what we're doing right now. But the idea is: I do kind of want to have this, like, you could either have the dependency graph live in memory, like ready to go, or it's like saved. And we have a common way to say, hey, this event came in, and here's how this event, like, I don't know. We take what we did here, genericize it, because there's a chance I might want to use it in other places where I want a reactivity graph that isn't resident and can do retraction, like a non-resident retraction algorithm. I don't need to save the materialized view the entire time. I need to save the event trigger tree. Like, I don't, or graph, like, I don't know. Or I'm too dumb and don't see the math here that where I would be wrong. Like, maybe DataFlow solved a sub-problem I don't see, but I just, there's got to be a way to do DAGs that are not resonant so that you can scale them or at least incrementally do the graph system on them of tasks and analysis so that you're not like blowing up a computer. Basically, how can I get these things, really hard things, without assuming I have linear infinite swap in memory

24. Oh, so I'm just now reading. I started using voice to text, which is why you got so many messages from me. I have not read any of your messages back, and I'm not even yet. I have. So I am still reading through things. Anyways, another thing to add is that this is also like I love bash, so there's a reason we put the GitHub cacher into here is that I wanted to stretch the language so it could do something impure with time code, like with polling. Because I know that once you solve polling, you get a lot of async shit, which is why I'm again very big on RXJS. So the reason for stretching the language to do that was: one, I didn't want to keep lugging GitHub Cacher around because it was a part of this, right? Because it uses repos and revisions. So, yeah, so that was a pretty dope experiment, and I'm glad I did it. But the everything I learned about parsing types, code analysis, graph theory, graph algorithms, new languages, et cetera, et cetera, like trying to figure out how to use AI, trying to figure out how to scale a database-like thing and how to know when to build versus buy. I mean, I kind of already know a lot of those answers, but yeah, so this whole repo is like I'm copying a lot of things, but I'm trying to also solve a bunch of sub-problems and explore them and research them so that I can help out at work. There's a lot of really advanced things going on, and I just want to make sure that this is just how I learn things, man. Like, I like code. I've always wanted to build something like this, especially the comment node techniques. The comment node techniques are fire

25. Please write all these things I've done and said through to you into a file and don't change my words. Like, I just want to, these are like my notes. Like, I want them written down somewhere. Probably in docs or something.

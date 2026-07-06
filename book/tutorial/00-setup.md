# 0. Setup

> install `dl`, build the fixture repo, meet `dl docs` and `dl examples`.

**Goal:** install `dl`, build the fixture repo every later lesson queries, and
learn the two commands that let you discover the rest of the surface.

## Install

`dl` is one binary. Install it from a clone of this repo (the crate lives at the
repo root):

```sh
cargo install --path . --force
```

Or from git, or grab a prebuilt release. Confirm it runs:

```sh
dl --help | head -3
```

```
datalog over files in repo/rev/time space

Usage: dl [OPTIONS] [PROGRAMS]...
```

## The two discovery commands

Before you write anything, know where the answers live. Two commands read
guides baked into the binary, so they work with no source tree on disk.

`dl docs` lists the reference topics and the book chapters:

```sh
dl docs
```

```
dl docs — read the embedded guides (plain stdout, no pager)

REFERENCE   (dl docs <topic>)
  syntax      the language surface: source ops, body constructs, sinks
  functions   scalar functions callable in a rule head or comparison
  relations   every built-in relation the engine exposes
  examples    one-line summary of every program under examples/
...
```

So `dl docs syntax` prints every operator, `dl docs relations` prints every
built-in relation, `dl docs 1` prints book chapter 1. Reach for these instead of
guessing a relation name or an operator's argument order.

`dl examples` lists the example corpus (also embedded), one summary each:

```sh
dl examples | head -5
```

```
embedded examples (104):
  agent-live.dl
      Live probe for the built-in agent-harness relations (agent.rs).
  anim-deck.dl
      Maintains the machine-written regions of the sprefa chapter in the anim deck
```

`dl examples --show <name>` prints one program to stdout. `dl examples <words>`
searches by meaning. You will return to these in lesson 14.

## Build the fixture

Every lesson runs against one small Rust repo. Build it exactly. The line
numbers in later lessons depend on these bytes.

```sh
mkdir -p notes-app/src && cd notes-app

cat > src/main.rs <<'EOF'
mod app;
mod note;

use app::App;

fn main() {
    let app = App::new();
    app.run();
}
EOF

cat > src/app.rs <<'EOF'
use crate::note::Note;

pub struct App {
    notes: Vec<Note>,
}

impl App {
    pub fn new() -> App {
        App { notes: Vec::new() }
    }

    pub fn run(&self) {
        let note = parse("hello");
        save(note);
    }
}

pub fn parse(text: &str) -> Note {
    Note::new(text)
}

pub fn save(note: Note) {
    log_note(note);
}

pub fn log_note(note: Note) {
    drop(note);
}

pub fn unused_helper() {
    let _ = 1;
}
EOF

cat > src/note.rs <<'EOF'
pub struct Note {
    pub body: String,
    pub pinned: bool,
}

impl Note {
    pub fn new(text: &str) -> Note {
        Note { body: text.to_string(), pinned: false }
    }
}
EOF

git init -q && git add -A && git commit -qm "notes fixture"
```

Three files. Three functions in a call chain (`main` runs `App::run`, which calls
`parse` and `save`, and `save` calls `log_note`). Two structs (`App`, `Note`).
One function nobody calls (`unused_helper`). One call to a function defined
outside the repo (`drop`). Each of those is a fact some later lesson finds.

The `git init` matters: `dl` keys every fact on a `(repo, path, rev)` coordinate,
and the `rev` comes from git. Lesson 1 is about that coordinate.

## Where the programs go

Keep your `.dl` programs anywhere you like. The lessons write them next to the
fixture and point `dl` at the repo with `--root`. A program from a directory
called `dl-lessons` beside `notes-app` would run as:

```sh
dl dl-lessons/01.dl --root notes-app --no-daemon
```

Adjust the paths to wherever you saved the fixture. Every command in this track
ends in `--no-daemon` for the reasons in the [track index](README.md).

## Exercise

Run `dl docs relations` and find the `file` relation. Note its four columns and
their order. Lesson 1 queries exactly this relation.

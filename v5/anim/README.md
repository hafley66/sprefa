# sprefa · animated explainer

A bespoke, animated, scroll-free way to follow code and models step by step.
Each step is one *frame*; the code panel tweens the token delta between frames
(shiki-magic-move), and a D2 graph sits beside it. Drive it with arrow keys.

## Run

```
cd v5/anim
npm install
npm run graphs     # render graphs/*.d2 -> public/*.svg   (needs the d2 binary)
npm run dev        # open the printed localhost URL
```

Arrow keys: `→` / space next, `←` prev.

## The two ways to make an animation

**1. By hand or by AI — author `src/frames.json`.**
It is a JSON array of `{ title, narration, lang, code, graph? }`. The `code` field
is the *full* snapshot at each step; the animator computes the delta. Ask the AI to
write or extend it with the project command:

```
/animate transitive closure over the call graph
```

(Run it as a background agent to author while you keep working.)

**2. From git history — your commits become the frames.**
Commit small, one idea per commit, with a real message. Then:

```
npm run frames -- <range> <path> [lang]
npm run frames -- feat/v5-lsp-diag~5..HEAD v5/src/engine.rs rust
```

Each commit becomes a frame: the file snapshot is the code, the commit subject is
the title, the body is the narration. This is the "spam commit like a save button
with comments" workflow — the git log *is* the storyboard.

## Files

| Path | Role |
|---|---|
| `src/frames.json` | the animation: array of frames |
| `src/Frames.jsx` | the player (magic-move code + graph + keys) |
| `graphs/*.d2` | graph sources; `npm run graphs` renders them to `public/*.svg` |
| `bin/from-git.mjs` | git range -> frames.json |
| `bin/render-d2.sh` | d2 -> svg (supports `--animate-interval` for animated graphs) |

## Cognitive rules baked in

- one idea per frame, so the eye can track the moving tokens
- unchanged code holds position (magic-move token matching)
- same graph node keeps its coordinates across frames (stable d2 ids)
- narration says *why the motion happens*, not what the syntax is

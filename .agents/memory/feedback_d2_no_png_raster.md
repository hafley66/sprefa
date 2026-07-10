---
name: feedback_d2_no_png_raster
description: Never render d2 to PNG/raster — it spawns a headless Chromium that balloons to GBs; use SVG + qlmanage instead
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 66dacec3-a1ea-4dae-9ada-78743b504ae8
---

`d2 file.png` (any d2 raster output target) spawns a **headless Chromium** to
rasterize. On 2026-05-31 a single `d2 --layout elk graphs/types.d2 public/types.png`
ballooned a Chromium helper to **11GB RSS and caused swap thrashing** on the user's mac.

**Why:** d2's SVG path is pure Go (safe, fast). The PNG/PDF path shells out to a
browser engine which is unbounded for large graphs.

**How to apply:**
- Render d2 to **SVG only**: `d2 graphs/x.d2 public/x.svg`. The frame-anim app pans
  SVGs via panzoom anyway — SVG is the real target, PNG was never needed.
- To *view* an SVG as a raster, use macOS native quicklook, which is lightweight:
  `qlmanage -t -s 1600 -o /tmp/ql public/x.svg` → `/tmp/ql/x.svg.png`.
- Do NOT run `d2 ... .png` / `.pdf`, and do not background it (it hides the balloon).

Related: [[reference_frame_anim_animator]] (the d2 + panzoom kit at v5/anim).

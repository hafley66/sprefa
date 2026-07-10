// Shared playwright fixtures for the flow-panel e2e suite.
//
// History: building this harness found a live bug -- the windowing math read
// `document.getElementById('gutterLeft').offsetTop` on an <svg> element, and
// SVGElement does not implement the HTMLElement offsetTop mixin (verified
// false on Chromium 149 and Chrome 150), so renderWindow() computed NaN
// bounds and the list view materialized ZERO rows in any Chromium-family
// browser. The panel now reads `listRows` (a div, same resolved top) at all
// three sites, so no shim is needed and this suite exercises the real code
// path. If the list view ever renders empty again, check those reads first.
import { test as base, expect } from "@playwright/test";

export const test = base.extend({});

export { expect };

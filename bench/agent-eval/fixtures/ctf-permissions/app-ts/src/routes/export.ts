// Export routes. Because the four permission abstractions were introduced in
// different releases, THIS file gates the export permission four different
// ways depending on when each handler was written.
import { User, currentUser } from "../models/user";
import { permissions } from "../perm/service";
import { requirePermission } from "../perm/guard";
import { enforceConfigRule, RuleContext } from "../perm/rules";
import { PERM_EXPORT } from "../perm/constants";

// --- release 1 handler: inline boolean flag (abstraction 1) ---
export function exportCsv(req: any, res: any): void {
  const user: User = currentUser(req);
  if (!user.canExport) {
    res.status(403).send("no export");
    return;
  }
  res.send("csv-data");
}

// --- release 1 handler: a second inline flag gate (abstraction 1) ---
export function exportPdf(req: any, res: any): void {
  const user: User = currentUser(req);
  if (!user.canExport) {
    res.status(403).send("no export");
    return;
  }
  res.send("pdf-data");
}

// --- release 2 handler: permission service (abstraction 2) ---
export function exportJson(req: any, res: any): void {
  const user: User = currentUser(req);
  permissions.require(user, PERM_EXPORT);
  res.send("json-data");
}

// --- release 2 handler: a second service gate (abstraction 2) ---
export function exportXml(req: any, res: any): void {
  const user: User = currentUser(req);
  permissions.require(user, PERM_EXPORT);
  res.send("xml-data");
}

// --- release 4 handler: config-file rule (abstraction 4) ---
export function exportArchive(req: any, res: any): void {
  const user: User = currentUser(req);
  const ctx: RuleContext = { user };
  enforceConfigRule(ctx, PERM_EXPORT);
  res.send("archive-data");
}

// --- release 3 wiring: middleware guard applied at the router (abstraction 3) ---
export function registerExportRoutes(router: any): void {
  router.get("/export/csv", exportCsv);
  router.get("/export/pdf", exportPdf);
  router.get("/export/json", exportJson);
  router.get("/export/xml", exportXml);
  router.get("/export/archive", exportArchive);
  // Bulk export was locked down later with the guard middleware.
  router.post("/export/bulk", requirePermission(PERM_EXPORT), (req: any, res: any) => {
    res.send("bulk-data");
  });
}

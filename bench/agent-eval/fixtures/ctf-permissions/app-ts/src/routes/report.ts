// Reporting routes. DECOYS live here: names that contain "export" but do NOT
// gate the export PERMISSION. `canExportReport` is a UI feature toggle (should
// the "Export report" button render), and `reportExportEnabled` is a
// build-time flag. Neither is a permission check — a correct answer must NOT
// list these lines as export-permission gates.
import { User, currentUser } from "../models/user";

// UI feature toggle, not a permission (whether to SHOW the export button).
const reportExportEnabled = true;

interface ReportView {
  canExportReport: boolean;
}

export function buildReportView(req: any): ReportView {
  const user: User = currentUser(req);
  // Purely presentational: viewers still see the button, they just get a
  // paywall modal. This is NOT the export permission gate.
  return { canExportReport: reportExportEnabled && user.role !== "viewer" };
}

export function renderReport(req: any, res: any): void {
  const view = buildReportView(req);
  if (view.canExportReport) {
    res.send("<button>Export report</button>");
  } else {
    res.send("<div>report</div>");
  }
}

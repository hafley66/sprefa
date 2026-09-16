// Import routes. This gates a DIFFERENT concept, the IMPORT permission
// (`can_import` / PERM_IMPORT / canImport). Structurally parallel to export so
// it is an attractive false positive; it is the negative-control target (a
// task asks for import gates specifically).
import { User, currentUser } from "../models/user";
import { permissions } from "../perm/service";
import { PERM_IMPORT } from "../perm/constants";

// release 1 style: inline flag, but the IMPORT flag.
export function importCsv(req: any, res: any): void {
  const user: User = currentUser(req);
  if (!user.canImport) {
    res.status(403).send("no import");
    return;
  }
  res.send("imported");
}

// release 2 style: service, but the IMPORT permission.
export function importJson(req: any, res: any): void {
  const user: User = currentUser(req);
  permissions.require(user, PERM_IMPORT);
  res.send("imported");
}

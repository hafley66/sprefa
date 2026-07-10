// Abstraction 3 (release 3): a middleware GUARD factory. `requirePermission`
// returns an Express-style handler; it gates a route wherever it is applied,
// so the enforcement site is the APPLICATION in the router, not this factory.
import { User, currentUser } from "../models/user";
import { permissions } from "./service";

export type Handler = (req: any, res: any, next: () => void) => void;

export function requirePermission(perm: string): Handler {
  return (req, res, next) => {
    const user: User = currentUser(req);
    if (!permissions.allows(user, perm)) {
      res.status(403).send(`forbidden: ${perm}`);
      return;
    }
    next();
  };
}

// Abstraction 2 (release 2): a permission SERVICE backed by a role->perm map.
// Enforcement is `permissions.require(user, PERM_EXPORT)`; the concept string
// never appears at the call site.
import { User } from "../models/user";

const ROLE_GRANTS: Record<string, string[]> = {
  admin: ["can_export", "can_import", "can_view"],
  analyst: ["can_export", "can_view"],
  viewer: ["can_view"],
};

export class PermissionService {
  allows(user: User, perm: string): boolean {
    const grants = ROLE_GRANTS[user.role] ?? [];
    return grants.includes(perm);
  }

  require(user: User, perm: string): void {
    if (!this.allows(user, perm)) {
      throw new Error(`permission denied: ${perm}`);
    }
  }
}

export const permissions = new PermissionService();

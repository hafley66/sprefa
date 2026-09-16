// The user account model. Permission fields accreted over several releases;
// `canExport` is the export-data permission this app gates everywhere.
export interface User {
  id: string;
  name: string;
  role: "admin" | "analyst" | "viewer";
  // Direct permission flags (the oldest gating style — release 1).
  canExport: boolean;
  canImport: boolean;
  canView: boolean;
}

export function currentUser(req: { userId: string }): User {
  // Test stand-in; a real app would load from a session store.
  return {
    id: req.userId,
    name: "test",
    role: "analyst",
    canExport: true,
    canImport: false,
    canView: true,
  };
}

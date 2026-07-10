// Abstraction 4 (release 4): a runtime, CONFIG-FILE-driven rule engine. The
// allowed roles per permission live in config/rules.json; `enforceConfigRule`
// reads that file and throws when the current user's role is not listed. The
// enforcement site is the `enforceConfigRule(ctx, PERM_EXPORT)` call.
import * as fs from "fs";
import * as path from "path";
import { User } from "../models/user";

interface RuleFile {
  [perm: string]: { roles: string[] };
}

let cache: RuleFile | null = null;

function loadRules(): RuleFile {
  if (cache) return cache;
  const p = path.join(__dirname, "..", "..", "config", "rules.json");
  cache = JSON.parse(fs.readFileSync(p, "utf-8")) as RuleFile;
  return cache;
}

export interface RuleContext {
  user: User;
}

export function evalConfigRule(ctx: RuleContext, perm: string): boolean {
  const rules = loadRules();
  const rule = rules[perm];
  if (!rule) return false;
  return rule.roles.includes(ctx.user.role);
}

export function enforceConfigRule(ctx: RuleContext, perm: string): void {
  if (!evalConfigRule(ctx, perm)) {
    throw new Error(`config rule denied: ${perm}`);
  }
}

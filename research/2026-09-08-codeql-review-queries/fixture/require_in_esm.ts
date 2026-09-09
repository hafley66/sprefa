import { existsSync } from "node:fs"
export function newest(dir: string): number {
  const { readdirSync } = require("node:fs") as typeof import("node:fs")
  return existsSync(dir) ? readdirSync(dir).length : 0
}

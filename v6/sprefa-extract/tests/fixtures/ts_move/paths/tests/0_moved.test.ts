import { readFile } from "node:fs/promises"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

declare function source(packageName: string, filename: string): Promise<string>
declare const issue: { path: string[] }
declare const segments: string[]

const receipts = new URL("../results/4_fixture_memory.json", import.meta.url)
const fixtureDirectory = new URL("../fixtures/sequence/", import.meta.url)
const fixtures = resolve(import.meta.dirname, "../fixtures/sequence")
const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const bin = fileURLToPath(new URL("../bin", import.meta.url))

const greeting = "hello"
const body = readFile("./b")
const picked = source("grapht", "./15_sequenceGeometry.ts")
const label = issue.path.join(".")
const stem = segments.join("..")
const tail = ["a", "b"].join("./")

export { receipts, fixtureDirectory, fixtures, packageRoot, bin, greeting, body, picked }
export { label, stem, tail }

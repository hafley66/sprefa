import { readFile } from "node:fs/promises"
import { resolve } from "node:path"

const fixtureDirectory = new URL("../fixtures/sequence/", import.meta.url)
const packageRoot = resolve(import.meta.dirname, "..")

const greeting = "hello"
const body = readFile("./b")

export { fixtureDirectory, packageRoot, greeting, body }

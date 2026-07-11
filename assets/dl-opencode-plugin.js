// dl opencode plugin — translates opencode plugin events into `dl --hook
// --dialect opencode` and applies the reply. Written by `dl setup --project`
// to `.opencode/plugins/dl.js`; the source of truth is the dl repo's
// assets/dl-opencode-plugin.js (embedded in the binary).
//
// opencode has no native hook config: lifecycle events reach JS plugins only.
// This plugin is the bridge. Wire contract (ours — dl's opencode dialect):
//   stdin  -> {"kind": <event name>, "session": <session id>, "json": <raw event JSON>}
//   stdout <- {"inject": <text>}   context to surface to the model
//          <- {"block": <reason>}  deny the pending tool call (tool.execute.before)
//          <- (empty)              condition didn't fire
//
// Event mapping (PostToolUse / UserPromptSubmit parity):
//   tool.execute.after   -> kind "PostToolUse"
//   tool.execute.before  -> kind "PreToolUse" (block support)
//   chat.message         -> kind "UserPromptSubmit" (user messages only)

import { spawn } from "node:child_process"

const runDlHook = (payload, directory) =>
  new Promise((resolve) => {
    const child = spawn("dl", ["--hook", "--dialect", "opencode"], {
      cwd: directory,
      stdio: ["pipe", "pipe", "inherit"],
    })
    let stdout = ""
    child.stdout.on("data", (chunk) => (stdout += chunk))
    child.on("error", () => resolve(null)) // dl not on PATH: stay silent
    child.on("close", () => {
      const line = stdout.trim()
      if (!line) return resolve(null)
      try {
        resolve(JSON.parse(line))
      } catch {
        resolve(null)
      }
    })
    child.stdin.end(JSON.stringify(payload))
  })

export const DlHookPlugin = async ({ directory }) => {
  const feed = async (kind, session, event) =>
    runDlHook({ kind, session: session ?? "", json: JSON.stringify(event ?? {}) }, directory)

  return {
    "tool.execute.before": async (input, output) => {
      const reply = await feed("PreToolUse", input?.sessionID, { input, output })
      if (reply?.block) {
        throw new Error(`dl --hook blocked this tool call: ${reply.block}`)
      }
    },
    "tool.execute.after": async (input, output) => {
      const reply = await feed("PostToolUse", input?.sessionID, { input, output })
      if (reply?.inject && output && typeof output.output === "string") {
        // Surface injected context by appending to the tool result the model reads.
        output.output += `\n\n${reply.inject}`
      }
    },
    "chat.message": async (_input, ctx) => {
      const message = ctx?.message
      if (message?.role && message.role !== "user") return
      const reply = await feed("UserPromptSubmit", message?.sessionID, ctx)
      if (reply?.inject && Array.isArray(ctx?.parts)) {
        ctx.parts.push({ type: "text", text: reply.inject })
      }
    },
  }
}

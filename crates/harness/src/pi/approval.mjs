// Host-owned Pi 1.4.0 extension. Loaded explicitly with -e; never installed globally.
//
// Policy (`__HALO_APPROVAL_POLICY__`, refreshed from `__HALO_APPROVAL_POLICY_PATH__`
// on each tool call so the composer chip applies to this process):
//   ask           — confirm every non-read tool
//   approve_edits — auto-allow write/edit; confirm bash
//   approve_safe  — auto-allow edits + bash that does not look unsafe
//   full_access   — allow every tool
import { readFileSync } from "node:fs";

const policy = "__HALO_APPROVAL_POLICY__";
const policyPath = "__HALO_APPROVAL_POLICY_PATH__";
const readTools = new Set(["read", "grep", "find", "ls"]);
const editTools = new Set(["write", "edit"]);
const timeout = 60000;

function looksUnsafe(input) {
  const text = typeof input === "string"
    ? input
    : (input && (input.command || input.cmd || input.script || JSON.stringify(input))) || "";
  const lower = String(text).toLowerCase();
  const patterns = [
    /\brm\s+(-[a-z]*f|.*\s-rf|\s+-fr)/i,
    /\bsudo\b/i,
    /\bchmod\s+(-r\s+)?777\b/i,
    /\bchown\b/i,
    /\bmkfs\b/i,
    /\bdd\s+if=/i,
    /\bcurl\b.+\|\s*(ba)?sh\b/i,
    /\bwget\b.+\|\s*(ba)?sh\b/i,
    /\b>\s*\/dev\/sd/i,
    /\bshutdown\b/i,
    /\breboot\b/i,
    /\bkill\s+-9\b/i,
    /\b:\(\)\s*\{\s*:\|:\s*&\s*\}\s*;/i,
    /\bnc\s+-[le]/i,
    /\bncat\b/i,
    /\bscp\b.+\s+\//i,
    /\bgit\s+push\s+.*--force\b/i,
    /\bgit\s+reset\s+--hard\b/i,
    /\bdrop\s+(database|table)\b/i,
    /\btruncate\s+table\b/i,
  ];
  return patterns.some((re) => re.test(lower));
}

function currentPolicy() {
  if (!policyPath || policyPath.startsWith("__")) return policy;
  try {
    const text = readFileSync(policyPath, "utf8").trim();
    if (text === "ask" || text === "approve_edits" || text === "approve_safe" || text === "full_access") {
      return text;
    }
  } catch {
    // Keep the policy baked at launch.
  }
  return policy;
}

function shouldAsk(toolName, input) {
  const active = currentPolicy();
  if (readTools.has(toolName)) return false;
  if (active === "full_access") return false;
  if (active === "approve_edits") {
    return !editTools.has(toolName);
  }
  if (active === "approve_safe") {
    if (editTools.has(toolName)) return false;
    if (toolName === "bash") return looksUnsafe(input);
    return true;
  }
  // ask (default)
  return true;
}

export default function approval(pi) {
  let ready = false;
  const pending = new Set();
  pi.on("tool_call", async (event, ctx) => {
    const denied = { block: true, reason: "Tool execution was not approved by the host." };
    if (!ready || ctx.mode !== "rpc" || !ctx.hasUI || ctx.signal?.aborted) return denied;
    if (!shouldAsk(event.toolName, event.input)) return;
    const controller = new AbortController();
    const abort = () => controller.abort();
    ctx.signal?.addEventListener("abort", abort, { once: true });
    pending.add(controller);
    let timer;
    let watch;
    try {
      // Bound the hook itself as well as the RPC dialog. Exceptions always deny.
      const expired = new Promise(resolve => {
        timer = setTimeout(() => { controller.abort(); resolve(false); }, timeout);
      });
      // The composer can switch to a looser mode while this dialog is open.
      const cleared = new Promise(resolve => {
        watch = setInterval(() => {
          if (!shouldAsk(event.toolName, event.input)) resolve("cleared");
        }, 200);
      });
      const result = await Promise.race([
        ctx.ui.confirm("Allow tool execution?", `${event.toolName}\n${JSON.stringify(event.input).slice(0, 12000)}`,
          { timeout, signal: controller.signal }),
        expired,
        cleared,
      ]);
      if (result === "cleared") {
        controller.abort();
        if (ready && !ctx.signal?.aborted) return;
      }
      if (result === true && ready && !controller.signal.aborted && !ctx.signal?.aborted) return;
    } catch {
      // Pi may otherwise treat a thrown extension hook as advisory and run the tool.
    } finally {
      clearTimeout(timer);
      clearInterval(watch);
      pending.delete(controller);
      ctx.signal?.removeEventListener("abort", abort);
    }
    return denied;
  });
  pi.on("session_start", async (_event, ctx) => {
    ready = ctx.mode === "rpc" && ctx.hasUI === true;
    if (ready) ctx.ui.setStatus("halo-approval-v1", "__HALO_APPROVAL_NONCE__");
  });
  pi.on("session_shutdown", async () => {
    ready = false;
    for (const controller of pending) controller.abort();
    pending.clear();
  });
}

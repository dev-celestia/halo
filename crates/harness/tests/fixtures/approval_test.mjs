// Network-free fake of the Pi 1.4.0 extension API. Exercises the bundled hook.
import { readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const nonce = "nonce-under-test";
const source = readFileSync(new URL("../../src/pi/approval.mjs", import.meta.url), "utf8");
const file = join(tmpdir(), `halo-approval-${process.pid}.mjs`);
writeFileSync(file, source.replaceAll("__HALO_APPROVAL_NONCE__", nonce));
const { default: approval } = await import(pathToFileURL(file).href);

function fakePi() {
  const handlers = {};
  return {
    on(name, fn) {
      handlers[name] = fn;
    },
    async emit(name, event, ctx) {
      return handlers[name](event, ctx);
    },
  };
}

function ctx(overrides = {}) {
  const calls = [];
  const base = {
    mode: "rpc",
    hasUI: true,
    signal: undefined,
    ui: {
      setStatus(key, text) {
        calls.push(["setStatus", key, text]);
      },
      confirm() {
        return Promise.resolve(true);
      },
    },
    calls,
  };
  return { ...base, ...overrides, ui: { ...base.ui, ...(overrides.ui ?? {}) }, calls };
}

function assert(condition, message) {
  if (!condition) {
    console.error(message);
    process.exit(1);
  }
}

const pi = fakePi();
approval(pi);

const unready = await pi.emit("tool_call", { toolName: "write", input: { path: "a.rs" } }, ctx());
assert(unready?.block === true, "tools must fail closed before the hook is ready");

const readWhileUnready = await pi.emit("tool_call", { toolName: "read", input: {} }, ctx());
assert(readWhileUnready?.block === true, "even read tools fail closed before the hook is ready");

const started = ctx();
await pi.emit("session_start", {}, started);
assert(
  started.calls.some((call) => call[0] === "setStatus" && call[1] === "halo-approval-v1" && call[2] === nonce),
  "ready status must echo the host nonce",
);
const read = await pi.emit("tool_call", { toolName: "read", input: { path: "a.rs" } }, ctx());
assert(read == null, "read, grep, find and ls pass once the hook is ready");
for (const toolName of ["grep", "find", "ls"]) {
  const passed = await pi.emit("tool_call", { toolName, input: {} }, ctx());
  assert(passed == null, `${toolName} passes without a dialog`);
}

const noUi = fakePi();
approval(noUi);
await noUi.emit("session_start", {}, ctx({ hasUI: false, mode: "rpc" }));
const blind = await noUi.emit("tool_call", { toolName: "bash", input: { command: "ls" } }, ctx({ hasUI: false }));
assert(blind?.block === true, "missing UI must deny");

const denied = await pi.emit(
  "tool_call",
  { toolName: "edit", input: { path: "a.rs" } },
  ctx({ ui: { confirm: async () => false } }),
);
assert(denied?.block === true, "a negative confirm denies");

const allowed = await pi.emit(
  "tool_call",
  { toolName: "write", input: { path: "a.rs" } },
  ctx({ ui: { confirm: async () => true } }),
);
assert(allowed == null, "an explicit confirm allows the call");

const thrown = await pi.emit(
  "tool_call",
  { toolName: "bash", input: { command: "rm" } },
  ctx({
    ui: {
      confirm: async () => {
        throw new Error("disconnected");
      },
    },
  }),
);
assert(thrown?.block === true, "a failed dialog denies");

const aborted = new AbortController();
aborted.abort();
const cancelled = await pi.emit(
  "tool_call",
  { toolName: "write", input: {} },
  ctx({ signal: aborted.signal }),
);
assert(cancelled?.block === true, "cancellation denies");

const originalTimeout = globalThis.setTimeout;
globalThis.setTimeout = (fn) => {
  fn();
  return 0;
};
try {
  const timedOut = await pi.emit(
    "tool_call",
    { toolName: "write", input: { path: "slow.rs" } },
    ctx({ ui: { confirm: () => new Promise(() => {}) } }),
  );
  assert(timedOut?.block === true, "timeout denies");
} finally {
  globalThis.setTimeout = originalTimeout;
}

await pi.emit("session_shutdown", {}, ctx());
const afterShutdown = await pi.emit("tool_call", { toolName: "write", input: {} }, ctx());
assert(afterShutdown?.block === true, "shutdown returns the hook to fail-closed");

const policyFile = join(tmpdir(), `halo-approval-policy-${process.pid}`);
writeFileSync(policyFile, "ask");
const liveFile = join(tmpdir(), `halo-approval-live-${process.pid}.mjs`);
writeFileSync(
  liveFile,
  source
    .replaceAll("__HALO_APPROVAL_NONCE__", nonce)
    .replaceAll("__HALO_APPROVAL_POLICY__", "ask")
    .replaceAll("__HALO_APPROVAL_POLICY_PATH__", policyFile),
);
const { default: live } = await import(pathToFileURL(liveFile).href);
const piLive = fakePi();
live(piLive);
await piLive.emit("session_start", {}, ctx());
const stillAsking = await piLive.emit(
  "tool_call",
  { toolName: "bash", input: { command: "ls -la" } },
  ctx({ ui: { confirm: async () => false } }),
);
assert(stillAsking?.block === true, "ask mode still confirms a command");
writeFileSync(policyFile, "full_access");
const freed = await piLive.emit(
  "tool_call",
  { toolName: "bash", input: { command: "ls -la" } },
  ctx(),
);
assert(freed == null, "full access read from the policy file skips the dialog");
writeFileSync(policyFile, "approve_edits");
const edit = await piLive.emit("tool_call", { toolName: "edit", input: { path: "a.rs" } }, ctx());
assert(edit == null, "approve edits allows a file edit without a dialog");
const command = await piLive.emit(
  "tool_call",
  { toolName: "bash", input: { command: "ls -la" } },
  ctx({ ui: { confirm: async () => false } }),
);
assert(command?.block === true, "approve edits still confirms a command");

console.log("approval hook denied unready, blind, negative, thrown, aborted, timed-out and post-shutdown calls");

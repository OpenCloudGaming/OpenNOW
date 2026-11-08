import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

async function fixture(closed, exited = true) {
  const calls = [];
  const state = { launcherAppId: 42, games: [] };
  const context = vm.createContext({ console, Date });
  const dependencies = {
    "@decky/ui": {
      ConfirmModal() {},
      Navigation: { CloseSideMenus() { calls.push("focus"); } },
      Router: { NavigateToRunningApp() { calls.push("navigate"); } },
      showModal(element) { element.props.onOK(); },
      sleep: async () => {},
    },
    "@decky/api": { toaster: { toast() {} } },
    "react/jsx-runtime": { jsx: (type, props) => ({ type, props }) },
    "./backend": { backend: {
      async closeOpenNOW() {
        calls.push("request");
        if (closed instanceof Error) throw closed;
        return closed;
      },
    } },
    "./steam": {
      isRunning: (id) => id === 42,
      terminate() { calls.push("terminate"); },
      launch(id) { calls.push(`launch:${id}`); },
      async waitForExit() { calls.push("wait"); return exited; },
    },
    "./stores": { storeLabel: (value) => value },
    "./store": {
      store: { get: () => ({ state, status: { running: false } }) },
      async refresh() { calls.push("refresh"); },
    },
  };
  const source = ts.transpileModule(readFileSync(new URL("../src/opennow.tsx", import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
  }).outputText;
  const module = new vm.SourceTextModule(source, { context });
  await module.link((specifier) => {
    const values = dependencies[specifier];
    assert.ok(values, specifier);
    return new vm.SyntheticModule(Object.keys(values), function () {
      for (const [name, value] of Object.entries(values)) this.setExport(name, value);
    }, { context });
  });
  await module.evaluate();
  return { calls, api: module.namespace };
}

test("closing requests Qt confirmation without terminating Steam", async () => {
  const { api, calls } = await fixture(true);
  await api.closeOpenNOW();
  assert.deepEqual(calls, ["navigate", "focus", "request", "wait", "refresh"]);
});

test("cancelled Qt confirmation never launches a replacement game", async () => {
  const { api, calls } = await fixture(false);
  await api.launchGame(99);
  assert.deepEqual(calls, ["refresh", "navigate", "focus", "request"]);
});

test("replacement launch waits for graceful shutdown and Steam exit", async () => {
  const { api, calls } = await fixture(true);
  await api.launchGame(99);
  assert.deepEqual(calls, ["refresh", "navigate", "focus", "request", "wait", "refresh", "launch:99"]);
});

test("a failed graceful request never launches a replacement", async () => {
  const { api, calls } = await fixture(new Error("request timed out"));
  await assert.rejects(api.launchGame(99), /request timed out/);
  assert.deepEqual(calls, ["refresh", "navigate", "focus", "request"]);
});

test("Steam exit timeout does not terminate or launch anything", async () => {
  const { api, calls } = await fixture(true, false);
  await assert.rejects(api.launchGame(99), /didn't close/);
  assert.deepEqual(calls, ["refresh", "navigate", "focus", "request", "wait"]);
});

import { spawn, type ChildProcess } from "node:child_process";
import { createConnection } from "node:net";
import { fileURLToPath } from "node:url";

const driverHost = "127.0.0.1";
const driverPort = 4_444;
const nativeDriverPort = 4_445;
const driverReadyTimeout = 15_000;
const application = fileURLToPath(
  new URL("../../.cache/cargo-target/debug/eutheto-desktop", import.meta.url),
);

let tauriDriver: ChildProcess | undefined;

function waitForDriver(child: ChildProcess): Promise<void> {
  const { promise, reject, resolve } = Promise.withResolvers<undefined>();
  let settled = false;
  let retryTimer: NodeJS.Timeout | undefined;
  const readinessPorts = [driverPort, nativeDriverPort] as const;
  let readinessIndex = 0;

  function cleanup(): void {
    clearTimeout(retryTimer);
    clearTimeout(timeoutTimer);
    child.off("error", onError);
    child.off("exit", onExit);
  }
  function finish(error?: Error): void {
    if (settled) return;
    settled = true;
    cleanup();
    if (error === undefined) resolve(undefined);
    else reject(error);
  }
  function onError(error: Error): void {
    finish(error);
  }
  function onExit(code: number | null, signal: NodeJS.Signals | null): void {
    const detail = code === null ? `signal ${signal ?? "unknown"}` : `code ${code.toString()}`;
    finish(
      new Error(
        `tauri-driver exited before ports ${readinessPorts.join(", ")} were ready (${detail})`,
      ),
    );
  }
  function connect(): void {
    if (settled) return;
    const port = readinessPorts[readinessIndex];
    if (port === undefined) {
      finish();
      return;
    }
    const socket = createConnection({
      host: driverHost,
      port,
    });
    socket.once("connect", () => {
      socket.destroy();
      readinessIndex += 1;
      connect();
    });
    socket.once("error", () => {
      socket.destroy();
      if (!settled) retryTimer = setTimeout(connect, 100);
    });
  }

  const timeoutTimer = setTimeout(() => {
    finish(
      new Error(`tauri-driver did not open ports ${readinessPorts.join(", ")} within 15 seconds`),
    );
  }, driverReadyTimeout);
  child.once("error", onError);
  child.once("exit", onExit);
  connect();
  return promise;
}

async function stopDriver(): Promise<void> {
  const child = tauriDriver;
  tauriDriver = undefined;
  if (child === undefined || child.exitCode !== null || child.signalCode !== null) return;
  const runningDriver = child;

  const { promise, resolve } = Promise.withResolvers<undefined>();
  let settled = false;
  function finish(): void {
    if (settled) return;
    settled = true;
    clearTimeout(forceTimer);
    runningDriver.off("exit", finish);
    resolve(undefined);
  }

  const forceTimer = setTimeout(() => {
    if (runningDriver.exitCode === null && runningDriver.signalCode === null) {
      runningDriver.kill("SIGKILL");
    }
  }, 5_000);
  runningDriver.once("exit", finish);
  if (!runningDriver.kill("SIGTERM")) finish();
  await promise;
}

export const config = {
  runner: "local",
  host: driverHost,
  port: driverPort,
  specs: ["./e2e/**/*.spec.ts"],
  maxInstances: 1,
  maxInstancesPerCapability: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": {
        application,
      },
    },
  ],
  logLevel: "warn",
  reporters: ["spec"],
  framework: "mocha",
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 1,
  mochaOpts: {
    ui: "bdd",
    timeout: 120_000,
  },
  onPrepare: async () => {
    const child = spawn(
      "tauri-driver",
      ["--port", driverPort.toString(), "--native-port", nativeDriverPort.toString()],
      {
        stdio: "inherit",
      },
    );
    tauriDriver = child;
    try {
      await waitForDriver(child);
    } catch (error) {
      await stopDriver();
      throw error;
    }
  },
  onComplete: async () => {
    await stopDriver();
  },
};

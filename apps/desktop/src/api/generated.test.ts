import { beforeEach, describe, expect, it, vi } from "vitest";

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => tauri);

import { getFoundationStatus } from "./generated";

describe("generated desktop API", () => {
  beforeEach(() => {
    tauri.invoke.mockReset();
  });

  it("invokes the single coarse foundation command", async () => {
    const status = {
      capability: "phase_00_foundation" as const,
      schemaVersion: 1 as const,
    };
    tauri.invoke.mockResolvedValue(status);

    await expect(getFoundationStatus()).resolves.toEqual(status);
    expect(tauri.invoke).toHaveBeenCalledWith("app_get_foundation_status");
    expect(tauri.invoke).toHaveBeenCalledTimes(1);
  });
});

import { createSSRApp, h } from "vue";
import { renderToString } from "@vue/server-renderer";
import { describe, expect, it } from "vitest";

import FoundationStatusPanel from "./FoundationStatusPanel.vue";
import type { FoundationStatusView } from "../foundation-status";

async function render(phase: FoundationStatusView): Promise<string> {
  return renderToString(
    createSSRApp({
      render: () => h(FoundationStatusPanel, { phase }),
    }),
  );
}

describe("FoundationStatusPanel", () => {
  it("announces loading without claiming product authority", async () => {
    const html = await render({ state: "loading" });

    expect(html).toContain('role="status"');
    expect(html).toContain("Checking the local application foundation");
  });

  it("renders the authoritative foundation DTO and its boundary", async () => {
    const html = await render({
      state: "ready",
      status: { capability: "phase_00_foundation", schemaVersion: 1 },
    });

    expect(html).toContain("Phase 00 repository foundation");
    expect(html).toContain("Version 1");
    expect(html).toContain("Domain planning and solver features are not part");
  });

  it("renders an alert and retry control when the command fails", async () => {
    const html = await render({ state: "error" });

    expect(html).toContain('role="alert"');
    expect(html).toContain("Try again");
  });
});

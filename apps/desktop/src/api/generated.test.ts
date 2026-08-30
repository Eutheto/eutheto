import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => tauri);

import * as generated from "./generated";
import type {
  ApiResponseDto,
  BackupSummaryDto,
  CatalogCommand,
  FixedExclusion,
  ImportOptions,
  PortableCountsDto,
  OmittedAssetDto,
  PortableScenarioDto,
  ProjectScope,
  ProjectSummaryDto,
  SourceBackupSelectionDto,
} from "./generated";

const REPRESENTATIVE_COMMANDS = [
  "app_get_info",
  "project_list",
  "project_import_preview",
  "project_backup_create",
  "project_restore_apply",
  "scenario_apply_command",
  "settings_update",
] as const satisfies readonly CatalogCommand[];

const REPRESENTATIVE_DEFERRED_COMMANDS = [
  "solve_start",
  "solution_get_view",
  "ai_send_turn",
] as const satisfies readonly CatalogCommand[];

const UUID_V7_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

const PATH_BEARING_REQUEST_KEY = /^(?:sourceArtifact|fileName|path|url)$/i;

function collectKeys(value: unknown): readonly string[] {
  if (Array.isArray(value)) {
    return value.flatMap(collectKeys);
  }
  if (typeof value !== "object" || value === null) {
    return [];
  }
  return Object.entries(value).flatMap(([key, nested]) => [key, ...collectKeys(nested)]);
}

describe("generated desktop API", () => {
  beforeEach(() => {
    tauri.invoke.mockReset();
    vi.stubGlobal("window", {
      crypto: {
        getRandomValues(bytes: Uint8Array) {
          bytes.fill(0x2a);
          return bytes;
        },
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("exports a unique snake-case command catalog with Phase-01 and deferred keys", () => {
    expect(new Set(generated.COMMAND_CATALOG).size).toBe(generated.COMMAND_CATALOG.length);
    expect(generated.COMMAND_CATALOG).toEqual(
      expect.arrayContaining([...REPRESENTATIVE_COMMANDS, ...REPRESENTATIVE_DEFERRED_COMMANDS]),
    );
    expect(
      generated.COMMAND_CATALOG.every((command) => /^[a-z]+(?:_[a-z0-9]+)+$/.test(command)),
    ).toBe(true);
  });

  it("propagates a UUIDv7 request ID inside the generated request envelope", async () => {
    const response = {
      schemaVersion: generated.API_SCHEMA_VERSION,
      requestId: "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a",
      currentRevision: null,
      warnings: [],
      result: [],
    } satisfies ApiResponseDto<readonly ProjectSummaryDto[]>;
    const scope = "archived" satisfies ProjectScope;
    tauri.invoke.mockResolvedValue(response);

    await expect(generated.listProjects(scope)).resolves.toBe(response);
    expect(tauri.invoke).toHaveBeenCalledWith("project_list", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        scope,
      },
    });
    expect(tauri.invoke).toHaveBeenCalledTimes(1);
  });

  it("models every portable preview count including historical revisions", () => {
    const counts = {
      scenarios: 1,
      scenarioRevisions: 2,
      results: 3,
      sharedRecords: 4,
      preferences: 5,
      assets: 6,
    } satisfies PortableCountsDto;
    expect(counts).toEqual({
      scenarios: 1,
      scenarioRevisions: 2,
      results: 3,
      sharedRecords: 4,
      preferences: 5,
      assets: 6,
    });
  });

  it("models durable scenario revision allocation warnings", () => {
    const tombstoned = {
      scenarioId: "01900000-0000-7000-8000-000000000099",
      title: "Previously deleted roster",
      collides: true,
      sourceRevision: 2,
      sameIdentityRevision: 6,
      sameIdentityRevisionWarning: "This ID was tombstoned; importing resumes at revision 6.",
    } satisfies PortableScenarioDto;
    expect(tombstoned).toMatchObject({
      sourceRevision: 2,
      sameIdentityRevision: 6,
    });
    expect(tombstoned.sameIdentityRevisionWarning).toContain("tombstoned");
  });

  const fixedExclusions = [
    "local-undo-and-audit-history",
    "sqlite-and-database-internals",
    "credentials-tokens-and-keychain-references",
    "device-local-paths-and-window-state",
    "logs-caches-and-temporary-data",
    "redistribution-prohibited-provider-data",
    "executable-content",
  ] as const satisfies readonly FixedExclusion[];

  it("models explicit source exclusions and omitted asset placeholders", () => {
    const selection = {
      includeResults: false,
      assetSelection: "v1-threshold",
      thresholdVersion: 1,
      thresholdBytes: 16_777_216,
      excludedAssetCount: 1,
      excludedAssetIds: ["large-video.mp4"],
      scope: "library",
      fixedExclusions,
    } satisfies SourceBackupSelectionDto;
    const omitted = {
      assetId: "large-video.mp4",
      format: "eutheto/omitted-asset",
      version: 1,
      reason: "above-v1-threshold",
      originalMediaType: "video/mp4",
      originalSize: 20_000_000,
      contentSha256: "a".repeat(64),
    } satisfies OmittedAssetDto;
    expect(selection.includeResults).toBe(false);
    expect(selection.excludedAssetIds).toEqual([omitted.assetId]);
    expect(omitted).toMatchObject({
      reason: "above-v1-threshold",
      originalMediaType: "video/mp4",
      originalSize: 20_000_000,
    });
    const inherited = {
      includeResults: true,
      assetSelection: "all",
      excludedAssetCount: 1,
      excludedAssetIds: ["inherited-placeholder.png"],
      exclusionScope: "inherited-placeholder",
      thresholdVersion: null,
      thresholdBytes: null,
      fixedExclusions,
    } satisfies BackupSummaryDto;
    expect(inherited).toMatchObject({
      assetSelection: "all",
      excludedAssetIds: ["inherited-placeholder.png"],
    });
    expect(selection.fixedExclusions).toEqual(fixedExclusions);
    expect(inherited.fixedExclusions).toEqual(fixedExclusions);
  });

  it("exports the Phase-01 project transfer and support-preview wrappers without invoking Tauri", () => {
    expect(generated).toMatchObject({
      listProjects: expect.any(Function),
      getProjectMetadata: expect.any(Function),
      createProject: expect.any(Function),
      duplicateProject: expect.any(Function),
      setProjectArchived: expect.any(Function),
      deleteProject: expect.any(Function),
      previewImport: expect.any(Function),
      applyImport: expect.any(Function),
      previewExport: expect.any(Function),
      createExport: expect.any(Function),
      previewBackup: expect.any(Function),
      createBackup: expect.any(Function),
      previewRestore: expect.any(Function),
      applyRestore: expect.any(Function),
      previewSupportBundle: expect.any(Function),
    });
    expect(tauri.invoke).not.toHaveBeenCalled();
  });

  it("keeps native file selections out of project transfer invoke payloads", async () => {
    const options = {
      restoreMode: "import-scenario",
      includeResults: true,
      includeAssets: false,
    } satisfies ImportOptions;
    const scenarioId = "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a";
    const previewId = "01900000-0000-7000-8000-000000000070";
    tauri.invoke.mockResolvedValue(undefined);

    await generated.previewImport(options);
    await generated.previewRestore(options);
    await generated.createBackup("Before migration", previewId);
    await generated.createExport(scenarioId, previewId);

    expect(tauri.invoke).toHaveBeenNthCalledWith(1, "project_import_preview", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        options,
      },
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(2, "project_restore_preview", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        options,
      },
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(3, "project_backup_create", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        title: "Before migration",
        previewId,
      },
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(4, "project_export_create", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        scenarioId,
        previewId,
      },
    });

    for (const [, payload] of tauri.invoke.mock.calls) {
      expect(collectKeys(payload)).not.toEqual(
        expect.arrayContaining([expect.stringMatching(PATH_BEARING_REQUEST_KEY)]),
      );
    }
  });

  it("preserves the maximum safe revision exactly in invoke payloads", async () => {
    const scenarioId = "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a";
    tauri.invoke.mockResolvedValue(undefined);

    await generated.undoScenario(scenarioId, Number.MAX_SAFE_INTEGER);

    expect(tauri.invoke).toHaveBeenCalledWith("scenario_undo", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        scenarioId,
        expectedRevision: Number.MAX_SAFE_INTEGER,
      },
    });
  });

  it.each([
    ["above the safe integer cap", Number.MAX_SAFE_INTEGER + 1],
    ["fractional", 1.5],
    ["negative", -1],
  ])("rejects %s revision input before invoking Tauri", async (_label, revision) => {
    const scenarioId = "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a";

    await expect(generated.redoScenario(scenarioId, revision)).rejects.toThrow(
      "Revision must be a non-negative JavaScript safe integer",
    );
    expect(tauri.invoke).not.toHaveBeenCalled();
  });

  it("propagates a UUIDv7 request ID when previewing a support bundle", async () => {
    tauri.invoke.mockResolvedValue(undefined);

    await generated.previewSupportBundle();

    expect(tauri.invoke).toHaveBeenCalledWith("app_create_support_bundle_preview", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
      },
    });
    expect(tauri.invoke).toHaveBeenCalledTimes(1);
  });
});

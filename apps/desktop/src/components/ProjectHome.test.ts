import { createSSRApp, h } from "vue";
import { renderToString } from "@vue/server-renderer";
import { describe, expect, it, vi, type Mock } from "vitest";

import type { FixedExclusion, ScenarioChangedEvent, ValidationIssue } from "../api/generated";
import ProjectHome from "./ProjectHome.vue";
import {
  defaultSupplementalCollisionChoices,
  createProjectHomeController,
  recoverFocus,
  scenarioRevisionOutcome,
  type ProjectHomeApi,
  type ProjectHomeController,
  type ProjectSummary,
} from "../project-home";

type ProjectHomeApiMocks = {
  [Method in keyof ProjectHomeApi]: ProjectHomeApi[Method] extends (
    ...args: infer Arguments
  ) => infer Result
    ? Mock<(...args: Arguments) => Result>
    : never;
};
const project: ProjectSummary = {
  scenarioId: "01900000-0000-7000-8000-000000000001",
  title: "Clinic roster",
  domainPackId: "official.test",
  revision: 3,
  updatedAt: "2026-08-29T12:00:00Z",
  archived: false,
};
const previewWarning: ValidationIssue = {
  code: "portable.preview.warning",
  severity: "warning",
  message: "Review migrated portable data.",
  fieldPath: null,
  resource: null,
};
const fixedExclusions = [
  "local-undo-and-audit-history",
  "sqlite-and-database-internals",
  "credentials-tokens-and-keychain-references",
  "device-local-paths-and-window-state",
  "logs-caches-and-temporary-data",
  "redistribution-prohibited-provider-data",
  "executable-content",
] as const satisfies readonly FixedExclusion[];
const fixedExclusionLabels = [
  "Local undo and audit history",
  "SQLite and database internals",
  "Credentials, tokens, and keychain references",
  "Device-local paths and window state",
  "Logs, caches, and temporary data",
  "Redistribution-prohibited provider data",
  "Executable content",
] as const;

function response<T>(result: T, warnings: readonly ValidationIssue[] = []) {
  return {
    schemaVersion: 1 as const,
    requestId: "01900000-0000-7000-8000-000000000099",
    currentRevision: null,
    warnings,
    result,
  };
}

function fakeApi(projects: ProjectSummary[] = []): ProjectHomeApiMocks {
  return {
    listProjects: vi.fn(() => Promise.resolve(response([...projects]))),
    createProject: vi.fn(() => Promise.resolve(response({}))),
    duplicateProject: vi.fn(() => Promise.resolve(response({}))),
    setProjectArchived: vi.fn(() => Promise.resolve(response({}))),
    deleteProject: vi.fn(() => Promise.resolve(response({}))),
    previewImport: vi.fn(() => Promise.resolve(response(portablePreview("scenario-export")))),
    applyImport: vi.fn(() => Promise.resolve(response({}))),
    previewBackup: vi.fn((title) =>
      Promise.resolve(
        response({
          title,
          byteLength: 4096,
          previewId: "01900000-0000-7000-8000-000000000070",
          digest: "b".repeat(64),
          currentRevision: null,
          libraryRevision: 1,
          backupSummary: {
            includeResults: true,
            assetSelection: "all" as const,
            excludedAssetCount: 1,
            excludedAssetIds: ["inherited-placeholder.png"],
            exclusionScope: "inherited-placeholder",
            thresholdVersion: null,
            thresholdBytes: null,
            fixedExclusions,
          },
        }),
      ),
    ),
    createBackup: vi.fn(() =>
      Promise.resolve(response({ artifactName: "before-changes.eutheto" })),
    ),
    previewRestore: vi.fn(() => Promise.resolve(response(portablePreview("full-backup")))),
    applyRestore: vi.fn(() => Promise.resolve(response({}))),
    cancelPortablePreview: vi.fn(() => Promise.resolve(response({}))),
    onAppNotification: vi.fn(() => Promise.resolve(vi.fn())),
    onLibraryRefreshRequired: vi.fn(() => Promise.resolve(vi.fn())),
    onScenarioChanged: vi.fn(() => Promise.resolve(vi.fn())),
    onScenarioValidationChanged: vi.fn(() => Promise.resolve(vi.fn())),
  };
}

function portablePreview(bundleKind: "scenario-export" | "full-backup") {
  return {
    previewId: "01900000-0000-7000-8000-000000000010",
    bundleId: "01900000-0000-7000-8000-000000000011",
    bundleKind,
    title: bundleKind === "full-backup" ? "Nightly backup" : "Imported roster",
    createdAt: "2026-08-29T11:00:00Z",
    sourceApplication: { name: "eutheto-core", version: "0.1.0" },
    sourceFormatVersion: 1,
    sourceSchemaVersion: 1,
    counts: {
      scenarios: 1,
      scenarioRevisions: 2,
      results: 0,
      sharedRecords: 4,
      preferences: 5,
      assets: 6,
    },
    requiredCapabilities: [{ id: "portable.history", version: 1 }],
    preservedExtensions: ["example.extension"],
    includedSections: ["scenarios", "results", "shared-records", "preferences", "assets"],
    sourceBackupSelection: {
      includeResults: false,
      assetSelection: "v1-threshold" as const,
      thresholdVersion: 1,
      thresholdBytes: 16_777_216,
      excludedAssetCount: 1,
      excludedAssetIds: ["large-video.mp4"],
      fixedExclusions,
      scope: bundleKind === "full-backup" ? ("library" as const) : ("scenario" as const),
    },
    omittedAssets: [
      {
        assetId: "large-video.mp4",
        format: "eutheto/omitted-asset",
        version: 1,
        reason: "above-v1-threshold" as const,
        originalMediaType: "video/mp4",
        originalSize: 20_000_000,
        contentSha256: "a".repeat(64),
      },
    ],
    excludedSections: [],
    scenarios: [
      {
        scenarioId: project.scenarioId,
        title: project.title,
        collides: true,
        sourceRevision: 2,
        sameIdentityRevision: 6,
        sameIdentityRevisionWarning:
          "A deleted project previously used this ID; importing resumes at revision 6.",
      },
    ],
    supplementalCollisions: [],
    removedScenarios:
      bundleKind === "full-backup"
        ? [
            {
              scenarioId: project.scenarioId,
              title: project.title,
              revision: project.revision,
              archived: project.archived,
            },
          ]
        : [],
    removedSupplemental: [],
    settingsChanged: bundleKind === "full-backup" ? ["appearance"] : [],
    settingsRemoved: bundleKind === "full-backup" ? ["units"] : [],
    appliedMigrations: [
      { registry: "portable", name: "portable-v0-to-v1", fromVersion: 0, toVersion: 1 },
    ],
  } as const;
}

async function render(home: ProjectHomeController): Promise<string> {
  return renderToString(
    createSSRApp({
      render: () => h(ProjectHome, { home }),
    }),
  );
}

function projectAt(home: ProjectHomeController, index: number): ProjectSummary {
  const item = home.state.projects[index];
  expect(item).toBeDefined();
  if (item === undefined) {
    throw new Error(`Expected project at index ${String(index)}`);
  }
  return item;
}

describe("ProjectHome", () => {
  it("renders distinct loading and empty states", async () => {
    const loading = createProjectHomeController(fakeApi());
    expect(await render(loading)).toContain("Loading saved projects");

    await loading.load();
    const emptyHtml = await render(loading);
    expect(emptyHtml).toContain("Begin with a local project");
    expect(emptyHtml).toContain("Create official.test project");
  });

  it("renders a user-safe message from a structured API error", async () => {
    const api = fakeApi();
    api.listProjects.mockRejectedValueOnce({
      category: "storage",
      code: "local_library.unavailable",
      message: "Library unavailable",
    });
    const home = createProjectHomeController(api);

    await home.load();

    const html = await render(home);
    expect(html).toContain('role="alert"');
    expect(html).toContain("Library unavailable");
    expect(html).toContain("Try again");
  });

  it("does not render a raw runtime error message", async () => {
    const api = fakeApi();
    const runtimeMessage = "Cannot read properties of undefined (reading 'invoke')";
    api.listProjects.mockRejectedValueOnce(new Error(runtimeMessage));
    const home = createProjectHomeController(api);

    await home.load();

    const html = await render(home);
    expect(html).toContain("The local project library could not complete that request.");
    expect(html).not.toContain(runtimeMessage);
  });

  it("creates with explicit official.test settings and reloads authoritative state", async () => {
    const saved: ProjectSummary[] = [];
    const api = fakeApi(saved);
    api.listProjects.mockImplementation(() => Promise.resolve(response([...saved])));
    api.createProject.mockImplementation((input) => {
      saved.push({ ...project, title: input.title });
      return Promise.resolve(response({}));
    });
    const home = createProjectHomeController(api);
    await home.load();
    await home.createProject({
      title: "Clinic roster",
      description: "Autumn plan",
      domainPack: { id: "official.test", schemaVersion: 1 },
      settings: {
        timeZone: "UTC",
        locale: "en-US",
        units: "metric",
        horizon: { start: "2026-09-01T00:00:00Z", end: "2026-10-01T00:00:00Z" },
        gapPolicy: "reject",
        overlapPolicy: "earlier",
      },
    });

    expect(api.createProject).toHaveBeenCalledWith(
      expect.objectContaining({ domainPack: { id: "official.test", schemaVersion: 1 } }),
    );
    expect(api.listProjects).toHaveBeenCalledTimes(2);
    expect(home.state.projects).toEqual(saved);
    expect(await render(home)).toContain("Open project Clinic roster");
  });

  it("archives, unarchives, duplicates, and deletes only after reloading saved data", async () => {
    const saved = [{ ...project }];
    const api = fakeApi(saved);
    api.listProjects.mockImplementation(() =>
      Promise.resolve(response(saved.map((item) => ({ ...item })))),
    );
    api.setProjectArchived.mockImplementation(({ scenarioId, archived }) => {
      const item = saved.find((candidate) => candidate.scenarioId === scenarioId);
      if (item) item.archived = archived;
      return Promise.resolve(response({}));
    });
    api.duplicateProject.mockImplementation(({ title }) => {
      saved.push({ ...project, scenarioId: "01900000-0000-7000-8000-000000000002", title });
      return Promise.resolve(response({}));
    });
    api.deleteProject.mockImplementation((scenarioId) => {
      const index = saved.findIndex((candidate) => candidate.scenarioId === scenarioId);
      if (index >= 0) saved.splice(index, 1);
      return Promise.resolve(response({}));
    });
    const home = createProjectHomeController(api);
    await home.load();

    await home.setArchived(projectAt(home, 0));
    expect(home.state.projects[0]?.archived).toBe(true);
    await home.setArchived(projectAt(home, 0));
    expect(home.state.projects[0]?.archived).toBe(false);
    await home.duplicateProject(projectAt(home, 0), "Clinic roster copy");
    expect(home.state.projects).toHaveLength(2);
    await home.deleteProject(projectAt(home, 1));
    expect(home.state.projects).toHaveLength(1);
    expect(api.setProjectArchived).toHaveBeenCalledWith(
      expect.objectContaining({ expectedRevision: project.revision }),
    );
    expect(api.duplicateProject).toHaveBeenCalledWith(
      expect.objectContaining({ expectedRevision: project.revision }),
    );
    expect(api.deleteProject).toHaveBeenCalledWith(expect.any(String), project.revision);

    const html = await render(home);
    expect(html).toContain("Archive project");
    expect(html).toContain("Duplicate project as");
    expect(html).toContain("Delete project");
  });

  it("resolves reviewed scenario revisions from the selected collision action", () => {
    const scenario = portablePreview("scenario-export").scenarios[0];
    expect(scenarioRevisionOutcome(scenario, "create-copy")).toEqual({
      revision: 2,
      warning: null,
    });
    expect(scenarioRevisionOutcome(scenario, "replace")).toEqual({
      revision: 6,
      warning: scenario.sameIdentityRevisionWarning,
    });
    expect(scenarioRevisionOutcome(scenario, "skip")).toEqual({
      revision: null,
      warning: null,
    });
    expect(
      scenarioRevisionOutcome(
        { ...scenario, collides: false, title: "Tombstoned identity" },
        undefined,
      ),
    ).toEqual({
      revision: 6,
      warning: scenario.sameIdentityRevisionWarning,
    });
  });

  it("renders collision review for chooser-backed import and add/replace restore previews", async () => {
    const api = fakeApi([project]);
    const supplementalIdentity = { section: "preferences" as const, key: "view.json" };
    const sharedIdentity = { section: "shared-records" as const, key: "team.json" };
    const assetIdentity = { section: "assets" as const, key: "logo.png" };
    const supplementalIdentities = [supplementalIdentity, sharedIdentity, assetIdentity] as const;
    const supplementalDefaults = defaultSupplementalCollisionChoices(supplementalIdentities);
    expect(supplementalDefaults).toEqual({
      [`preferences\u0000view.json`]: "skip",
      [`shared-records\u0000team.json`]: "skip",
      [`assets\u0000logo.png`]: "skip",
    });
    api.previewImport.mockResolvedValue(
      response(
        {
          ...portablePreview("scenario-export"),
          supplementalCollisions: supplementalIdentities,
        },
        [previewWarning],
      ),
    );
    api.previewRestore.mockResolvedValue(
      response(
        {
          ...portablePreview("full-backup"),
          supplementalCollisions: supplementalIdentities,
          removedSupplemental: supplementalIdentities,
        },
        [previewWarning],
      ),
    );
    const home = createProjectHomeController(api);
    await home.load();
    await home.previewImport({ includeResults: true, includeAssets: true });
    let html = await render(home);
    expect(api.previewImport).toHaveBeenCalledWith({
      restoreMode: "import-scenario",
      includeResults: true,
      includeAssets: true,
    });
    expect(html).toContain("Import preview: Imported roster");
    expect(html).toContain("Collision action");
    expect(html).toContain("Existing supplemental records");
    expect(html).toContain("Supplemental collision action");
    expect(html).toContain("team.json");
    expect(html).toContain("logo.png");
    expect(html).toContain("Apply reviewed import");
    expect(html).toContain("Include retained results");
    expect(html).toContain("Include referenced assets");
    expect(html).toContain("Create a copy");
    expect(html).toContain("eutheto-core");
    expect(html).toContain("2026-08-29T11:00:00Z");
    expect(html).toContain("format 1, schema 1");
    expect(html).toContain("2 historical revisions");
    expect(html).toContain("0 results");
    expect(html).toContain("4 shared records");
    expect(html).toContain("5 preferences");
    expect(html).toContain("6 assets");
    expect(html).toContain("explicitly excluded");
    expect(html).toContain("v1-threshold");
    expect(html).toContain("large-video.mp4");
    expect(html).toContain("above-v1-threshold");
    expect(html).toContain("video/mp4");
    expect(html).toContain("20000000 bytes");
    expect(html).toContain("portable.history");
    expect(html).toContain("example.extension");
    expect(html).toContain("portable-v0-to-v1");
    expect(html).toContain("Review migrated portable data.");
    expect(html).toContain("Source revision 2");
    expect(html).toContain("selected outcome revision");
    expect(html).toContain("2");
    expect(html).not.toContain('role="alert"');
    expect(html).not.toContain("Same-identity revision warning:");
    for (const label of fixedExclusionLabels) {
      expect(html).toContain(label);
      expect(html.indexOf(label)).toBeLessThan(html.indexOf("Apply reviewed import"));
    }
    expect(html).not.toContain("everything is included");
    await home.applyImport(
      {
        [project.scenarioId]: "create-copy",
        "01900000-0000-7000-8000-000000000099": "skip",
      },
      supplementalDefaults,
    );
    expect(api.applyImport).toHaveBeenCalledWith({
      previewId: "01900000-0000-7000-8000-000000000010",
      collisionPlan: {
        scenarios: { [project.scenarioId]: "create-copy" },
        supplementalChoices: [
          { ...supplementalIdentity, action: "skip" },
          { ...sharedIdentity, action: "skip" },
          { ...assetIdentity, action: "skip" },
        ],
      },
    });

    await home.previewRestore("add-backup");
    html = await render(home);
    expect(api.previewRestore).toHaveBeenLastCalledWith({
      restoreMode: "add-backup",
      includeResults: true,
      includeAssets: true,
    });
    expect(html).toContain("Add preview:");
    for (const label of fixedExclusionLabels) expect(html).toContain(label);
    expect(html).toContain("Review and confirm restore");
    expect(html).toContain("Source revision 2");
    expect(html).toContain("selected outcome revision");
    expect(html).not.toContain("Same-identity revision warning:");

    await home.previewRestore("replace-library");
    html = await render(home);
    expect(html).toContain("selected outcome revision");
    expect(html).toContain("6");
    expect(html).toContain('role="alert"');
    expect(html).toContain("Same-identity revision warning:");
    expect(html).toContain("A deleted project previously used this ID");
    expect(html).toContain("Library replacement:");
    expect(html).toContain("projects absent from this backup will be removed");
    expect(html).toContain("Current projects that will be removed");
    expect(html).toContain("revision 3");
    expect(html).toContain("active");
    expect(html).toContain("Current supplemental records that will be replaced or removed");
    expect(html).toContain("Application setting changes");
    expect(html).toContain("appearance");
    expect(html).toContain("units");
    expect(html).toContain("2 historical revisions");
    expect(html).toContain("2026-08-29T11:00:00Z");
    expect(html).toContain("portable.history");
    expect(html).toContain("example.extension");
    expect(html).toContain("portable-v0-to-v1");
    expect(html).toContain("0 results");
    expect(html).toContain("explicitly excluded");
    expect(html).toContain("large-video.mp4");
    expect(html).toContain("above-v1-threshold");
    expect(html).toContain("video/mp4");
    expect(html).toContain("Review migrated portable data.");
    expect(html).toContain("Included by library replacement");
    expect(html).not.toContain("Supplemental collision action");
    expect(html).not.toContain(`restore-collision-${project.scenarioId}`);
    const replaceDefaults = defaultSupplementalCollisionChoices(supplementalIdentities, "replace");
    expect(Object.values(replaceDefaults)).toEqual(["replace", "replace", "replace"]);
    await home.applyRestore({ [project.scenarioId]: "create-copy" }, replaceDefaults);
    expect(api.applyRestore).toHaveBeenLastCalledWith({
      previewId: "01900000-0000-7000-8000-000000000010",
      collisionPlan: {
        scenarios: {},
        supplementalChoices: [],
      },
      authorization: {
        destructiveActionConfirmed: true,
        safetyBackupBypassPhrase: null,
      },
    });
  });

  it("previews and saves a backup through the native Save dialog", async () => {
    const api = fakeApi([project]);
    const home = createProjectHomeController(api);
    await home.load();
    await home.previewBackup("Before changes");
    const html = await render(home);
    expect(html).toContain("Prepared library revision 1");
    expect(html).toContain("b".repeat(64));
    expect(html).toContain("Backup preview: Before changes");
    expect(html).toContain("4,096 bytes");
    expect(html).toContain("Save backup file");
    expect(html).toContain("Results included");
    expect(html).toContain("Asset selection: all");
    expect(html).toContain("1 preserved omission placeholder");
    expect(html).toContain("inherited-placeholder.png");
    expect(html).toContain("inherited-placeholder");
    for (const label of fixedExclusionLabels) {
      expect(html).toContain(label);
      expect(html.indexOf(label)).toBeLessThan(html.indexOf("Save backup file"));
    }
    expect(html).not.toContain("everything is included");

    await home.createBackup("Before changes");
    expect(api.createBackup).toHaveBeenCalledWith(
      "Before changes",
      "01900000-0000-7000-8000-000000000070",
    );
    expect(api.listProjects).toHaveBeenCalledTimes(2);
    expect(home.state.announcement).toBe("Backup saved as before-changes.eutheto.");
    expect(home.state.announcement).not.toContain("/");
  });

  it("treats typed native-dialog cancellation as a non-mutating result", async () => {
    const api = fakeApi([project]);
    const home = createProjectHomeController(api);
    await home.load();
    await home.previewImport({ includeResults: true, includeAssets: true });
    await home.previewRestore("add-backup");
    await home.previewBackup("Before changes");
    home.state.errorMessage = "Keep this notice";
    home.state.announcement = "Keep this announcement";
    const importPreview = home.state.importPreview;
    const restorePreview = home.state.restorePreview;
    const projects = [...home.state.projects];
    const cancelled = {
      category: "protocol",
      code: "operation.cancelled",
      message: "No file was selected.",
      retryable: true,
    };

    api.previewImport.mockRejectedValueOnce(cancelled);
    api.previewRestore.mockRejectedValueOnce(cancelled);
    api.createBackup.mockRejectedValueOnce(cancelled);

    expect(await home.previewImport({ includeResults: false, includeAssets: false })).toBe(false);
    expect(await home.previewRestore("replace-library")).toBe(false);
    expect(await home.createBackup("Before changes")).toBe(false);
    expect(home.state.busyAction).toBeNull();
    expect(home.state.importPreview).toBe(importPreview);
    expect(home.state.restorePreview).toBe(restorePreview);
    expect(home.state.backupPreview).toBeNull();
    expect(home.state.restoreMode).toBe("add-backup");
    expect(home.state.projects).toEqual(projects);

    expect(home.state.errorMessage).toBe("Keep this notice");
    expect(home.state.announcement).toBe("Keep this announcement");
    expect(api.listProjects).toHaveBeenCalledOnce();
  });
  it("retains a failed replace preview only for an informed second backup bypass", async () => {
    const api = fakeApi([project]);
    const home = createProjectHomeController(api);
    await home.load();
    await home.previewRestore("replace-library");
    api.applyRestore
      .mockRejectedValueOnce({
        category: "protocol",
        code: "restore.safety_backup_failed",
        message: "The private backup destination is unavailable.",
        retryable: false,
      })
      .mockResolvedValueOnce(response({}));

    expect(await home.applyRestore({}, {})).toBe(false);
    expect(home.state.restorePreview).not.toBeNull();
    expect(home.state.restoreSafetyBackupFailure).toBe(
      "The private backup destination is unavailable.",
    );
    let html = await render(home);
    expect(html).toContain("Safety backup failed:");
    expect(html).toContain("REPLACE WITHOUT BACKUP");

    expect(await home.applyRestore({}, {}, "REPLACE WITHOUT BACKUP")).toBe(true);
    expect(home.state.restorePreview).toBeNull();
    expect(home.state.restoreSafetyBackupFailure).toBeNull();
    expect(api.applyRestore).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({
        authorization: expect.objectContaining({ safetyBackupBypassPhrase: null }),
      }),
    );
    expect(api.applyRestore).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        authorization: expect.objectContaining({
          safetyBackupBypassPhrase: "REPLACE WITHOUT BACKUP",
        }),
      }),
    );
    html = await render(home);
    expect(html).not.toContain("Safety backup failed:");
  });

  it("announces revision conflicts and reloads the authoritative project list", async () => {
    const saved = [{ ...project }];
    const api = fakeApi(saved);
    api.listProjects.mockImplementation(() =>
      Promise.resolve(response(saved.map((item) => ({ ...item })))),
    );
    api.duplicateProject.mockRejectedValueOnce({
      category: "conflict",
      code: "project.revision_conflict",
      message: "Revision conflict",
    });
    const home = createProjectHomeController(api);
    await home.load();
    const savedProject = saved[0];
    expect(savedProject).toBeDefined();
    if (savedProject === undefined) {
      throw new Error("Expected the saved project to exist");
    }
    saved[0] = { ...savedProject, title: "Authoritative title", revision: 4 };
    await home.duplicateProject(project, "Copy");

    expect(api.listProjects).toHaveBeenCalledTimes(2);
    expect(home.state.projects[0]?.title).toBe("Authoritative title");
    expect(home.state.announcement).toContain("changed in another window");
    expect(await render(home)).toContain('aria-live="polite"');
  });

  it("refreshes from native scenario events and disposes listeners and previews", async () => {
    const api = fakeApi([project]);
    let changed: ((event: ScenarioChangedEvent) => void) | undefined;
    const unlistenChanged = vi.fn();
    const unlistenValidation = vi.fn();
    let notification: (() => void) | undefined;
    const unlistenNotification = vi.fn();
    let refreshRequired: (() => void) | undefined;
    const unlistenRefreshRequired = vi.fn();
    api.onScenarioChanged.mockImplementation((listener) => {
      changed = listener;
      return Promise.resolve(unlistenChanged);
    });
    api.onAppNotification.mockImplementation((listener) => {
      notification = listener;
      return Promise.resolve(unlistenNotification);
    });
    api.onScenarioValidationChanged.mockResolvedValue(unlistenValidation);
    api.onLibraryRefreshRequired.mockImplementation((listener) => {
      refreshRequired = listener;
      return Promise.resolve(unlistenRefreshRequired);
    });
    const home = createProjectHomeController(api);
    await home.startEventListeners();
    await home.load();
    await home.previewImport({ includeResults: true, includeAssets: true });

    changed?.({
      type: "scenarioChanged",
      payload: {
        context: {
          eventVersion: 1,
          timestamp: "2026-08-29T12:00:00Z",
          requestId: "01900000-0000-7000-8000-000000000099",
          scenarioId: project.scenarioId,
          revision: project.revision,
          solveRunId: null,
        },
        changeSet: {},
      },
    });
    await Promise.resolve();
    expect(api.listProjects).toHaveBeenCalledTimes(2);
    notification?.();
    await Promise.resolve();
    expect(api.listProjects).toHaveBeenCalledTimes(3);
    refreshRequired?.();
    await Promise.resolve();
    expect(api.listProjects).toHaveBeenCalledTimes(4);

    await home.dispose();
    expect(unlistenChanged).toHaveBeenCalledOnce();
    expect(unlistenValidation).toHaveBeenCalledOnce();
    expect(unlistenNotification).toHaveBeenCalledOnce();
    expect(unlistenRefreshRequired).toHaveBeenCalledOnce();
    expect(api.cancelPortablePreview).toHaveBeenCalledWith("01900000-0000-7000-8000-000000000010");
  });

  it("provides explicit accessible names and a focus-recovery contract", async () => {
    const home = createProjectHomeController(fakeApi([project]));
    await home.load();
    const html = await render(home);
    expect(html).toContain('aria-label="Open project Clinic roster"');
    expect(html).toContain("Project title");
    expect(html).toContain("Choose import file");
    expect(html).toContain("Choose backup file");
    expect(html).toContain("Cancelling a file chooser or Save dialog");
    expect(html).toContain('tabindex="-1"');

    const focus = vi.fn();
    recoverFocus({ focus });
    expect(focus).toHaveBeenCalledOnce();
  });
});

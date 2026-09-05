import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }));
const tauriEvents = vi.hoisted(() => ({ listen: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => tauri);
vi.mock("@tauri-apps/api/event", () => tauriEvents);

import * as generated from "./generated";
import type {
  ApiResponseDto,
  BackupSummaryDto,
  CatalogCommand,
  DeferredSolverGateDto,
  DomainPackDescriptorDto,
  FixedExclusion,
  ImportOptions,
  OmittedAssetDto,
  PortableCountsDto,
  PortableScenarioDto,
  ProjectScope,
  ProjectSummaryDto,
  SolverSupportMatrixDto,
  SourceBackupSelectionDto,
  UnopenedBundlePreviewDto,
} from "./generated";

const REPRESENTATIVE_COMMANDS = [
  "app_get_info",
  "pack_list",
  "pack_describe",
  "solver_list",
  "solver_describe",
  "solver_get_support_matrix",
  "solver_get_deferred_gates",
  "project_list",
  "project_import_preview",
  "project_backup_create",
  "project_restore_apply",
  "scenario_apply_command",
  "project_unopened_bundle_inspect",
  "project_unopened_bundle_reexport",
  "settings_update",
] as const satisfies readonly CatalogCommand[];

const REPRESENTATIVE_DEFERRED_COMMANDS = [
  "solve_start",
  "solution_lock_assignment",
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
    tauriEvents.listen.mockReset();
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
      listDomainPacks: expect.any(Function),
      describeDomainPack: expect.any(Function),
      listSolvers: expect.any(Function),
      describeSolver: expect.any(Function),
      getSolverSupportMatrix: expect.any(Function),
      getDeferredSolverGates: expect.any(Function),
      inspectUnopenedBundle: expect.any(Function),
      reexportUnopenedBundle: expect.any(Function),
      cancelPortablePreview: expect.any(Function),
    });
    expect(tauri.invoke).not.toHaveBeenCalled();
  });

  it("exports exactly one wrapper for every implemented solution command", () => {
    expect(generated).toMatchObject({
      listSolutions: expect.any(Function),
      getSolutionSummary: expect.any(Function),
      getSolutionView: expect.any(Function),
      selectSolution: expect.any(Function),
      verifySolution: expect.any(Function),
      compareSolutions: expect.any(Function),
      explainSolution: expect.any(Function),
      startCounterfactual: expect.any(Function),
      cancelCounterfactual: expect.any(Function),
    });
    expect(tauri.invoke).not.toHaveBeenCalled();
  });

  it("invokes all solution commands with strict versioned identity-based request shapes", async () => {
    const scenarioId = "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a";
    const solutionId = "01900000-0000-7000-8000-000000000070";
    const candidateSolutionId = "01900000-0000-7000-8000-000000000071";
    const jobId = "01900000-0000-7000-8000-000000000072";
    const explanation = {
      schemaVersion: 1,
      subject: { kind: "validation", issueId: null },
    } as const satisfies generated.ExplanationRequestV1;
    const condition = {
      type: "forceAssignmentValue",
      assignmentId: "shift.primary",
      value: { type: "boolean", value: true },
    } as const satisfies generated.CounterfactualConditionPayloadV1;
    tauri.invoke.mockResolvedValue(undefined);

    await generated.listSolutions(scenarioId);
    await generated.getSolutionSummary(scenarioId, solutionId);
    await generated.getSolutionView({ scenarioId, solutionId, viewId: "schedule" });
    await generated.selectSolution({ scenarioId, solutionId, expectedRevision: 7 });
    await generated.verifySolution(scenarioId, solutionId);
    await generated.compareSolutions({
      scenarioId,
      baseSolutionId: solutionId,
      candidateSolutionId,
    });
    await generated.explainSolution({ scenarioId, request: explanation });
    await generated.startCounterfactual({
      scenarioId,
      expectedRevision: 7,
      baseSolutionId: solutionId,
      condition,
      totalBudgetMilliseconds: 5_000,
    });
    await generated.cancelCounterfactual({ scenarioId, expectedRevision: 7, jobId });

    const calls = tauri.invoke.mock.calls;
    expect(calls.map(([command]) => command)).toEqual([
      "solution_list",
      "solution_get_summary",
      "solution_get_view",
      "solution_select",
      "solution_verify",
      "solution_compare",
      "solution_explain",
      "solution_start_counterfactual",
      "solution_cancel_counterfactual",
    ]);
    expect(calls[0]?.[1]).toEqual({
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        schemaVersion: generated.SOLUTION_API_SCHEMA_VERSION,
        scenarioId,
      },
    });
    expect(calls[5]?.[1]).toEqual({
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        schemaVersion: generated.SOLUTION_API_SCHEMA_VERSION,
        scenarioId,
        baseSolutionId: solutionId,
        candidateSolutionId,
      },
    });
    expect(calls[6]?.[1]).toEqual({
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        schemaVersion: generated.SOLUTION_API_SCHEMA_VERSION,
        scenarioId,
        request: explanation,
      },
    });
    expect(calls[7]?.[1]).toEqual({
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        schemaVersion: generated.COUNTERFACTUAL_API_SCHEMA_VERSION,
        scenarioId,
        expectedRevision: 7,
        baseSolutionId: solutionId,
        condition,
        totalBudgetMilliseconds: 5_000,
      },
    });
    expect(calls[8]?.[1]).toEqual({
      request: {
        cancelRequestId: expect.stringMatching(UUID_V7_PATTERN),
        schemaVersion: generated.COUNTERFACTUAL_API_SCHEMA_VERSION,
        scenarioId,
        expectedRevision: 7,
        jobId,
      },
    });
  });

  it("models every explanation discriminant and nullable counterfactual lifecycle state", () => {
    const scenarioId = "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a";
    const solutionId = "01900000-0000-7000-8000-000000000070";
    const runId = "01900000-0000-7000-8000-000000000071";
    const assignment = {
      id: "shift.primary",
      entity: { kind: "shift", id: "primary" },
      value: { type: "integer", value: "9007199254740993" },
      evidence: [],
    } as const satisfies generated.DomainAssignment;
    const score = {
      feasibility: "0",
      levels: [
        {
          levelId: "preference",
          value: "9007199254740993",
          direction: "maximize",
          categoryBreakdown: { fairness: "9007199254740993" },
        },
      ],
    } as const satisfies generated.ScoreVector;
    const binding = {
      packId: "official.test",
      scenarioId,
      scenarioRevision: 7,
      documentHash: "a".repeat(64),
      projectionVersion: 1,
      verificationScopeChecksum: "b".repeat(64),
      acceptedResult: { solutionId, resultChecksum: "c".repeat(64) },
    } satisfies generated.ComparisonBindingV1;
    const comparison = {
      schemaVersion: 1,
      base: binding,
      candidate: binding,
      baseScore: score,
      candidateScore: score,
      assignments: [],
      rules: [],
      scoreLevels: [],
      metrics: [],
      locks: [],
      runs: null,
      affectedEntities: [],
      ordering: "equivalent",
      checksum: "d".repeat(64),
    } satisfies generated.SolutionComparisonV1;
    const solveOptions = {
      backend: { kind: "auto" },
      mode: "balanced",
      timeLimitMilliseconds: 1_000,
      memoryLimitBytes: "18446744073709551615",
      workerThreads: { kind: "auto" },
      randomSeed: "18446744073709551615",
      solutionLimit: null,
      stopAfterFirstFeasible: false,
      collectIntermediateSolutions: false,
      explanationMode: "standard",
      preserveExisting: "none",
      reproducibility: "deterministic",
      resourceLimits: {
        maxEntities: 10,
        maxRules: 10,
        maxVariables: "18446744073709551615",
        maxConstraints: "18446744073709551615",
      },
    } as const satisfies generated.SolveOptions;
    const runInput = {
      schemaVersion: 1,
      runId,
      requestId: "01900000-0000-7000-8000-000000000072",
      requestHash: "e".repeat(64),
      scenarioId,
      scenarioRevision: 7,
      snapshotId: "01900000-0000-7000-8000-000000000073",
      snapshotDocumentHash: "f".repeat(64),
      snapshotCreatedAt: "2026-09-04T00:00:00Z",
      packId: "official.test",
      packSchemaVersion: 1,
      planningIrSchemaVersion: 1,
      compilerVersion: "1",
      applicationVersion: "1",
      backendId: "synthetic.test",
      backendVersion: "1",
      adapterVersion: "1",
      workerVersion: "1",
      solverVersion: "1",
      protocolMajor: 1,
      protocolMinor: 0,
      modelHash: "1".repeat(64),
      objectivePolicyHash: "2".repeat(64),
      solveOptions,
      scenarioTimezone: "UTC",
      temporaryConditionHash: null,
      checksum: "3".repeat(64),
    } satisfies generated.RunInputV1;
    const runManifest = {
      schemaVersion: 1,
      runId,
      runInputChecksum: runInput.checksum,
      outcome: { type: "noResult", status: "infeasible" },
      startedAt: "2026-09-04T00:00:00Z",
      finishedAt: "2026-09-04T00:00:01Z",
      elapsedMilliseconds: 1_000,
      firstIncumbentMilliseconds: null,
      firstVerifiedFeasibleMilliseconds: null,
      phaseTimings: {
        compileMilliseconds: 10,
        backendMilliseconds: 900,
        projectionMilliseconds: null,
        structuralValidationMilliseconds: null,
        scoreRecomputationMilliseconds: null,
        requiredRuleVerificationMilliseconds: null,
        evidencePersistenceMilliseconds: 10,
        optionalExplanationMilliseconds: null,
      },
      verificationWarnings: [],
      checksum: "4".repeat(64),
    } as const satisfies generated.RunManifestV1;
    const condition = {
      schemaVersion: 1,
      condition: {
        type: "forceAssignmentValue",
        assignmentId: assignment.id,
        value: assignment.value,
      },
      checksum: "5".repeat(64),
    } as const satisfies generated.CounterfactualConditionV1;
    const counterfactualRequest = {
      schemaVersion: 1,
      jobId: "01900000-0000-7000-8000-000000000074",
      requestId: "01900000-0000-7000-8000-000000000075",
      semantics: {
        schemaVersion: 1,
        scenarioId,
        scenarioRevision: 7,
        snapshotId: runInput.snapshotId,
        snapshotDocumentHash: runInput.snapshotDocumentHash,
        base: binding.acceptedResult,
        baseRunId: runId,
        baseRunInputChecksum: runInput.checksum,
        baseModelHash: runInput.modelHash,
        objectivePolicyHash: runInput.objectivePolicyHash,
        conditionChecksum: condition.checksum,
        totalBudgetMilliseconds: 1_000,
      },
      condition,
      requestHash: "6".repeat(64),
      createdAt: "2026-09-04T00:00:00Z",
    } satisfies generated.CounterfactualJobRequestV1;
    const counterfactual = {
      schemaVersion: 1,
      request: counterfactualRequest,
      baseRunInput: runInput,
      baseRunManifest: runManifest,
      compilation: {
        schemaVersion: 1,
        baseModelHash: runInput.modelHash,
        conditionChecksum: condition.checksum,
        derivedModelHash: "7".repeat(64),
        objectivePolicyHash: runInput.objectivePolicyHash,
        checksum: "8".repeat(64),
      },
      runInput,
      runManifest,
      conclusion: { type: "provenImpossible" },
      checksum: "9".repeat(64),
    } as const satisfies generated.CounterfactualResultV1;
    const payloads = [
      {
        kind: "validation",
        issue: {
          issueId: "validation.issue",
          severity: "mustFix",
          messageKey: "validation.issue",
          parameters: {},
          fieldPath: ["scenario"],
          entity: null,
          ruleId: null,
        },
      },
      {
        kind: "infeasibility",
        infeasibility: { type: "unavailable", reason: "assumptionsUnavailable" },
      },
      {
        kind: "assignment",
        assignment: {
          assignment,
          relatedRules: [],
          scoreContributions: [
            {
              evidenceId: "evidence.preference",
              levelId: "preference",
              categoryId: "fairness",
              value: "9007199254740993",
            },
          ],
          metrics: {},
          lockState: { state: "unlocked" },
        },
      },
      { kind: "counterfactual", result: counterfactual },
      { kind: "solutionDifference", comparison },
      { kind: "repair", repair: { comparison, causality: "notEstablished" } },
      {
        kind: "optimalityStatus",
        status: { runInput, runManifest, result: null },
      },
    ] as const satisfies readonly generated.ExplanationEvidencePayloadV1[];
    const records = [
      {
        schemaVersion: 1,
        request: counterfactualRequest,
        state: "queued",
        startedAt: null,
        finishedAt: null,
        cancelRequestId: null,
        cancelRequestedAt: null,
        result: null,
        error: null,
      },
      {
        schemaVersion: 1,
        request: counterfactualRequest,
        state: "completed",
        startedAt: "2026-09-04T00:00:00Z",
        finishedAt: "2026-09-04T00:00:01Z",
        cancelRequestId: null,
        cancelRequestedAt: null,
        result: counterfactual,
        error: null,
      },
      {
        schemaVersion: 1,
        request: counterfactualRequest,
        state: "failed",
        startedAt: null,
        finishedAt: "2026-09-04T00:00:01Z",
        cancelRequestId: null,
        cancelRequestedAt: null,
        result: null,
        error: { kind: "backendFailed" },
      },
    ] as const satisfies readonly generated.CounterfactualJobRecordV1[];

    expect(payloads.map((payload) => payload.kind)).toEqual([
      "validation",
      "infeasibility",
      "assignment",
      "counterfactual",
      "solutionDifference",
      "repair",
      "optimalityStatus",
    ]);
    expect(records.map((record) => record.state)).toEqual(["queued", "completed", "failed"]);
  });

  it("listens on each Phase-04 event topic without changing payloads", async () => {
    const unlisten = vi.fn();
    tauriEvents.listen.mockResolvedValue(unlisten);
    const listener = vi.fn();
    const scenarioId = "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a";

    await generated.onSolveProgress(listener);
    await generated.onSolveCompleted(listener);
    await generated.onScenarioValidationChanged(listener);
    await generated.onCounterfactualProgress(listener);

    expect(tauriEvents.listen.mock.calls.map(([topic]) => topic)).toEqual([
      "solve://progress",
      "solve://completed",
      "scenario://validation-changed",
      "counterfactual://progress",
    ]);
    const forwardedPayload = {
      type: "solveCompleted",
      payload: {
        context: {
          eventVersion: 1,
          timestamp: "2026-09-04T00:00:00Z",
          requestId: null,
          scenarioId,
          revision: 7,
          solveRunId: null,
        },
        status: "optimal",
        solutionId: null,
      },
    } satisfies generated.SolveCompletedEvent;
    const validationPayload = {
      type: "scenarioValidationChanged",
      payload: {
        context: forwardedPayload.payload.context,
        validationDelta: {
          added: [
            {
              code: "validation.required",
              severity: "error",
              message: "A required value is missing.",
              fieldPath: "/rules/0",
              resource: null,
            },
          ],
          resolved: ["validation.previous"],
        },
      },
    } satisfies generated.ScenarioValidationChangedEvent;
    tauriEvents.listen.mock.calls[1]?.[1]({ payload: forwardedPayload });
    tauriEvents.listen.mock.calls[2]?.[1]({ payload: validationPayload });
    expect(listener).toHaveBeenNthCalledWith(1, forwardedPayload);
    expect(listener).toHaveBeenNthCalledWith(2, validationPayload);
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

  it("keeps unopened bundle bytes and native paths behind the Rust capability", async () => {
    const previewId = "01900000-0000-7000-8000-000000000071";
    tauri.invoke.mockResolvedValue(undefined);

    await generated.inspectUnopenedBundle();
    await generated.reexportUnopenedBundle(previewId);
    await generated.cancelPortablePreview(previewId);

    expect(tauri.invoke).toHaveBeenNthCalledWith(1, "project_unopened_bundle_inspect", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
      },
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(2, "project_unopened_bundle_reexport", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        previewId,
      },
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(3, "project_operation_cancel", {
      request: {
        requestId: expect.stringMatching(UUID_V7_PATTERN),
        previewId,
      },
    });
    for (const [, payload] of tauri.invoke.mock.calls) {
      expect(collectKeys(payload)).not.toEqual(
        expect.arrayContaining(["bytes", expect.stringMatching(PATH_BEARING_REQUEST_KEY)]),
      );
    }
  });

  it("models registry metadata, empty solver authority, and safe unopened previews", () => {
    const pack = {
      id: "official.test",
      displayName: { key: "official.test.name", defaultText: "Official synthetic test pack" },
      description: { key: "official.test.description", defaultText: "Conformance pack" },
      packVersion: "0.1.0",
      scenarioVersions: { latest: 1, migratableFrom: [0] },
      iconId: "official.test",
      capabilities: ["commands", "portableData"],
      explanationCapabilities: [
        "validation",
        "infeasibility",
        "assignment",
        "counterfactual",
        "solutionDifference",
        "repair",
        "optimalityStatus",
      ],
      portableSchemaVersion: 1,
      portableCapabilities: [],
      shareResultSchemaVersion: 1,
      documentationUrl: null,
      license: { spdxExpression: "Apache-2.0", attribution: "Eutheto contributors" },
      syntheticTestOnly: true,
    } satisfies DomainPackDescriptorDto;
    const matrix = {
      schemaVersion: 1,
      planningIrSchemaVersion: 2,
      features: [
        {
          id: "solve.cancellation",
          category: "solve",
          gate: { kind: "unconditional" },
        },
      ],
      productionBackendIds: [],
      backendColumns: [],
    } satisfies SolverSupportMatrixDto;
    const deferred = [
      { backendId: "solver.ortools-cp-sat", candidateVersion: "9.15", owningPhase: 3 },
      { backendId: "solver.pumpkin", candidateVersion: "0.5.0", owningPhase: 8 },
    ] satisfies readonly DeferredSolverGateDto[];
    const unopened = {
      previewId: "01900000-0000-7000-8000-000000000071",
      metadata: {
        fileSha256: "a".repeat(64),
        format: "eutheto/bundle",
        formatVersion: 2,
        portableSchemaVersion: 3,
        bundleKind: "scenario-export",
        title: "Newer unopened bundle",
        requiredCapabilities: [{ id: "future.pack", version: 2 }],
        scenarios: [
          {
            path: "scenarios/01900000-0000-7000-8000-000000000072.json",
            scenarioId: "01900000-0000-7000-8000-000000000072",
            packId: "future.pack",
            internalPackSchemaVersion: null,
            portablePackSchemaVersion: 4,
          },
        ],
      },
    } satisfies UnopenedBundlePreviewDto;

    expect(pack.id).toBe("official.test");
    expect(matrix.productionBackendIds).toEqual([]);
    expect(matrix.backendColumns).toEqual([]);
    expect(deferred.map(({ backendId }) => backendId)).toEqual([
      "solver.ortools-cp-sat",
      "solver.pumpkin",
    ]);
    expect(collectKeys(unopened)).not.toContain("bytes");
  });

  it("preserves exact unsupported and degraded solver matrix cells", async () => {
    const matrix = {
      schemaVersion: 1,
      planningIrSchemaVersion: 2,
      features: [
        {
          id: "primitive.fixture-unsupported",
          category: "primitive",
          gate: { kind: "unconditional" },
        },
        {
          id: "solve.fixture-degraded",
          category: "solve",
          gate: { kind: "enabled", gateId: "phase.fixture" },
        },
      ],
      productionBackendIds: ["solver.fixture"],
      backendColumns: [
        {
          backendId: "solver.fixture",
          backendVersion: "0.0-fixture",
          adapterVersion: "adapter-fixture-v2",
          cells: [
            {
              featureId: "primitive.fixture-unsupported",
              support: "unsupported",
              reason: "Fixture unsupported reason",
              remediation: "Choose the fixture alternative",
              fixtureId: "fixture.unsupported-exact",
            },
            {
              featureId: "solve.fixture-degraded",
              support: "degraded",
              restrictionId: "restriction.fixture-cap",
              reason: "Fixture degradation reason",
              remediation: "Use the unrestricted fixture mode",
              fixtureId: "fixture.degraded-exact",
            },
          ],
        },
      ],
    } satisfies SolverSupportMatrixDto;
    const response = {
      schemaVersion: generated.API_SCHEMA_VERSION,
      requestId: "01900000-0000-7000-8000-000000000070",
      currentRevision: null,
      warnings: [],
      result: matrix,
    } satisfies ApiResponseDto<SolverSupportMatrixDto>;
    tauri.invoke.mockResolvedValue(response);

    const actual = await generated.getSolverSupportMatrix();

    expect(tauri.invoke).toHaveBeenCalledWith("solver_get_support_matrix", {
      request: { requestId: expect.stringMatching(UUID_V7_PATTERN) },
    });
    expect(actual.result.backendColumns).toEqual(matrix.backendColumns);
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

  it("forwards lossless signed 64-bit assignment values without number coercion", async () => {
    const value = "9007199254740993";
    tauri.invoke.mockResolvedValue(undefined);

    await generated.startCounterfactual({
      scenarioId: "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a",
      expectedRevision: 0,
      baseSolutionId: "01900000-0000-7000-8000-000000000070",
      condition: {
        type: "forceAssignmentValue",
        assignmentId: "shift.primary",
        value: { type: "integer", value },
      },
      totalBudgetMilliseconds: 1_000,
    });

    expect(tauri.invoke).toHaveBeenCalledWith("solution_start_counterfactual", {
      request: expect.objectContaining({
        condition: expect.objectContaining({
          value: { type: "integer", value },
        }),
      }),
    });
  });

  it.each(["9223372036854775808", "-9223372036854775809", "01", "-0"])(
    "rejects invalid signed 64-bit assignment value %s before invoking Tauri",
    (value) => {
      expect(() =>
        generated.startCounterfactual({
          scenarioId: "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a",
          expectedRevision: 0,
          baseSolutionId: "01900000-0000-7000-8000-000000000070",
          condition: {
            type: "forceAssignmentValue",
            assignmentId: "shift.primary",
            value: { type: "integer", value },
          },
          totalBudgetMilliseconds: 1_000,
        }),
      ).toThrow(/signed 64-bit/);
      expect(tauri.invoke).not.toHaveBeenCalled();
    },
  );

  it("rejects oversized counterfactual budgets before invoking Tauri", async () => {
    await expect(
      generated.startCounterfactual({
        scenarioId: "018f47f8-62a1-7a2a-aa2a-2a2a2a2a2a2a",
        expectedRevision: 0,
        baseSolutionId: "01900000-0000-7000-8000-000000000070",
        condition: {
          type: "forceAssignmentValue",
          assignmentId: "shift.primary",
          value: { type: "boolean", value: true },
        },
        totalBudgetMilliseconds: generated.COUNTERFACTUAL_TOTAL_BUDGET_MAX_MILLISECONDS_V1 + 1,
      }),
    ).rejects.toThrow("Counterfactual budget must be an integer between 1 and 30000 milliseconds");
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

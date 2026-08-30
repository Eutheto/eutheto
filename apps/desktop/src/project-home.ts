import { reactive } from "vue";

import {
  applyImport,
  applyRestore,
  cancelPortablePreview,
  createBackup,
  createProject,
  deleteProject,
  duplicateProject,
  listProjects,
  onAppNotification,
  onLibraryRefreshRequired,
  onScenarioChanged,
  onScenarioValidationChanged,
  previewBackup,
  previewImport,
  previewRestore,
  setProjectArchived,
} from "./api/generated";
import type {
  ApiResponseDto,
  CollisionAction,
  CollisionPlan,
  DomainPackRef,
  ImportOptions,
  PortableArtifactDto,
  Revision,
  PortableFilePreviewDto,
  PortablePreviewDto,
  PortableScenarioDto,
  ProjectSummaryDto,
  ScenarioChangedEvent,
  SupplementalCollisionAction,
  SupplementalIdentity,
  ScenarioSettings,
  ScenarioValidationChangedEvent,
  ValidationIssue,
} from "./api/generated";

export type ProjectPhase = "loading" | "ready" | "error";
export type SupplementalCollisionChoice = SupplementalCollisionAction;
export type CollisionChoice = CollisionAction;
export type ProjectSummary = ProjectSummaryDto;
export type PortablePreview = PortablePreviewDto;
export type BackupPreview = PortableFilePreviewDto;

export interface CreateProjectInput {
  readonly title: string;
  readonly description: string;
  readonly domainPack: DomainPackRef;
  readonly settings: ScenarioSettings;
}

export interface ProjectHomeApi {
  listProjects(scope: "all"): Promise<ApiResponseDto<readonly ProjectSummaryDto[]>>;
  createProject(input: CreateProjectInput): Promise<ApiResponseDto<unknown>>;
  duplicateProject(input: {
    readonly sourceId: string;
    readonly expectedRevision: Revision;
    readonly title: string;
  }): Promise<ApiResponseDto<unknown>>;
  setProjectArchived(input: {
    readonly scenarioId: string;
    readonly expectedRevision: Revision;
    readonly archived: boolean;
  }): Promise<ApiResponseDto<unknown>>;
  deleteProject(scenarioId: string, expectedRevision: Revision): Promise<ApiResponseDto<unknown>>;
  previewImport(options: ImportOptions): Promise<ApiResponseDto<PortablePreviewDto>>;
  applyImport(input: {
    readonly previewId: string;
    readonly collisionPlan: CollisionPlan;
  }): Promise<ApiResponseDto<unknown>>;
  previewBackup(title: string): Promise<ApiResponseDto<PortableFilePreviewDto>>;
  createBackup(title: string, previewId: string): Promise<ApiResponseDto<PortableArtifactDto>>;
  previewRestore(options: ImportOptions): Promise<ApiResponseDto<PortablePreviewDto>>;
  applyRestore(input: {
    readonly previewId: string;
    readonly collisionPlan: CollisionPlan;
    readonly authorization: {
      readonly destructiveActionConfirmed: boolean;
      readonly safetyBackupBypassPhrase: string | null;
    };
  }): Promise<ApiResponseDto<unknown>>;
  cancelPortablePreview(previewId: string): Promise<ApiResponseDto<unknown>>;
  onAppNotification(listener: () => void): Promise<() => void>;
  onLibraryRefreshRequired(listener: () => void): Promise<() => void>;
  onScenarioChanged(listener: (event: ScenarioChangedEvent) => void): Promise<() => void>;
  onScenarioValidationChanged(
    listener: (event: ScenarioValidationChangedEvent) => void,
  ): Promise<() => void>;
}

const generatedProjectHomeApi: ProjectHomeApi = {
  listProjects,
  createProject,
  duplicateProject,
  setProjectArchived,
  deleteProject,
  previewImport,
  applyImport,
  previewBackup,
  createBackup,
  previewRestore,
  applyRestore,
  cancelPortablePreview,
  onAppNotification,
  onLibraryRefreshRequired,
  onScenarioChanged,
  onScenarioValidationChanged,
};

export interface ProjectHomeState {
  phase: ProjectPhase;
  projects: readonly ProjectSummary[];
  selectedId: string | null;
  busyAction: string | null;
  errorMessage: string | null;
  announcement: string;
  importPreview: PortablePreview | null;
  importWarnings: readonly ValidationIssue[];
  backupPreview: BackupPreview | null;
  restorePreview: PortablePreview | null;
  restoreWarnings: readonly ValidationIssue[];
  restoreSafetyBackupFailure: string | null;
  restoreMode: "add-backup" | "replace-library";
}

export interface ProjectHomeController {
  readonly state: ProjectHomeState;
  load(): Promise<void>;
  startEventListeners(): Promise<void>;
  dispose(): Promise<void>;
  selectProject(scenarioId: string): void;
  createProject(input: CreateProjectInput): Promise<boolean>;
  duplicateProject(project: ProjectSummary, title: string): Promise<boolean>;
  setArchived(project: ProjectSummary): Promise<boolean>;
  deleteProject(project: ProjectSummary): Promise<boolean>;
  previewImport(options: Pick<ImportOptions, "includeResults" | "includeAssets">): Promise<boolean>;
  applyImport(
    collisions: Readonly<Record<string, CollisionChoice>>,
    supplemental: Readonly<Record<string, SupplementalCollisionChoice>>,
  ): Promise<boolean>;
  previewBackup(title: string): Promise<boolean>;
  createBackup(title: string): Promise<boolean>;
  previewRestore(mode: Exclude<ImportOptions["restoreMode"], "import-scenario">): Promise<boolean>;
  applyRestore(
    collisions: Readonly<Record<string, CollisionChoice>>,
    supplemental: Readonly<Record<string, SupplementalCollisionChoice>>,
    safetyBackupBypassPhrase?: string,
  ): Promise<boolean>;
}

export interface FocusTarget {
  focus(): void;
}

export function recoverFocus(target: FocusTarget | null | undefined): void {
  target?.focus();
}

interface ApiFailure {
  readonly category?: unknown;
  readonly code?: unknown;
  readonly message?: unknown;
  readonly retryable?: unknown;
}

function safeMessage(error: unknown): string {
  const failure = error as ApiFailure | null;
  if (
    typeof failure === "object" &&
    failure !== null &&
    typeof failure.category === "string" &&
    typeof failure.code === "string" &&
    typeof failure.message === "string"
  ) {
    return failure.message;
  }

  return "The local project library could not complete that request.";
}

function isRevisionConflict(error: unknown): boolean {
  const failure = error as ApiFailure;
  return (
    failure.category === "conflict" ||
    (typeof failure.code === "string" && failure.code.includes("revision"))
  );
}

function isFileSelectionCancelled(error: unknown): boolean {
  const failure = error as ApiFailure;
  return (
    failure.code === "operation.cancelled" &&
    failure.category === "protocol" &&
    failure.retryable === true
  );
}

export function supplementalIdentityKey(identity: SupplementalIdentity): string {
  return `${identity.section}\u0000${identity.key}`;
}

export function defaultSupplementalCollisionChoices(
  identities: readonly SupplementalIdentity[],
  action: SupplementalCollisionChoice = "skip",
): Record<string, SupplementalCollisionChoice> {
  const choices: Record<string, SupplementalCollisionChoice> = {};
  for (const identity of identities) choices[supplementalIdentityKey(identity)] = action;
  return choices;
}
export interface ScenarioRevisionOutcome {
  readonly revision: Revision | null;
  readonly warning: string | null;
}

export function scenarioRevisionOutcome(
  scenario: PortableScenarioDto,
  action: CollisionChoice | undefined,
  replaceLibrary = false,
): ScenarioRevisionOutcome {
  if (scenario.collides && !replaceLibrary && action === "skip") {
    return { revision: null, warning: null };
  }
  if (!scenario.collides || replaceLibrary || action === "replace") {
    return {
      revision: scenario.sameIdentityRevision,
      warning: scenario.sameIdentityRevisionWarning,
    };
  }
  return { revision: scenario.sourceRevision, warning: null };
}

function collisionPlan(
  preview: PortablePreviewDto,
  scenarios: Readonly<Record<string, CollisionChoice>>,
  supplemental: Readonly<Record<string, SupplementalCollisionChoice>>,
  replaceLibrary = false,
): CollisionPlan | null {
  if (replaceLibrary) return { scenarios: {}, supplementalChoices: [] };
  const scenarioChoices: Record<string, CollisionChoice> = {};
  for (const scenario of preview.scenarios) {
    if (!scenario.collides) continue;
    const action = scenarios[scenario.scenarioId];
    if (!action) return null;
    scenarioChoices[scenario.scenarioId] = action;
  }
  const supplementalChoices = preview.supplementalCollisions.map((identity) => {
    const action = supplemental[supplementalIdentityKey(identity)];
    return action ? { ...identity, action } : null;
  });
  if (supplementalChoices.some((choice) => choice === null)) return null;
  return {
    scenarios: scenarioChoices,
    supplementalChoices: supplementalChoices.filter(
      (choice): choice is NonNullable<typeof choice> => choice !== null,
    ),
  };
}

export function createProjectHomeController(
  api: ProjectHomeApi = generatedProjectHomeApi,
): ProjectHomeController {
  const state = reactive<ProjectHomeState>({
    phase: "loading",
    projects: [],
    selectedId: null,
    busyAction: null,
    errorMessage: null,
    announcement: "",
    importPreview: null,
    importWarnings: [],
    backupPreview: null,
    restorePreview: null,
    restoreWarnings: [],
    restoreSafetyBackupFailure: null,
    restoreMode: "add-backup",
  });
  const eventUnlisteners: Array<() => void> = [];
  let listenersStarted = false;

  async function reload(showLoading = true): Promise<boolean> {
    if (showLoading) {
      state.phase = "loading";
    }
    state.errorMessage = null;

    try {
      const response = await api.listProjects("all");
      state.projects = response.result;
      if (
        !state.selectedId ||
        !response.result.some(({ scenarioId }) => scenarioId === state.selectedId)
      ) {
        state.selectedId = response.result[0]?.scenarioId ?? null;
      }
      state.phase = "ready";
      return true;
    } catch (error) {
      state.phase = "error";
      state.errorMessage = safeMessage(error);
      return false;
    }
  }

  function refreshFromEvent(event: ScenarioChangedEvent | ScenarioValidationChangedEvent): void {
    if (event.payload.context.scenarioId !== null) {
      void reload(false);
    }
  }

  async function startEventListeners(): Promise<void> {
    if (listenersStarted) return;
    listenersStarted = true;
    try {
      const [changed, validationChanged, notification, refreshRequired] = await Promise.all([
        api.onScenarioChanged(refreshFromEvent),
        api.onScenarioValidationChanged(refreshFromEvent),
        api.onAppNotification(() => {
          void reload(false);
        }),
        api.onLibraryRefreshRequired(() => {
          void reload(false);
        }),
      ]);
      eventUnlisteners.push(changed, validationChanged, notification, refreshRequired);
    } catch (error) {
      listenersStarted = false;
      state.errorMessage = safeMessage(error);
    }
  }

  async function dispose(): Promise<void> {
    for (const unlisten of eventUnlisteners.splice(0)) unlisten();
    listenersStarted = false;
    const previewIds = [state.importPreview?.previewId, state.restorePreview?.previewId].filter(
      (previewId): previewId is string => previewId !== undefined,
    );
    state.importPreview = null;
    state.importWarnings = [];
    state.restorePreview = null;
    state.restoreWarnings = [];
    state.restoreSafetyBackupFailure = null;
    await Promise.all(
      previewIds.map(async (previewId) => {
        try {
          await api.cancelPortablePreview(previewId);
        } catch {
          // A preview consumed by apply or server eviction is already safely unavailable.
        }
      }),
    );
  }

  async function mutate(
    action: string,
    operation: () => Promise<ApiResponseDto<unknown>>,
    successAnnouncement: string,
  ): Promise<boolean> {
    state.busyAction = action;
    state.errorMessage = null;
    try {
      await operation();
      const reloaded = await reload(false);
      if (!reloaded) return false;
      state.announcement = successAnnouncement;
      return true;
    } catch (error) {
      if (isRevisionConflict(error)) {
        state.announcement =
          "The project changed in another window. The latest saved version has been reloaded.";
        await reload(false);
      } else {
        state.errorMessage = safeMessage(error);
      }
      return false;
    } finally {
      state.busyAction = null;
    }
  }

  return {
    state,
    startEventListeners,
    dispose,
    load: async () => {
      await reload();
    },
    selectProject(scenarioId) {
      state.selectedId = scenarioId;
    },
    createProject(input) {
      return mutate("create", () => api.createProject(input), `Created ${input.title}.`);
    },
    duplicateProject(project, title) {
      return mutate(
        `duplicate:${project.scenarioId}`,
        () =>
          api.duplicateProject({
            sourceId: project.scenarioId,
            expectedRevision: project.revision,
            title,
          }),
        `Duplicated ${project.title} as ${title}.`,
      );
    },
    setArchived(project) {
      const archived = !project.archived;
      return mutate(
        `archive:${project.scenarioId}`,
        () =>
          api.setProjectArchived({
            scenarioId: project.scenarioId,
            expectedRevision: project.revision,
            archived,
          }),
        `${archived ? "Archived" : "Unarchived"} ${project.title}.`,
      );
    },
    deleteProject(project) {
      return mutate(
        `delete:${project.scenarioId}`,
        () => api.deleteProject(project.scenarioId, project.revision),
        `Deleted ${project.title}.`,
      );
    },
    async previewImport(selection) {
      const previousError = state.errorMessage;
      state.busyAction = "preview-import";
      state.errorMessage = null;
      try {
        const previousPreview = state.importPreview;
        const response = await api.previewImport({
          restoreMode: "import-scenario",
          includeResults: selection.includeResults,
          includeAssets: selection.includeAssets,
        });
        state.importPreview = response.result;
        state.importWarnings = response.warnings;
        if (previousPreview && previousPreview.previewId !== response.result.previewId) {
          await api.cancelPortablePreview(previousPreview.previewId).catch(() => undefined);
        }
        state.announcement = "Import preview ready. Review every collision before applying it.";
        return true;
      } catch (error) {
        state.errorMessage = isFileSelectionCancelled(error) ? previousError : safeMessage(error);
        return false;
      } finally {
        state.busyAction = null;
      }
    },
    applyImport(collisions, supplemental) {
      const preview = state.importPreview;
      if (!preview) {
        return Promise.resolve(false);
      }
      const plan = collisionPlan(preview, collisions, supplemental);
      if (!plan) {
        state.errorMessage = "Choose an action for every collision shown in the preview.";
        return Promise.resolve(false);
      }
      return mutate(
        "apply-import",
        () =>
          api.applyImport({
            previewId: preview.previewId,
            collisionPlan: plan,
          }),
        "Import applied. The saved project library has been reloaded.",
      ).then((applied) => {
        state.importPreview = null;
        state.importWarnings = [];
        return applied;
      });
    },
    async previewBackup(title) {
      state.busyAction = "preview-backup";
      state.errorMessage = null;
      try {
        const response = await api.previewBackup(title);
        state.backupPreview = response.result;
        state.announcement = "Backup preview ready.";
        return true;
      } catch (error) {
        state.errorMessage = safeMessage(error);
        return false;
      } finally {
        state.busyAction = null;
      }
    },
    async createBackup(title) {
      const preview = state.backupPreview;
      if (!preview) return false;
      const previousError = state.errorMessage;
      state.busyAction = "create-backup";
      state.errorMessage = null;
      try {
        const response = await api.createBackup(title, preview.previewId);
        const reloaded = await reload(false);
        if (!reloaded) return false;
        state.announcement = `Backup saved as ${response.result.artifactName}.`;
        return true;
      } catch (error) {
        if (isFileSelectionCancelled(error)) {
          state.errorMessage = previousError;
        } else if (isRevisionConflict(error)) {
          state.announcement =
            "The project changed in another window. The latest saved version has been reloaded.";
          await reload(false);
        } else {
          state.errorMessage = safeMessage(error);
        }
        return false;
      } finally {
        state.backupPreview = null;
        state.busyAction = null;
      }
    },
    async previewRestore(mode) {
      const previousError = state.errorMessage;
      state.busyAction = "preview-restore";
      state.errorMessage = null;
      try {
        const previousPreview = state.restorePreview;
        const response = await api.previewRestore({
          restoreMode: mode,
          includeResults: true,
          includeAssets: true,
        });
        state.restoreSafetyBackupFailure = null;
        state.restorePreview = response.result;
        state.restoreWarnings = response.warnings;
        if (previousPreview && previousPreview.previewId !== response.result.previewId) {
          await api.cancelPortablePreview(previousPreview.previewId).catch(() => undefined);
        }
        state.restoreMode = mode;
        state.announcement = `${mode === "replace-library" ? "Replace" : "Add"} restore preview ready.`;
        return true;
      } catch (error) {
        state.errorMessage = isFileSelectionCancelled(error) ? previousError : safeMessage(error);
        return false;
      } finally {
        state.busyAction = null;
      }
    },
    async applyRestore(collisions, supplemental, safetyBackupBypassPhrase = "") {
      const preview = state.restorePreview;
      if (!preview) return false;
      const plan = collisionPlan(
        preview,
        collisions,
        supplemental,
        state.restoreMode === "replace-library",
      );
      if (!plan) {
        state.errorMessage = "Choose an action for every collision shown in the preview.";
        return false;
      }
      state.busyAction = "apply-restore";
      state.errorMessage = null;
      try {
        await api.applyRestore({
          previewId: preview.previewId,
          collisionPlan: plan,
          authorization: {
            destructiveActionConfirmed: state.restoreMode === "replace-library",
            safetyBackupBypassPhrase: safetyBackupBypassPhrase || null,
          },
        });
        state.restorePreview = null;
        state.restoreWarnings = [];
        state.restoreSafetyBackupFailure = null;
        if (!(await reload(false))) return false;
        state.announcement = "Restore applied. The saved project library has been reloaded.";
        return true;
      } catch (error) {
        if ((error as ApiFailure).code === "restore.safety_backup_failed") {
          state.restoreSafetyBackupFailure = safeMessage(error);
          state.errorMessage = null;
          state.announcement =
            "The safety backup failed. Review the reason before choosing whether to continue.";
        } else {
          state.restorePreview = null;
          state.restoreWarnings = [];
          state.restoreSafetyBackupFailure = null;
          if (isRevisionConflict(error)) {
            state.announcement =
              "The project changed in another window. The latest saved version has been reloaded.";
            await reload(false);
          } else {
            state.errorMessage = safeMessage(error);
          }
        }
        return false;
      } finally {
        state.busyAction = null;
      }
    },
  };
}

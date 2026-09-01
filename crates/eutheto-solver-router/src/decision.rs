use crate::profile::{ROUTING_PROFILE_VERSION, RoutingProfile};
use eutheto_planning_ir::{
    ComponentAnalysis, PlanningIrLimitsV1, PlanningProblem, PlanningProblemSummary,
    SplitAuthorization, analyze_components, summarize, validate,
};
use eutheto_solver_api::{CompatibilityReport, SolverRegistry, compatibility_for};
use eutheto_types::{BackendId, BackendSelection, SolveOptions};
use serde::{Deserialize, Serialize};

/// Version of the deterministic backend ordering and fallback policy.
pub const ROUTER_POLICY_VERSION: u32 = 1;

/// Hard bounds for decision diagnostics. Messages are fixed router-owned text and never include
/// backend payloads, scenario labels, or model values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterPolicy {
    pub version: u32,
    pub max_diagnostics: usize,
}

impl Default for RouterPolicy {
    fn default() -> Self {
        Self {
            version: ROUTER_POLICY_VERSION,
            max_diagnostics: 32,
        }
    }
}

/// A bounded, redacted router diagnostic.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouterDiagnostic {
    pub code: String,
    pub message: String,
}

impl RouterDiagnostic {
    fn fixed(code: &'static str, message: &'static str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }

    pub(crate) fn backend_code(code: &str) -> Self {
        let bounded = if code.len() <= 96 && code.bytes().all(|byte| byte.is_ascii_graphic()) {
            code
        } else {
            "solver.backend_failure"
        };
        Self {
            code: bounded.to_owned(),
            message: "backend failed before producing a candidate".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CompatibilityAssessment {
    Compatible,
    MatrixIncompatible,
    BackendIncompatible,
    CompatibilityError,
}

/// Exact reports considered for one registry entry, retained in deterministic backend-ID order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsideredBackend {
    pub backend_id: BackendId,
    pub matrix_report: Option<CompatibilityReport>,
    pub backend_report: Option<CompatibilityReport>,
    pub assessment: CompatibilityAssessment,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "handling", content = "backendId")]
pub enum OverrideHandling {
    Automatic,
    CompatibleOverride(BackendId),
    UnknownOverride(BackendId),
    IncompatibleOverride(BackendId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "disposition", content = "evidence")]
pub enum SplitDisposition {
    InvalidModel,
    SingleComponent,
    MissingAuthorization,
    ComponentHashMismatch {
        authorized_hash: String,
        actual_hash: String,
    },
    MissingDomainMergeContract,
    ProjectionNotIndependent,
    /// The proof is valid, but Phase 02 has no domain merge executor. The whole model is run once.
    AuthorizedWholeModelOnly {
        authorization: SplitAuthorization,
        component_ids: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DecisionStatus {
    Ready,
    InvalidModel,
    UnsupportedOverride,
    NoCompatibleBackend,
}

/// Serializable, redacted evidence for a deterministic routing decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingDecision {
    pub policy_version: u32,
    pub profile: Option<RoutingProfile>,
    pub profile_contract_version: u32,
    pub requested_backend: BackendSelection,
    pub status: DecisionStatus,
    pub summary: Option<PlanningProblemSummary>,
    pub component_analysis: ComponentAnalysis,
    pub split: SplitDisposition,
    pub considered_backends: Vec<ConsideredBackend>,
    pub chosen_backend: Option<BackendId>,
    pub override_handling: OverrideHandling,
    pub fallback_order: Vec<BackendId>,
    pub diagnostics: Vec<RouterDiagnostic>,
}

struct BackendSelectionResult {
    status: DecisionStatus,
    chosen_backend: Option<BackendId>,
    override_handling: OverrideHandling,
    fallback_order: Vec<BackendId>,
    diagnostics: Vec<RouterDiagnostic>,
}

impl RouterPolicy {
    /// Builds the complete route without invoking a backend.
    #[must_use]
    pub fn decide(
        self,
        registry: &SolverRegistry,
        problem: &PlanningProblem,
        options: &SolveOptions,
    ) -> RoutingDecision {
        let profile = RoutingProfile::for_mode(options.mode);
        let limits = PlanningIrLimitsV1::DEFAULT.tightened_by(options.resource_limits);
        if validate(problem, limits).is_err() {
            return self.invalid_decision(
                profile,
                options,
                ComponentAnalysis {
                    components: Vec::new(),
                    edge_count: 0,
                    component_hash: String::new(),
                },
                SplitDisposition::InvalidModel,
            );
        }

        let component_analysis = analyze_components(problem);
        let split = split_disposition(problem, &component_analysis);
        let Ok(summary) = summarize(problem, limits) else {
            return self.invalid_decision(profile, options, component_analysis, split);
        };
        let considered_backends = considered_backends(registry, &summary, options);
        let selection = select_backend(&considered_backends, &options.backend);

        RoutingDecision {
            policy_version: self.version,
            profile,
            profile_contract_version: ROUTING_PROFILE_VERSION,
            requested_backend: options.backend.clone(),
            status: selection.status,
            summary: Some(summary),
            component_analysis,
            split,
            considered_backends,
            chosen_backend: selection.chosen_backend,
            override_handling: selection.override_handling,
            fallback_order: selection.fallback_order,
            diagnostics: self.bounded_diagnostics(selection.diagnostics),
        }
    }

    fn invalid_decision(
        self,
        profile: Option<RoutingProfile>,
        options: &SolveOptions,
        component_analysis: ComponentAnalysis,
        split: SplitDisposition,
    ) -> RoutingDecision {
        RoutingDecision {
            policy_version: self.version,
            profile,
            profile_contract_version: ROUTING_PROFILE_VERSION,
            requested_backend: options.backend.clone(),
            status: DecisionStatus::InvalidModel,
            summary: None,
            component_analysis,
            split,
            considered_backends: Vec::new(),
            chosen_backend: None,
            override_handling: override_handling_without_summary(&options.backend),
            fallback_order: Vec::new(),
            diagnostics: self.bounded_diagnostics(vec![RouterDiagnostic::fixed(
                "solver.invalid_model",
                "planning IR validation failed before backend selection",
            )]),
        }
    }

    pub(crate) fn bounded_diagnostics(
        self,
        mut diagnostics: Vec<RouterDiagnostic>,
    ) -> Vec<RouterDiagnostic> {
        diagnostics.truncate(self.max_diagnostics);
        diagnostics
    }
}

fn considered_backends(
    registry: &SolverRegistry,
    summary: &PlanningProblemSummary,
    options: &SolveOptions,
) -> Vec<ConsideredBackend> {
    registry
        .descriptors()
        .map(|descriptor| {
            let matrix_report =
                compatibility_for(registry.matrix(), &descriptor.id, summary, options).ok();
            let backend_report = registry
                .get(&descriptor.id)
                .map(|backend| backend.compatibility(summary, options));
            let assessment = match (&matrix_report, &backend_report) {
                (Some(matrix), Some(backend)) if matrix.compatible() && backend.compatible() => {
                    CompatibilityAssessment::Compatible
                }
                (Some(matrix), _) if !matrix.compatible() => {
                    CompatibilityAssessment::MatrixIncompatible
                }
                (Some(_), Some(_)) => CompatibilityAssessment::BackendIncompatible,
                _ => CompatibilityAssessment::CompatibilityError,
            };
            ConsideredBackend {
                backend_id: descriptor.id.clone(),
                matrix_report,
                backend_report,
                assessment,
            }
        })
        .collect()
}

fn select_backend(
    considered: &[ConsideredBackend],
    requested_backend: &BackendSelection,
) -> BackendSelectionResult {
    let compatible_ids: Vec<_> = considered
        .iter()
        .filter(|entry| entry.assessment == CompatibilityAssessment::Compatible)
        .map(|entry| entry.backend_id.clone())
        .collect();
    match requested_backend {
        BackendSelection::Auto => automatic_selection(&compatible_ids),
        BackendSelection::Specific(requested) => {
            let matching = considered
                .iter()
                .find(|entry| &entry.backend_id == requested);
            specific_selection(matching, requested)
        }
    }
}

fn automatic_selection(compatible_ids: &[BackendId]) -> BackendSelectionResult {
    let chosen_backend = compatible_ids.first().cloned();
    let fallback_order = compatible_ids.iter().skip(1).cloned().collect();
    if chosen_backend.is_some() {
        BackendSelectionResult {
            status: DecisionStatus::Ready,
            chosen_backend,
            override_handling: OverrideHandling::Automatic,
            fallback_order,
            diagnostics: Vec::new(),
        }
    } else {
        BackendSelectionResult {
            status: DecisionStatus::NoCompatibleBackend,
            chosen_backend: None,
            override_handling: OverrideHandling::Automatic,
            fallback_order,
            diagnostics: vec![RouterDiagnostic::fixed(
                "solver.no_compatible_backend",
                "no supplied registry backend is compatible",
            )],
        }
    }
}

fn specific_selection(
    matching: Option<&ConsideredBackend>,
    requested: &BackendId,
) -> BackendSelectionResult {
    match matching {
        Some(entry) if entry.assessment == CompatibilityAssessment::Compatible => {
            BackendSelectionResult {
                status: DecisionStatus::Ready,
                chosen_backend: Some(requested.clone()),
                override_handling: OverrideHandling::CompatibleOverride(requested.clone()),
                fallback_order: Vec::new(),
                diagnostics: Vec::new(),
            }
        }
        Some(_) => BackendSelectionResult {
            status: DecisionStatus::UnsupportedOverride,
            chosen_backend: None,
            override_handling: OverrideHandling::IncompatibleOverride(requested.clone()),
            fallback_order: Vec::new(),
            diagnostics: vec![RouterDiagnostic::fixed(
                "solver.incompatible_override",
                "the requested backend is incompatible with this model or options",
            )],
        },
        None => BackendSelectionResult {
            status: DecisionStatus::UnsupportedOverride,
            chosen_backend: None,
            override_handling: OverrideHandling::UnknownOverride(requested.clone()),
            fallback_order: Vec::new(),
            diagnostics: vec![RouterDiagnostic::fixed(
                "solver.unknown_override",
                "the requested backend is not present in the supplied registry",
            )],
        },
    }
}

fn override_handling_without_summary(selection: &BackendSelection) -> OverrideHandling {
    match selection {
        BackendSelection::Auto => OverrideHandling::Automatic,
        BackendSelection::Specific(id) => OverrideHandling::UnknownOverride(id.clone()),
    }
}

fn split_disposition(problem: &PlanningProblem, analysis: &ComponentAnalysis) -> SplitDisposition {
    if analysis.components.len() <= 1 {
        return SplitDisposition::SingleComponent;
    }
    let Some(authorization) = &problem.split_authorization else {
        return SplitDisposition::MissingAuthorization;
    };
    if authorization.component_hash != analysis.component_hash {
        return SplitDisposition::ComponentHashMismatch {
            authorized_hash: authorization.component_hash.clone(),
            actual_hash: analysis.component_hash.clone(),
        };
    }
    if authorization.domain_merge_contract.is_empty() {
        return SplitDisposition::MissingDomainMergeContract;
    }
    if !authorization.projection_independent {
        return SplitDisposition::ProjectionNotIndependent;
    }
    SplitDisposition::AuthorizedWholeModelOnly {
        authorization: authorization.clone(),
        component_ids: analysis
            .components
            .iter()
            .map(|component| component.id.to_string())
            .collect(),
    }
}

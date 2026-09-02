use crate::ortools_descriptor;
use eutheto_planning_ir::PlanningProblemSummary;
use eutheto_solver_api::{
    CapabilityMatrix, CompatibilityReport, PreflightError, compatibility_for,
};
use eutheto_types::SolveOptions;

/// Produces the OR-Tools compatibility report from the immutable planning summary,
/// solve options, and authoritative production support matrix.
///
/// Descriptor-to-matrix agreement is checked first so callers cannot receive a
/// report from stale or mismatched capability metadata.
///
/// # Errors
///
/// Returns an error when the reviewed descriptor is invalid, its production
/// matrix column is missing or mismatched, or an internal feature identifier
/// cannot be represented by the shared solver API.
pub fn ortools_compatibility(
    matrix: &CapabilityMatrix,
    summary: &PlanningProblemSummary,
    options: &SolveOptions,
) -> Result<CompatibilityReport, PreflightError> {
    let descriptor = ortools_descriptor()
        .map_err(|error| PreflightError::InvalidDescriptor(error.to_string()))?;
    matrix
        .validate_descriptor(&descriptor)
        .map_err(|error| PreflightError::Matrix(error.to_string()))?;
    compatibility_for(matrix, &descriptor.id, summary, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eutheto_planning_ir::{
        Capability, ComponentAnalysis, FeatureUsageManifest, LexicographicStrategy,
        PLANNING_IR_SCHEMA_VERSION,
    };
    use eutheto_solver_api::{
        BackendSupportColumn, CompatibilityLevel, SUPPORT_FEATURES,
        SUPPORT_MATRIX_IR_SCHEMA_VERSION, SUPPORT_MATRIX_SCHEMA_VERSION, SupportCell,
        SupportFeature, SupportFeatureCategory, SupportFeatureGate, SupportFeatureId,
        SupportMatrixError,
    };
    use eutheto_types::{
        BackendSelection, DurationMillis, ExplanationMode, PreservationPolicy, ReproducibilityMode,
        ResourceLimits, SolveMode, WorkerThreadPolicy,
    };
    use std::collections::BTreeSet;
    use std::error::Error;

    type TestResult = Result<(), Box<dyn Error>>;

    fn matrix() -> Result<CapabilityMatrix, Box<dyn Error>> {
        matrix_with_backend_version(crate::ORTOOLS_VERSION)
    }

    fn matrix_with_backend_version(
        backend_version: &str,
    ) -> Result<CapabilityMatrix, Box<dyn Error>> {
        let descriptor = ortools_descriptor()?;
        let features = SUPPORT_FEATURES
            .iter()
            .map(|(id, category, gate)| {
                let category = match *category {
                    "primitive" => SupportFeatureCategory::Primitive,
                    "objective" => SupportFeatureCategory::Objective,
                    "projection" => SupportFeatureCategory::Projection,
                    "solve" => SupportFeatureCategory::Solve,
                    other => {
                        return Err(SupportMatrixError::UnknownGeneratedCategory(
                            other.to_owned(),
                        ));
                    }
                };
                Ok(SupportFeature {
                    id: SupportFeatureId::new(*id)?,
                    category,
                    gate: if *gate == "unconditional" {
                        SupportFeatureGate::Unconditional
                    } else {
                        SupportFeatureGate::Enabled((*gate).to_owned())
                    },
                })
            })
            .collect::<Result<Vec<_>, SupportMatrixError>>()?;
        let cells = features
            .iter()
            .map(|feature| {
                let cell = if descriptor.capabilities.supported.contains(&feature.id) {
                    SupportCell::Supported {
                        fixture_id: "ortools.compatibility.supported".to_owned(),
                    }
                } else {
                    SupportCell::Unsupported {
                        reason: "This feature is outside the Phase 03 OR-Tools subset.".to_owned(),
                        remediation: "Use only the documented Phase 03 subset.".to_owned(),
                        fixture_id: "ortools.compatibility.unsupported".to_owned(),
                    }
                };
                (feature.id.clone(), cell)
            })
            .collect();
        Ok(CapabilityMatrix::new(
            SUPPORT_MATRIX_SCHEMA_VERSION,
            SUPPORT_MATRIX_IR_SCHEMA_VERSION,
            features,
            vec![BackendSupportColumn {
                backend_id: descriptor.id,
                backend_version: backend_version.to_owned(),
                adapter_version: descriptor.adapter_version,
                cells,
            }],
            Vec::new(),
        )?)
    }

    fn summary() -> PlanningProblemSummary {
        PlanningProblemSummary {
            schema_version: PLANNING_IR_SCHEMA_VERSION,
            variable_count: 0,
            bool_variable_count: 0,
            int_variable_count: 0,
            interval_variable_count: 0,
            constraint_count: 0,
            assumption_count: 0,
            objective_level_count: 0,
            objective_term_count: 0,
            lexicographic_strategy: LexicographicStrategy::ExactScalarization {
                weights: Vec::new(),
            },
            projection_count: 0,
            provenance_count: 0,
            domain_range_count: 0,
            total_reference_count: 0,
            min_coefficient: None,
            max_coefficient: None,
            manifest: FeatureUsageManifest::default(),
            components: ComponentAnalysis {
                components: Vec::new(),
                edge_count: 0,
                component_hash: "fixture-component-hash".to_owned(),
            },
            canonical_ir_hash: "fixture-ir-hash".to_owned(),
        }
    }

    fn options() -> Result<SolveOptions, Box<dyn Error>> {
        Ok(SolveOptions {
            backend: BackendSelection::Specific(ortools_descriptor()?.id),
            mode: SolveMode::Balanced,
            time_limit_milliseconds: DurationMillis::new(1_000)?,
            memory_limit_bytes: None,
            worker_threads: WorkerThreadPolicy::Exact(1),
            random_seed: 1,
            solution_limit: None,
            stop_after_first_feasible: false,
            collect_intermediate_solutions: false,
            explanation_mode: ExplanationMode::None,
            preserve_existing: PreservationPolicy::None,
            reproducibility: ReproducibilityMode::Deterministic,
            resource_limits: ResourceLimits {
                max_entities: 100,
                max_rules: 100,
                max_variables: 100,
                max_constraints: 100,
            },
        })
    }

    #[test]
    fn report_accepts_only_the_exact_advertised_phase03_surface() -> TestResult {
        let matrix = matrix()?;
        let baseline = ortools_compatibility(&matrix, &summary(), &options()?)?;
        assert_eq!(baseline.level, CompatibilityLevel::Supported);
        assert!(baseline.unsupported_features.is_empty());
        assert!(baseline.warnings.is_empty());
        let mut objective_summary = summary();
        objective_summary.objective_level_count = 1;
        objective_summary.objective_term_count = 1;
        objective_summary.lexicographic_strategy =
            LexicographicStrategy::ExactScalarization { weights: vec![1] };
        objective_summary
            .manifest
            .capability_counts
            .insert(Capability::ObjectivePenalty, 1);
        let objective = ortools_compatibility(&matrix, &objective_summary, &options()?)?;
        assert_eq!(objective.level, CompatibilityLevel::Supported);

        objective_summary.lexicographic_strategy = LexicographicStrategy::Multipass;
        let multipass = ortools_compatibility(&matrix, &objective_summary, &options()?)?;
        assert_eq!(multipass.level, CompatibilityLevel::Unsupported);
        assert_eq!(multipass.unsupported_features.len(), 1);
        assert_eq!(
            multipass.unsupported_features[0].feature_id.as_str(),
            "ir.multipass-objectives"
        );

        let mut unsupported_summary = summary();
        unsupported_summary
            .manifest
            .capability_counts
            .insert(Capability::IntervalProjection, 2);
        unsupported_summary.projection_count = 2;
        let unsupported = ortools_compatibility(&matrix, &unsupported_summary, &options()?)?;
        assert_eq!(unsupported.level, CompatibilityLevel::Unsupported);
        assert_eq!(unsupported.unsupported_features.len(), 1);
        assert_eq!(
            unsupported.unsupported_features[0].feature_id.as_str(),
            "projection.interval"
        );
        assert_eq!(unsupported.unsupported_features[0].usage_count, 2);

        let mut optional_features = options()?;
        optional_features.collect_intermediate_solutions = true;
        optional_features.explanation_mode = ExplanationMode::Standard;
        let unsupported = ortools_compatibility(&matrix, &summary(), &optional_features)?;
        assert_eq!(
            unsupported
                .unsupported_features
                .iter()
                .map(|feature| feature.feature_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "solve.infeasibility-evidence",
                "solve.intermediate-candidates",
            ])
        );
        Ok(())
    }

    #[test]
    fn report_rejects_matrix_descriptor_drift() -> TestResult {
        let mismatched = matrix_with_backend_version("9.15.9999")?;
        assert!(matches!(
            ortools_compatibility(&mismatched, &summary(), &options()?),
            Err(PreflightError::Matrix(_))
        ));
        Ok(())
    }
}

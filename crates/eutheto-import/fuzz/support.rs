#![allow(dead_code)]

use eutheto_domain_ir::{DomainEntityId, DomainEntityKindId, DomainEntityRef};
use eutheto_planning_ir::*;
use eutheto_types::{PackId, ScenarioId, SolutionId};
use std::collections::{BTreeMap, BTreeSet};

pub const LIMITS: PlanningIrLimitsV1 = PlanningIrLimitsV1 {
    max_ir_bytes: 32 * 1024,
    max_variables: 32,
    max_constraints: 32,
    max_assumptions: 16,
    max_objective_levels: 4,
    max_objective_terms: 32,
    max_provenance_records: 32,
    max_provenance_depth: 8,
    max_parameters_per_record: 8,
    max_parameter_text_bytes: 128,
    max_entity_refs_per_record: 16,
    max_projections: 32,
    max_projection_expression_depth: 4,
    max_domain_ranges: 16,
    max_refs_per_node: 64,
    max_total_refs: 512,
    max_table_rows: 32,
    max_table_arity: 8,
    max_table_cells: 128,
    max_intervals_per_global: 16,
    max_enforcement_literals: 8,
    max_tags: 8,
    max_component_nodes: 32,
    max_component_edges: 256,
    max_id_bytes: 80,
    max_metadata_text_bytes: 256,
    max_abs_coefficient: 1_000_000,
    max_abs_value: 1_000_000_000,
};

pub fn bool_id(name: &str) -> BoolVariableId {
    BoolVariableId::new(format!("bool.{name}")).expect("fixed Boolean ID is canonical")
}

pub fn int_id(name: &str) -> IntVariableId {
    IntVariableId::new(format!("int.{name}")).expect("fixed integer ID is canonical")
}

pub fn provenance_id() -> ProvenanceId {
    ProvenanceId::new("provenance.fuzz").expect("fixed provenance ID is canonical")
}

pub fn entity() -> DomainEntityRef {
    DomainEntityRef {
        kind: DomainEntityKindId::new("fuzz.entity").expect("fixed entity kind is canonical"),
        id: DomainEntityId::new("fuzz.entity-1").expect("fixed entity ID is canonical"),
    }
}

pub fn solution_id() -> SolutionId {
    "0195a5e4-7c00-7000-8000-000000000003"
        .parse()
        .expect("fixed solution ID is UUIDv7")
}

pub fn problem_with_variables(include_boolean: bool) -> PlanningProblem {
    let provenance = provenance_id();
    let domain = IntDomain::new(vec![InclusiveRange {
        start: -1_000,
        end: 1_000,
    }])
    .expect("fixed domain is valid");
    let mut variables = vec![
        Variable::Integer(IntVariable {
            id: int_id("x"),
            domain: domain.clone(),
            provenance: provenance.clone(),
        }),
        Variable::Integer(IntVariable {
            id: int_id("y"),
            domain,
            provenance: provenance.clone(),
        }),
    ];
    if include_boolean {
        variables.push(Variable::Boolean(BoolVariable {
            id: bool_id("enabled"),
            provenance: provenance.clone(),
        }));
    }
    let mut problem = PlanningProblem {
        schema_version: PLANNING_IR_SCHEMA_VERSION,
        variables,
        constraints: Vec::new(),
        objectives: ObjectivePlan::default(),
        assumptions: Vec::new(),
        projections: Vec::new(),
        provenance: vec![ProvenanceRecord {
            id: provenance,
            source_kind: ProvenanceSourceKind::Fact,
            source_id: "fuzz.fixture".to_owned(),
            entity_refs: Vec::new(),
            message_key: "fuzz.fixture".to_owned(),
            parameters: BTreeMap::new(),
            parent: None,
        }],
        metadata: PlanningMetadata {
            pack_id: PackId::new("official.fuzz").expect("fixed pack ID is canonical"),
            scenario_id: "0195a5e4-7c00-7000-8000-000000000001"
                .parse::<ScenarioId>()
                .expect("fixed scenario ID is UUIDv7"),
            scenario_revision: 1,
            projection_version: PROJECTION_SCHEMA_VERSION,
            compiler_id: CompilerId::new("compiler.fuzz").expect("fixed compiler ID is canonical"),
            compiler_version: "1".to_owned(),
            compile_metadata: BTreeMap::new(),
            display_text: BTreeMap::new(),
        },
        declared_capabilities: BTreeSet::new(),
        split_authorization: None,
    };
    problem
        .canonicalize()
        .expect("fixed planning problem canonicalizes");
    problem
}

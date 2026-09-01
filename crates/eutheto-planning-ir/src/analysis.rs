//! Capability, feature-usage, cost-summary, objective, and component analysis.

use crate::ids::{BoolVariableId, ComponentId, IntVariableId, IntervalVariableId};
use crate::model::{
    Capability, Constraint, LinearExpression, ObjectivePlan, ObjectiveTermKind, PlanningProblem,
    ProjectionExpression, Variable,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Rich deterministic feature inventory used before routing.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeatureUsageManifest {
    /// Count by required capability.
    pub capability_counts: BTreeMap<Capability, u64>,
    /// Count of optional intervals.
    pub optional_interval_count: u64,
    /// Count of global interval constraints.
    pub global_interval_constraint_count: u64,
    /// Count of table rows.
    pub table_row_count: u64,
    /// Count of table cells.
    pub table_cell_count: u64,
    /// Number of linear terms across constraints, objectives, and projections.
    pub linear_term_count: u64,
    /// Number of Boolean literal references.
    pub literal_reference_count: u64,
}

impl FeatureUsageManifest {
    fn add(&mut self, capability: Capability) {
        let entry = self.capability_counts.entry(capability).or_insert(0);
        *entry = entry.saturating_add(1);
    }

    /// Exact set of capabilities used by the problem.
    #[must_use]
    pub fn required_capabilities(&self) -> BTreeSet<Capability> {
        self.capability_counts.keys().copied().collect()
    }
}

/// One mathematical connected component.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MathematicalComponent {
    /// Deterministic component identity derived from its first node.
    pub id: ComponentId,
    /// Typed variable IDs encoded as `bool:...`, `int:...`, or `interval:...`.
    pub variable_nodes: Vec<String>,
}

/// Component hypergraph result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentAnalysis {
    /// Canonical components sorted by their first node.
    pub components: Vec<MathematicalComponent>,
    /// Conservative number of pairwise union edges produced from hyperedges.
    pub edge_count: u64,
    /// BLAKE3 of the canonical components and edge count.
    pub component_hash: String,
}

/// Safe exact scalarization weights or required multipass solving.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "strategy",
    content = "value",
    deny_unknown_fields
)]
pub enum LexicographicStrategy {
    /// Each weight corresponds to the objective level at the same vector position.
    ExactScalarization { weights: Vec<i64> },
    /// Exact priority cannot be represented safely in `i64`.
    Multipass,
}

/// Deterministic redacted size and cost summary. Contains no scenario display text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningProblemSummary {
    pub schema_version: u32,
    pub variable_count: u64,
    pub bool_variable_count: u64,
    pub int_variable_count: u64,
    pub interval_variable_count: u64,
    pub constraint_count: u64,
    pub assumption_count: u64,
    pub objective_level_count: u64,
    pub objective_term_count: u64,
    pub lexicographic_strategy: LexicographicStrategy,
    pub projection_count: u64,
    pub provenance_count: u64,
    pub domain_range_count: u64,
    pub total_reference_count: u64,
    pub min_coefficient: Option<i64>,
    pub max_coefficient: Option<i64>,
    pub manifest: FeatureUsageManifest,
    pub components: ComponentAnalysis,
    pub canonical_ir_hash: String,
}

/// Typed variable node for graph and reference analysis.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum VariableNode {
    Bool(BoolVariableId),
    Int(IntVariableId),
    Interval(IntervalVariableId),
}

impl VariableNode {
    fn encoded(&self) -> String {
        match self {
            Self::Bool(id) => format!("bool:{id}"),
            Self::Int(id) => format!("int:{id}"),
            Self::Interval(id) => format!("interval:{id}"),
        }
    }
}

/// Extracts the full feature inventory.
#[must_use]
pub fn feature_usage(problem: &PlanningProblem) -> FeatureUsageManifest {
    let mut usage = FeatureUsageManifest::default();
    for variable in &problem.variables {
        if let Variable::Interval(interval) = variable
            && interval.presence.is_some()
        {
            usage.add(Capability::OptionalIntervals);
            usage.optional_interval_count = usage.optional_interval_count.saturating_add(1);
            usage.literal_reference_count = usage.literal_reference_count.saturating_add(1);
        }
    }
    for record in &problem.constraints {
        usage.literal_reference_count = usage
            .literal_reference_count
            .saturating_add(record.enforcement.len() as u64);
        let capability = constraint_capability(&record.body);
        usage.add(capability);
        match &record.body {
            Constraint::BoolOr { literals }
            | Constraint::BoolAnd { literals }
            | Constraint::AtMostOne { literals }
            | Constraint::ExactlyOne { literals }
            | Constraint::CardinalityRange { literals, .. } => {
                usage.literal_reference_count = usage
                    .literal_reference_count
                    .saturating_add(literals.len() as u64);
            }
            Constraint::Implication { .. } | Constraint::Equivalence { .. } => {
                usage.literal_reference_count = usage.literal_reference_count.saturating_add(2);
            }
            Constraint::LinearComparison(comparison) => {
                usage.linear_term_count = usage
                    .linear_term_count
                    .saturating_add(comparison.expression.terms.len() as u64);
            }
            Constraint::ReifiedLinearComparison { comparison, .. } => {
                usage.literal_reference_count = usage.literal_reference_count.saturating_add(1);
                usage.linear_term_count = usage
                    .linear_term_count
                    .saturating_add(comparison.expression.terms.len() as u64);
            }
            Constraint::AllowedTable { variables, rows }
            | Constraint::ForbiddenTable { variables, rows } => {
                usage.table_row_count = usage.table_row_count.saturating_add(rows.len() as u64);
                usage.table_cell_count = usage
                    .table_cell_count
                    .saturating_add((variables.len() as u64).saturating_mul(rows.len() as u64));
            }
            Constraint::NoOverlap { .. } | Constraint::Cumulative { .. } => {
                usage.global_interval_constraint_count =
                    usage.global_interval_constraint_count.saturating_add(1);
            }
            Constraint::AllDifferent { .. }
            | Constraint::Element { .. }
            | Constraint::Min { .. }
            | Constraint::Max { .. }
            | Constraint::Equality { .. }
            | Constraint::AbsDifference { .. } => {}
        }
    }
    for level in &problem.objectives.levels {
        for term in &level.terms {
            usage.add(match term.kind {
                ObjectiveTermKind::Penalty => Capability::ObjectivePenalty,
                ObjectiveTermKind::Reward => Capability::ObjectiveReward,
            });
            usage.linear_term_count = usage
                .linear_term_count
                .saturating_add(term.expression.terms.len() as u64);
        }
    }
    if !problem.assumptions.is_empty() {
        usage.add(Capability::Assumptions);
        usage.literal_reference_count = usage
            .literal_reference_count
            .saturating_add(problem.assumptions.len() as u64);
    }
    for projection in &problem.projections {
        match &projection.expression {
            ProjectionExpression::Boolean(_) => usage.add(Capability::BooleanProjection),
            ProjectionExpression::Integer(_) => usage.add(Capability::IntegerProjection),
            ProjectionExpression::Linear(expression) => {
                usage.add(Capability::IntegerProjection);
                usage.linear_term_count = usage
                    .linear_term_count
                    .saturating_add(expression.terms.len() as u64);
            }
            ProjectionExpression::Interval(_) => usage.add(Capability::IntervalProjection),
            ProjectionExpression::Constant(value) => usage.add(match value {
                eutheto_domain_ir::AssignmentValue::Boolean(_) => Capability::BooleanProjection,
                eutheto_domain_ir::AssignmentValue::Integer(_) => Capability::IntegerProjection,
                eutheto_domain_ir::AssignmentValue::Interval(_) => Capability::IntervalProjection,
                eutheto_domain_ir::AssignmentValue::Absent => Capability::AbsentProjection,
            }),
        }
    }
    usage
}

fn constraint_capability(constraint: &Constraint) -> Capability {
    match constraint {
        Constraint::BoolOr { .. } => Capability::BoolOr,
        Constraint::BoolAnd { .. } => Capability::BoolAnd,
        Constraint::Implication { .. } => Capability::Implication,
        Constraint::Equivalence { .. } => Capability::Equivalence,
        Constraint::AtMostOne { .. } => Capability::AtMostOne,
        Constraint::ExactlyOne { .. } => Capability::ExactlyOne,
        Constraint::CardinalityRange { .. } => Capability::CardinalityRange,
        Constraint::LinearComparison(_) => Capability::LinearComparison,
        Constraint::ReifiedLinearComparison { .. } => Capability::ReifiedLinearComparison,
        Constraint::AllDifferent { .. } => Capability::AllDifferent,
        Constraint::AllowedTable { .. } => Capability::AllowedTable,
        Constraint::ForbiddenTable { .. } => Capability::ForbiddenTable,
        Constraint::Element { .. } => Capability::Element,
        Constraint::Min { .. } => Capability::Min,
        Constraint::Max { .. } => Capability::Max,
        Constraint::Equality { .. } => Capability::Equality,
        Constraint::AbsDifference { .. } => Capability::AbsDifference,
        Constraint::NoOverlap { .. } => Capability::NoOverlap,
        Constraint::Cumulative { .. } => Capability::Cumulative,
    }
}

/// Proves exact scalarization or selects exact multipass semantics.
#[must_use]
pub fn lexicographic_strategy(plan: &ObjectivePlan) -> LexicographicStrategy {
    if plan.levels.is_empty() {
        return LexicographicStrategy::ExactScalarization {
            weights: Vec::new(),
        };
    }
    let mut weights = vec![1_i64; plan.levels.len()];
    let mut lower_span = 1_i64;
    for index in (0..plan.levels.len()).rev() {
        weights[index] = lower_span;
        let level = &plan.levels[index];
        let Some(span) = level.upper_bound.checked_sub(level.lower_bound) else {
            return LexicographicStrategy::Multipass;
        };
        let Some(radix) = span.checked_add(1) else {
            return LexicographicStrategy::Multipass;
        };
        let Some(next) = lower_span.checked_mul(radix) else {
            return LexicographicStrategy::Multipass;
        };
        lower_span = next;
    }
    LexicographicStrategy::ExactScalarization { weights }
}

fn declared_nodes(problem: &PlanningProblem) -> Vec<VariableNode> {
    problem
        .variables
        .iter()
        .map(|variable| match variable {
            Variable::Boolean(value) => VariableNode::Bool(value.id.clone()),
            Variable::Integer(value) => VariableNode::Int(value.id.clone()),
            Variable::Interval(value) => VariableNode::Interval(value.id.clone()),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Computes mathematical components and a hash binding the exact result.
///
/// Constraints, interval structure, every objective level containing more than one variable,
/// projection expressions, and assumptions contribute hyperedges. A split is still forbidden
/// unless a validated [`crate::model::SplitAuthorization`] is present.
#[must_use]
pub fn analyze_components(problem: &PlanningProblem) -> ComponentAnalysis {
    let ordered = declared_nodes(problem);
    let indexes: BTreeMap<_, _> = ordered
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, node)| (node, index))
        .collect();
    let mut parent: Vec<usize> = (0..ordered.len()).collect();
    let mut edge_count = 0_u64;

    for variable in &problem.variables {
        if let Variable::Interval(interval) = variable {
            let mut edge = vec![
                VariableNode::Interval(interval.id.clone()),
                VariableNode::Int(interval.start.clone()),
                VariableNode::Int(interval.duration.clone()),
                VariableNode::Int(interval.end.clone()),
            ];
            if let Some(literal) = &interval.presence {
                edge.push(VariableNode::Bool(literal.variable.clone()));
            }
            union_edge(&edge, &indexes, &mut parent, &mut edge_count);
        }
    }
    for record in &problem.constraints {
        let mut edge = constraint_nodes(&record.body);
        edge.extend(
            record
                .enforcement
                .iter()
                .map(|literal| VariableNode::Bool(literal.variable.clone())),
        );
        union_edge(&edge, &indexes, &mut parent, &mut edge_count);
    }
    for level in &problem.objectives.levels {
        let edge: Vec<_> = level
            .terms
            .iter()
            .flat_map(|term| expression_nodes(&term.expression))
            .collect();
        union_edge(&edge, &indexes, &mut parent, &mut edge_count);
    }
    for projection in &problem.projections {
        let edge = projection_nodes(&projection.expression);
        union_edge(&edge, &indexes, &mut parent, &mut edge_count);
    }
    for assumption in &problem.assumptions {
        union_edge(
            &[VariableNode::Bool(assumption.literal.variable.clone())],
            &indexes,
            &mut parent,
            &mut edge_count,
        );
    }

    let mut grouped: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (index, node) in ordered.iter().enumerate() {
        let root = find_root(&mut parent, index);
        grouped.entry(root).or_default().push(node.encoded());
    }
    let mut raw_groups: Vec<Vec<String>> = grouped.into_values().collect();
    for group in &mut raw_groups {
        group.sort();
    }
    raw_groups.sort();
    let components: Vec<_> = raw_groups
        .into_iter()
        .enumerate()
        .filter_map(|(index, variable_nodes)| {
            ComponentId::new(format!("component.c{index}"))
                .ok()
                .map(|id| MathematicalComponent { id, variable_nodes })
        })
        .collect();
    let component_hash = hash_components(&components, edge_count);
    ComponentAnalysis {
        components,
        edge_count,
        component_hash,
    }
}

fn hash_components(components: &[MathematicalComponent], edge_count: u64) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&edge_count.to_le_bytes());
    for component in components {
        hasher.update(&(component.variable_nodes.len() as u64).to_le_bytes());
        for node in &component.variable_nodes {
            hasher.update(&(node.len() as u64).to_le_bytes());
            hasher.update(node.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn union_edge(
    edge: &[VariableNode],
    indexes: &BTreeMap<VariableNode, usize>,
    parent: &mut [usize],
    edge_count: &mut u64,
) {
    let unique: BTreeSet<_> = edge.iter().collect();
    let mut positions = unique
        .into_iter()
        .filter_map(|node| indexes.get(node).copied());
    let Some(first) = positions.next() else {
        return;
    };
    for position in positions {
        union(parent, first, position);
        *edge_count = edge_count.saturating_add(1);
    }
}

fn find_root(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        let grandparent = parent[parent[node]];
        parent[node] = grandparent;
        node = grandparent;
    }
    node
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find_root(parent, left);
    let right_root = find_root(parent, right);
    if left_root != right_root {
        let (low, high) = if left_root < right_root {
            (left_root, right_root)
        } else {
            (right_root, left_root)
        };
        parent[high] = low;
    }
}

pub(crate) fn expression_nodes(expression: &LinearExpression) -> Vec<VariableNode> {
    expression
        .terms
        .iter()
        .map(|term| VariableNode::Int(term.variable.clone()))
        .collect()
}

pub(crate) fn projection_nodes(expression: &ProjectionExpression) -> Vec<VariableNode> {
    match expression {
        ProjectionExpression::Boolean(id) => vec![VariableNode::Bool(id.clone())],
        ProjectionExpression::Integer(id) => vec![VariableNode::Int(id.clone())],
        ProjectionExpression::Linear(expression) => expression_nodes(expression),
        ProjectionExpression::Interval(id) => vec![VariableNode::Interval(id.clone())],
        ProjectionExpression::Constant(_) => Vec::new(),
    }
}

pub(crate) fn constraint_nodes(constraint: &Constraint) -> Vec<VariableNode> {
    match constraint {
        Constraint::BoolOr { literals }
        | Constraint::BoolAnd { literals }
        | Constraint::AtMostOne { literals }
        | Constraint::ExactlyOne { literals }
        | Constraint::CardinalityRange { literals, .. } => literals
            .iter()
            .map(|literal| VariableNode::Bool(literal.variable.clone()))
            .collect(),
        Constraint::Implication {
            antecedent,
            consequent,
        } => vec![
            VariableNode::Bool(antecedent.variable.clone()),
            VariableNode::Bool(consequent.variable.clone()),
        ],
        Constraint::Equivalence { left, right } => vec![
            VariableNode::Bool(left.variable.clone()),
            VariableNode::Bool(right.variable.clone()),
        ],
        Constraint::LinearComparison(comparison) => expression_nodes(&comparison.expression),
        Constraint::ReifiedLinearComparison {
            literal,
            comparison,
        } => {
            let mut nodes = expression_nodes(&comparison.expression);
            nodes.push(VariableNode::Bool(literal.variable.clone()));
            nodes
        }
        Constraint::AllDifferent { variables }
        | Constraint::AllowedTable { variables, .. }
        | Constraint::ForbiddenTable { variables, .. } => {
            variables.iter().cloned().map(VariableNode::Int).collect()
        }
        Constraint::Min { target, inputs } | Constraint::Max { target, inputs } => {
            let mut nodes = Vec::with_capacity(inputs.len().saturating_add(1));
            nodes.push(VariableNode::Int(target.clone()));
            nodes.extend(inputs.iter().cloned().map(VariableNode::Int));
            nodes
        }
        Constraint::Element { index, target, .. } => vec![
            VariableNode::Int(index.clone()),
            VariableNode::Int(target.clone()),
        ],
        Constraint::Equality { left, right } => {
            vec![
                VariableNode::Int(left.clone()),
                VariableNode::Int(right.clone()),
            ]
        }
        Constraint::AbsDifference {
            target,
            left,
            right,
        } => vec![
            VariableNode::Int(target.clone()),
            VariableNode::Int(left.clone()),
            VariableNode::Int(right.clone()),
        ],
        Constraint::NoOverlap { intervals } | Constraint::Cumulative { intervals, .. } => intervals
            .iter()
            .cloned()
            .map(VariableNode::Interval)
            .collect(),
    }
}

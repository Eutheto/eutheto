use eutheto_domain_api::DomainPack;
use eutheto_planning_ir::PlanningProblem;
use eutheto_solver_api::BackendCandidate;
use eutheto_solver_router::{CandidateReview, CandidateReviewer};
use eutheto_types::{BackendId, IdGenerator, ScenarioDocument, SolutionId};
use eutheto_verify::{AcceptanceDecision, AcceptanceReviewer, CorrectnessAlarm, VerificationClock};

const SOLUTION_ID_FAILURE_CODE: &str = "verification.solution_id_generation_failed";

/// Application-layer bridge from router candidates to independent acceptance verification.
///
/// The router receives only the terminal disposition. The complete accepted result or bounded
/// correctness alarm remains available through [`Self::decision`] for authoritative persistence.
pub struct RouterCandidateReviewer<'a> {
    acceptance: AcceptanceReviewer<'a>,
    id_generator: &'a dyn IdGenerator,
    decision: Option<AcceptanceDecision>,
    before_review: Option<Box<dyn FnMut() + Send + 'a>>,
}

impl<'a> RouterCandidateReviewer<'a> {
    /// Binds candidate review to one immutable scenario revision and planning problem.
    ///
    /// # Errors
    ///
    /// Returns a bounded correctness alarm when the planning problem or immutable bindings are
    /// invalid.
    pub fn new(
        pack: &'a dyn DomainPack,
        document: &'a ScenarioDocument,
        scenario_revision: u64,
        problem: &'a PlanningProblem,
        clock: &'a dyn VerificationClock,
        id_generator: &'a dyn IdGenerator,
    ) -> Result<Self, CorrectnessAlarm> {
        Ok(Self {
            acceptance: AcceptanceReviewer::new(pack, document, scenario_revision, problem, clock)?,
            id_generator,
            decision: None,
            before_review: None,
        })
    }

    /// Binds candidate review and invokes `before_review` immediately before independent review.
    ///
    /// # Errors
    ///
    /// Returns a bounded correctness alarm when the planning problem or immutable bindings are
    /// invalid.
    pub fn new_notifying(
        pack: &'a dyn DomainPack,
        document: &'a ScenarioDocument,
        scenario_revision: u64,
        problem: &'a PlanningProblem,
        clock: &'a dyn VerificationClock,
        id_generator: &'a dyn IdGenerator,
        before_review: impl FnMut() + Send + 'a,
    ) -> Result<Self, CorrectnessAlarm> {
        Ok(Self {
            acceptance: AcceptanceReviewer::new(pack, document, scenario_revision, problem, clock)?,
            id_generator,
            decision: None,
            before_review: Some(Box::new(before_review)),
        })
    }

    /// Returns the most recent complete acceptance decision.
    #[must_use]
    pub const fn decision(&self) -> Option<&AcceptanceDecision> {
        self.decision.as_ref()
    }
}

impl CandidateReviewer for RouterCandidateReviewer<'_> {
    fn review(&mut self, _backend_id: &BackendId, candidate: &BackendCandidate) -> CandidateReview {
        let Ok(solution_id) = SolutionId::new(self.id_generator) else {
            return CandidateReview::VerificationFailed {
                diagnostic_code: SOLUTION_ID_FAILURE_CODE.to_owned(),
            };
        };
        if let Some(before_review) = &mut self.before_review {
            before_review();
        }
        let decision = self.acceptance.review(candidate, solution_id);
        let disposition = match &decision {
            AcceptanceDecision::Awaiting => CandidateReview::AwaitingIndependentVerification,
            AcceptanceDecision::Accepted { .. } => CandidateReview::Verified,
            AcceptanceDecision::Quarantined { alarm, .. } => CandidateReview::VerificationFailed {
                diagnostic_code: alarm.diagnostic_code.clone(),
            },
        };
        self.decision = Some(decision);
        disposition
    }
}

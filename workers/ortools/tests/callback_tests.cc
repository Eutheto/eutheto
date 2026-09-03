// SPDX-License-Identifier: Apache-2.0
#include <cmath>
#include <cstdint>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include "ortools/base/version.h"
#include "ortools/sat/cp_model.pb.h"
#include "ortools/sat/cp_model_solver.h"
#include "ortools/sat/model.h"
#include "ortools/sat/sat_parameters.pb.h"

namespace {

namespace sat = operations_research::sat;

constexpr std::int32_t kSeed = 1;
constexpr double kTolerance = 1e-9;

struct CallbackEvidence {
  int fixed_feasible_incumbents = 0;
  int infeasible_incumbents = 0;
  int optimization_incumbents = 0;
  double initial_incumbent = 0.0;
  double final_incumbent = 0.0;
  int best_bound_updates = 0;
  double initial_best_bound = 0.0;
  double final_best_bound = 0.0;
  int stop_incumbents = 0;
};

void Require(bool condition, std::string_view message) {
  if (!condition)
    throw std::runtime_error(std::string(message));
}

void RequireNear(double actual, double expected, std::string_view message) {
  Require(std::isfinite(actual) && std::abs(actual - expected) <= kTolerance,
          message);
}

std::int32_t AddVariable(sat::CpModelProto *model, std::int64_t lower,
                         std::int64_t upper) {
  const auto index = static_cast<std::int32_t>(model->variables_size());
  auto *variable = model->add_variables();
  variable->add_domain(lower);
  variable->add_domain(upper);
  return index;
}

std::int32_t AddBoolVariable(sat::CpModelProto *model) {
  return AddVariable(model, 0, 1);
}

sat::SatParameters DeterministicParameters() {
  sat::SatParameters parameters;
  parameters.set_num_workers(1);
  parameters.set_random_seed(kSeed);
  return parameters;
}

void VerifyFixedFeasibleIncumbent(CallbackEvidence *evidence) {
  sat::CpModelProto problem;
  AddVariable(&problem, 7, 7);

  bool shape_valid = true;
  std::int64_t observed_value = 0;
  sat::Model model;
  model.Add(sat::NewSatParameters(DeterministicParameters()));
  model.Add(sat::NewFeasibleSolutionObserver(
      [&](const sat::CpSolverResponse &response) {
        ++evidence->fixed_feasible_incumbents;
        shape_valid = shape_valid && response.solution_size() == 1;
        if (response.solution_size() == 1)
          observed_value = response.solution(0);
      }));

  const sat::CpSolverResponse response = sat::SolveCpModel(problem, &model);
  Require(response.status() == sat::CpSolverStatus::OPTIMAL,
          "fixed feasibility model did not terminate as optimal");
  Require(evidence->fixed_feasible_incumbents == 1,
          "fixed feasibility model did not emit exactly one incumbent");
  Require(shape_valid && observed_value == 7,
          "fixed feasibility incumbent carried the wrong projection");
}

void VerifyInfeasibleHasNoIncumbent(CallbackEvidence *evidence) {
  sat::CpModelProto problem;
  const std::int32_t variable = AddVariable(&problem, 0, 0);
  auto *linear = problem.add_constraints()->mutable_linear();
  linear->add_vars(variable);
  linear->add_coeffs(1);
  linear->add_domain(1);
  linear->add_domain(1);

  sat::Model model;
  model.Add(sat::NewSatParameters(DeterministicParameters()));
  model.Add(
      sat::NewFeasibleSolutionObserver([&](const sat::CpSolverResponse &) {
        ++evidence->infeasible_incumbents;
      }));

  const sat::CpSolverResponse response = sat::SolveCpModel(problem, &model);
  Require(response.status() == sat::CpSolverStatus::INFEASIBLE,
          "contradictory model did not terminate as infeasible");
  Require(evidence->infeasible_incumbents == 0,
          "infeasible model emitted an incumbent callback");
}

void VerifyImprovingIncumbentAndBound(CallbackEvidence *evidence) {
  sat::CpModelProto problem;
  std::vector<std::int32_t> variables;
  for (int index = 0; index < 4; ++index)
    variables.push_back(AddBoolVariable(&problem));

  auto *clause = problem.add_constraints()->mutable_bool_or();
  for (const std::int32_t variable : variables)
    clause->add_literals(variable);

  auto *objective = problem.mutable_objective();
  constexpr std::int64_t coefficients[] = {3, 2, 4, 5};
  for (std::size_t index = 0; index < variables.size(); ++index) {
    objective->add_vars(variables[index]);
    objective->add_coeffs(coefficients[index]);
  }
  objective->set_offset(0.6);
  objective->set_scaling_factor(1.0);
  auto *hint = problem.mutable_solution_hint();
  constexpr std::int64_t hinted_values[] = {0, 0, 0, 1};
  for (std::size_t index = 0; index < variables.size(); ++index) {
    hint->add_vars(variables[index]);
    hint->add_values(hinted_values[index]);
  }

  std::vector<double> incumbent_values;
  std::vector<double> best_bounds;
  sat::SatParameters parameters = DeterministicParameters();
  parameters.set_linearization_level(2);
  parameters.set_cp_model_presolve(false);
  parameters.set_search_branching(sat::SatParameters::FIXED_SEARCH);

  sat::Model model;
  model.Add(sat::NewSatParameters(parameters));
  model.Add(sat::NewFeasibleSolutionObserver(
      [&](const sat::CpSolverResponse &response) {
        ++evidence->optimization_incumbents;
        incumbent_values.push_back(response.objective_value());
      }));
  model.Add(sat::NewBestBoundCallback([&](double bound) {
    ++evidence->best_bound_updates;
    best_bounds.push_back(bound);
  }));

  const sat::CpSolverResponse response = sat::SolveCpModel(problem, &model);
  Require(response.status() == sat::CpSolverStatus::OPTIMAL,
          "bound fixture did not terminate as optimal");
  RequireNear(response.objective_value(), 2.6,
              "bound fixture returned the wrong objective");
  RequireNear(response.best_objective_bound(), 2.6,
              "bound fixture returned the wrong final bound");
  Require(incumbent_values.size() >= 2,
          "optimization model did not emit improving incumbent callbacks");
  for (std::size_t index = 1; index < incumbent_values.size(); ++index) {
    Require(incumbent_values[index] < incumbent_values[index - 1],
            "incumbent callback values did not strictly improve");
  }
  Require(best_bounds.size() >= 2,
          "optimization model did not emit improving bound callbacks");
  for (std::size_t index = 0; index < best_bounds.size(); ++index) {
    Require(std::isfinite(best_bounds[index]),
            "bound callback emitted a non-finite value");
    if (index > 0) {
      Require(best_bounds[index] > best_bounds[index - 1],
              "best-bound callback values did not strictly improve");
    }
  }
  evidence->initial_incumbent = incumbent_values.front();
  evidence->final_incumbent = incumbent_values.back();
  evidence->initial_best_bound = best_bounds.front();
  evidence->final_best_bound = best_bounds.back();
  RequireNear(evidence->initial_incumbent, 5.6,
              "complete feasible hint was not the initial incumbent");
  RequireNear(evidence->final_incumbent, 2.6,
              "incumbent callback did not reach the optimum");
  Require(evidence->initial_best_bound < evidence->final_best_bound,
          "best-bound callbacks did not span a real improvement");
  RequireNear(evidence->final_best_bound, 2.6,
              "best-bound callback did not reach the proved optimum");
}

void VerifyCallbackDrivenStop(CallbackEvidence *evidence) {
  sat::CpModelProto problem;
  constexpr int kVariableCount = 20;
  for (int index = 0; index < kVariableCount; ++index)
    AddBoolVariable(&problem);

  sat::SatParameters parameters = DeterministicParameters();
  parameters.set_enumerate_all_solutions(true);

  bool shape_valid = true;
  sat::Model model;
  model.Add(sat::NewSatParameters(parameters));
  model.Add(sat::NewFeasibleSolutionObserver(
      [&](const sat::CpSolverResponse &response) {
        ++evidence->stop_incumbents;
        shape_valid = shape_valid && response.solution_size() == kVariableCount;
        if (evidence->stop_incumbents == 1)
          sat::StopSearch(&model);
      }));

  const sat::CpSolverResponse response = sat::SolveCpModel(problem, &model);
  Require(response.status() == sat::CpSolverStatus::FEASIBLE,
          "callback-stopped search did not terminate as feasible");
  Require(evidence->stop_incumbents == 1,
          "callback-driven stop allowed another incumbent");
  Require(shape_valid,
          "callback-stopped incumbent had the wrong projection size");
}

} // namespace

int main() {
  try {
    CallbackEvidence evidence;
    VerifyFixedFeasibleIncumbent(&evidence);
    VerifyInfeasibleHasNoIncumbent(&evidence);
    VerifyImprovingIncumbentAndBound(&evidence);
    VerifyCallbackDrivenStop(&evidence);

    std::cout << "callback_schema_version=1\n"
              << "ortools_version="
              << operations_research::OrToolsVersionString() << '\n'
              << "seed=" << kSeed << '\n'
              << "worker_threads=1\n"
              << "fixed_feasible_incumbents="
              << evidence.fixed_feasible_incumbents << '\n'
              << "infeasible_incumbents=" << evidence.infeasible_incumbents
              << '\n'
              << "optimization_incumbents=" << evidence.optimization_incumbents
              << '\n'
              << "initial_incumbent=" << evidence.initial_incumbent << '\n'
              << "final_incumbent=" << evidence.final_incumbent << '\n'
              << "best_bound_updates=" << evidence.best_bound_updates << '\n'
              << "initial_best_bound=" << evidence.initial_best_bound << '\n'
              << "final_best_bound=" << evidence.final_best_bound << '\n'
              << "stop_incumbents=" << evidence.stop_incumbents << '\n'
              << "stop_status=feasible\n"
              << "multi_worker_scope=not-tested\n"
              << "protocol_event_count_scope=not-tested\n"
              << "callback_result=passed\n";
    return 0;
  } catch (const std::exception &error) {
    std::cerr << "callback_result=failed\nreason=" << error.what() << '\n';
    return 1;
  }
}

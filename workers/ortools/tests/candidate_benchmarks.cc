// SPDX-License-Identifier: Apache-2.0
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <initializer_list>
#include <iomanip>
#include <iostream>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "ortools/sat/cp_model.pb.h"
#include "protocol-policy.h"
#include "sha256.h"
#include "solver-worker.pb.h"
#include "wire.h"
#include "worker.h"

namespace {

namespace native = eutheto::ortools_worker;
namespace protocol = eutheto::worker::v1;
namespace policy = eutheto::worker::v1::policy;
namespace sat = operations_research::sat;

constexpr std::int32_t kSeed = 1;
constexpr std::uint32_t kWorkerThreads = 1;
constexpr std::uint64_t kWallTimeMillis = 2000;
constexpr int kRepetitions = 3;

struct Fixture {
  std::string id;
  std::string primitives;
  sat::CpModelProto model;
  std::vector<std::int32_t> projections;
  std::vector<std::int64_t> expected_projection;
  std::optional<double> expected_objective;
};

struct Sample {
  protocol::HandshakeSuccess handshake;
  protocol::Finished finished;
  double worker_wall_seconds = 0.0;
};

void Require(bool condition, std::string_view message) {
  if (!condition)
    throw std::runtime_error(std::string(message));
}

std::int32_t Negated(std::int32_t variable) { return -variable - 1; }

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

std::int32_t AddFixedBoolVariable(sat::CpModelProto *model, bool value) {
  const std::int64_t fixed = value ? 1 : 0;
  return AddVariable(model, fixed, fixed);
}

void AddBoolOr(sat::CpModelProto *model,
               std::initializer_list<std::int32_t> literals) {
  auto *constraint = model->add_constraints()->mutable_bool_or();
  for (const std::int32_t literal : literals)
    constraint->add_literals(literal);
}

void AddBoolAnd(sat::CpModelProto *model,
                std::initializer_list<std::int32_t> literals,
                std::optional<std::int32_t> enforcement = std::nullopt) {
  auto *record = model->add_constraints();
  if (enforcement)
    record->add_enforcement_literal(*enforcement);
  auto *constraint = record->mutable_bool_and();
  for (const std::int32_t literal : literals)
    constraint->add_literals(literal);
}

void AddAtMostOne(sat::CpModelProto *model,
                  std::initializer_list<std::int32_t> literals) {
  auto *constraint = model->add_constraints()->mutable_at_most_one();
  for (const std::int32_t literal : literals)
    constraint->add_literals(literal);
}

void AddExactlyOne(sat::CpModelProto *model,
                   std::initializer_list<std::int32_t> literals) {
  auto *constraint = model->add_constraints()->mutable_exactly_one();
  for (const std::int32_t literal : literals)
    constraint->add_literals(literal);
}

void AddLinear(
    sat::CpModelProto *model,
    std::initializer_list<std::pair<std::int32_t, std::int64_t>> terms,
    std::int64_t lower, std::int64_t upper,
    std::optional<std::int32_t> enforcement = std::nullopt) {
  auto *record = model->add_constraints();
  if (enforcement)
    record->add_enforcement_literal(*enforcement);
  auto *constraint = record->mutable_linear();
  for (const auto &[variable, coefficient] : terms) {
    constraint->add_vars(variable);
    constraint->add_coeffs(coefficient);
  }
  constraint->add_domain(lower);
  constraint->add_domain(upper);
}

void AddObjectiveTerm(sat::CpModelProto *model, std::int32_t variable,
                      std::int64_t coefficient) {
  auto *objective = model->mutable_objective();
  objective->add_vars(variable);
  objective->add_coeffs(coefficient);
}

Fixture BoolOrFixture() {
  Fixture fixture{"bool-or", "bool-or,objective-penalty,projection", {}, {}, {},
                  1.0};
  const auto a = AddBoolVariable(&fixture.model);
  const auto b = AddBoolVariable(&fixture.model);
  AddBoolOr(&fixture.model, {a, b});
  AddObjectiveTerm(&fixture.model, a, 1);
  AddObjectiveTerm(&fixture.model, b, 1);
  fixture.projections = {a, b};
  return fixture;
}

Fixture BoolAndFixture() {
  Fixture fixture{"bool-and", "bool-and,projection", {}, {}, {}, std::nullopt};
  const auto a = AddBoolVariable(&fixture.model);
  const auto b = AddBoolVariable(&fixture.model);
  AddBoolAnd(&fixture.model, {a, b});
  fixture.projections = {a, b};
  fixture.expected_projection = {1, 1};
  return fixture;
}

Fixture ImplicationEquivalenceFixture() {
  Fixture fixture{"implication-equivalence",
                  "implication,equivalence",
                  {},
                  {},
                  {},
                  std::nullopt};
  const auto a = AddFixedBoolVariable(&fixture.model, true);
  const auto b = AddBoolVariable(&fixture.model);
  const auto c = AddBoolVariable(&fixture.model);
  AddBoolOr(&fixture.model, {Negated(a), b});
  AddBoolOr(&fixture.model, {Negated(b), c});
  AddBoolOr(&fixture.model, {Negated(c), b});
  fixture.projections = {a, b, c};
  fixture.expected_projection = {1, 1, 1};
  return fixture;
}

Fixture AtMostOneFixture() {
  Fixture fixture{"at-most-one", "at-most-one", {}, {}, {}, std::nullopt};
  const auto a = AddFixedBoolVariable(&fixture.model, true);
  const auto b = AddBoolVariable(&fixture.model);
  const auto c = AddBoolVariable(&fixture.model);
  AddAtMostOne(&fixture.model, {a, b, c});
  fixture.projections = {a, b, c};
  fixture.expected_projection = {1, 0, 0};
  return fixture;
}

Fixture ExactlyOneFixture() {
  Fixture fixture{"exactly-one",
                  "exactly-one,objective-penalty,projection",
                  {},
                  {},
                  {},
                  1.0};
  const auto a = AddFixedBoolVariable(&fixture.model, true);
  const auto b = AddBoolVariable(&fixture.model);
  const auto c = AddBoolVariable(&fixture.model);
  AddExactlyOne(&fixture.model, {a, b, c});
  const auto d = AddBoolVariable(&fixture.model);
  const auto e = AddBoolVariable(&fixture.model);
  AddExactlyOne(&fixture.model, {d, e});
  AddObjectiveTerm(&fixture.model, d, 1);
  AddObjectiveTerm(&fixture.model, e, 2);
  fixture.projections = {a, b, c, d, e};
  fixture.expected_projection = {1, 0, 0, 1, 0};
  return fixture;
}

Fixture CardinalityRangeFixture() {
  Fixture fixture{
      "cardinality-range",
      "cardinality-range,objective-penalty,objective-reward,projection",
      {},
      {},
      {},
      -1.0};
  const auto a = AddBoolVariable(&fixture.model);
  const auto b = AddBoolVariable(&fixture.model);
  const auto c = AddBoolVariable(&fixture.model);
  const auto d = AddBoolVariable(&fixture.model);
  AddLinear(&fixture.model, {{a, 1}, {b, 1}, {c, 1}, {d, 1}}, 2, 3);
  for (const auto variable : {a, b, c, d}) {
    AddObjectiveTerm(&fixture.model, variable, 1);
    fixture.projections.push_back(variable);
  }
  const auto e = AddBoolVariable(&fixture.model);
  const auto f = AddBoolVariable(&fixture.model);
  const auto g = AddBoolVariable(&fixture.model);
  const auto h = AddBoolVariable(&fixture.model);
  AddLinear(&fixture.model, {{e, 1}, {f, 1}, {g, 1}, {h, 1}}, 2, 3);
  for (const auto variable : {e, f, g, h}) {
    AddObjectiveTerm(&fixture.model, variable, -1);
    fixture.projections.push_back(variable);
  }
  return fixture;
}

Fixture LinearFixture() {
  Fixture fixture{
      "integer-linear",
      "linear-equality,linear-inequality,equality,objective-penalty,objective-"
      "reward,projection",
      {},
      {},
      {},
      0.0};
  const auto x = AddVariable(&fixture.model, 0, 10);
  const auto y = AddVariable(&fixture.model, 0, 10);
  AddLinear(&fixture.model, {{x, 1}, {y, 1}}, 10, 10);
  AddLinear(&fixture.model, {{x, 1}, {y, -1}}, 0, 10);
  AddObjectiveTerm(&fixture.model, x, 1);
  AddObjectiveTerm(&fixture.model, y, -1);
  fixture.projections = {x, y};
  fixture.expected_projection = {5, 5};
  return fixture;
}

Fixture ReifiedLinearFixture() {
  Fixture fixture{"reified-linear",
                  "reified-linear-comparison,enforcement-literal",
                  {},
                  {},
                  {},
                  std::nullopt};
  const auto x = AddVariable(&fixture.model, 7, 7);
  const auto gate = AddBoolVariable(&fixture.model);
  AddLinear(&fixture.model, {{x, 1}}, 5, 10, gate);
  AddLinear(&fixture.model, {{x, 1}}, 0, 4, Negated(gate));
  fixture.projections = {x, gate};
  fixture.expected_projection = {7, 1};
  return fixture;
}

Fixture ScaledMixedFixture() {
  Fixture fixture{
      "scaled-mixed-supported-subset",
      "bool-or,bool-and,implication,equivalence,at-most-one,exactly-one,"
      "cardinality-range,linear-equality,linear-inequality,reified-linear-"
      "comparison,enforcement-literal,objective,projection",
      {},
      {},
      {},
      2688.0};
  constexpr int kBlocks = 128;
  for (int block = 0; block < kBlocks; ++block) {
    const auto a = AddBoolVariable(&fixture.model);
    const auto b = AddBoolVariable(&fixture.model);
    const auto c = AddBoolVariable(&fixture.model);
    const auto d = AddBoolVariable(&fixture.model);
    const auto x = AddVariable(&fixture.model, 0, 100);
    const auto y = AddVariable(&fixture.model, 0, 100);

    AddExactlyOne(&fixture.model, {a, b, c, d});
    AddAtMostOne(&fixture.model, {a, b, c, d});
    AddBoolOr(&fixture.model, {a, b, c, d});
    AddBoolOr(&fixture.model, {Negated(a), b});
    AddBoolOr(&fixture.model, {Negated(b), c});
    AddBoolOr(&fixture.model, {Negated(c), b});
    AddBoolAnd(&fixture.model, {Negated(a), Negated(b), Negated(c)}, d);
    AddLinear(&fixture.model, {{a, 1}, {b, 1}, {c, 1}, {d, 1}}, 1, 1);
    AddLinear(&fixture.model, {{x, 1}, {y, 1}}, 100, 100);
    AddLinear(&fixture.model, {{x, 1}, {y, -1}}, -100, 0);
    AddLinear(&fixture.model, {{x, 1}}, 20, 100, d);
    AddObjectiveTerm(&fixture.model, x, 1);
    AddObjectiveTerm(&fixture.model, d, 1);

    if (block == 0 || block == kBlocks - 1) {
      fixture.projections.insert(fixture.projections.end(), {d, x, y});
      fixture.expected_projection.insert(fixture.expected_projection.end(),
                                         {1, 20, 80});
    }
  }
  return fixture;
}

std::vector<Fixture> Fixtures() {
  std::vector<Fixture> fixtures;
  fixtures.reserve(9);
  fixtures.push_back(BoolOrFixture());
  fixtures.push_back(BoolAndFixture());
  fixtures.push_back(ImplicationEquivalenceFixture());
  fixtures.push_back(AtMostOneFixture());
  fixtures.push_back(ExactlyOneFixture());
  fixtures.push_back(CardinalityRangeFixture());
  fixtures.push_back(LinearFixture());
  fixtures.push_back(ReifiedLinearFixture());
  fixtures.push_back(ScaledMixedFixture());
  return fixtures;
}

std::string EncodeParent(const protocol::ParentFrame &frame) {
  const std::string payload = frame.SerializeAsString();
  const auto size = static_cast<std::uint32_t>(payload.size());
  std::string framed;
  framed.reserve(policy::kLengthPrefixBytes + payload.size());
  framed.push_back(static_cast<char>(size >> 24U));
  framed.push_back(static_cast<char>(size >> 16U));
  framed.push_back(static_cast<char>(size >> 8U));
  framed.push_back(static_cast<char>(size));
  framed += payload;
  return framed;
}

protocol::ParentFrame Handshake() {
  protocol::ParentFrame frame;
  auto *request = frame.mutable_handshake_request();
  request->set_protocol_major(policy::kProtocolMajor);
  request->set_protocol_minor(policy::kProtocolMinor);
  request->set_core_version("1.0.0-alpha.1");
  request->set_expected_backend_id("ortools-cp-sat");
  request->set_expected_manifest_sha256(std::string(32, '\x5a'));
  request->add_required_capabilities(protocol::CAPABILITY_CP_SAT);
  request->add_required_capabilities(protocol::CAPABILITY_SOLUTION_PROJECTION);
  request->add_required_capabilities(protocol::CAPABILITY_OBJECTIVE_BOUNDS);
  request->add_required_capabilities(protocol::CAPABILITY_SOLUTION_STATS);
  request->add_required_capabilities(protocol::CAPABILITY_DETERMINISTIC_TIME);
  return frame;
}

protocol::ParentFrame SolveRequest(const Fixture &fixture, int run_index) {
  protocol::ParentFrame frame;
  auto *request = frame.mutable_solve_request();
  request->set_request_id("benchmark-" + fixture.id + "-" +
                          std::to_string(run_index));
  request->set_cp_model_proto(fixture.model.SerializeAsString());
  const auto fingerprint = native::Sha256String(request->cp_model_proto());
  request->set_model_fingerprint(fingerprint.data(), fingerprint.size());
  request->mutable_parameters()->set_random_seed(kSeed);
  request->mutable_parameters()->set_deterministic_test_profile(true);
  request->mutable_resource_limits()->set_wall_time_millis(kWallTimeMillis);
  request->mutable_resource_limits()->set_worker_threads(kWorkerThreads);
  for (std::size_t index = 0; index < fixture.projections.size(); ++index) {
    auto *projection = request->add_projections();
    projection->set_projection_id(static_cast<std::uint64_t>(index + 1));
    projection->set_cp_sat_variable_index(fixture.projections[index]);
  }
  return frame;
}

std::vector<protocol::WorkerFrame> DecodeOutput(const std::string &bytes) {
  std::istringstream input(bytes, std::ios::binary);
  native::FrameReader reader(input);
  std::vector<protocol::WorkerFrame> frames;
  for (;;) {
    const native::FrameReadResult result =
        reader.Read(policy::kWorkerEventMaxPayloadBytes);
    if (result.status == native::FrameReadStatus::kEof)
      break;
    Require(result.status == native::FrameReadStatus::kOk,
            "benchmark worker emitted an invalid frame");
    protocol::WorkerFrame frame;
    Require(frame.ParseFromArray(result.payload.data(),
                                 static_cast<int>(result.payload.size())),
            "benchmark worker emitted malformed protobuf");
    frames.push_back(std::move(frame));
  }
  return frames;
}

Sample RunFixture(const Fixture &fixture, int run_index) {
  const auto handshake = Handshake();
  const auto solve = SolveRequest(fixture, run_index);
  std::string input_bytes = EncodeParent(handshake);
  input_bytes += EncodeParent(solve);
  std::istringstream input(input_bytes, std::ios::binary);
  std::ostringstream output(std::ios::binary);
  std::ostringstream diagnostics;

  const auto start = std::chrono::steady_clock::now();
  const int exit_code = native::RunSession(input, output, diagnostics);
  const auto end = std::chrono::steady_clock::now();
  Require(exit_code == 0, "benchmark session returned a nonzero exit code");
  Require(diagnostics.str().empty(),
          "benchmark session emitted dynamic diagnostics");

  const auto frames = DecodeOutput(output.str());
  Require(frames.size() == 3, "benchmark session emitted unexpected frames");
  Require(frames[0].has_handshake_response() &&
              frames[0].handshake_response().has_success(),
          "benchmark handshake did not succeed");
  Require(frames[1].has_started(), "benchmark Started frame is missing");
  Require(frames[2].has_finished(), "benchmark Finished frame is missing");

  Sample sample;
  sample.handshake = frames[0].handshake_response().success();
  sample.finished = frames[2].finished();
  sample.worker_wall_seconds =
      std::chrono::duration<double>(end - start).count();
  return sample;
}

bool NearlyEqual(double left, double right) {
  return std::abs(left - right) <= 1e-9;
}

void ValidateSample(const Fixture &fixture, const Sample &sample) {
  const auto &finished = sample.finished;
  Require(finished.raw_cp_sat_status() == sat::OPTIMAL,
          "benchmark fixture did not reach raw OPTIMAL");
  Require(finished.status() == protocol::WORKER_SOLVE_STATUS_OPTIMAL,
          "benchmark fixture did not normalize to Optimal");
  Require(finished.termination_reason() == protocol::TERMINATION_REASON_OPTIMAL,
          "benchmark fixture did not terminate as Optimal");
  Require(finished.has_wall_time_seconds() &&
              finished.has_user_time_seconds() &&
              finished.has_deterministic_time() && finished.has_conflicts() &&
              finished.has_branches() && finished.has_binary_propagations() &&
              finished.has_integer_propagations(),
          "benchmark timing or solver statistics evidence is incomplete");
  Require(finished.applied_parameters_sha256().size() == 32 &&
              finished.model_fingerprint().size() == 32,
          "benchmark reproducibility hashes are incomplete");
  Require(finished.has_final_candidate(),
          "benchmark fixture did not return its requested projection");
  Require(finished.final_candidate().values_size() ==
              static_cast<int>(fixture.projections.size()),
          "benchmark projection count differs from the request");

  if (!fixture.expected_projection.empty()) {
    Require(fixture.expected_projection.size() == fixture.projections.size(),
            "benchmark fixture has an incomplete expected projection");
    for (std::size_t index = 0; index < fixture.expected_projection.size();
         ++index) {
      const auto &value =
          finished.final_candidate().values(static_cast<int>(index));
      Require(value.projection_id() == index + 1 &&
                  value.value() == fixture.expected_projection[index],
              "benchmark projection value differs from the expected solution");
    }
  }

  if (fixture.expected_objective) {
    Require(finished.objective_values_size() == 1 &&
                finished.best_bound_values_size() == 1,
            "benchmark objective evidence is incomplete");
    Require(NearlyEqual(finished.objective_values(0),
                        *fixture.expected_objective) &&
                NearlyEqual(finished.best_bound_values(0),
                            *fixture.expected_objective),
            "benchmark objective or bound differs from the proven optimum");
  } else {
    Require(finished.objective_values_size() == 0 &&
                finished.best_bound_values_size() == 0,
            "satisfaction benchmark emitted unexpected objective evidence");
  }
}

std::string Hex(std::string_view bytes) {
  std::ostringstream output;
  output << std::hex << std::setfill('0');
  for (const unsigned char byte : bytes)
    output << std::setw(2) << static_cast<unsigned>(byte);
  return output.str();
}

template <typename Value> std::string Join(const std::vector<Value> &values) {
  std::ostringstream output;
  output << std::setprecision(17);
  for (std::size_t index = 0; index < values.size(); ++index) {
    if (index != 0)
      output << ',';
    output << values[index];
  }
  return output.str();
}

double Median(std::vector<double> values) {
  std::sort(values.begin(), values.end());
  return values[values.size() / 2];
}

void PrintFixtureEvidence(const Fixture &fixture,
                          const std::vector<Sample> &samples) {
  std::vector<double> raw_solver_wall_seconds;
  std::vector<double> in_process_worker_wall_seconds;
  std::vector<double> deterministic_time;
  std::vector<std::uint64_t> conflicts;
  std::vector<std::uint64_t> branches;
  std::vector<std::uint64_t> binary_propagations;
  std::vector<std::uint64_t> integer_propagations;
  raw_solver_wall_seconds.reserve(samples.size());
  in_process_worker_wall_seconds.reserve(samples.size());
  deterministic_time.reserve(samples.size());

  for (const auto &sample : samples) {
    const auto &finished = sample.finished;
    raw_solver_wall_seconds.push_back(finished.wall_time_seconds());
    in_process_worker_wall_seconds.push_back(sample.worker_wall_seconds);
    deterministic_time.push_back(finished.deterministic_time());
    conflicts.push_back(finished.conflicts());
    branches.push_back(finished.branches());
    binary_propagations.push_back(finished.binary_propagations());
    integer_propagations.push_back(finished.integer_propagations());
  }

  const auto &final = samples.back().finished;
  std::cout << "fixture=" << fixture.id << '\n';
  std::cout << "primitives=" << fixture.primitives << '\n';
  std::cout << "variables=" << fixture.model.variables_size() << '\n';
  std::cout << "constraints=" << fixture.model.constraints_size() << '\n';
  std::cout << "model_bytes=" << fixture.model.ByteSizeLong() << '\n';
  std::cout << "projection_count=" << fixture.projections.size() << '\n';
  std::cout << "status=optimal\n";
  std::cout << "termination=optimal\n";
  if (fixture.expected_objective) {
    std::cout << "objective=" << std::setprecision(17)
              << final.objective_values(0) << '\n';
    std::cout << "best_bound=" << std::setprecision(17)
              << final.best_bound_values(0) << '\n';
  } else {
    std::cout << "objective=none\n";
    std::cout << "best_bound=none\n";
  }
  std::cout << "raw_solver_wall_seconds_samples="
            << Join(raw_solver_wall_seconds) << '\n';
  std::cout << "raw_solver_wall_seconds_median=" << std::setprecision(17)
            << Median(raw_solver_wall_seconds) << '\n';
  std::cout << "in_process_worker_wall_seconds_samples="
            << Join(in_process_worker_wall_seconds) << '\n';
  std::cout << "in_process_worker_wall_seconds_median=" << std::setprecision(17)
            << Median(in_process_worker_wall_seconds) << '\n';
  std::cout << "deterministic_time_samples=" << Join(deterministic_time)
            << '\n';
  std::cout << "conflicts_samples=" << Join(conflicts) << '\n';
  std::cout << "branches_samples=" << Join(branches) << '\n';
  std::cout << "binary_propagations_samples=" << Join(binary_propagations)
            << '\n';
  std::cout << "integer_propagations_samples=" << Join(integer_propagations)
            << '\n';
  std::cout << "model_fingerprint_sha256=" << Hex(final.model_fingerprint())
            << '\n';
  std::cout << "applied_parameters_sha256="
            << Hex(final.applied_parameters_sha256()) << '\n';
}

} // namespace

int main() {
  try {
    auto fixtures = Fixtures();
    Require(fixtures.size() == 9, "candidate benchmark fixture count drifted");

    std::cout << "benchmark_schema_version=1\n";
    std::cout << "classification=candidate-non-distributable\n";
    std::cout << "scope=pre-pin-in-process-worker\n";
    std::cout << "translation_scope=prebuilt-cp-model-proto\n";
    std::cout << "full_adapter_timing_included=false\n";
    std::cout << "process_spawn_timing_included=false\n";
    std::cout << "product_sla_claim=false\n";
    std::cout << "seed=" << kSeed << '\n';
    std::cout << "worker_threads=" << kWorkerThreads << '\n';
    std::cout << "wall_time_budget_millis=" << kWallTimeMillis << '\n';
    std::cout << "repetitions=" << kRepetitions << '\n';
    std::cout << "fixture_count=" << fixtures.size() << '\n';

    std::string ortools_version;
    std::string worker_version;
    std::string adapter_version;
    for (const auto &fixture : fixtures) {
      std::vector<Sample> samples;
      samples.reserve(kRepetitions);
      for (int run_index = 0; run_index < kRepetitions; ++run_index) {
        auto sample = RunFixture(fixture, run_index);
        ValidateSample(fixture, sample);
        if (ortools_version.empty()) {
          ortools_version = sample.handshake.ortools_version();
          worker_version = sample.handshake.worker_version();
          adapter_version = sample.handshake.adapter_version();
        } else {
          Require(sample.handshake.ortools_version() == ortools_version &&
                      sample.handshake.worker_version() == worker_version &&
                      sample.handshake.adapter_version() == adapter_version,
                  "benchmark handshake version metadata changed between runs");
        }
        samples.push_back(std::move(sample));
      }
      PrintFixtureEvidence(fixture, samples);
    }

    std::cout << "ortools_version=" << ortools_version << '\n';
    std::cout << "worker_version=" << worker_version << '\n';
    std::cout << "adapter_version=" << adapter_version << '\n';
    std::cout << "benchmark_result=passed\n";
    return EXIT_SUCCESS;
  } catch (const std::exception &error) {
    std::cerr << "candidate benchmark failure: " << error.what() << '\n';
    return EXIT_FAILURE;
  }
}

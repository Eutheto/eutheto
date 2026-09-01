// SPDX-License-Identifier: Apache-2.0
#include <array>
#include <cstdint>
#include <cstdlib>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
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

static_assert(native::kExitUsage == 64);
static_assert(native::kExitProtocol == 64);
static_assert(native::kExitInvalidModel == 65);
static_assert(native::kExitInternal == 70);
static_assert(native::kExitOutput == 70);
static_assert(native::kExitOrtoolsInitialization == 71);
static_assert(native::kExitTemporary == 75);
static_assert(native::kExitConfiguration == 78);

void Require(bool condition, std::string_view message) {
  if (!condition) throw std::runtime_error(std::string(message));
}

std::string Hex(const native::Sha256Digest& digest) {
  std::ostringstream output;
  output << std::hex << std::setfill('0');
  for (const std::uint8_t byte : digest)
    output << std::setw(2) << static_cast<unsigned>(byte);
  return output.str();
}

std::string EncodeParent(const protocol::ParentFrame& frame) {
  const std::string payload = frame.SerializeAsString();
  const std::uint32_t size = static_cast<std::uint32_t>(payload.size());
  std::string framed;
  framed.push_back(static_cast<char>(size >> 24U));
  framed.push_back(static_cast<char>(size >> 16U));
  framed.push_back(static_cast<char>(size >> 8U));
  framed.push_back(static_cast<char>(size));
  framed += payload;
  return framed;
}

protocol::ParentFrame Handshake() {
  protocol::ParentFrame frame;
  auto* request = frame.mutable_handshake_request();
  request->set_protocol_major(policy::kProtocolMajor);
  request->set_protocol_minor(policy::kProtocolMinor);
  request->set_core_version("1.0.0-alpha.1-x+build.01");
  request->set_expected_backend_id("ortools-cp-sat");
  request->set_expected_manifest_sha256(std::string(32, '\x5a'));
  request->add_required_capabilities(protocol::CAPABILITY_CP_SAT);
  request->add_required_capabilities(protocol::CAPABILITY_SOLUTION_PROJECTION);
  request->add_required_capabilities(protocol::CAPABILITY_DETERMINISTIC_TIME);
  return frame;
}

protocol::ParentFrame Solve(const sat::CpModelProto& model) {
  protocol::ParentFrame frame;
  auto* request = frame.mutable_solve_request();
  request->set_request_id("native-test-1");
  request->set_cp_model_proto(model.SerializeAsString());
  const auto fingerprint = native::Sha256String(request->cp_model_proto());
  request->set_model_fingerprint(fingerprint.data(), fingerprint.size());
  request->mutable_parameters()->set_random_seed(1);
  request->mutable_parameters()->set_deterministic_test_profile(true);
  request->mutable_resource_limits()->set_wall_time_millis(1000);
  request->mutable_resource_limits()->set_worker_threads(1);
  return frame;
}

std::vector<protocol::WorkerFrame> DecodeOutput(const std::string& bytes) {
  std::istringstream input(bytes, std::ios::binary);
  native::FrameReader reader(input);
  std::vector<protocol::WorkerFrame> frames;
  for (;;) {
    native::FrameReadResult result = reader.Read(policy::kWorkerEventMaxPayloadBytes);
    if (result.status == native::FrameReadStatus::kEof) break;
    Require(result.status == native::FrameReadStatus::kOk,
            "worker emitted an invalid frame");
    protocol::WorkerFrame frame;
    Require(frame.ParseFromArray(result.payload.data(),
                                 static_cast<int>(result.payload.size())),
            "worker emitted malformed protobuf");
    frames.push_back(std::move(frame));
  }
  return frames;
}

std::vector<protocol::WorkerFrame> Run(
    const protocol::ParentFrame& handshake,
    const protocol::ParentFrame* solve = nullptr,
    const protocol::ParentFrame* extra = nullptr) {
  std::string input_bytes = EncodeParent(handshake);
  if (solve != nullptr) input_bytes += EncodeParent(*solve);
  if (extra != nullptr) input_bytes += EncodeParent(*extra);
  std::istringstream input(input_bytes, std::ios::binary);
  std::ostringstream output(std::ios::binary);
  std::ostringstream diagnostics;
  const int exit_code = native::RunSession(input, output, diagnostics);
  Require(exit_code == 0, "session unexpectedly returned a nonzero exit");
  Require(diagnostics.str().empty(), "session emitted dynamic diagnostics");
  return DecodeOutput(output.str());
}

void TestSha256() {
  Require(Hex(native::Sha256String("")) ==
              "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
          "empty SHA-256 vector failed");
  Require(Hex(native::Sha256String("abc")) ==
              "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
          "abc SHA-256 vector failed");
  const std::string long_vector(1000000, 'a');
  Require(Hex(native::Sha256String(long_vector)) ==
              "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
          "million-a SHA-256 vector failed");

  native::AppliedParameters parameters;
  parameters.wall_time_millis = 100;
  parameters.worker_threads = 1;
  parameters.random_seed = 1;
  parameters.emit_intermediate_solutions = true;
  parameters.deterministic_test_profile = true;
  const auto preimage = native::AppliedParametersPreimage(parameters);
  Require(preimage.size() == 56 && preimage[43] == 100 && preimage[47] == 1 &&
              preimage[51] == 1 && preimage[52] == 0 && preimage[53] == 1 &&
              preimage[54] == 0 && preimage[55] == 1,
          "applied parameter preimage layout failed");
  Require(Hex(native::AppliedParametersHash(parameters)) ==
              "58a02d86a3b57c8a132865ae6fb5260c4cbef5ab243bcb937d16764bfa9ec4f1",
          "applied parameter hash fixture failed");
}

void TestFrameBoundariesAndWire() {
  {
    std::istringstream input(std::string("\0\0\0\0", 4), std::ios::binary);
    native::FrameReader reader(input);
    Require(reader.Read(10).status == native::FrameReadStatus::kEmpty,
            "zero frame was accepted");
  }
  {
    std::istringstream input(std::string("\0\0\0\x0b", 4), std::ios::binary);
    native::FrameReader reader(input);
    Require(reader.Read(10).status == native::FrameReadStatus::kTooLarge,
            "oversize frame was accepted before allocation");
  }
  {
    std::string truncated("\0\0\0\x02", 4);
    truncated.push_back('\x08');
    std::istringstream input(truncated, std::ios::binary);
    native::FrameReader reader(input);
    Require(reader.Read(10).status == native::FrameReadStatus::kTruncated,
            "truncated frame was accepted");
  }
  std::string reason;
  Require(!native::PreflightParentFrame(
              std::array<std::uint8_t, 4>{0x0a, 0x00, 0x0a, 0x00}, &reason),
          "duplicate oneof was accepted");
  Require(!native::PreflightParentFrame(
              std::array<std::uint8_t, 2>{0x18, 0x00}, &reason),
          "reserved tag was accepted");
  Require(!native::PreflightParentFrame(
              std::array<std::uint8_t, 4>{0xc0, 0xa3, 0x09, 0x00},
              &reason),
          "globally reserved protobuf tag was accepted");
  Require(!native::PreflightParentFrame(
              std::array<std::uint8_t, 3>{0x8a, 0x00, 0x00}, &reason),
          "noncanonical varint was accepted");
  Require(native::PreflightParentFrame(
              std::array<std::uint8_t, 5>{0x0a, 0x00, 0x80, 0x01, 0x00},
              &reason),
          "bounded additive unknown field was rejected");

  protocol::ParentFrame exact_projection_cap;
  auto* capped_solve = exact_projection_cap.mutable_solve_request();
  capped_solve->set_request_id("exact-cap");
  capped_solve->set_cp_model_proto(std::string("\x00", 1));
  capped_solve->mutable_parameters()->set_random_seed(1);
  capped_solve->mutable_resource_limits()->set_wall_time_millis(1);
  capped_solve->mutable_resource_limits()->set_worker_threads(1);
  capped_solve->set_model_fingerprint(std::string(32, '\x01'));
  for (std::size_t index = 0;
       index < policy::kSolveRequestProjectionsMaxCount; ++index) {
    capped_solve->add_projections()->set_projection_id(
        static_cast<std::uint64_t>(index + 1));
  }
  const std::string capped_payload = exact_projection_cap.SerializeAsString();
  Require(native::PreflightParentFrame(
              std::span<const std::uint8_t>(
                  reinterpret_cast<const std::uint8_t*>(capped_payload.data()),
                  capped_payload.size()),
              &reason),
          "exact projection-count policy cap was rejected");

  std::vector<std::uint8_t> excessive_unknown_fields;
  excessive_unknown_fields.reserve(
      (policy::kMaxRepeatedFieldItems + 1) * 3);
  for (std::size_t index = 0; index <= policy::kMaxRepeatedFieldItems;
       ++index) {
    excessive_unknown_fields.insert(excessive_unknown_fields.end(),
                                    {0x80, 0x01, 0x01});
  }
  Require(!native::PreflightParentFrame(excessive_unknown_fields, &reason),
          "unknown field count above policy was accepted");
}

void TestHandshake() {
  sat::CpModelProto empty_model;
  const auto accepted_solve = Solve(empty_model);
  const auto accepted = Run(Handshake(), &accepted_solve);
  Require(accepted.size() >= 3 && accepted[0].has_handshake_response() &&
              accepted[0].handshake_response().has_success(),
          "handshake success was not emitted");
  const auto& success = accepted[0].handshake_response().success();
  Require(success.protocol_major() == 1 && success.protocol_minor() == 1 &&
              success.worker_identity() == "eutheto-ortools-worker" &&
              success.worker_version() == "0.1.0" &&
              success.backend_id() == "ortools-cp-sat" &&
              success.ortools_version() == "9.15.6755" &&
              success.manifest_sha256() == std::string(32, '\x5a'),
          "handshake identity or correlation echo was wrong");
  for (const int capability : success.capabilities())
    Require(capability != protocol::CAPABILITY_SUFFICIENT_ASSUMPTIONS,
            "assumptions capability was advertised");


  const std::array<std::string_view, 6> invalid_versions = {
      "1.0.0-01",
      "18446744073709551616.0.0",
      "1.0.0-",
      "1.0.0+",
      "1.0.0-alpha..one",
      "1.0.0+build.",
  };
  for (const std::string_view version : invalid_versions) {
    auto invalid_version = Handshake();
    invalid_version.mutable_handshake_request()->set_core_version(
        std::string(version));
    const auto version_rejection = Run(invalid_version);
    Require(version_rejection[0].handshake_response().error().code() ==
                protocol::HANDSHAKE_ERROR_CODE_INVALID_VERSION,
            "invalid semantic version was accepted");
  }
  auto wrong_major = Handshake();
  wrong_major.mutable_handshake_request()->set_protocol_major(2);
  auto rejected = Run(wrong_major);
  Require(rejected[0].handshake_response().error().code() ==
              protocol::HANDSHAKE_ERROR_CODE_UNSUPPORTED_PROTOCOL_MAJOR,
          "wrong major was not typed");

  auto wrong_backend = Handshake();
  wrong_backend.mutable_handshake_request()->set_expected_backend_id("other");
  rejected = Run(wrong_backend);
  Require(rejected[0].handshake_response().error().code() ==
              protocol::HANDSHAKE_ERROR_CODE_UNEXPECTED_BACKEND,
          "wrong backend was not typed");

  auto bad_manifest = Handshake();
  bad_manifest.mutable_handshake_request()->set_expected_manifest_sha256("short");
  rejected = Run(bad_manifest);
  Require(rejected[0].handshake_response().error().code() ==
              protocol::HANDSHAKE_ERROR_CODE_MANIFEST_MISMATCH,
          "bad manifest correlation length was not typed");

  auto assumptions = Handshake();
  assumptions.mutable_handshake_request()->add_required_capabilities(
      protocol::CAPABILITY_SUFFICIENT_ASSUMPTIONS);
  rejected = Run(assumptions);
  Require(rejected[0].handshake_response().error().code() ==
              protocol::HANDSHAKE_ERROR_CODE_MISSING_CAPABILITY,
          "assumptions capability request was not rejected");
}

void RequireSolveError(const protocol::ParentFrame& solve,
                       protocol::WorkerErrorCode code) {
  const auto frames = Run(Handshake(), &solve);
  Require(frames.size() == 3 && frames[0].has_handshake_response() &&
              frames[1].has_started() && frames[2].has_error() &&
              frames[2].error().code() == code,
          "solve rejection had wrong ordering or error code");
}

void TestSolveRejections() {
  sat::CpModelProto empty;
  auto fingerprint = Solve(empty);
  fingerprint.mutable_solve_request()->set_model_fingerprint(std::string(32, 0));
  RequireSolveError(fingerprint,
                    protocol::WORKER_ERROR_CODE_PROTOCOL_VIOLATION);

  auto zero_time = Solve(empty);
  zero_time.mutable_solve_request()
      ->mutable_resource_limits()
      ->set_wall_time_millis(0);
  RequireSolveError(zero_time, protocol::WORKER_ERROR_CODE_RESOURCE_LIMIT);

  auto deterministic_conflict = Solve(empty);
  deterministic_conflict.mutable_solve_request()
      ->mutable_parameters()
      ->set_random_seed(2);
  RequireSolveError(deterministic_conflict,
                    protocol::WORKER_ERROR_CODE_INVALID_PARAMETERS);

  auto malformed_model = Solve(empty);
  malformed_model.mutable_solve_request()->set_cp_model_proto("\x80");
  const auto malformed_hash = native::Sha256String("\x80");
  malformed_model.mutable_solve_request()->set_model_fingerprint(
      malformed_hash.data(), malformed_hash.size());
  RequireSolveError(malformed_model, protocol::WORKER_ERROR_CODE_INVALID_MODEL);

  auto zero_memory = Solve(empty);
  zero_memory.mutable_solve_request()
      ->mutable_resource_limits()
      ->set_memory_bytes(0);
  RequireSolveError(zero_memory, protocol::WORKER_ERROR_CODE_RESOURCE_LIMIT);

  auto invalid_projection = Solve(empty);
  auto* projection =
      invalid_projection.mutable_solve_request()->add_projections();
  projection->set_projection_id(1);
  projection->set_cp_sat_variable_index(0);
  RequireSolveError(invalid_projection,
                    protocol::WORKER_ERROR_CODE_PROTOCOL_VIOLATION);

  sat::CpModelProto named_model;
  named_model.set_name("forbidden");
  const auto named = Solve(named_model);
  RequireSolveError(named, protocol::WORKER_ERROR_CODE_UNSUPPORTED_MODEL);

  sat::CpModelProto one_variable;
  auto* bounded = one_variable.add_variables();
  bounded->add_domain(0);
  bounded->add_domain(1);
  auto duplicate_projection = Solve(one_variable);
  auto* first =
      duplicate_projection.mutable_solve_request()->add_projections();
  first->set_projection_id(9);
  first->set_cp_sat_variable_index(0);
  auto* duplicate =
      duplicate_projection.mutable_solve_request()->add_projections();
  duplicate->set_projection_id(9);
  duplicate->set_cp_sat_variable_index(0);
  RequireSolveError(duplicate_projection,
                    protocol::WORKER_ERROR_CODE_PROTOCOL_VIOLATION);

  auto valid = Solve(empty);
  const auto frames = Run(Handshake(), &valid, &valid);
  Require(frames.size() == 3 && frames[1].has_started() &&
              frames[2].has_error() &&
              frames[2].error().code() ==
                  protocol::WORKER_ERROR_CODE_PROTOCOL_VIOLATION,
          "extra solve frame was not rejected after Started");
}

void TestRealSolveOrderingAndMapping() {
  sat::CpModelProto feasible;
  auto* variable = feasible.add_variables();
  variable->add_domain(7);
  variable->add_domain(7);
  auto solve = Solve(feasible);
  auto* projection = solve.mutable_solve_request()->add_projections();
  projection->set_projection_id(42);
  projection->set_cp_sat_variable_index(0);
  auto* alias_projection = solve.mutable_solve_request()->add_projections();
  alias_projection->set_projection_id(43);
  alias_projection->set_cp_sat_variable_index(0);
  const auto frames = Run(Handshake(), &solve);
  Require(frames.size() == 3 && frames[0].has_handshake_response() &&
              frames[1].has_started() && frames[2].has_finished(),
          "one-solve frame ordering failed");
  const auto& finished = frames[2].finished();
  Require(finished.raw_cp_sat_status() == sat::OPTIMAL &&
              finished.status() == protocol::WORKER_SOLVE_STATUS_OPTIMAL &&
              finished.termination_reason() ==
                  protocol::TERMINATION_REASON_OPTIMAL &&
              finished.has_final_candidate() &&
              finished.final_candidate().values_size() == 2 &&
              finished.final_candidate().values(0).projection_id() == 42 &&
              finished.final_candidate().values(0).value() == 7 &&
              finished.final_candidate().values(1).projection_id() == 43 &&
              finished.final_candidate().values(1).value() == 7 &&
              finished.sufficient_assumptions_size() == 0,
          "optimal status or projection mapping failed");
  Require(finished.applied_parameters_sha256().size() == 32 &&
              finished.model_fingerprint() ==
                  solve.solve_request().model_fingerprint(),
          "terminal correlation hashes were absent");

  sat::CpModelProto infeasible;
  auto* fixed = infeasible.add_variables();
  fixed->add_domain(0);
  fixed->add_domain(0);
  auto* linear = infeasible.add_constraints()->mutable_linear();
  linear->add_vars(0);
  linear->add_coeffs(1);
  linear->add_domain(1);
  linear->add_domain(1);
  auto infeasible_solve = Solve(infeasible);
  const auto infeasible_frames = Run(Handshake(), &infeasible_solve);
  Require(infeasible_frames.back().finished().raw_cp_sat_status() ==
              sat::INFEASIBLE &&
              infeasible_frames.back().finished().status() ==
                  protocol::WORKER_SOLVE_STATUS_INFEASIBLE &&
              infeasible_frames.back().finished().termination_reason() ==
                  protocol::TERMINATION_REASON_INFEASIBLE &&
              infeasible_frames.back()
                      .finished()
                      .sufficient_assumptions_size() == 0,
          "infeasible status mapping failed");
}

}  // namespace

int main() {
  try {
    TestSha256();
    TestFrameBoundariesAndWire();
    TestHandshake();
    TestSolveRejections();
    TestRealSolveOrderingAndMapping();
    return EXIT_SUCCESS;
  } catch (const std::exception& error) {
    std::cerr << "native worker test failure: " << error.what() << '\n';
    return EXIT_FAILURE;
  }
}

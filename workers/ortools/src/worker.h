// SPDX-License-Identifier: Apache-2.0
#pragma once

#include <array>
#include <cstdint>
#include <iosfwd>
#include <span>

#include "sha256.h"
#include "solver-worker.pb.h"

namespace operations_research::sat {
class CpSolverResponse;
}

namespace eutheto::ortools_worker {

inline constexpr int kExitUsage = 64;
inline constexpr int kExitProtocol = 64;
inline constexpr int kExitInvalidModel = 65;
inline constexpr int kExitInternal = 70;
inline constexpr int kExitOutput = 70;
inline constexpr int kExitOrtoolsInitialization = 71;
inline constexpr int kExitTemporary = 75;
inline constexpr int kExitConfiguration = 78;

struct AppliedParameters {
  std::uint64_t wall_time_millis = 0;
  std::uint32_t worker_threads = 0;
  std::int32_t random_seed = 1;
  bool stop_after_first_feasible = false;
  bool emit_intermediate_solutions = false;
  bool log_search_progress = false;
  bool deterministic_test_profile = false;
};

[[nodiscard]] std::array<std::uint8_t, 56> AppliedParametersPreimage(
    const AppliedParameters& parameters);
[[nodiscard]] Sha256Digest AppliedParametersHash(
    const AppliedParameters& parameters);

// Runs one complete protocol session. Inputs and outputs are injected so native
// tests exercise the real generated messages and real solver without a mock.
[[nodiscard]] int RunSession(std::istream& input, std::ostream& output,
                             std::ostream& diagnostics);

}  // namespace eutheto::ortools_worker

// SPDX-License-Identifier: Apache-2.0
#pragma once

#include <cstddef>
#include <cstdint>
#include <iosfwd>
#include <span>
#include <string>
#include <vector>

#include "solver-worker.pb.h"

namespace eutheto::ortools_worker {

enum class FrameReadStatus {
  kOk,
  kEof,
  kTruncated,
  kEmpty,
  kTooLarge,
  kSessionTooLarge,
  kIoError,
};

struct FrameReadResult {
  FrameReadStatus status = FrameReadStatus::kIoError;
  std::vector<std::uint8_t> payload;
};

class FrameReader final {
 public:
  [[nodiscard]] FrameReadStatus CheckEof();
  explicit FrameReader(std::istream& input) : input_(input) {}
  [[nodiscard]] FrameReadResult Read(std::size_t maximum_payload);
  [[nodiscard]] std::uint64_t session_bytes() const { return session_bytes_; }

 private:
  std::istream& input_;
  std::uint64_t session_bytes_ = 0;
};

[[nodiscard]] bool WriteFrame(std::ostream& output,
                              const eutheto::worker::v1::WorkerFrame& frame);

// Validates project-owned protobuf wire structure before generated parsing.
// Unknown, non-reserved additive fields are skipped; known fields are checked
// for canonical scalar encoding, schema wire type, cardinality, and policy caps.
[[nodiscard]] bool PreflightParentFrame(std::span<const std::uint8_t> payload,
                                        std::string* static_reason);

}  // namespace eutheto::ortools_worker

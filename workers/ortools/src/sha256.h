// SPDX-License-Identifier: Apache-2.0
#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>
#include <string_view>

namespace eutheto::ortools_worker {

using Sha256Digest = std::array<std::uint8_t, 32>;

class Sha256 final {
 public:
  Sha256();
  void Update(std::span<const std::uint8_t> bytes);
  [[nodiscard]] Sha256Digest Final();

 private:
  void Transform(const std::uint8_t* block);

  std::array<std::uint32_t, 8> state_{};
  std::array<std::uint8_t, 64> buffer_{};
  std::uint64_t byte_count_ = 0;
  std::size_t buffered_ = 0;
  bool finalized_ = false;
};

[[nodiscard]] Sha256Digest Sha256Bytes(std::span<const std::uint8_t> bytes);
[[nodiscard]] Sha256Digest Sha256String(std::string_view value);

}  // namespace eutheto::ortools_worker

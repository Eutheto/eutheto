// SPDX-License-Identifier: Apache-2.0
#include "sha256.h"

#include <algorithm>
#include <stdexcept>

namespace eutheto::ortools_worker {
namespace {

constexpr std::array<std::uint32_t, 64> kRoundConstants = {
    0x428a2f98U, 0x71374491U, 0xb5c0fbcfU, 0xe9b5dba5U, 0x3956c25bU,
    0x59f111f1U, 0x923f82a4U, 0xab1c5ed5U, 0xd807aa98U, 0x12835b01U,
    0x243185beU, 0x550c7dc3U, 0x72be5d74U, 0x80deb1feU, 0x9bdc06a7U,
    0xc19bf174U, 0xe49b69c1U, 0xefbe4786U, 0x0fc19dc6U, 0x240ca1ccU,
    0x2de92c6fU, 0x4a7484aaU, 0x5cb0a9dcU, 0x76f988daU, 0x983e5152U,
    0xa831c66dU, 0xb00327c8U, 0xbf597fc7U, 0xc6e00bf3U, 0xd5a79147U,
    0x06ca6351U, 0x14292967U, 0x27b70a85U, 0x2e1b2138U, 0x4d2c6dfcU,
    0x53380d13U, 0x650a7354U, 0x766a0abbU, 0x81c2c92eU, 0x92722c85U,
    0xa2bfe8a1U, 0xa81a664bU, 0xc24b8b70U, 0xc76c51a3U, 0xd192e819U,
    0xd6990624U, 0xf40e3585U, 0x106aa070U, 0x19a4c116U, 0x1e376c08U,
    0x2748774cU, 0x34b0bcb5U, 0x391c0cb3U, 0x4ed8aa4aU, 0x5b9cca4fU,
    0x682e6ff3U, 0x748f82eeU, 0x78a5636fU, 0x84c87814U, 0x8cc70208U,
    0x90befffaU, 0xa4506cebU, 0xbef9a3f7U, 0xc67178f2U};

constexpr std::uint32_t RotateRight(std::uint32_t value, unsigned count) {
  return (value >> count) | (value << (32U - count));
}

std::uint32_t LoadBe32(const std::uint8_t* bytes) {
  return (static_cast<std::uint32_t>(bytes[0]) << 24U) |
         (static_cast<std::uint32_t>(bytes[1]) << 16U) |
         (static_cast<std::uint32_t>(bytes[2]) << 8U) |
         static_cast<std::uint32_t>(bytes[3]);
}

void StoreBe32(std::uint32_t value, std::uint8_t* output) {
  output[0] = static_cast<std::uint8_t>(value >> 24U);
  output[1] = static_cast<std::uint8_t>(value >> 16U);
  output[2] = static_cast<std::uint8_t>(value >> 8U);
  output[3] = static_cast<std::uint8_t>(value);
}

}  // namespace

Sha256::Sha256()
    : state_{0x6a09e667U, 0xbb67ae85U, 0x3c6ef372U, 0xa54ff53aU,
             0x510e527fU, 0x9b05688cU, 0x1f83d9abU, 0x5be0cd19U} {}

void Sha256::Update(std::span<const std::uint8_t> bytes) {
  if (finalized_) {
    throw std::logic_error("SHA-256 already finalized");
  }
  byte_count_ += bytes.size();
  while (!bytes.empty()) {
    const std::size_t amount =
        std::min(buffer_.size() - buffered_, bytes.size());
    std::copy_n(bytes.begin(), amount, buffer_.begin() + buffered_);
    buffered_ += amount;
    bytes = bytes.subspan(amount);
    if (buffered_ == buffer_.size()) {
      Transform(buffer_.data());
      buffered_ = 0;
    }
  }
}

Sha256Digest Sha256::Final() {
  if (finalized_) {
    throw std::logic_error("SHA-256 already finalized");
  }
  finalized_ = true;
  const std::uint64_t bit_count = byte_count_ * 8U;
  buffer_[buffered_++] = 0x80U;
  if (buffered_ > 56) {
    std::fill(buffer_.begin() + buffered_, buffer_.end(), 0);
    Transform(buffer_.data());
    buffered_ = 0;
  }
  std::fill(buffer_.begin() + buffered_, buffer_.begin() + 56, 0);
  for (unsigned index = 0; index < 8; ++index) {
    buffer_[63 - index] = static_cast<std::uint8_t>(bit_count >> (index * 8U));
  }
  Transform(buffer_.data());

  Sha256Digest digest{};
  for (std::size_t index = 0; index < state_.size(); ++index) {
    StoreBe32(state_[index], digest.data() + index * 4);
  }
  return digest;
}

void Sha256::Transform(const std::uint8_t* block) {
  std::array<std::uint32_t, 64> schedule{};
  for (std::size_t index = 0; index < 16; ++index) {
    schedule[index] = LoadBe32(block + index * 4);
  }
  for (std::size_t index = 16; index < schedule.size(); ++index) {
    const std::uint32_t s0 = RotateRight(schedule[index - 15], 7) ^
                             RotateRight(schedule[index - 15], 18) ^
                             (schedule[index - 15] >> 3U);
    const std::uint32_t s1 = RotateRight(schedule[index - 2], 17) ^
                             RotateRight(schedule[index - 2], 19) ^
                             (schedule[index - 2] >> 10U);
    schedule[index] = schedule[index - 16] + s0 + schedule[index - 7] + s1;
  }

  std::uint32_t a = state_[0];
  std::uint32_t b = state_[1];
  std::uint32_t c = state_[2];
  std::uint32_t d = state_[3];
  std::uint32_t e = state_[4];
  std::uint32_t f = state_[5];
  std::uint32_t g = state_[6];
  std::uint32_t h = state_[7];
  for (std::size_t index = 0; index < schedule.size(); ++index) {
    const std::uint32_t sum1 = RotateRight(e, 6) ^ RotateRight(e, 11) ^
                               RotateRight(e, 25);
    const std::uint32_t choice = (e & f) ^ (~e & g);
    const std::uint32_t temp1 =
        h + sum1 + choice + kRoundConstants[index] + schedule[index];
    const std::uint32_t sum0 = RotateRight(a, 2) ^ RotateRight(a, 13) ^
                               RotateRight(a, 22);
    const std::uint32_t majority = (a & b) ^ (a & c) ^ (b & c);
    const std::uint32_t temp2 = sum0 + majority;
    h = g;
    g = f;
    f = e;
    e = d + temp1;
    d = c;
    c = b;
    b = a;
    a = temp1 + temp2;
  }
  state_[0] += a;
  state_[1] += b;
  state_[2] += c;
  state_[3] += d;
  state_[4] += e;
  state_[5] += f;
  state_[6] += g;
  state_[7] += h;
}

Sha256Digest Sha256Bytes(std::span<const std::uint8_t> bytes) {
  Sha256 hash;
  hash.Update(bytes);
  return hash.Final();
}

Sha256Digest Sha256String(std::string_view value) {
  return Sha256Bytes(std::span<const std::uint8_t>(
      reinterpret_cast<const std::uint8_t*>(value.data()), value.size()));
}

}  // namespace eutheto::ortools_worker

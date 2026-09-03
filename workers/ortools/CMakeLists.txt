# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.25)
project(eutheto_ortools_worker VERSION 0.1.0 LANGUAGES CXX)

option(
  EUTHETO_ORTOOLS_DEVELOPMENT_BUILD
  "Build an explicitly non-installable worker without an approved source contract"
  OFF
)
option(EUTHETO_ORTOOLS_BUILD_TESTS "Build focused native worker tests" OFF)
option(
  EUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS
  "Build non-distributable pre-pin primitive benchmark evidence"
  OFF
)
set(
  EUTHETO_ORTOOLS_PHASE3_CONTRACT
  ""
  CACHE STRING
  "Approved Phase-03 OR-Tools/protobuf source contract"
)

set(EUTHETO_EXPECTED_ORTOOLS_VERSION "9.15.6755")
set(EUTHETO_EXPECTED_WORKER_VERSION "0.1.0")
set(EUTHETO_EXPECTED_WORKER_IDENTITY "eutheto-ortools-worker")
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/VERSION" worker_version_file)
list(LENGTH worker_version_file worker_version_line_count)
if(NOT worker_version_line_count EQUAL 1
   OR NOT worker_version_file STREQUAL EUTHETO_EXPECTED_WORKER_VERSION)
  message(FATAL_ERROR "workers/ortools/VERSION must contain exactly 0.1.0.")
endif()


if(EUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS AND
   NOT EUTHETO_ORTOOLS_BUILD_TESTS)
  message(FATAL_ERROR
    "Candidate benchmarks require EUTHETO_ORTOOLS_BUILD_TESTS=ON."
  )
endif()

if(EUTHETO_ORTOOLS_DEVELOPMENT_BUILD)
  message(WARNING
    "EUTHETO_ORTOOLS_DEVELOPMENT_BUILD is non-distributable: install rules are disabled "
    "and this build does not establish source approval."
  )
elseif(EUTHETO_ORTOOLS_PHASE3_CONTRACT STREQUAL "")
  message(FATAL_ERROR
    "Production OR-Tools worker builds require "
    "-DEUTHETO_ORTOOLS_PHASE3_CONTRACT=/absolute/path/to/an/approved-contract.json. "
    "The checked-in example is intentionally unapproved. For local compilation only, "
    "explicitly set -DEUTHETO_ORTOOLS_DEVELOPMENT_BUILD=ON; development builds cannot install."
  )
endif()

set(source_contract_approved FALSE)
if(NOT EUTHETO_ORTOOLS_PHASE3_CONTRACT STREQUAL "")
  file(TO_CMAKE_PATH
    "${EUTHETO_ORTOOLS_PHASE3_CONTRACT}"
    phase3_contract_path
  )
  cmake_path(IS_ABSOLUTE phase3_contract_path contract_path_is_absolute)
  if(NOT contract_path_is_absolute)
    message(FATAL_ERROR
      "EUTHETO_ORTOOLS_PHASE3_CONTRACT must be an absolute path to reviewed Phase-03 build input."
    )
  endif()
  if(NOT EXISTS "${phase3_contract_path}")
    message(FATAL_ERROR
      "EUTHETO_ORTOOLS_PHASE3_CONTRACT does not exist: ${phase3_contract_path}"
    )
  endif()
  set(canonical_contract_path
    "${CMAKE_CURRENT_SOURCE_DIR}/source-contract.json")
  if(NOT EXISTS "${canonical_contract_path}")
    message(FATAL_ERROR
      "The generated approved source contract is missing: ${canonical_contract_path}")
  endif()
  file(READ "${phase3_contract_path}" phase3_contract_json)
  file(READ "${canonical_contract_path}" canonical_contract_json)
  if(NOT phase3_contract_json STREQUAL canonical_contract_json)
    message(FATAL_ERROR
      "EUTHETO_ORTOOLS_PHASE3_CONTRACT must be byte-identical to the generated approved source-contract.json."
    )
  endif()

  function(contract_get output_name)
    string(JSON field_value ERROR_VARIABLE field_error GET "${phase3_contract_json}" ${ARGN})
    if(NOT field_error STREQUAL "NOTFOUND")
      string(JOIN "." field_path ${ARGN})
      message(FATAL_ERROR
        "Invalid Phase-03 source contract field '${field_path}': ${field_error}"
      )
    endif()
    set(${output_name} "${field_value}" PARENT_SCOPE)
  endfunction()

  function(require_sha256 field_name field_value)
    string(LENGTH "${field_value}" digest_length)
    if(NOT digest_length EQUAL 64 OR NOT "${field_value}" MATCHES "^[0-9a-f]+$")
      message(FATAL_ERROR
        "Approved Phase-03 field '${field_name}' must be exactly 64 lowercase hexadecimal SHA-256 characters."
      )
    endif()
  endfunction()

  contract_get(contract_schema_version schema_version)
  contract_get(approval_phase approval phase)
  contract_get(approval_record approval record)
  contract_get(approval_status approval status)
  contract_get(ortools_version ortools version)
  contract_get(ortools_patch_path ortools patch_path)
  contract_get(ortools_patch_sha256 ortools patch_sha256)
  contract_get(ortools_source_url ortools source_url)
  contract_get(ortools_sha256 ortools sha256)
  contract_get(protobuf_source_version protobuf source_version)
  contract_get(protobuf_source_url protobuf source_url)
  contract_get(protobuf_sha256 protobuf sha256)
  contract_get(protobuf_protoc_version protobuf protoc_version)
  contract_get(protobuf_runtime_version protobuf cpp_runtime_version)
  contract_get(protocol_wire_version protocol wire_version)
  contract_get(protocol_schema_sha256 protocol schema_sha256)
  contract_get(worker_identity worker identity)
  contract_get(worker_version worker version)
  string(JSON cmake_cache_type ERROR_VARIABLE cmake_cache_error
    TYPE "${phase3_contract_json}" cmake cache_entries)

  if(NOT contract_schema_version STREQUAL "1")
    message(FATAL_ERROR "Unsupported source-contract schema_version: ${contract_schema_version}")
  endif()
  if(NOT approval_phase STREQUAL "3" OR NOT approval_status STREQUAL "approved")
    message(FATAL_ERROR
      "The source contract is not approved for Phase 03 (phase=${approval_phase}, status=${approval_status})."
    )
  endif()
  if(approval_record STREQUAL "" OR approval_record STREQUAL "UNRESOLVED")
    message(FATAL_ERROR "An approved Phase-03 contract requires a non-placeholder approval.record.")
  endif()
  if(NOT ortools_source_url MATCHES "^https://"
     OR NOT ortools_version STREQUAL EUTHETO_EXPECTED_ORTOOLS_VERSION)
    message(FATAL_ERROR "The approved contract must select exact OR-Tools 9.15.6755 and an HTTPS source URL.")
  endif()
  if(NOT protobuf_source_url MATCHES "^https://"
     OR protobuf_source_version STREQUAL "UNRESOLVED"
     OR protobuf_protoc_version STREQUAL "UNRESOLVED"
     OR protobuf_runtime_version STREQUAL "UNRESOLVED")
    message(FATAL_ERROR "Approved matched protobuf source, protoc, and C++ runtime versions are required.")
  endif()
  if(NOT protocol_wire_version STREQUAL "1")
    message(FATAL_ERROR "This worker accepts only the generated solver-worker v1 protocol.")
  endif()
  if(NOT worker_identity STREQUAL EUTHETO_EXPECTED_WORKER_IDENTITY
     OR NOT worker_version STREQUAL EUTHETO_EXPECTED_WORKER_VERSION)
    message(FATAL_ERROR "The approved worker identity/version must be eutheto-ortools-worker 0.1.0.")
  endif()
  if(NOT cmake_cache_error STREQUAL "NOTFOUND" OR NOT cmake_cache_type STREQUAL "OBJECT")
    message(FATAL_ERROR
      "Approved Phase-03 contract field 'cmake.cache_entries' must contain the exact reviewed CMake cache object."
    )
  endif()

  if(NOT ortools_patch_path STREQUAL
     "workers/ortools/patches/9.15-candidate-fixes.patch")
    message(FATAL_ERROR
      "The approved contract must select the reviewed OR-Tools 9.15 candidate patch.")
  endif()
  require_sha256("ortools.patch_sha256" "${ortools_patch_sha256}")
  require_sha256("ortools.sha256" "${ortools_sha256}")
  require_sha256("protobuf.sha256" "${protobuf_sha256}")
  require_sha256("protocol.schema_sha256" "${protocol_schema_sha256}")
  file(SHA256
    "${CMAKE_CURRENT_SOURCE_DIR}/../../protocol/solver-worker.proto"
    actual_protocol_schema_sha256
  )
  if(NOT actual_protocol_schema_sha256 STREQUAL protocol_schema_sha256)
    message(FATAL_ERROR
      "Approved protocol.schema_sha256 does not match protocol/solver-worker.proto."
    )
  endif()
  file(SHA256
    "${CMAKE_CURRENT_SOURCE_DIR}/patches/9.15-candidate-fixes.patch"
    actual_ortools_patch_sha256
  )
  if(NOT actual_ortools_patch_sha256 STREQUAL ortools_patch_sha256)
    message(FATAL_ERROR
      "Approved ortools.patch_sha256 does not match the repository-owned patch.")
  endif()


  string(JSON cache_entry_count LENGTH "${phase3_contract_json}" cmake cache_entries)
  math(EXPR cache_entry_last "${cache_entry_count} - 1")
  foreach(cache_entry_index RANGE 0 ${cache_entry_last})
    string(JSON cache_entry_name MEMBER "${phase3_contract_json}"
      cmake cache_entries ${cache_entry_index})
    contract_get(cache_entry_expected cmake cache_entries "${cache_entry_name}")
    if(NOT DEFINED ${cache_entry_name})
      message(FATAL_ERROR
        "Approved CMake cache entry '${cache_entry_name}' was not supplied."
      )
    endif()
    if(NOT "${${cache_entry_name}}" STREQUAL "${cache_entry_expected}")
      message(FATAL_ERROR
        "CMake cache entry '${cache_entry_name}' differs from its approved value."
      )
    endif()
  endforeach()
  set(source_contract_approved TRUE)
endif()

find_package(Protobuf CONFIG REQUIRED)
find_package(ortools ${EUTHETO_EXPECTED_ORTOOLS_VERSION} EXACT CONFIG REQUIRED)
find_package(Threads REQUIRED)

if(DEFINED ortools_VERSION AND
   NOT ortools_VERSION STREQUAL EUTHETO_EXPECTED_ORTOOLS_VERSION)
  message(FATAL_ERROR "Found OR-Tools ${ortools_VERSION}; exact 9.15.6755 is required.")
endif()
if(NOT EUTHETO_ORTOOLS_DEVELOPMENT_BUILD AND
   NOT Protobuf_VERSION STREQUAL protobuf_runtime_version)
  message(FATAL_ERROR
    "Found protobuf C++ runtime ${Protobuf_VERSION}; approved runtime ${protobuf_runtime_version} is required."
  )
endif()

set(EUTHETO_PROTOCOL_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../protocol/generated/cpp")
add_library(eutheto-worker-protocol STATIC
  "${EUTHETO_PROTOCOL_DIR}/solver-worker.pb.cc"
)
target_include_directories(eutheto-worker-protocol PUBLIC "${EUTHETO_PROTOCOL_DIR}")
target_link_libraries(eutheto-worker-protocol PUBLIC protobuf::libprotobuf)
target_compile_features(eutheto-worker-protocol PUBLIC cxx_std_20)

add_library(eutheto-ortools-worker-lib STATIC
  src/sha256.cc
  src/wire.cc
  src/worker.cc
)
target_include_directories(eutheto-ortools-worker-lib PUBLIC src)
target_compile_features(eutheto-ortools-worker-lib PUBLIC cxx_std_20)
target_compile_definitions(eutheto-ortools-worker-lib PUBLIC EIGEN_MPL2_ONLY)
target_link_libraries(eutheto-ortools-worker-lib
  PUBLIC
    eutheto-worker-protocol
    ortools::ortools
    Threads::Threads
)

add_executable(ortools-worker src/main.cc)
target_link_libraries(ortools-worker PRIVATE eutheto-ortools-worker-lib)

foreach(worker_target
    eutheto-ortools-worker-lib
    ortools-worker)
  if(MSVC)
    target_compile_options(${worker_target} PRIVATE /W4 /WX /permissive- /wd4702)
  else()
    target_compile_options(${worker_target} PRIVATE
      -Wall -Wextra -Wpedantic -Werror
    )
  endif()
endforeach()
set_target_properties(
  eutheto-worker-protocol
  eutheto-ortools-worker-lib
  ortools-worker
  PROPERTIES CXX_EXTENSIONS OFF
)


if(EUTHETO_ORTOOLS_BUILD_TESTS)
  enable_testing()
  add_subdirectory(tests)
endif()

if(source_contract_approved AND NOT EUTHETO_ORTOOLS_DEVELOPMENT_BUILD)
  include(GNUInstallDirs)
  install(TARGETS ortools-worker RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}")
endif()

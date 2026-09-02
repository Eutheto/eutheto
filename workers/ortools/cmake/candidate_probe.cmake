# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.25)

foreach(required_variable
    REPOSITORY_ROOT
    PROBE_ROOT
    PROBE_TARGET
    PROBE_RUNNER_OS
    PROBE_RUNNER_ARCH
    PROBE_GENERATOR
    PROBE_LINKAGE)
  if(NOT DEFINED ${required_variable} OR "${${required_variable}}" STREQUAL "")
    message(FATAL_ERROR "${required_variable} is required.")
  endif()
endforeach()

cmake_path(ABSOLUTE_PATH REPOSITORY_ROOT NORMALIZE)
cmake_path(ABSOLUTE_PATH PROBE_ROOT NORMALIZE)
if(PROBE_ROOT STREQUAL REPOSITORY_ROOT OR PROBE_ROOT STREQUAL "/")
  message(FATAL_ERROR "PROBE_ROOT must be a dedicated directory outside the repository root.")
endif()

set(expected_runner_os "")
set(expected_runner_arch "")
if(PROBE_TARGET STREQUAL "linux-x86_64")
  set(expected_runner_os "Linux")
  set(expected_runner_arch "X64")
elseif(PROBE_TARGET STREQUAL "windows-x86_64")
  set(expected_runner_os "Windows")
  set(expected_runner_arch "X64")
elseif(PROBE_TARGET STREQUAL "macos-arm64")
  set(expected_runner_os "macOS")
  set(expected_runner_arch "ARM64")
elseif(PROBE_TARGET STREQUAL "macos-x86_64")
  set(expected_runner_os "macOS")
  set(expected_runner_arch "X64")
else()
  message(FATAL_ERROR "Unsupported Phase 03 candidate target: ${PROBE_TARGET}")
endif()
if(NOT PROBE_RUNNER_OS STREQUAL expected_runner_os
   OR NOT PROBE_RUNNER_ARCH STREQUAL expected_runner_arch)
  message(FATAL_ERROR
    "Target ${PROBE_TARGET} requires ${expected_runner_os}/${expected_runner_arch}, "
    "not ${PROBE_RUNNER_OS}/${PROBE_RUNNER_ARCH}.")
endif()

if(PROBE_LINKAGE STREQUAL "shared")
  set(build_shared_libraries ON)
elseif(PROBE_LINKAGE STREQUAL "static")
  set(build_shared_libraries OFF)
else()
  message(FATAL_ERROR "PROBE_LINKAGE must be shared or static.")
endif()

set(source_tag "v9.15")
set(source_commit "551ad10d94835c99e5e1e684500d3db398c0e345")
set(bzip2_commit "66c46b8c9436613fd81bc5d03f63a61933a4dcc3")
set(source_url "https://github.com/google/or-tools/archive/refs/tags/v9.15.tar.gz")
set(expected_source_sha256
  "6395a00a97ff30af878ee8d7fd5ad0ab1c7844f7219182c6d71acbee1b5f3026")
set(source_patch_relative
  "workers/ortools/patches/9.15-candidate-fixes.patch")
set(source_patch "${REPOSITORY_ROOT}/${source_patch_relative}")
set(expected_source_patch_sha256
  "3ab9c8c45d76aab2416195bc97266718986a395a37fcd6c8d6e6fa5322ecf6a6")
set(protobuf_source_url
  "https://github.com/protocolbuffers/protobuf/releases/download/v33.1/protobuf-33.1.tar.gz")
set(expected_protobuf_source_sha256
  "fda132cb0c86400381c0af1fe98bd0f775cb566cb247cdcc105e344e00acc30e")
string(LENGTH "${expected_source_sha256}" expected_source_sha256_length)
if(NOT expected_source_sha256_length EQUAL 64
   OR NOT expected_source_sha256 MATCHES "^[0-9a-f]+$")
  message(FATAL_ERROR "The pinned source SHA-256 must be 64 lowercase hexadecimal characters.")
endif()
string(LENGTH "${expected_protobuf_source_sha256}"
  expected_protobuf_source_sha256_length)
if(NOT expected_protobuf_source_sha256_length EQUAL 64
   OR NOT expected_protobuf_source_sha256 MATCHES "^[0-9a-f]+$")
  message(FATAL_ERROR
    "The pinned protobuf source SHA-256 must be 64 lowercase hexadecimal characters.")
endif()

file(REMOVE_RECURSE "${PROBE_ROOT}")
set(download_dir "${PROBE_ROOT}/download")
set(source_parent "${PROBE_ROOT}/source")
set(source_dir "${source_parent}/or-tools-9.15")
set(protobuf_source_parent "${PROBE_ROOT}/protobuf-source")
set(protobuf_source_dir "${protobuf_source_parent}/protobuf-33.1")
set(ortools_build_dir "${PROBE_ROOT}/ortools-build")
set(ortools_install_dir "${PROBE_ROOT}/ortools-install")
set(worker_build_dir "${PROBE_ROOT}/worker-build")
set(evidence_dir "${PROBE_ROOT}/evidence")
file(MAKE_DIRECTORY
  "${download_dir}"
  "${source_parent}"
  "${protobuf_source_parent}"
  "${evidence_dir}")
set(source_archive "${download_dir}/or-tools-v9.15.tar.gz")
set(protobuf_source_archive "${download_dir}/protobuf-33.1.tar.gz")
set(report_file "${evidence_dir}/candidate-evidence.txt")
set(cache_evidence_file "${evidence_dir}/cmake-cache-entries.txt")
set(test_outcome_file "${evidence_dir}/test-outcome.txt")

file(WRITE "${report_file}"
  "classification=candidate\n"
  "distribution=non-distributable\n"
  "artifact_contents=text-evidence-only\n"
  "target=${PROBE_TARGET}\n"
  "runner_os=${PROBE_RUNNER_OS}\n"
  "runner_arch=${PROBE_RUNNER_ARCH}\n"
  "cmake_host_system_name=${CMAKE_HOST_SYSTEM_NAME}\n"
  "cmake_host_system_processor=${CMAKE_HOST_SYSTEM_PROCESSOR}\n"
  "cmake_generator=${PROBE_GENERATOR}\n"
  "source_tag=${source_tag}\n"
  "source_commit=${source_commit}\n"
  "source_archive_url=${source_url}\n"
  "source_archive_sha256_expected=${expected_source_sha256}\n"
  "source_patch=${source_patch_relative}\n"
  "source_patch_sha256_expected=${expected_source_patch_sha256}\n"
  "protobuf_dependency_expected=v33.1\n"
  "protobuf_source_archive_url=${protobuf_source_url}\n"
  "protobuf_source_archive_sha256_expected=${expected_protobuf_source_sha256}\n"
  "protobuf_source_override=FETCHCONTENT_SOURCE_DIR_PROTOBUF\n"
  "bzip2_dependency_expected=${bzip2_commit}\n"
  "linkage_probe=${PROBE_LINKAGE}\n"
  "linkage_policy=measured-candidate-evidence-not-final-target-policy\n")
file(WRITE "${cache_evidence_file}"
  "classification=candidate-non-distributable\n")
file(WRITE "${test_outcome_file}" "ctest=not-run\n")

function(run_stage stage working_directory)
  set(log_file "${evidence_dir}/${stage}.log")
  execute_process(
    COMMAND ${ARGN}
    WORKING_DIRECTORY "${working_directory}"
    RESULT_VARIABLE stage_result
    OUTPUT_FILE "${log_file}"
    ERROR_FILE "${log_file}"
    COMMAND_ECHO STDOUT
    ENCODING UTF-8
  )
  file(APPEND "${report_file}" "${stage}_exit_code=${stage_result}\n")
  if(NOT "${stage_result}" STREQUAL "0")
    message(FATAL_ERROR "${stage} failed with exit code ${stage_result}; see ${log_file}.")
  endif()
endfunction()
function(append_inspection log_name subject working_directory)
  set(log_file "${evidence_dir}/${log_name}.log")
  execute_process(
    COMMAND ${ARGN}
    WORKING_DIRECTORY "${working_directory}"
    RESULT_VARIABLE inspection_result
    OUTPUT_VARIABLE inspection_output
    ERROR_VARIABLE inspection_error
    TIMEOUT 60
    ENCODING UTF-8
  )
  file(APPEND "${log_file}"
    "subject=${subject}\n"
    "exit_code=${inspection_result}\n"
    "stdout:\n${inspection_output}\n"
    "stderr:\n${inspection_error}\n---\n")
  if(NOT "${inspection_result}" STREQUAL "0")
    message(FATAL_ERROR
      "${log_name} inspection failed for ${subject} with exit code "
      "${inspection_result}; see ${log_file}.")
  endif()
  if(WIN32 AND log_name STREQUAL "binary-linkage")
    string(REGEX MATCHALL "[A-Za-z0-9_.+-]+\\.[dD][lL][lL]"
      runtime_dependencies "${inspection_output}")
    list(REMOVE_DUPLICATES runtime_dependencies)
    cmake_path(GET subject PARENT_PATH subject_parent)
    foreach(runtime_dependency IN LISTS runtime_dependencies)
      string(TOLOWER "${runtime_dependency}" normalized_runtime_dependency)
      if(normalized_runtime_dependency MATCHES "^(api|ext)-ms-win-.*\\.dll$")
        continue()
      endif()
      if(NOT EXISTS "${working_directory}/${subject_parent}/${runtime_dependency}"
         AND NOT EXISTS "${ortools_install_dir}/bin/${runtime_dependency}"
         AND NOT EXISTS "$ENV{SystemRoot}/System32/${runtime_dependency}")
        file(APPEND "${log_file}"
          "unresolved_dependency=${subject}:${runtime_dependency}\n")
        message(FATAL_ERROR
          "Unresolved Windows runtime dependency ${runtime_dependency} for "
          "${subject}; see ${log_file}.")
      endif()
    endforeach()
  endif()
endfunction()


function(require_cache_entry scope cache_file entry_name expected_value)
  file(STRINGS "${cache_file}" matching_lines REGEX "^${entry_name}:[^=]*=")
  list(LENGTH matching_lines matching_line_count)
  if(NOT matching_line_count EQUAL 1)
    file(APPEND "${cache_evidence_file}"
      "ERROR.${scope}.${entry_name}=expected-one-cache-entry-found-${matching_line_count}\n")
    message(FATAL_ERROR "Expected exactly one ${entry_name} entry in ${cache_file}.")
  endif()
  list(GET matching_lines 0 matching_line)
  string(REGEX REPLACE "^[^=]*=" "" actual_value "${matching_line}")
  file(APPEND "${cache_evidence_file}" "${scope}.${entry_name}=${actual_value}\n")
  if(NOT actual_value STREQUAL expected_value)
    file(APPEND "${cache_evidence_file}"
      "ERROR.${scope}.${entry_name}=expected-${expected_value}\n")
    message(FATAL_ERROR
      "${scope} cache entry ${entry_name} is '${actual_value}', expected '${expected_value}'.")
  endif()
endfunction()

function(record_cache_entry scope cache_file entry_name)
  file(STRINGS "${cache_file}" matching_lines REGEX "^${entry_name}:[^=]*=")
  list(LENGTH matching_lines matching_line_count)
  if(NOT matching_line_count EQUAL 1)
    file(APPEND "${cache_evidence_file}"
      "ERROR.${scope}.${entry_name}=expected-one-cache-entry-found-${matching_line_count}\n")
    message(FATAL_ERROR "Expected exactly one ${entry_name} entry in ${cache_file}.")
  endif()
  list(GET matching_lines 0 matching_line)
  string(REGEX REPLACE "^[^=]*=" "" actual_value "${matching_line}")
  file(APPEND "${cache_evidence_file}" "${scope}.${entry_name}=${actual_value}\n")
endfunction()

find_program(git_executable git REQUIRED)
set(expected_source_ref "${source_commit}\trefs/tags/${source_tag}")
execute_process(
  COMMAND "${git_executable}" ls-remote --refs
    https://github.com/google/or-tools.git "refs/tags/${source_tag}"
  RESULT_VARIABLE source_ref_result
  OUTPUT_VARIABLE actual_source_ref
  ERROR_VARIABLE source_ref_error
  OUTPUT_STRIP_TRAILING_WHITESPACE
  ENCODING UTF-8
)
file(WRITE "${evidence_dir}/source-tag-ref.log"
  "command=git ls-remote --refs https://github.com/google/or-tools.git refs/tags/${source_tag}\n"
  "exit_code=${source_ref_result}\n"
  "stdout=${actual_source_ref}\n"
  "stderr=${source_ref_error}")
file(APPEND "${report_file}" "source_tag_ref_exit_code=${source_ref_result}\n")
if(NOT "${source_ref_result}" STREQUAL "0"
   OR NOT actual_source_ref STREQUAL expected_source_ref)
  message(FATAL_ERROR
    "OR-Tools ${source_tag} did not resolve exactly to the required commit ${source_commit}.")
endif()
file(APPEND "${report_file}" "source_commit_actual=${source_commit}\n")

file(DOWNLOAD
  "${source_url}"
  "${source_archive}"
  STATUS download_status
  LOG download_log
  SHOW_PROGRESS
  TLS_VERIFY ON
)
list(GET download_status 0 download_result)
list(GET download_status 1 download_message)
file(WRITE "${evidence_dir}/download.log"
  "url=${source_url}\nstatus=${download_result}\nmessage=${download_message}\n${download_log}")
file(APPEND "${report_file}" "download_exit_code=${download_result}\n")
if(NOT download_result EQUAL 0)
  message(FATAL_ERROR "OR-Tools candidate source download failed: ${download_message}")
endif()

file(SHA256 "${source_archive}" actual_source_sha256)
file(APPEND "${report_file}" "source_archive_sha256_actual=${actual_source_sha256}\n")
string(LENGTH "${actual_source_sha256}" actual_source_sha256_length)
if(NOT actual_source_sha256_length EQUAL 64
   OR NOT actual_source_sha256 MATCHES "^[0-9a-f]+$"
   OR NOT actual_source_sha256 STREQUAL expected_source_sha256)
  message(FATAL_ERROR
    "OR-Tools source archive SHA-256 is ${actual_source_sha256}, expected ${expected_source_sha256}.")
endif()

run_stage(extract-source "${source_parent}"
  "${CMAKE_COMMAND}" -E tar xzf "${source_archive}")
if(NOT EXISTS "${source_dir}/CMakeLists.txt")
  message(FATAL_ERROR "The verified archive did not extract the expected or-tools-9.15 source root.")
endif()
if(NOT EXISTS "${source_patch}")
  message(FATAL_ERROR "The required OR-Tools source patch is missing.")
endif()
file(SHA256 "${source_patch}" actual_source_patch_sha256)
file(APPEND "${report_file}"
  "source_patch_sha256_actual=${actual_source_patch_sha256}\n")
if(NOT actual_source_patch_sha256 STREQUAL expected_source_patch_sha256)
  message(FATAL_ERROR
    "OR-Tools source patch SHA-256 is ${actual_source_patch_sha256}, "
    "expected ${expected_source_patch_sha256}.")
endif()
run_stage(check-source-patch "${source_dir}"
  "${git_executable}" apply --check "${source_patch}")
run_stage(apply-source-patch "${source_dir}"
  "${git_executable}" apply "${source_patch}")
file(DOWNLOAD
  "${protobuf_source_url}"
  "${protobuf_source_archive}"
  EXPECTED_HASH "SHA256=${expected_protobuf_source_sha256}"
  STATUS protobuf_download_status
  LOG protobuf_download_log
  SHOW_PROGRESS
  TLS_VERIFY ON
)
list(GET protobuf_download_status 0 protobuf_download_result)
list(GET protobuf_download_status 1 protobuf_download_message)
file(WRITE "${evidence_dir}/protobuf-download.log"
  "url=${protobuf_source_url}\n"
  "status=${protobuf_download_result}\n"
  "message=${protobuf_download_message}\n"
  "${protobuf_download_log}")
file(APPEND "${report_file}"
  "protobuf_download_exit_code=${protobuf_download_result}\n")
if(NOT protobuf_download_result EQUAL 0)
  message(FATAL_ERROR
    "Protobuf source download or hash verification failed: ${protobuf_download_message}")
endif()
file(SHA256 "${protobuf_source_archive}" actual_protobuf_source_sha256)
file(APPEND "${report_file}"
  "protobuf_source_archive_sha256_actual=${actual_protobuf_source_sha256}\n")
if(NOT actual_protobuf_source_sha256 STREQUAL expected_protobuf_source_sha256)
  message(FATAL_ERROR
    "Protobuf source archive SHA-256 is ${actual_protobuf_source_sha256}, "
    "expected ${expected_protobuf_source_sha256}.")
endif()
run_stage(extract-protobuf-source "${protobuf_source_parent}"
  "${CMAKE_COMMAND}" -E tar xzf "${protobuf_source_archive}")
if(NOT EXISTS "${protobuf_source_dir}/CMakeLists.txt")
  message(FATAL_ERROR
    "The verified protobuf archive did not extract the expected protobuf-33.1 source root.")
endif()

set(dependency_file "${source_dir}/cmake/dependencies/CMakeLists.txt")
file(STRINGS "${dependency_file}" dependency_lines)
set(pending_fetch_content FALSE)
set(in_protobuf_declaration FALSE)
set(found_protobuf_declaration FALSE)
set(protobuf_git_tags "")
foreach(dependency_line IN LISTS dependency_lines)
  string(STRIP "${dependency_line}" stripped_line)
  if(NOT in_protobuf_declaration)
    if(stripped_line STREQUAL "FetchContent_Declare(")
      set(pending_fetch_content TRUE)
    elseif(pending_fetch_content)
      set(pending_fetch_content FALSE)
      if(stripped_line STREQUAL "Protobuf")
        if(found_protobuf_declaration)
          message(FATAL_ERROR "Found more than one Protobuf FetchContent declaration.")
        endif()
        set(found_protobuf_declaration TRUE)
        set(in_protobuf_declaration TRUE)
      endif()
    endif()
  else()
    if(stripped_line MATCHES "^GIT_TAG[ \t]+\"?([^\" \t]+)\"?[ \t]*$")
      list(APPEND protobuf_git_tags "${CMAKE_MATCH_1}")
    elseif(stripped_line STREQUAL ")")
      set(in_protobuf_declaration FALSE)
    endif()
  endif()
endforeach()
list(LENGTH protobuf_git_tags protobuf_git_tag_count)
if(NOT found_protobuf_declaration OR in_protobuf_declaration
   OR NOT protobuf_git_tag_count EQUAL 1)
  message(FATAL_ERROR "Could not identify exactly one complete upstream Protobuf dependency pin.")
endif()
list(GET protobuf_git_tags 0 protobuf_git_tag)
file(WRITE "${evidence_dir}/protobuf-dependency.txt"
  "declaration=FetchContent_Declare(Protobuf)\nGIT_TAG=${protobuf_git_tag}\n")
file(APPEND "${report_file}" "protobuf_dependency_actual=${protobuf_git_tag}\n")
if(NOT protobuf_git_tag STREQUAL "v33.1")
  message(FATAL_ERROR "OR-Tools v9.15 declares Protobuf ${protobuf_git_tag}, expected v33.1.")
endif()

set(expected_ortools_version "9.15.6755")
set(ENV{OR_TOOLS_PATCH} "6755")
file(WRITE "${evidence_dir}/version-normalization.txt"
  "classification=candidate-non-distributable\n"
  "purpose=normalize-version-for-verified-tag-archive-without-git-metadata\n"
  "source_approval=false\n"
  "environment_override=OR_TOOLS_PATCH\n"
  "environment_value=$ENV{OR_TOOLS_PATCH}\n"
  "expected_ortools_version=${expected_ortools_version}\n")
file(APPEND "${report_file}"
  "version_normalization=OR_TOOLS_PATCH\n"
  "version_normalization_value=$ENV{OR_TOOLS_PATCH}\n"
  "version_normalization_source_approval=false\n")

if(WIN32)
  set(eigen_license_cxx_flag "/DEIGEN_MPL2_ONLY")
else()
  set(eigen_license_cxx_flag "-DEIGEN_MPL2_ONLY")
endif()
file(APPEND "${report_file}"
  "eigen_license_compile_guard=EIGEN_MPL2_ONLY\n")

set(ortools_configure_command
  "${CMAKE_COMMAND}"
  -S "${source_dir}"
  -B "${ortools_build_dir}"
  -G "${PROBE_GENERATOR}"
)
if(DEFINED PROBE_GENERATOR_PLATFORM AND NOT PROBE_GENERATOR_PLATFORM STREQUAL "")
  list(APPEND ortools_configure_command -A "${PROBE_GENERATOR_PLATFORM}")
endif()
if(WIN32 AND PROBE_GENERATOR STREQUAL "Ninja")
  list(APPEND ortools_configure_command
    "-DCMAKE_C_COMPILER=cl.exe"
    "-DCMAKE_CXX_COMPILER=cl.exe")
endif()
list(APPEND ortools_configure_command
  "-DCMAKE_BUILD_TYPE=Release"
  "-DCMAKE_INSTALL_PREFIX=${ortools_install_dir}"
  "-DCMAKE_CXX_FLAGS=${eigen_license_cxx_flag}"
  "-DFETCHCONTENT_SOURCE_DIR_PROTOBUF=${protobuf_source_dir}"
  "-DBUILD_SHARED_LIBS=${build_shared_libraries}"
  "-DBUILD_CXX=ON"
  "-DBUILD_DEPS=ON"
  "-DINSTALL_BUILD_DEPS=ON"
  "-DBUILD_PYTHON=OFF"
  "-DBUILD_JAVA=OFF"
  "-DBUILD_DOTNET=OFF"
  "-DBUILD_TESTING=OFF"
  "-DBUILD_SAMPLES=OFF"
  "-DBUILD_CXX_SAMPLES=OFF"
  "-DBUILD_EXAMPLES=OFF"
  "-DBUILD_CXX_EXAMPLES=OFF"
  "-DBUILD_DOC=OFF"
  "-DBUILD_FLATZINC=OFF"
  "-DBUILD_MATH_OPT=OFF"
  "-DUSE_COINOR=OFF"
  "-DUSE_SCIP=OFF"
  "-DUSE_GLPK=OFF"
  "-DUSE_HIGHS=OFF"
  "-DUSE_GUROBI=OFF"
  "-DUSE_CPLEX=OFF"
  "-DUSE_XPRESS=OFF"
  "-DUSE_PDLP=OFF"
  "-DUSE_BOP=ON"
  "-DUSE_GLOP=ON"
)
run_stage(ortools-configure "${PROBE_ROOT}" ${ortools_configure_command})
set(bzip2_source_dir "${ortools_build_dir}/_deps/bzip2-src")
execute_process(
  COMMAND "${git_executable}" rev-parse HEAD
  WORKING_DIRECTORY "${bzip2_source_dir}"
  RESULT_VARIABLE bzip2_revision_result
  OUTPUT_VARIABLE bzip2_revision_actual
  ERROR_VARIABLE bzip2_revision_error
  OUTPUT_STRIP_TRAILING_WHITESPACE
  TIMEOUT 30
  ENCODING UTF-8
)
file(WRITE "${evidence_dir}/dependency-revisions.txt"
  "bzip2_expected=${bzip2_commit}\n"
  "bzip2_actual=${bzip2_revision_actual}\n"
  "bzip2_revision_exit_code=${bzip2_revision_result}\n"
  "bzip2_revision_stderr=${bzip2_revision_error}\n")
file(APPEND "${report_file}"
  "bzip2_revision_exit_code=${bzip2_revision_result}\n"
  "bzip2_dependency_actual=${bzip2_revision_actual}\n")
if(NOT "${bzip2_revision_result}" STREQUAL "0"
   OR NOT bzip2_revision_actual STREQUAL bzip2_commit)
  message(FATAL_ERROR
    "Fetched bzip2 revision ${bzip2_revision_actual}; expected ${bzip2_commit}.")
endif()

set(ortools_version_config "${ortools_build_dir}/ortoolsConfigVersion.cmake")
if(NOT EXISTS "${ortools_version_config}")
  message(FATAL_ERROR
    "OR-Tools configure did not generate the expected ortoolsConfigVersion.cmake.")
endif()
file(STRINGS "${ortools_version_config}" package_version_lines
  REGEX "^set\\(PACKAGE_VERSION \"[^\"]+\"\\)$")
list(LENGTH package_version_lines package_version_line_count)
if(NOT package_version_line_count EQUAL 1)
  message(FATAL_ERROR
    "Expected exactly one PACKAGE_VERSION declaration in ortoolsConfigVersion.cmake.")
endif()
list(GET package_version_lines 0 package_version_line)
string(REGEX REPLACE "^set\\(PACKAGE_VERSION \"([^\"]+)\"\\)$" "\\1"
  actual_ortools_version "${package_version_line}")
file(APPEND "${evidence_dir}/version-normalization.txt"
  "generated_config=${ortools_version_config}\n"
  "actual_ortools_version=${actual_ortools_version}\n")
file(APPEND "${report_file}"
  "ortools_version_config_actual=${actual_ortools_version}\n")
if(NOT actual_ortools_version STREQUAL expected_ortools_version)
  message(FATAL_ERROR
    "Generated OR-Tools package version is ${actual_ortools_version}, "
    "expected exactly ${expected_ortools_version}.")
endif()

set(ortools_cache "${ortools_build_dir}/CMakeCache.txt")
foreach(cache_expectation
    "CMAKE_BUILD_TYPE|Release"
    "CMAKE_CXX_FLAGS|${eigen_license_cxx_flag}"
    "BUILD_SHARED_LIBS|${build_shared_libraries}"
    "BUILD_CXX|ON"
    "BUILD_DEPS|ON"
    "INSTALL_BUILD_DEPS|ON"
    "BUILD_PYTHON|OFF"
    "BUILD_JAVA|OFF"
    "BUILD_DOTNET|OFF"
    "BUILD_TESTING|OFF"
    "BUILD_SAMPLES|OFF"
    "BUILD_CXX_SAMPLES|OFF"
    "BUILD_EXAMPLES|OFF"
    "BUILD_CXX_EXAMPLES|OFF"
    "BUILD_DOC|OFF"
    "BUILD_FLATZINC|OFF"
    "BUILD_MATH_OPT|OFF"
    "USE_COINOR|OFF"
    "USE_SCIP|OFF"
    "USE_GLPK|OFF"
    "USE_HIGHS|OFF"
    "USE_GUROBI|OFF"
    "USE_CPLEX|OFF"
    "USE_XPRESS|OFF"
    "USE_PDLP|OFF"
    "USE_BOP|ON"
    "USE_GLOP|ON")
  string(REPLACE "|" ";" cache_expectation_parts "${cache_expectation}")
  list(GET cache_expectation_parts 0 cache_entry_name)
  list(GET cache_expectation_parts 1 cache_entry_value)
  require_cache_entry(ortools "${ortools_cache}" "${cache_entry_name}" "${cache_entry_value}")
endforeach()
require_cache_entry(ortools "${ortools_cache}"
  FETCHCONTENT_SOURCE_DIR_PROTOBUF "${protobuf_source_dir}")
record_cache_entry(ortools "${ortools_cache}" CMAKE_INSTALL_PREFIX)
record_cache_entry(ortools "${ortools_cache}" CMAKE_C_COMPILER)
record_cache_entry(ortools "${ortools_cache}" CMAKE_CXX_COMPILER)

run_stage(ortools-build "${PROBE_ROOT}"
  "${CMAKE_COMMAND}" --build "${ortools_build_dir}" --config Release --parallel 2)
run_stage(ortools-install "${PROBE_ROOT}"
  "${CMAKE_COMMAND}" --install "${ortools_build_dir}" --config Release)

set(worker_configure_command
  "${CMAKE_COMMAND}"
  -S "${REPOSITORY_ROOT}/workers/ortools"
  -B "${worker_build_dir}"
  -G "${PROBE_GENERATOR}"
)
if(DEFINED PROBE_GENERATOR_PLATFORM AND NOT PROBE_GENERATOR_PLATFORM STREQUAL "")
  list(APPEND worker_configure_command -A "${PROBE_GENERATOR_PLATFORM}")
endif()
if(WIN32 AND PROBE_GENERATOR STREQUAL "Ninja")
  list(APPEND worker_configure_command "-DCMAKE_CXX_COMPILER=cl.exe")
endif()
list(APPEND worker_configure_command
  "-DCMAKE_BUILD_TYPE=Release"
  "-DCMAKE_PREFIX_PATH=${ortools_install_dir}"
  "-DEUTHETO_ORTOOLS_DEVELOPMENT_BUILD=ON"
  "-DEUTHETO_ORTOOLS_BUILD_TESTS=ON"
  "-DEUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS=ON"
)
run_stage(worker-configure "${PROBE_ROOT}" ${worker_configure_command})

set(worker_cache "${worker_build_dir}/CMakeCache.txt")
require_cache_entry(worker "${worker_cache}" CMAKE_BUILD_TYPE Release)
require_cache_entry(worker "${worker_cache}" EUTHETO_ORTOOLS_DEVELOPMENT_BUILD ON)
require_cache_entry(worker "${worker_cache}" EUTHETO_ORTOOLS_BUILD_TESTS ON)
require_cache_entry(worker "${worker_cache}" EUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS ON)
require_cache_entry(worker "${worker_cache}" EUTHETO_ORTOOLS_PHASE3_CONTRACT "")
record_cache_entry(worker "${worker_cache}" CMAKE_PREFIX_PATH)
record_cache_entry(worker "${worker_cache}" ortools_DIR)
record_cache_entry(worker "${worker_cache}" Protobuf_DIR)
record_cache_entry(worker "${worker_cache}" CMAKE_CXX_COMPILER)

run_stage(worker-build "${PROBE_ROOT}"
  "${CMAKE_COMMAND}" --build "${worker_build_dir}" --config Release --parallel 2)

if(WIN32 AND PROBE_GENERATOR MATCHES "^Visual Studio")
  set(worker_executable "${worker_build_dir}/Release/ortools-worker.exe")
  set(worker_test_executable
    "${worker_build_dir}/tests/Release/ortools-worker-native-tests.exe")
  set(worker_callback_test_executable
    "${worker_build_dir}/tests/Release/ortools-worker-callback-tests.exe")
  set(worker_benchmark_executable
    "${worker_build_dir}/tests/Release/ortools-worker-candidate-benchmarks.exe")
elseif(WIN32)
  set(worker_executable "${worker_build_dir}/ortools-worker.exe")
  set(worker_test_executable
    "${worker_build_dir}/tests/ortools-worker-native-tests.exe")
  set(worker_callback_test_executable
    "${worker_build_dir}/tests/ortools-worker-callback-tests.exe")
  set(worker_benchmark_executable
    "${worker_build_dir}/tests/ortools-worker-candidate-benchmarks.exe")
else()
  set(worker_executable "${worker_build_dir}/ortools-worker")
  set(worker_test_executable
    "${worker_build_dir}/tests/ortools-worker-native-tests")
  set(worker_callback_test_executable
    "${worker_build_dir}/tests/ortools-worker-callback-tests")
  set(worker_benchmark_executable
    "${worker_build_dir}/tests/ortools-worker-candidate-benchmarks")
endif()
if(NOT EXISTS "${worker_executable}")
  message(FATAL_ERROR "Expected worker executable does not exist: ${worker_executable}")
endif()
if(NOT EXISTS "${worker_test_executable}")
  message(FATAL_ERROR
    "Expected worker test executable does not exist: ${worker_test_executable}")
endif()
if(NOT EXISTS "${worker_callback_test_executable}")
  message(FATAL_ERROR
    "Expected worker callback test executable does not exist: ${worker_callback_test_executable}")
endif()
if(NOT EXISTS "${worker_benchmark_executable}")
  message(FATAL_ERROR
    "Expected worker benchmark executable does not exist: ${worker_benchmark_executable}")
endif()
file(APPEND "${report_file}"
  "worker_executable_inspected=true\n"
  "worker_test_executable_inspected=true\n"
  "worker_callback_test_executable_inspected=true\n"
  "worker_benchmark_executable_inspected=true\n")

if(WIN32)
  file(GLOB runtime_libraries "${ortools_install_dir}/bin/*.dll")
  find_program(dumpbin_executable dumpbin REQUIRED)
elseif(APPLE)
  file(GLOB runtime_libraries
    "${ortools_install_dir}/lib/*.dylib"
    "${ortools_install_dir}/lib64/*.dylib")
  find_program(file_executable file REQUIRED)
  find_program(otool_executable otool REQUIRED)
else()
  file(GLOB runtime_libraries
    "${ortools_install_dir}/lib/*.so"
    "${ortools_install_dir}/lib/*.so.*"
    "${ortools_install_dir}/lib64/*.so"
    "${ortools_install_dir}/lib64/*.so.*")
  find_program(file_executable file REQUIRED)
  find_program(ldd_executable ldd REQUIRED)
endif()

file(REAL_PATH "${worker_executable}" worker_real_path)
set(runtime_payload_candidates ${runtime_libraries} "${worker_executable}")
set(runtime_payload_files "")
foreach(runtime_payload_candidate IN LISTS runtime_payload_candidates)
  file(REAL_PATH "${runtime_payload_candidate}" runtime_payload_real_path)
  list(APPEND runtime_payload_files "${runtime_payload_real_path}")
endforeach()
list(REMOVE_DUPLICATES runtime_payload_files)
list(SORT runtime_payload_files)

set(runtime_payload_size_file
  "${evidence_dir}/runtime-payload-sizes.txt")
file(WRITE "${runtime_payload_size_file}"
  "classification=candidate-non-distributable\n"
  "scope=worker-plus-installed-runtime-libraries\n"
  "excludes=native-tests-callback-tests-candidate-benchmark\n"
  "identity=normalized-path-relative-to-probe-root\n"
  "linkage=${PROBE_LINKAGE}\n")
set(runtime_payload_aggregate_bytes 0)
set(runtime_payload_index 0)
foreach(runtime_payload_file IN LISTS runtime_payload_files)
  math(EXPR runtime_payload_index "${runtime_payload_index} + 1")
  file(SIZE "${runtime_payload_file}" runtime_payload_file_bytes)
  math(EXPR runtime_payload_aggregate_bytes
    "${runtime_payload_aggregate_bytes} + ${runtime_payload_file_bytes}")
  set(runtime_payload_identity "${runtime_payload_file}")
  cmake_path(RELATIVE_PATH runtime_payload_identity
    BASE_DIRECTORY "${PROBE_ROOT}")
  cmake_path(NORMAL_PATH runtime_payload_identity)
  if(runtime_payload_file STREQUAL worker_real_path)
    set(runtime_payload_classification "worker-executable")
  else()
    set(runtime_payload_classification "installed-runtime-library")
  endif()
  file(APPEND "${runtime_payload_size_file}"
    "file.${runtime_payload_index}.classification=${runtime_payload_classification}\n"
    "file.${runtime_payload_index}.scope=runtime-payload\n"
    "file.${runtime_payload_index}.identity=${runtime_payload_identity}\n"
    "file.${runtime_payload_index}.bytes=${runtime_payload_file_bytes}\n")
endforeach()
list(LENGTH runtime_payload_files runtime_payload_count)
file(APPEND "${runtime_payload_size_file}"
  "count=${runtime_payload_count}\n"
  "aggregate_bytes=${runtime_payload_aggregate_bytes}\n")
file(APPEND "${report_file}"
  "runtime_payload_scope=worker-plus-installed-runtime-libraries\n"
  "runtime_payload_excludes=native-tests-callback-tests-candidate-benchmark\n"
  "runtime_payload_count=${runtime_payload_count}\n"
  "runtime_payload_aggregate_bytes=${runtime_payload_aggregate_bytes}\n")

set(inspection_candidates
  ${runtime_payload_files}
  "${worker_test_executable}"
  "${worker_callback_test_executable}"
  "${worker_benchmark_executable}")
set(inspection_binaries "")
foreach(inspection_candidate IN LISTS inspection_candidates)
  file(REAL_PATH "${inspection_candidate}" inspection_real_path)
  list(APPEND inspection_binaries "${inspection_real_path}")
endforeach()
list(REMOVE_DUPLICATES inspection_binaries)
list(SORT inspection_binaries)
list(LENGTH inspection_binaries runtime_binary_count)
file(APPEND "${report_file}" "runtime_binary_count=${runtime_binary_count}\n")

foreach(runtime_binary IN LISTS inspection_binaries)
  set(runtime_subject "${runtime_binary}")
  cmake_path(RELATIVE_PATH runtime_subject BASE_DIRECTORY "${PROBE_ROOT}")
  cmake_path(NORMAL_PATH runtime_subject)
  if(WIN32)
    append_inspection(binary-architecture "${runtime_subject}" "${PROBE_ROOT}"
      "${dumpbin_executable}" /headers "${runtime_binary}")
    append_inspection(binary-linkage "${runtime_subject}" "${PROBE_ROOT}"
      "${dumpbin_executable}" /dependents "${runtime_binary}")
  elseif(APPLE)
    append_inspection(binary-architecture "${runtime_subject}" "${PROBE_ROOT}"
      "${file_executable}" "${runtime_binary}")
    append_inspection(binary-linkage "${runtime_subject}" "${PROBE_ROOT}"
      "${otool_executable}" -L "${runtime_binary}")
  else()
    append_inspection(binary-architecture "${runtime_subject}" "${PROBE_ROOT}"
      "${file_executable}" "${runtime_binary}")
    append_inspection(binary-linkage "${runtime_subject}" "${PROBE_ROOT}"
      "${ldd_executable}" "${runtime_binary}")
  endif()
endforeach()
file(APPEND "${report_file}" "runtime_closure_inspection=passed\n")

if(WIN32)
  set(ENV{PATH} "${ortools_install_dir}/bin;$ENV{PATH}")
endif()

set(worker_empty_stdin_file "${evidence_dir}/worker-empty-stdin.txt")
set(worker_startup_loading_file
  "${evidence_dir}/worker-startup-loading.txt")
file(WRITE "${worker_empty_stdin_file}" "")
file(WRITE "${worker_startup_loading_file}"
  "classification=candidate-non-distributable\n"
  "scope=worker-process-spawn-runtime-loading-ortools-version-initialization-eof-rejection\n"
  "not_evidence_of=handshake-adapter-solve-latency-cold-start-or-sla\n"
  "protocol_expectation=empty-stdin-exit-64-empty-stdout\n"
  "run_count=4\n")
set(worker_high_resolution_elapsed_pattern
  "Elapsed time \\(seconds\\):[ \t]*([0-9]+([.][0-9]+)?([eE][+-]?[0-9]+)?)")
set(worker_legacy_elapsed_pattern
  "Elapsed time: ([0-9]+) s[.] \\(time\\), ([0-9]+([.][0-9]+)?) s[.] \\(clock\\)")
set(worker_elapsed_pattern
  "(${worker_high_resolution_elapsed_pattern}|${worker_legacy_elapsed_pattern})")
foreach(worker_run RANGE 1 4)
  set(worker_run_stdout
    "${evidence_dir}/worker-startup-loading-run-${worker_run}.stdout.txt")
  set(worker_run_stderr
    "${evidence_dir}/worker-startup-loading-run-${worker_run}.stderr.txt")
  set(worker_run_timer_stdout
    "${evidence_dir}/worker-startup-loading-run-${worker_run}.timer-stdout.txt")
  set(worker_run_result_file
    "${evidence_dir}/worker-startup-loading-run-${worker_run}.result.txt")
  execute_process(
    COMMAND "${CMAKE_COMMAND}" -E time "${worker_executable}"
    INPUT_FILE "${worker_empty_stdin_file}"
    OUTPUT_VARIABLE worker_run_timed_stdout
    ERROR_FILE "${worker_run_stderr}"
    RESULT_VARIABLE worker_run_result
    TIMEOUT 60
    ENCODING UTF-8
  )
  file(WRITE "${worker_run_timer_stdout}" "${worker_run_timed_stdout}")
  file(WRITE "${worker_run_result_file}" "${worker_run_result}\n")
  string(REPLACE "\r\n" "\n"
    worker_run_timed_stdout_normalized "${worker_run_timed_stdout}")
  string(REGEX MATCHALL "${worker_elapsed_pattern}"
    worker_elapsed_matches "${worker_run_timed_stdout_normalized}")
  list(LENGTH worker_elapsed_matches worker_elapsed_match_count)
  if(NOT "${worker_run_result}" STREQUAL "64")
    message(FATAL_ERROR
      "Worker startup/loading run ${worker_run} returned ${worker_run_result}, expected 64.")
  endif()
  if(NOT worker_elapsed_match_count EQUAL 1)
    message(FATAL_ERROR
      "Worker startup/loading run ${worker_run} produced "
      "${worker_elapsed_match_count} CMake elapsed-seconds records, expected one.")
  endif()
  list(GET worker_elapsed_matches 0 worker_elapsed_match)
  string(REPLACE "${worker_elapsed_match}" ""
    worker_run_stdout_content "${worker_run_timed_stdout_normalized}")
  if(worker_run_stdout_content STREQUAL "\n")
    set(worker_run_stdout_content "")
  endif()
  file(WRITE "${worker_run_stdout}" "${worker_run_stdout_content}")
  file(SIZE "${worker_run_stdout}" worker_run_stdout_bytes)
  if(NOT worker_run_stdout_bytes EQUAL 0)
    message(FATAL_ERROR
      "Worker startup/loading run ${worker_run} wrote ${worker_run_stdout_bytes} "
      "unexpected stdout bytes.")
  endif()
  if(worker_elapsed_match MATCHES "^Elapsed time \\(seconds\\):")
    string(REGEX REPLACE
      "^Elapsed time \\(seconds\\):[ \t]*" ""
      worker_elapsed_seconds "${worker_elapsed_match}")
    set(worker_timer_format "high-resolution-wall-seconds")
  else()
    string(REGEX REPLACE
      "^Elapsed time: ([0-9]+) s[.] \\(time\\),.*$" "\\1"
      worker_elapsed_seconds "${worker_elapsed_match}")
    set(worker_timer_format "legacy-whole-wall-seconds")
  endif()
  file(APPEND "${worker_startup_loading_file}"
    "run.${worker_run}.result=${worker_run_result}\n"
    "run.${worker_run}.stdout_bytes=${worker_run_stdout_bytes}\n"
    "run.${worker_run}.timer_format=${worker_timer_format}\n"
    "run.${worker_run}.elapsed_seconds_raw=${worker_elapsed_seconds}\n")
endforeach()
file(APPEND "${report_file}"
  "worker_startup_loading_evidence=passed\n"
  "worker_startup_loading_scope=process-spawn-runtime-loading-ortools-version-initialization-eof-rejection\n"
  "worker_startup_loading_excludes=handshake-adapter-solve-latency-cold-start-sla\n"
  "worker_startup_loading_run_count=4\n"
  "worker_startup_loading_expected_exit_code=64\n"
  "worker_startup_loading_stdout=empty\n")
execute_process(
  COMMAND "${worker_callback_test_executable}"
  RESULT_VARIABLE callback_result
  OUTPUT_FILE "${evidence_dir}/callback-behavior.txt"
  ERROR_FILE "${evidence_dir}/callback-behavior.stderr.txt"
  TIMEOUT 60
  COMMAND_ECHO STDOUT
  ENCODING UTF-8
)
file(APPEND "${report_file}" "callback_test_exit_code=${callback_result}\n")
if(NOT "${callback_result}" STREQUAL "0")
  message(FATAL_ERROR
    "Candidate callback behavior tests failed with exit code ${callback_result}.")
endif()
file(READ "${evidence_dir}/callback-behavior.txt" callback_evidence)
string(FIND "${callback_evidence}" "callback_result=passed" callback_result_marker)
string(FIND "${callback_evidence}" "stop_status=feasible" callback_stop_marker)
if(callback_result_marker EQUAL -1 OR callback_stop_marker EQUAL -1)
  message(FATAL_ERROR
    "Candidate callback behavior evidence is incomplete or malformed.")
endif()
file(APPEND "${report_file}" "callback_behavior_evidence=passed\n")
execute_process(
  COMMAND "${worker_benchmark_executable}"
  RESULT_VARIABLE benchmark_result
  OUTPUT_FILE "${evidence_dir}/primitive-benchmarks.txt"
  ERROR_FILE "${evidence_dir}/primitive-benchmarks.stderr.txt"
  TIMEOUT 60
  COMMAND_ECHO STDOUT
  ENCODING UTF-8
)
file(APPEND "${report_file}" "primitive_benchmark_exit_code=${benchmark_result}\n")
if(NOT "${benchmark_result}" STREQUAL "0")
  message(FATAL_ERROR
    "Candidate primitive benchmarks failed with exit code ${benchmark_result}.")
endif()
file(READ "${evidence_dir}/primitive-benchmarks.txt" benchmark_evidence)
string(FIND "${benchmark_evidence}" "fixture_count=9" benchmark_fixture_marker)
string(FIND "${benchmark_evidence}" "benchmark_result=passed" benchmark_result_marker)
if(benchmark_fixture_marker EQUAL -1 OR benchmark_result_marker EQUAL -1)
  message(FATAL_ERROR
    "Candidate primitive benchmark evidence is incomplete or malformed.")
endif()
file(APPEND "${report_file}" "primitive_benchmark_evidence=passed\n")
execute_process(
  COMMAND "${CMAKE_CTEST_COMMAND}"
    --test-dir "${worker_build_dir}"
    --build-config Release
    --output-on-failure
    --no-tests=error
  RESULT_VARIABLE ctest_result
  OUTPUT_FILE "${evidence_dir}/ctest.log"
  ERROR_FILE "${evidence_dir}/ctest.log"
  TIMEOUT 120
  COMMAND_ECHO STDOUT
  ENCODING UTF-8
)
file(APPEND "${report_file}" "ctest_exit_code=${ctest_result}\n")
if("${ctest_result}" STREQUAL "0")
  file(WRITE "${test_outcome_file}" "ctest=passed\n")
else()
  file(WRITE "${test_outcome_file}" "ctest=failed\nexit_code=${ctest_result}\n")
  message(FATAL_ERROR "Focused native worker ctest failed with exit code ${ctest_result}.")
endif()

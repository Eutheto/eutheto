# SPDX-License-Identifier: Apache-2.0
add_executable(ortools-worker-native-tests native_tests.cc)
target_link_libraries(ortools-worker-native-tests PRIVATE eutheto-ortools-worker-lib)
set(native_worker_test_targets ortools-worker-native-tests)

if(EUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS)
  add_executable(
    ortools-worker-candidate-benchmarks
    candidate_benchmarks.cc
  )
  target_link_libraries(
    ortools-worker-candidate-benchmarks
    PRIVATE eutheto-ortools-worker-lib
  )
  list(APPEND native_worker_test_targets ortools-worker-candidate-benchmarks)
endif()

foreach(native_worker_test_target IN LISTS native_worker_test_targets)
  target_compile_features(${native_worker_test_target} PRIVATE cxx_std_20)
  set_target_properties(
    ${native_worker_test_target}
    PROPERTIES CXX_EXTENSIONS OFF
  )
  if(MSVC)
    target_compile_options(
      ${native_worker_test_target}
      PRIVATE /W4 /WX /permissive-
    )
  else()
    target_compile_options(
      ${native_worker_test_target}
      PRIVATE -Wall -Wextra -Wpedantic -Werror
    )
  endif()
endforeach()

add_test(NAME ortools-worker-native COMMAND ortools-worker-native-tests)
set_tests_properties(ortools-worker-native PROPERTIES TIMEOUT 60)

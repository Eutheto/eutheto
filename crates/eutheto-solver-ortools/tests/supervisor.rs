#![forbid(unsafe_code)]

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use eutheto_protocol::wire::{
    Capability, ResourceLimits, SolveParameters, SolveRequest, WorkerErrorCode, worker_frame,
};
use eutheto_protocol::{CompletedSession, ProtocolFault, STDERR_TRUNCATION_MARKER};
use eutheto_solver_ortools::{
    SafeStopReason, SessionCompletion, SessionFailure, SessionRequest, SupervisorError,
    VerifiedExecutable, WorkerIdentity, supervise,
};
use eutheto_types::{
    CancellationToken, DurationMillis, FixedMonotonicClock, ParentSolveBudget, SolveBudgetView,
    SystemMonotonicClock,
};
use prost::bytes::Bytes;
use sha2::{Digest, Sha256};

const MANIFEST: [u8; 32] = [0x5a; 32];

async fn verified_helper() -> Result<VerifiedExecutable, Box<dyn Error>> {
    let path = Path::new(env!("CARGO_BIN_EXE_worker-helper"));
    let bytes = tokio::fs::read(path).await?;
    let executable_sha256: [u8; 32] = Sha256::digest(bytes).into();
    Ok(VerifiedExecutable::verify(path, executable_sha256, MANIFEST).await?)
}

async fn request(mode: &str, progress: bool) -> Result<SessionRequest, Box<dyn Error>> {
    let executable = verified_helper().await?;
    let mut capabilities = vec![Capability::CpSat];
    if progress {
        capabilities.push(Capability::Progress);
    }
    let model = Bytes::from_static(b"not-a-solver-model");
    let fingerprint = Sha256::digest(&model);
    Ok(SessionRequest {
        executable,
        identity: WorkerIdentity {
            worker_identity: "eutheto-test-worker".to_owned(),
            worker_version: "0.1.0".to_owned(),
            backend_id: "ortools-cp-sat".to_owned(),
            ortools_version: "9.15.6755".to_owned(),
            adapter_version: "0.1.0".to_owned(),
            required_capabilities: capabilities.clone(),
            advertised_capabilities: capabilities,
        },
        core_version: mode.to_owned(),
        solve: SolveRequest {
            request_id: "test-request".to_owned(),
            cp_model_proto: model,
            parameters: Some(SolveParameters::default()),
            resource_limits: Some(ResourceLimits {
                wall_time_millis: 30_000,
                memory_bytes: None,
                worker_threads: 1,
            }),
            model_fingerprint: Bytes::copy_from_slice(&fingerprint),
            ..SolveRequest::default()
        },
    })
}

fn budget(milliseconds: u64) -> Result<(SolveBudgetView, CancellationToken), Box<dyn Error>> {
    let cancellation = CancellationToken::new();
    let parent = ParentSolveBudget::new(
        DurationMillis::new(milliseconds)?,
        Arc::new(SystemMonotonicClock::new()),
        cancellation.clone(),
    )?;
    Ok((parent.phase_view(), cancellation))
}

fn failure(
    result: Result<SessionCompletion, SessionFailure>,
    context: &'static str,
) -> Result<SessionFailure, Box<dyn Error>> {
    match result {
        Err(failure) => Ok(failure),
        Ok(_) => Err(context.into()),
    }
}

#[tokio::test]
async fn executable_identity_rejects_relative_paths_and_digest_changes()
-> Result<(), Box<dyn Error>> {
    let path = Path::new(env!("CARGO_BIN_EXE_worker-helper"));
    let bytes = tokio::fs::read(path).await?;
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    assert!(matches!(
        VerifiedExecutable::verify("worker-helper", digest, MANIFEST).await,
        Err(eutheto_solver_ortools::ExecutableIdentityError::NotAbsolute)
    ));
    let mut changed = digest;
    changed[0] ^= 1;
    assert!(matches!(
        VerifiedExecutable::verify(path, changed, MANIFEST).await,
        Err(eutheto_solver_ortools::ExecutableIdentityError::DigestMismatch)
    ));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn executable_identity_rejects_final_symlinks() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir()?;
    let target = Path::new(env!("CARGO_BIN_EXE_worker-helper"));
    let link = directory.path().join("worker-link");
    symlink(target, &link)?;
    let digest: [u8; 32] = Sha256::digest(tokio::fs::read(target).await?).into();
    assert!(matches!(
        VerifiedExecutable::verify(link, digest, MANIFEST).await,
        Err(eutheto_solver_ortools::ExecutableIdentityError::SymbolicLink)
    ));
    Ok(())
}

#[tokio::test]
async fn public_debug_omits_paths_hashes_models_messages_and_stderr() -> Result<(), Box<dyn Error>>
{
    let session = request("0.1.0", false).await?;
    let debug = format!("{session:?}");
    assert!(!debug.contains(env!("CARGO_BIN_EXE_worker-helper")));
    assert!(!debug.contains("not-a-solver-model"));
    assert!(!debug.contains("test-request"));

    let (budget, _) = budget(30_000)?;
    let completion = supervise(session, budget).await?;
    let completion_debug = format!("{completion:?}");
    assert!(!completion_debug.contains("sanitized test process"));
    assert!(!completion_debug.contains("not-a-solver-model"));
    assert!(completion_debug.contains("retained_bytes"));
    Ok(())
}

#[tokio::test]
async fn launches_without_arguments_in_private_sanitized_environment() -> Result<(), Box<dyn Error>>
{
    let (budget, _) = budget(30_000)?;
    let result = supervise(request("0.1.0", false).await?, budget).await?;
    assert_eq!(result.stop_reason, SafeStopReason::Completed);
    let CompletedSession::Solve(evidence) = result.evidence else {
        return Err("expected solve evidence".into());
    };
    let Some(worker_frame::Body::Error(error)) = &evidence.frame().body else {
        return Err("expected helper error terminal".into());
    };
    assert_eq!(error.code, WorkerErrorCode::UnsupportedModel as i32);
    assert_eq!(error.message, "sanitized test process");
    Ok(())
}

#[tokio::test]
async fn handshake_rejection_is_a_complete_normal_outcome() -> Result<(), Box<dyn Error>> {
    let (budget, _) = budget(30_000)?;
    let result = supervise(request("0.1.1", false).await?, budget).await?;
    assert_eq!(result.stop_reason, SafeStopReason::HandshakeRejected);
    assert!(matches!(
        result.evidence,
        CompletedSession::HandshakeRejected(_)
    ));
    Ok(())
}

#[tokio::test]
async fn stderr_is_sanitized_and_bounded_while_stdout_completes() -> Result<(), Box<dyn Error>> {
    let (budget, _) = budget(30_000)?;
    let result = supervise(request("0.1.4", false).await?, budget).await?;
    assert!(result.stderr.is_truncated());
    assert!(result.stderr.as_str().ends_with(STDERR_TRUNCATION_MARKER));
    Ok(())
}

#[tokio::test]
async fn deadline_and_explicit_cancellation_remain_distinct() -> Result<(), Box<dyn Error>> {
    let (deadline, _) = budget(50)?;
    let timeout = failure(
        supervise(request("0.1.2", false).await?, deadline).await,
        "hanging helper did not time out",
    )?;
    assert_eq!(timeout.stop_reason, SafeStopReason::DeadlineExceeded);
    assert!(matches!(timeout.error, SupervisorError::DeadlineExceeded));

    let (cancel_budget, cancellation) = budget(30_000)?;
    let cancel = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });
    let cancelled = failure(
        supervise(request("0.1.2", false).await?, cancel_budget).await,
        "cancelled helper did not stop",
    )?;
    assert_eq!(cancelled.stop_reason, SafeStopReason::Cancelled);
    assert!(matches!(cancelled.error, SupervisorError::Cancelled));
    Ok(())
}

#[tokio::test]
async fn crashes_missing_and_duplicate_terminals_are_not_evidence() -> Result<(), Box<dyn Error>> {
    let (crash_budget, _) = budget(30_000)?;
    let crash = failure(
        supervise(request("0.1.3", false).await?, crash_budget).await,
        "crashed worker completed",
    )?;
    assert!(matches!(
        crash.error,
        SupervisorError::Io { .. } | SupervisorError::Protocol(_)
    ));
    for mode in ["0.1.5", "0.1.6", "0.2.0"] {
        let (budget, _) = budget(30_000)?;
        let failure = failure(
            supervise(request(mode, false).await?, budget).await,
            "abnormal terminal sequence completed",
        )?;
        assert_eq!(failure.stop_reason, SafeStopReason::ProtocolViolation);
        assert!(matches!(failure.error, SupervisorError::Protocol(_)));
    }
    Ok(())
}

#[tokio::test]
async fn frame_size_and_progress_rate_caps_terminate_the_worker() -> Result<(), Box<dyn Error>> {
    let (size_budget, _) = budget(30_000)?;
    let oversized = failure(
        supervise(request("0.1.7", false).await?, size_budget).await,
        "oversized frame completed",
    )?;
    assert!(matches!(
        oversized.error,
        SupervisorError::Protocol(ProtocolFault::Frame(_))
    ));

    let (rate_budget, _) = budget(30_000)?;
    let rate = failure(
        supervise(request("0.1.9", true).await?, rate_budget).await,
        "event burst completed",
    )?;
    assert_eq!(rate.stop_reason, SafeStopReason::OutputLimit);
    assert!(matches!(rate.error, SupervisorError::EventRateLimit));
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn timeout_reaps_the_spawned_process_group_descendant() -> Result<(), Box<dyn Error>> {
    let cancellation = CancellationToken::new();
    let clock = Arc::new(FixedMonotonicClock::new(Duration::ZERO));
    let parent = ParentSolveBudget::new(DurationMillis::new(30_000)?, clock.clone(), cancellation)?;
    let session = tokio::spawn(supervise(
        request("0.1.8", false).await?,
        parent.phase_view(),
    ));
    let observed_pid = wait_for_descendant("0.1.8").await?;
    clock.advance(Duration::from_secs(31))?;
    let failure = failure(session.await?, "process tree helper did not time out")?;
    assert_eq!(failure.stop_reason, SafeStopReason::DeadlineExceeded);
    wait_until_reaped(observed_pid).await
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn cancellation_and_normal_exit_reap_sigterm_ignoring_descendants()
-> Result<(), Box<dyn Error>> {
    let (cancel_budget, cancellation) = budget(30_000)?;
    let session = tokio::spawn(supervise(request("0.2.2", false).await?, cancel_budget));
    let observed_cancelled_pid = wait_for_descendant("0.2.2").await?;
    cancellation.cancel();
    let cancelled = failure(session.await?, "descendant cancellation completed")?;
    assert_eq!(cancelled.stop_reason, SafeStopReason::Cancelled);
    wait_until_reaped(observed_cancelled_pid).await?;

    let (normal_budget, _) = budget(30_000)?;
    let normal = supervise(request("0.2.1", false).await?, normal_budget).await?;
    assert_eq!(normal.stop_reason, SafeStopReason::Completed);
    let normal_pid = descendant_pid(normal.stderr.as_str())?;
    wait_until_reaped(normal_pid).await
}

#[cfg(target_os = "linux")]
fn descendant_pid(stderr: &str) -> Result<u32, Box<dyn Error>> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("descendant="))
        .ok_or_else(|| "helper did not report descendant".into())
        .and_then(|pid| pid.parse::<u32>().map_err(Into::into))
}

#[cfg(target_os = "linux")]
async fn wait_until_reaped(pid: u32) -> Result<(), Box<dyn Error>> {
    for _ in 0..100 {
        if !Path::new("/proc").join(pid.to_string()).exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("descendant survived process-tree cleanup".into())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn dropping_the_caller_future_force_kills_and_reaps_the_process_tree()
-> Result<(), Box<dyn Error>> {
    let (budget, _) = budget(30_000)?;
    let session = tokio::spawn(supervise(request("0.2.3", false).await?, budget));
    let descendant = wait_for_descendant("0.2.3").await?;
    session.abort();
    let _ = session.await;
    for _ in 0..100 {
        if !Path::new("/proc").join(descendant.to_string()).exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("descendant survived caller future drop".into())
}

#[cfg(target_os = "linux")]
async fn wait_for_descendant(mode: &str) -> Result<u32, Box<dyn Error>> {
    for _ in 0..3_000 {
        for entry in std::fs::read_dir("/proc")? {
            let entry = entry?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let command = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
            let marker = command
                .split(|byte| *byte == 0)
                .any(|argument| argument == b"descendant-ignore-sigterm");
            let matching_mode = command
                .split(|byte| *byte == 0)
                .any(|argument| argument == mode.as_bytes());
            if marker && matching_mode && process_catches_sigterm(&entry.path()) {
                return Ok(pid);
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("helper descendant did not start".into())
}

#[cfg(target_os = "linux")]
fn process_catches_sigterm(process_path: &Path) -> bool {
    const SIGTERM_BIT: u128 = 1 << 14;
    std::fs::read_to_string(process_path.join("status"))
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("SigCgt:"))
                .and_then(|mask| u128::from_str_radix(mask.trim(), 16).ok())
        })
        .is_some_and(|mask| mask & SIGTERM_BIT != 0)
}

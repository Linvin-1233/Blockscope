use crate::capacity::Capacity;
use crate::serve::PROTOCOL_VERSION;
use crate::{AssetArgs, CliError};
use clap::Args;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const MIB: u64 = 1024 * 1024;

#[derive(Debug, Args)]
pub struct SuperviseArgs {
    #[command(flatten)]
    assets: AssetArgs,
    /// Number of worker processes; 0 uses the capacity suggestion.
    #[arg(long, default_value_t = 0)]
    workers: u32,
    /// Rasterizer threads per worker; 0 uses the capacity suggestion.
    #[arg(long, default_value_t = 0)]
    threads: u32,
    /// Jobs that may wait for a busy worker; 0 uses workers * 2.
    #[arg(long, default_value_t = 0)]
    queue: u32,
    /// Parsed scenes each worker keeps for repeated inputs.
    #[arg(long, default_value_t = 1)]
    cache: u32,
    /// Per-job timeout in seconds; 0 disables it.
    #[arg(long, default_value_t = 60)]
    timeout: u64,
    /// Per-worker memory ceiling in MiB; 0 derives it from capacity.
    #[arg(long, default_value_t = 0)]
    memory: u64,
}

struct Job {
    id: String,
    hash: String,
    line: String,
    attempts: u8,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    busy: Option<Job>,
    deadline: Option<Instant>,
    generation: u64,
    rss_bytes: Option<u64>,
}

struct Spec {
    threads: u32,
    cache: u32,
    assets: Vec<OsString>,
    timeout: Option<Duration>,
    memory_limit: Option<u64>,
}

enum Event {
    Request(Job),
    WorkerLine {
        index: usize,
        generation: u64,
        line: String,
    },
    WorkerClosed {
        index: usize,
        generation: u64,
    },
    Invalid {
        id: String,
        message: String,
    },
    Shutdown,
    StdinClosed,
}

pub fn run(args: SuperviseArgs) -> Result<(), CliError> {
    let capacity = Capacity::detect();
    let workers = if args.workers == 0 {
        capacity.suggested_workers
    } else {
        args.workers
    }
    .max(1);
    let threads = if args.threads == 0 {
        capacity.suggested_threads
    } else {
        args.threads
    }
    .max(1);
    let queue_limit = if args.queue == 0 {
        workers * 2
    } else {
        args.queue
    } as usize;

    let timeout = if args.timeout == 0 {
        None
    } else {
        Some(Duration::from_secs(args.timeout))
    };
    let memory_limit = memory_limit(&capacity, workers, args.memory);

    let spec = Spec {
        threads,
        cache: args.cache,
        assets: args.assets.cli_args(),
        timeout,
        memory_limit,
    };

    if workers.saturating_mul(threads) > capacity.cores {
        eprintln!(
            "renderd supervise: {} workers x {} threads exceeds {} cores",
            workers, threads, capacity.cores
        );
    }

    let (tx, rx) = mpsc::channel();
    let input = tx.clone();
    thread::spawn(move || read_stdin(input));

    let mut generation = 0u64;
    let mut pool = Vec::with_capacity(workers as usize);
    for index in 0..workers as usize {
        pool.push(spawn_worker(index, generation, &spec, tx.clone())?);
        generation += 1;
    }

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    write_line(
        &mut out,
        &json!({
            "type": "hello",
            "protocolVersion": PROTOCOL_VERSION,
            "engineVersion": env!("CARGO_PKG_VERSION"),
            "platform": &capacity.platform,
            "cores": capacity.cores,
            "memoryLimitBytes": capacity.memory_limit_bytes,
            "memoryAvailableBytes": capacity.memory_available_bytes,
            "workers": workers,
            "threadsPerWorker": threads,
            "queueLimit": queue_limit,
            "cacheEntries": spec.cache,
            "timeoutSeconds": timeout.map(|value| value.as_secs()),
            "memoryLimitPerWorkerBytes": memory_limit,
            "defaultTier": {
                "width": capacity.default_width,
                "height": capacity.default_height,
                "supersample": capacity.default_supersample,
            },
        }),
    )?;

    let mut queue: VecDeque<Job> = VecDeque::new();
    let mut sticky: HashMap<String, usize> = HashMap::new();
    let mut shutdown_at: Option<Instant> = None;
    let mut last_sample = Instant::now();

    loop {
        let event = match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(event) => Some(event),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        match event {
            Some(Event::Request(job)) => {
                if shutdown_at.is_some() {
                    write_error(&mut out, &job.id, "SHUTDOWN", "worker is shutting down")?;
                    continue;
                }
                let idle = pool.iter().any(|worker| worker.busy.is_none());
                if !idle && queue.len() >= queue_limit {
                    write_error(&mut out, &job.id, "BUSY", "queue is full")?;
                    continue;
                }
                queue.push_back(job);
                dispatch(&mut pool, &mut queue, &mut sticky, &spec, &mut out)?;
            }
            Some(Event::WorkerLine {
                index,
                generation: event_generation,
                line,
            }) => {
                if pool
                    .get(index)
                    .is_none_or(|worker| worker.generation != event_generation)
                {
                    continue;
                }

                let value: Value = match serde_json::from_str(&line) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                match value.get("type").and_then(Value::as_str) {
                    Some("result") => {
                        writeln!(out, "{line}")
                            .and_then(|_| out.flush())
                            .map_err(|error| CliError::internal(format!("stdout: {error}")))?;
                        if let Some(worker) = pool.get_mut(index) {
                            worker.busy = None;
                            worker.deadline = None;
                            worker.rss_bytes = None;
                        }
                        dispatch(&mut pool, &mut queue, &mut sticky, &spec, &mut out)?;
                    }
                    Some("hello") | Some("bye") => {}
                    _ => {}
                }
            }
            Some(Event::WorkerClosed {
                index,
                generation: event_generation,
            }) => {
                if pool
                    .get(index)
                    .is_none_or(|worker| worker.generation != event_generation)
                {
                    continue;
                }
                if shutdown_at.is_some() {
                    continue;
                }

                let job = pool.get_mut(index).and_then(|worker| worker.busy.take());
                let _ = pool[index].child.wait();

                if let Some(job) = job {
                    fail_job(
                        job,
                        "INTERNAL_ERROR",
                        "worker exited during the job",
                        &mut queue,
                        &mut out,
                    )?;
                }

                match spawn_worker(index, generation, &spec, tx.clone()) {
                    Ok(worker) => {
                        generation += 1;
                        pool[index] = worker;
                        dispatch(&mut pool, &mut queue, &mut sticky, &spec, &mut out)?;
                    }
                    Err(error) => {
                        eprintln!("renderd supervise: cannot restart worker {index}: {error}");
                        shutdown_at = Some(Instant::now() + SHUTDOWN_GRACE);
                    }
                }
            }
            Some(Event::Invalid { id, message }) => {
                write_error(&mut out, &id, "INVALID_REQUEST", &message)?;
            }
            Some(Event::Shutdown) | Some(Event::StdinClosed) if shutdown_at.is_none() => {
                for job in queue.drain(..) {
                    write_error(&mut out, &job.id, "SHUTDOWN", "worker is shutting down")?;
                }
                for worker in &mut pool {
                    let _ = writeln!(worker.stdin, "{{\"type\":\"shutdown\"}}");
                    let _ = worker.stdin.flush();
                }
                shutdown_at = Some(Instant::now() + SHUTDOWN_GRACE);
            }
            Some(Event::Shutdown) | Some(Event::StdinClosed) => {}
            None => {}
        }

        if shutdown_at.is_none() {
            enforce_limits(
                &mut pool,
                &mut queue,
                &mut sticky,
                &spec,
                &tx,
                &mut generation,
                &mut out,
            )?;

            if last_sample.elapsed() >= Duration::from_millis(500) {
                last_sample = Instant::now();
                for worker in &mut pool {
                    worker.rss_bytes = if worker.busy.is_some() {
                        process_rss_bytes(&worker.child)
                    } else {
                        None
                    };
                }
            }
        }

        if let Some(deadline) = shutdown_at {
            let all_exited = pool
                .iter_mut()
                .all(|worker| matches!(worker.child.try_wait(), Ok(Some(_))));
            if all_exited {
                break;
            }
            if Instant::now() >= deadline {
                for worker in &mut pool {
                    let _ = worker.child.kill();
                    let _ = worker.child.wait();
                }
                break;
            }
        }
    }

    write_line(&mut out, &json!({ "type": "bye" }))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn enforce_limits(
    pool: &mut [Worker],
    queue: &mut VecDeque<Job>,
    sticky: &mut HashMap<String, usize>,
    spec: &Spec,
    tx: &Sender<Event>,
    generation: &mut u64,
    out: &mut impl Write,
) -> Result<(), CliError> {
    let now = Instant::now();
    let mut restarted = false;

    for (index, worker) in pool.iter_mut().enumerate() {
        let timed_out = worker.deadline.is_some_and(|deadline| now >= deadline);
        let over_memory = spec
            .memory_limit
            .zip(worker.rss_bytes)
            .is_some_and(|(limit, rss)| rss > limit);

        if !timed_out && !over_memory {
            continue;
        }

        let (code, message) = if timed_out {
            ("TIMEOUT", "job exceeded the timeout")
        } else {
            ("MEMORY_LIMIT", "worker exceeded its memory ceiling")
        };

        let job = worker.busy.take();
        worker.deadline = None;
        worker.rss_bytes = None;
        let _ = worker.child.kill();
        let _ = worker.child.wait();

        match spawn_worker(index, *generation, spec, tx.clone()) {
            Ok(replacement) => {
                *generation += 1;
                *worker = replacement;
                restarted = true;
            }
            Err(error) => {
                eprintln!("renderd supervise: cannot restart worker {index}: {error}");
                return Err(CliError::internal("worker restart failed"));
            }
        }

        if let Some(job) = job {
            fail_job(job, code, message, queue, out)?;
        }
    }

    if restarted {
        dispatch(pool, queue, sticky, spec, out)?;
    }

    Ok(())
}

fn dispatch(
    pool: &mut [Worker],
    queue: &mut VecDeque<Job>,
    sticky: &mut HashMap<String, usize>,
    spec: &Spec,
    out: &mut impl Write,
) -> Result<(), CliError> {
    loop {
        let Some(index) = pool.iter().position(|worker| worker.busy.is_none()) else {
            return Ok(());
        };
        if queue.is_empty() {
            return Ok(());
        }

        let position = queue
            .iter()
            .position(|job| !job.hash.is_empty() && sticky.get(&job.hash) == Some(&index))
            .unwrap_or(0);
        let Some(job) = queue.remove(position) else {
            return Ok(());
        };

        if !job.hash.is_empty() {
            sticky.insert(job.hash.clone(), index);
        }

        let worker = &mut pool[index];
        if writeln!(worker.stdin, "{}", job.line)
            .and_then(|_| worker.stdin.flush())
            .is_err()
        {
            let _ = worker.child.kill();
            let _ = worker.child.wait();
            write_error(out, &job.id, "INTERNAL_ERROR", "worker stdin is closed")?;
            continue;
        }
        worker.deadline = spec.timeout.map(|timeout| Instant::now() + timeout);
        worker.busy = Some(job);
    }
}

fn fail_job(
    mut job: Job,
    code: &str,
    message: &str,
    queue: &mut VecDeque<Job>,
    out: &mut impl Write,
) -> Result<(), CliError> {
    if job.attempts == 0 {
        job.attempts = 1;
        queue.push_front(job);
        Ok(())
    } else {
        write_error(out, &job.id, code, message)
    }
}

fn memory_limit(capacity: &Capacity, workers: u32, override_mib: u64) -> Option<u64> {
    if override_mib > 0 {
        return Some(override_mib * MIB);
    }
    if capacity.memory_available_bytes == 0 {
        return None;
    }

    let per_worker = capacity.memory_available_bytes / u64::from(workers.max(1));
    Some((per_worker * 85 / 100).max(256 * MIB))
}

fn spawn_worker(
    index: usize,
    generation: u64,
    spec: &Spec,
    tx: Sender<Event>,
) -> Result<Worker, CliError> {
    let exe = std::env::current_exe()
        .map_err(|error| CliError::internal(format!("current exe: {error}")))?;

    let mut command = Command::new(exe);
    command
        .arg("serve")
        .arg("--threads")
        .arg(spec.threads.to_string())
        .arg("--cache")
        .arg(spec.cache.to_string())
        .args(&spec.assets)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command
        .spawn()
        .map_err(|error| CliError::internal(format!("spawn worker {index}: {error}")))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| CliError::internal("worker has no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CliError::internal("worker has no stdout"))?;

    let mut reader = BufReader::new(stdout);
    let mut hello = String::new();
    reader
        .read_line(&mut hello)
        .map_err(|error| CliError::internal(format!("read worker hello: {error}")))?;
    if !hello.contains("\"hello\"") {
        let _ = child.kill();
        let _ = child.wait();
        return Err(CliError::internal("worker did not send hello"));
    }

    thread::spawn(move || {
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if tx
                .send(Event::WorkerLine {
                    index,
                    generation,
                    line,
                })
                .is_err()
            {
                return;
            }
        }
        let _ = tx.send(Event::WorkerClosed { index, generation });
    });

    Ok(Worker {
        child,
        stdin,
        busy: None,
        deadline: None,
        generation,
        rss_bytes: None,
    })
}

fn read_stdin(tx: Sender<Event>) {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                let _ = tx.send(Event::Invalid {
                    id: String::new(),
                    message: error.to_string(),
                });
                continue;
            }
        };

        match value.get("type").and_then(Value::as_str) {
            Some("shutdown") => {
                let _ = tx.send(Event::Shutdown);
                return;
            }
            Some("render") => {
                let id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let hash = value
                    .pointer("/input/path")
                    .and_then(Value::as_str)
                    .map(file_hash)
                    .unwrap_or_default();
                let _ = tx.send(Event::Request(Job {
                    id,
                    hash,
                    line: line.to_string(),
                    attempts: 0,
                }));
            }
            other => {
                let id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let message = format!("unknown message type '{}'", other.unwrap_or_default());
                let _ = tx.send(Event::Invalid { id, message });
            }
        }
    }

    let _ = tx.send(Event::StdinClosed);
}

fn file_hash(path: &str) -> String {
    std::fs::read(path)
        .map(|bytes| litematic_scene::sha256(&bytes).to_hex())
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn process_rss_bytes(child: &Child) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmRSS:") {
            let kb = value.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[cfg(windows)]
fn process_rss_bytes(child: &Child) -> Option<u64> {
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };

    let handle = child.as_raw_handle();
    let ok = unsafe { K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) };
    if ok == 0 {
        None
    } else {
        Some(counters.working_set_size as u64)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn process_rss_bytes(_child: &Child) -> Option<u64> {
    None
}

fn write_error(out: &mut impl Write, id: &str, code: &str, message: &str) -> Result<(), CliError> {
    write_line(
        out,
        &json!({
            "type": "result",
            "id": id,
            "ok": false,
            "error": { "code": code, "message": message, "retryable": true },
        }),
    )
}

fn write_line(out: &mut impl Write, value: &Value) -> Result<(), CliError> {
    serde_json::to_writer(&mut *out, value).map_err(CliError::internal)?;
    out.write_all(b"\n")
        .and_then(|_| out.flush())
        .map_err(|error| CliError::input(format!("IO_ERROR: stdout: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capacity_with_memory(available: u64) -> Capacity {
        Capacity {
            platform: "test".to_string(),
            cores: 4,
            cpu_quota_milli: None,
            memory_limit_bytes: available,
            memory_available_bytes: available,
            suggested_workers: 2,
            suggested_threads: 2,
            default_width: 1024,
            default_height: 1024,
            default_supersample: 2,
        }
    }

    #[test]
    fn memory_limit_keeps_headroom() {
        let capacity = capacity_with_memory(8 * 1024 * MIB);
        let limit = memory_limit(&capacity, 2, 0).unwrap();
        // 4 GiB per worker minus 15 percent headroom.
        assert!(limit > 3 * 1024 * MIB && limit < 4 * 1024 * MIB, "{limit}");
    }

    #[test]
    fn memory_limit_override_wins() {
        let capacity = capacity_with_memory(8 * 1024 * MIB);
        assert_eq!(memory_limit(&capacity, 2, 512), Some(512 * MIB));
    }

    #[test]
    fn unknown_memory_disables_the_limit() {
        let capacity = capacity_with_memory(0);
        assert_eq!(memory_limit(&capacity, 2, 0), None);
    }
}

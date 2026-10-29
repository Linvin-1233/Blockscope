use crate::capacity::Capacity;
use crate::serve::PROTOCOL_VERSION;
use crate::{AssetArgs, CliError, MeshOptionArgs};
use clap::Args;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

mod rss;

use self::rss::process_rss_bytes;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const MIB: u64 = 1024 * 1024;
/// Upper bound for worker processes; each worker also owns threads and a cache.
const MAX_SUPERVISOR_WORKERS: u32 = 256;
/// The sticky map is only a cache-locality hint; cap it so a long-lived
/// supervisor does not grow with every distinct input file.
const MAX_STICKY_ENTRIES: usize = 4096;

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
    /// Estimated cache budget per worker in bytes; 0 uses half available memory.
    #[arg(long, default_value_t = 0)]
    cache_bytes: u64,
    /// Per-job timeout in seconds; 0 disables it.
    #[arg(long, default_value_t = 60)]
    timeout: u64,
    /// Per-worker memory ceiling in MiB; 0 derives it from capacity.
    #[arg(long, default_value_t = 0)]
    memory: u64,
    #[command(flatten)]
    mesh: MeshOptionArgs,
    #[command(flatten)]
    limits: crate::LimitArgs,
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
    cache_bytes: u64,
    greedy_mesh: bool,
    assets: Vec<OsString>,
    timeout: Option<Duration>,
    memory_limit: Option<u64>,
    force_limits: bool,
    ignore_block_entity_nbt: bool,
}

enum Event {
    Request(Job),
    RequestReady(Job),
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
    crate::warn_forced(args.limits.force_limits);
    let capacity = Capacity::detect();
    let workers = if args.workers == 0 {
        capacity.suggested_workers
    } else {
        args.workers
    }
    .max(1);
    if workers > MAX_SUPERVISOR_WORKERS {
        return Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: workers must be at most {MAX_SUPERVISOR_WORKERS}"
        )));
    }
    let threads = if args.threads == 0 {
        capacity.suggested_threads.min(crate::MAX_THREADS)
    } else {
        args.threads
    }
    .max(1);
    if threads > crate::MAX_THREADS {
        return Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: threads must be at most {}",
            crate::MAX_THREADS
        )));
    }
    let queue_limit = if args.queue == 0 {
        workers.saturating_mul(2)
    } else {
        args.queue
    } as usize;

    let timeout = if args.timeout == 0 {
        None
    } else {
        Some(Duration::from_secs(args.timeout))
    };
    let memory_limit = memory_limit(&capacity, workers, args.memory);
    let cache_bytes = cache_budget(args.cache_bytes, memory_limit);

    let spec = Spec {
        threads,
        cache: args.cache,
        cache_bytes,
        greedy_mesh: args.mesh.options().greedy,
        assets: args.assets.cli_args(),
        timeout,
        memory_limit,
        force_limits: args.limits.force_limits,
        ignore_block_entity_nbt: args.limits.ignore_block_entity_nbt,
    };

    if workers.saturating_mul(threads) > capacity.cores {
        eprintln!(
            "blockscope supervise: {} workers x {} threads exceeds {} cores",
            workers, threads, capacity.cores
        );
    }

    let channel_capacity = queue_limit
        .saturating_add(workers as usize)
        .saturating_add(16);
    let (tx, rx) = mpsc::sync_channel(channel_capacity);
    let input = tx.clone();
    thread::spawn(move || read_stdin(input));

    let (hash_tx, hash_rx) = mpsc::sync_channel::<Job>(channel_capacity);
    let hasher_tx = tx.clone();
    thread::spawn(move || hash_requests(hash_rx, hasher_tx));

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
            "cacheBytes": spec.cache_bytes,
            "backends": if cfg!(feature = "gpu") {
                vec!["cpu", "auto", "gpu"]
            } else {
                vec!["cpu", "auto"]
            },
            "defaultLod": 1,
            "suggestedMaxQuads": capacity.suggested_max_quads,
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
    let mut pending_hashes = 0usize;
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
                let busy = pool.iter().filter(|worker| worker.busy.is_some()).count();
                let outstanding = queue
                    .len()
                    .saturating_add(pending_hashes)
                    .saturating_add(busy);
                if outstanding >= queue_limit.saturating_add(pool.len()) {
                    write_error(&mut out, &job.id, "BUSY", "queue is full")?;
                    continue;
                }

                if job.hash.is_empty() {
                    pending_hashes = pending_hashes.saturating_add(1);
                    if let Err(error) = hash_tx.send(job) {
                        pending_hashes = pending_hashes.saturating_sub(1);
                        write_error(
                            &mut out,
                            &error.0.id,
                            "INTERNAL_ERROR",
                            "hash worker stopped",
                        )?;
                    }
                } else {
                    queue.push_back(job);
                    dispatch(&mut pool, &mut queue, &mut sticky, &spec, &mut out)?;
                }
            }
            Some(Event::RequestReady(job)) => {
                pending_hashes = pending_hashes.saturating_sub(1);
                if shutdown_at.is_some() {
                    write_error(&mut out, &job.id, "SHUTDOWN", "worker is shutting down")?;
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
                        eprintln!("blockscope supervise: cannot restart worker {index}: {error}");
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

            enforce_limits(
                &mut pool,
                &mut queue,
                &mut sticky,
                &spec,
                &tx,
                &mut generation,
                &mut out,
            )?;
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
    tx: &SyncSender<Event>,
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
                eprintln!("blockscope supervise: cannot restart worker {index}: {error}");
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
            remember_sticky(sticky, &job.hash, index);
        }

        let line = job
            .hash
            .split_once(':')
            .map(|(hash, _)| hash)
            .filter(|hash| !hash.is_empty())
            .map(|hash| inject_input_hash(&job.line, hash))
            .unwrap_or_else(|| job.line.clone());
        let worker = &mut pool[index];
        if writeln!(worker.stdin, "{line}")
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

/// Remember a routing hint while keeping the map bounded.
fn remember_sticky(sticky: &mut HashMap<String, usize>, hash: &str, index: usize) {
    if sticky.len() >= MAX_STICKY_ENTRIES && !sticky.contains_key(hash) {
        sticky.clear();
    }
    sticky.insert(hash.to_string(), index);
}

/// Add the supervisor's already-computed hash when the caller omitted it, so
/// the worker can answer cache hits without re-reading the input file.
fn inject_input_hash(line: &str, hash: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(line) else {
        return line.to_string();
    };
    let Some(input) = value.get_mut("input").and_then(Value::as_object_mut) else {
        return line.to_string();
    };
    if input.contains_key("hash") {
        return line.to_string();
    }
    input.insert("hash".to_string(), Value::String(hash.to_string()));
    serde_json::to_string(&value).unwrap_or_else(|_| line.to_string())
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

/// Derive a per-worker cache budget from the memory ceiling.
///
/// A worker that derives its own budget uses half of the *host* available
/// memory, which is far above its share when several workers share the host.
fn cache_budget(requested: u64, memory_limit: Option<u64>) -> u64 {
    if requested > 0 {
        return requested;
    }
    memory_limit.map(|limit| limit / 2).unwrap_or(0)
}

fn spawn_worker(
    index: usize,
    generation: u64,
    spec: &Spec,
    tx: SyncSender<Event>,
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
        .arg("--cache-bytes")
        .arg(spec.cache_bytes.to_string())
        .args(&spec.assets)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if spec.force_limits {
        command.arg("--force-limits");
    }
    if spec.ignore_block_entity_nbt {
        command.arg("--ignore-block-entity-nbt");
    }
    if spec.greedy_mesh {
        command.arg("--greedy-mesh");
    }

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

/// Hash requests off the supervisor's event loop.
///
/// The supervisor still validates the queue before accepting a job, so an
/// overloaded pool rejects requests without reading their input files.
fn hash_requests(rx: Receiver<Job>, tx: SyncSender<Event>) {
    while let Ok(mut job) = rx.recv() {
        if job.hash.is_empty() {
            job.hash = route_key(&job.line);
        }
        if tx.send(Event::RequestReady(job)).is_err() {
            break;
        }
    }
}

fn read_stdin(tx: SyncSender<Event>) {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let line = match crate::file_io::read_line_limited(
            &mut input,
            crate::file_io::MAX_REQUEST_LINE_BYTES,
        ) {
            Ok(crate::file_io::LineRead::Line(line)) => line,
            Ok(crate::file_io::LineRead::Eof) => break,
            Ok(crate::file_io::LineRead::TooLong) => {
                let _ = tx.send(Event::Invalid {
                    id: String::new(),
                    message: format!(
                        "request line exceeds {} bytes",
                        crate::file_io::MAX_REQUEST_LINE_BYTES
                    ),
                });
                continue;
            }
            Ok(crate::file_io::LineRead::InvalidUtf8) => {
                let _ = tx.send(Event::Invalid {
                    id: String::new(),
                    message: "request line is not valid UTF-8".to_string(),
                });
                continue;
            }
            Err(_) => break,
        };

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
                    .pointer("/input/hash")
                    .and_then(Value::as_str)
                    .filter(|hash| !hash.is_empty())
                    .map(|hash| hash.to_ascii_lowercase());
                let lod = value.get("lod").and_then(Value::as_u64).unwrap_or(1);
                let hash = hash.map(|hash| format!("{hash}:{lod}")).unwrap_or_default();
                if tx
                    .send(Event::Request(Job {
                        id,
                        hash,
                        line: line.to_string(),
                        attempts: 0,
                    }))
                    .is_err()
                {
                    return;
                }
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

/// Build the sticky-routing key without blocking the stdin reader.
fn route_key(line: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return String::new();
    };

    let hash = value
        .pointer("/input/path")
        .and_then(Value::as_str)
        .and_then(|path| {
            crate::file_io::read_file_limited(
                std::path::Path::new(path),
                crate::file_io::DEFAULT_INPUT_LIMIT,
            )
            .ok()
        })
        .map(|bytes| litematic_scene::sha256(&bytes).to_hex())
        .unwrap_or_default();
    let lod = value.get("lod").and_then(Value::as_u64).unwrap_or(1);

    format!("{hash}:{lod}")
}

fn write_error(out: &mut impl Write, id: &str, code: &str, message: &str) -> Result<(), CliError> {
    let retryable = !matches!(code, "INVALID_REQUEST" | "INVALID_RENDER_OPTIONS");
    write_line(
        out,
        &json!({
            "type": "result",
            "id": id,
            "ok": false,
            "error": { "code": code, "message": message, "retryable": retryable },
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
mod tests;

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
}

struct Job {
    id: String,
    hash: String,
    line: String,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    busy: Option<Job>,
}

struct Spec {
    threads: u32,
    cache: u32,
    assets: Vec<OsString>,
}

enum Event {
    Request(Job),
    WorkerLine { index: usize, line: String },
    WorkerClosed { index: usize },
    Invalid { id: String, message: String },
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

    let spec = Spec {
        threads,
        cache: args.cache,
        assets: args.assets.cli_args(),
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

    let mut pool = Vec::with_capacity(workers as usize);
    for index in 0..workers as usize {
        pool.push(spawn_worker(index, &spec, tx.clone())?);
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

    loop {
        let event = if shutdown_at.is_some() {
            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(event) => Some(event),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(event) => Some(event),
                Err(_) => break,
            }
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
                dispatch(&mut pool, &mut queue, &mut sticky, &mut out)?;
            }
            Some(Event::WorkerLine { index, line }) => {
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
                        }
                        dispatch(&mut pool, &mut queue, &mut sticky, &mut out)?;
                    }
                    Some("hello") | Some("bye") => {}
                    _ => {}
                }
            }
            Some(Event::WorkerClosed { index }) => {
                if shutdown_at.is_some() {
                    continue;
                }

                let job = pool.get_mut(index).and_then(|worker| worker.busy.take());
                if let Some(job) = job {
                    write_error(
                        &mut out,
                        &job.id,
                        "INTERNAL_ERROR",
                        "worker exited during the job",
                    )?;
                }

                match spawn_worker(index, &spec, tx.clone()) {
                    Ok(worker) => {
                        pool[index] = worker;
                        dispatch(&mut pool, &mut queue, &mut sticky, &mut out)?;
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

fn dispatch(
    pool: &mut [Worker],
    queue: &mut VecDeque<Job>,
    sticky: &mut HashMap<String, usize>,
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
        if let Err(error) =
            writeln!(worker.stdin, "{}", job.line).and_then(|_| worker.stdin.flush())
        {
            let _ = error;
            write_error(out, &job.id, "INTERNAL_ERROR", "worker stdin is closed")?;
            continue;
        }
        worker.busy = Some(job);
    }
}

fn spawn_worker(index: usize, spec: &Spec, tx: Sender<Event>) -> Result<Worker, CliError> {
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
        return Err(CliError::internal("worker did not send hello"));
    }

    thread::spawn(move || {
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if tx.send(Event::WorkerLine { index, line }).is_err() {
                return;
            }
        }
        let _ = tx.send(Event::WorkerClosed { index });
    });

    Ok(Worker {
        child,
        stdin,
        busy: None,
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

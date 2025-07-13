use std::{io::Write, path::PathBuf};

use anyhow::{Context, ensure};
use clap::Parser;

use rayon::iter::{IntoParallelIterator, ParallelBridge, ParallelIterator};
use wskdf_core::{KEY_SIZE, PREIMAGE_SIZE, SALT_SIZE};

const DEFAULT_OPS_LIMIT: u32 = 7;
const DEFAULT_MEM_LIMIT_KBYTES: u32 = 4096 * 1024;

#[derive(Clone, clap::Args)]
struct KdfParams {
    #[arg(long, default_value_t = DEFAULT_OPS_LIMIT)]
    ops_limit: u32,

    #[arg(long, default_value_t = DEFAULT_MEM_LIMIT_KBYTES)]
    mem_limit_kbytes: u32,
}

#[derive(Clone, clap::Parser)]
#[command(name = "wskdf", about = "Weak, Slow, Key Derivation Function", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, clap::Subcommand)]
enum Commands {
    /// Outputs a random preimage and the derived key as hex to two files
    OutputRandomKey {
        #[arg(short, long)]
        n_bits: u8,

        #[arg(short, long)]
        preimage_output: PathBuf,

        #[arg(short, long)]
        key_output: PathBuf,

        #[arg(short, long)]
        salt: String,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
    /// Derives a key from a preimage
    DeriveKey {
        #[arg(short, long)]
        preimage: String,

        #[arg(short, long)]
        key_output: PathBuf,

        #[arg(short, long)]
        salt: String,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
    /// Brute force finds the preimage/key pair using the external command.
    /// The command should receive one the hex encoded derived key on the stdin.
    /// It should exit with 0 if the key is correct, and non-zero otherwise
    FindKey {
        #[arg(short, long)]
        command: String,

        #[arg(short, long)]
        preimage_output: PathBuf,

        #[arg(short, long)]
        key_output: PathBuf,

        #[arg(short, long)]
        n_bits: u8,

        /// Number of threads. If in doubt, run the benchmark first with a smaller number of threads
        #[arg(short, long)]
        threads: usize,

        #[arg(short, long)]
        salt: String,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
    /// Checks if a preimage derives to a given key. Returns exit code 0 if it does, non-zero otherwise
    CheckPreimage {
        #[arg(short, long)]
        key: String,

        #[arg(short, long)]
        preimage: String,

        #[arg(short, long)]
        salt: String,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
    Benchmark {
        /// Iterations per thread
        #[arg(short, long)]
        iterations: usize,

        /// Number of threads. If in doubt, start low then increase until performance peaks
        #[arg(short, long)]
        threads: usize,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    match args.command {
        Commands::OutputRandomKey {
            n_bits,
            preimage_output,
            key_output,
            salt,
            kdf_params,
        } => {
            ensure!(
                !preimage_output.exists(),
                "preimage output file already exists"
            );
            ensure!(!key_output.exists(), "key output file already exists");
            let salt: [u8; SALT_SIZE] = hex::decode(salt)
                .context("salt isn't valid hex")?
                .try_into()
                .map_err(|k| anyhow::anyhow!("salt doesn't fit in [u8; SALT_SIZE]: {k:?}"))?;
            let preimage = wskdf_core::gen_rand_preimage(n_bits)?;
            let preimage_hex = hex::encode(preimage);
            let key = wskdf_core::wskdf_derive_key(
                &preimage,
                &salt,
                kdf_params.ops_limit,
                kdf_params.mem_limit_kbytes,
            )
            .context("derive key failed")?;
            let key_hex = hex::encode(key);

            std::fs::write(preimage_output, preimage_hex)?;
            std::fs::write(key_output, key_hex)?;
        }
        Commands::DeriveKey {
            preimage,
            salt,
            kdf_params,
            key_output,
        } => {
            ensure!(!key_output.exists(), "key output file already exists");
            let preimage: [u8; PREIMAGE_SIZE] = hex::decode(preimage)
                .context("preimage isn't valid hex")?
                .try_into()
                .map_err(|k| {
                    anyhow::anyhow!("preimage doesn't fit in [u8; PREIMAGE_SIZE]: {k:?}")
                })?;
            let salt: [u8; SALT_SIZE] = hex::decode(salt)
                .context("salt isn't valid hex")?
                .try_into()
                .map_err(|k| anyhow::anyhow!("salt doesn't fit in [u8; SALT_SIZE]: {k:?}"))?;
            let key = wskdf_core::wskdf_derive_key(
                &preimage,
                &salt,
                kdf_params.ops_limit,
                kdf_params.mem_limit_kbytes,
            )
            .context("derive key failed")?;
            let key_hex = hex::encode(key);
            std::fs::write(key_output, key_hex)?;
        }
        Commands::FindKey {
            command,
            preimage_output,
            key_output,
            n_bits,
            threads,
            salt,
            kdf_params,
        } => {
            ensure!(
                !preimage_output.exists(),
                "preimage output file already exists"
            );
            ensure!(threads > 0, "threads must be > 0");
            ensure!(!key_output.exists(), "key output file already exists");
            let salt: [u8; SALT_SIZE] = hex::decode(salt)
                .context("salt isn't valid hex")?
                .try_into()
                .map_err(|k| anyhow::anyhow!("salt doesn't fit in [u8; SALT_SIZE]: {k:?}"))?;
            eprintln!("Using {threads} rayon threads");

            // Build a dedicated rayon pool with the requested number of threads so that we
            // don't interfere with any global pool settings.
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .context("failed to build rayon pool")?;

            eprintln!("Starting parallel search");

            let found_preimage = pool.install(|| {
                std::iter::repeat_with(|| {
                    // Each task keeps its own RNG to avoid contention
                    let mut rng = rand::rngs::ThreadRng::default();
                    wskdf_core::core_gen_rand_preimage(n_bits, &mut rng)
                })
                .par_bridge() // turn the iterator into a parallel iterator
                .find_map_any(|random_preimage| {
                    let random_preimage_hex = hex::encode(random_preimage);
                    eprintln!("Deriving key for {random_preimage_hex}");
                    let derived_key = wskdf_core::wskdf_derive_key(
                        &random_preimage,
                        &salt,
                        kdf_params.ops_limit,
                        kdf_params.mem_limit_kbytes,
                    )
                    .expect("derive key to complete");
                    let derived_key_hex = hex::encode(derived_key);
                    let exit_status =
                        exec_and_send_to_stdin(derived_key_hex.as_bytes(), command.clone());
                    if exit_status.map(|s| s.success()).unwrap_or(false) {
                        Some((random_preimage_hex, derived_key_hex))
                    } else {
                        None
                    }
                })
            });
            match found_preimage {
                Some((preimage_hex, derived_key_hex)) => {
                    std::fs::write(preimage_output, preimage_hex)?;
                    std::fs::write(key_output, derived_key_hex)?;
                }
                None => anyhow::bail!("Search terminated without a result"),
            }
        }
        Commands::CheckPreimage {
            key,
            preimage,
            salt,
            kdf_params,
        } => {
            let key: [u8; KEY_SIZE] = hex::decode(key)
                .context("key isn't valid hex")?
                .try_into()
                .map_err(|k| anyhow::anyhow!("key doesn't fit in [u8; KEY_SIZE]: {k:?}"))?;
            let preimage: [u8; PREIMAGE_SIZE] = hex::decode(preimage)
                .context("preimage isn't valid hex")?
                .try_into()
                .map_err(|k| {
                    anyhow::anyhow!("preimage doesn't fit in [u8; PREIMAGE_SIZE]: {k:?}")
                })?;
            let salt: [u8; SALT_SIZE] = hex::decode(salt)
                .context("salt isn't valid hex")?
                .try_into()
                .map_err(|k| anyhow::anyhow!("salt doesn't fit in [u8; SALT_SIZE]: {k:?}"))?;
            let derived_key = wskdf_core::wskdf_derive_key(
                &preimage,
                &salt,
                kdf_params.ops_limit,
                kdf_params.mem_limit_kbytes,
            )
            .context("derive key failed")?;
            anyhow::ensure!(derived_key == key, "derived key doesn't match");
        }
        Commands::Benchmark {
            iterations,
            threads,
            kdf_params,
        } => {
            ensure!(iterations > 0, "iterations must be > 0");
            ensure!(threads > 0, "threads must be > 0");
            eprintln!("Using {threads} threads for benchmark");

            // Build a dedicated rayon pool with the requested number of threads
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .context("failed to build rayon pool")?;

            // Generate a random preimage and salt for benchmarking
            let preimage = wskdf_core::gen_rand_preimage(32)?; // Using 32 bits for preimage
            let salt = [0u8; SALT_SIZE]; // Fixed salt for consistent benchmarking

            eprintln!(
                "Starting benchmark with {iterations} iterations across {threads} threads..."
            );
            let start = std::time::Instant::now();

            let total_iterations = iterations * threads;
            // Execute the benchmark in parallel using the thread pool
            pool.install(|| {
                (0..total_iterations).into_par_iter().for_each(|_i| {
                    let _key = wskdf_core::wskdf_derive_key(
                        &preimage,
                        &salt,
                        kdf_params.ops_limit,
                        kdf_params.mem_limit_kbytes,
                    )
                    .expect("key derivation to work");
                });
            });

            let f64_iterations = iterations as f64;
            let f64_total_iterations = total_iterations as f64;
            let f64_duration_secs = start.elapsed().as_secs_f64();
            let avg_time = f64_duration_secs / f64_total_iterations;
            let derivations_per_second = f64_total_iterations / f64_duration_secs;
            let thread_avg_time = f64_duration_secs / f64_iterations;
            let thread_derivations_per_second = f64_iterations / f64_duration_secs;

            eprintln!("\nBenchmark results:");
            eprintln!("Threads: {threads}");
            eprintln!("Total time: {f64_duration_secs:.2?}s");
            eprintln!("Total iterations: {total_iterations}");
            eprintln!(
                "Global average time per derivation: {avg_time:.2?}ms",
                avg_time = avg_time * 1000.0
            );
            eprintln!("Global derivations per second: {derivations_per_second:.2?}");
            eprintln!("Thread average time per derivation: {thread_avg_time:.2?}s");
            eprintln!("Thread derivations per second: {thread_derivations_per_second:.2?}");

            eprintln!("\nEstimated time to brute-force one preimage/key pair:");
            eprintln!("{:>4} │ {:>12}", "bits", "expected time");
            eprintln!("{:->4}-┼-{:->12}", "", "");

            for bits in 1u8..=32 {
                // space = 2^(bits-1) because MSB is always 1
                let p_block = 1.0 - (1.0 - 2f64.powi(-(bits as i32 - 1))).powi(threads as i32);
                let exp_secs = (1.0 / p_block) * thread_avg_time;
                let human = pretty(exp_secs);
                eprintln!("{bits:>4} │ {human:>12}");
            }
        }
    };
    Ok(())
}

fn exec_and_send_to_stdin(
    bytes: &[u8],
    command: String,
) -> anyhow::Result<std::process::ExitStatus> {
    let mut command = std::process::Command::new(command);
    command.stdin(std::process::Stdio::piped());
    let mut child = command.spawn()?;
    child
        .stdin
        .as_mut()
        .context("failed to get stdin")?
        .write_all(bytes)
        .context("failed to write stdin")?;
    Ok(child.wait()?)
}

fn pretty(secs: f64) -> String {
    const MIN: f64 = 60.0;
    const H: f64 = 60.0 * MIN;
    const D: f64 = 24.0 * H;
    const Y: f64 = 365.25 * D;

    let (value, unit, rest) = if secs < MIN {
        (secs, "s", 0.0)
    } else if secs < H {
        (secs / MIN, "min", secs % MIN)
    } else if secs < D {
        (secs / H, "h", secs % H)
    } else if secs < Y {
        (secs / D, "d", secs % D)
    } else {
        (secs / Y, "y", secs % Y)
    };

    // second unit: pick the next smaller scale
    let second = if unit == "y" {
        format!(" {:.0}d", (rest / D).round())
    } else if unit == "d" {
        format!(" {:.0}h", (rest / H).round())
    } else if unit == "h" {
        format!(" {:.0}min", (rest / MIN).round())
    } else if unit == "min" {
        format!(" {:.0}s", rest.round())
    } else {
        String::new()
    };
    format!("{value:.0}{unit}{second}")
}

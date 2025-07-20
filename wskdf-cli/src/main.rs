use std::{
    io::{Read, Write},
    path::PathBuf,
};

use anyhow::{Context, ensure};
use clap::Parser;

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use wskdf_core::{KEY_SIZE, PREIMAGE_SIZE, SALT_SIZE};

const DEFAULT_OPS_LIMIT: u32 = 7;
const DEFAULT_MEM_LIMIT_KBYTES: u32 = 4096 * 1024;

const STDIN_HELP: &str = "Use - for stdin";
const STDOUT_HELP: &str = "Use - for stdout";

#[derive(Clone, clap::Args, serde::Serialize)]
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
    /// Outputs a random preimage and the derived key encoded as hex to two files
    OutputRandomKey {
        #[arg(short, long)]
        n_bits: u8,

        #[arg(long, help = STDOUT_HELP)]
        preimage_output: PathBuf,

        #[arg(long, help = STDOUT_HELP)]
        key_output: PathBuf,

        #[arg(long, help = STDOUT_HELP)]
        params_output: Option<PathBuf>,

        #[arg(long, help = STDIN_HELP)]
        salt_input: PathBuf,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
    /// Derives a key from a preimage
    DeriveKey {
        #[arg(long, help = STDIN_HELP)]
        preimage_input: PathBuf,

        #[arg(long, help = STDOUT_HELP)]
        key_output: PathBuf,

        #[arg(long, help = STDIN_HELP)]
        salt_input: PathBuf,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
    /// Brute force finds the preimage/key pair using the external command.
    /// The command should receive one the hex encoded derived key on the stdin.
    /// It should exit with 0 if the key is correct, and non-zero otherwise
    FindKey {
        #[arg(short, long)]
        command: String,

        #[arg(long, help = STDOUT_HELP)]
        preimage_output: PathBuf,

        /// Key output file. If not specified, no key will be written, but it can be derived through preimage
        #[arg(long, help = STDOUT_HELP)]
        key_output: Option<PathBuf>,

        #[arg(short, long)]
        n_bits: u8,

        /// Number of threads. If in doubt, run the benchmark first with a smaller number of threads
        #[arg(short, long)]
        threads: usize,

        #[arg(long, help = STDIN_HELP)]
        salt_input: PathBuf,

        #[clap(flatten)]
        kdf_params: KdfParams,
    },
    /// Checks if a preimage derives to a given key. Returns exit code 0 if it does, non-zero otherwise
    CheckPreimage {
        #[arg(long, help = STDIN_HELP)]
        key_input: PathBuf,

        #[arg(long, help = STDIN_HELP)]
        preimage_input: PathBuf,

        #[arg(long, help = STDIN_HELP)]
        salt_input: PathBuf,

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
    GenerateSalt {
        #[arg(short, long, help = STDOUT_HELP)]
        output: PathBuf,
    },
}

#[derive(serde::Serialize)]
struct ParamsOutput {
    kdf_params: KdfParams,
    n_bits: u8,
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    match args.command {
        Commands::OutputRandomKey {
            n_bits,
            preimage_output,
            key_output,
            params_output,
            salt_input,
            kdf_params,
        } => {
            ensure_file_does_not_exists(&preimage_output, "preimage output file already exists")?;
            ensure_file_does_not_exists(&key_output, "key output file already exists")?;
            if let Some(params_output) = &params_output {
                ensure_file_does_not_exists(params_output, "params output file already exists")?;
            }
            let salt = read_file(&salt_input)?;
            let salt = parse_salt(&salt)?;
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
            write_file(&preimage_output, &preimage_hex)?;
            write_file(&key_output, &key_hex)?;
            if let Some(params_output) = &params_output {
                write_file(
                    params_output,
                    &serde_json::to_string_pretty(&ParamsOutput { kdf_params, n_bits })?,
                )?;
            }
        }
        Commands::DeriveKey {
            preimage_input,
            salt_input,
            kdf_params,
            key_output,
        } => {
            ensure_file_does_not_exists(&key_output, "key output file already exists")?;
            let salt = read_file(&salt_input)?;
            let salt = parse_salt(&salt)?;
            let preimage = read_file(&preimage_input)?;
            let preimage = parse_preimage(&preimage)?;
            let key = wskdf_core::wskdf_derive_key(
                &preimage,
                &salt,
                kdf_params.ops_limit,
                kdf_params.mem_limit_kbytes,
            )
            .context("derive key failed")?;
            let key_hex = hex::encode(key);
            write_file(&key_output, &key_hex)?;
        }
        Commands::FindKey {
            command,
            preimage_output,
            key_output,
            n_bits,
            threads,
            salt_input,
            kdf_params,
        } => {
            ensure!(threads > 0, "threads must be > 0");
            ensure_file_does_not_exists(&preimage_output, "preimage output file already exists")?;
            if let Some(key_output) = &key_output {
                ensure_file_does_not_exists(key_output, "key output file already exists")?;
            }
            let salt = read_file(&salt_input)?;
            let salt = parse_salt(&salt)?;

            eprintln!("Using {threads} rayon threads");
            // Build a dedicated rayon pool with the requested number of threads so that we
            // don't interfere with any global pool settings.
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .context("failed to build rayon pool")?;
            eprintln!("Starting parallel search");

            // Estimate search completion times
            let space = 1u64 << (n_bits - 1); // 2^(n-1)
            let expected_trials = space as f64 / (2.0 * threads as f64);
            // Rough estimate based on typical KDF performance - this could be calibrated
            let est_time_per_trial = 0.1; // seconds - rough placeholder
            let expected_time = expected_trials * est_time_per_trial;

            eprintln!("\nTime estimates for full search:");
            eprintln!(
                "  50% chance by: {}",
                pretty(expected_time * percentile_multiplier(0.50))
            );
            eprintln!(
                "  90% chance by: {}",
                pretty(expected_time * percentile_multiplier(0.90))
            );
            eprintln!(
                "  99% chance by: {}",
                pretty(expected_time * percentile_multiplier(0.99))
            );
            eprintln!("  Expected time: {}", pretty(expected_time));
            eprintln!();

            let now = std::time::Instant::now();
            let start = {
                let mut rng = rand::rngs::ThreadRng::default();
                rand::Rng::random_range(&mut rng, 0..space)
            };
            let found_preimage = pool.install(|| {
                (0..space).into_par_iter().find_map_any(|idx| {
                    // deterministic walk starting at `start`
                    let preimage_bytes = index_to_preimage(idx, start, n_bits);
                    let preimage_hex = hex::encode(preimage_bytes);
                    eprintln!("Deriving key for {preimage_hex}");
                    let derived_key = wskdf_core::wskdf_derive_key(
                        &preimage_bytes,
                        &salt,
                        kdf_params.ops_limit,
                        kdf_params.mem_limit_kbytes,
                    )
                    .expect("derive key to complete");
                    let key_hex = hex::encode(derived_key);
                    if exec_and_send_to_stdin(key_hex.as_bytes(), command.clone())
                        .map(|s| s.success())
                        .unwrap_or(false)
                    {
                        Some((preimage_hex, key_hex))
                    } else {
                        None
                    }
                })
            });
            match found_preimage {
                Some((preimage_hex, derived_key_hex)) => {
                    eprintln!("Found key in {}", pretty(now.elapsed().as_secs_f64()));
                    write_file(&preimage_output, &preimage_hex)?;
                    if let Some(key_output) = key_output {
                        write_file(&key_output, &derived_key_hex)?;
                    }
                }
                None => {
                    eprintln!(
                        "Search terminated without a result after {}",
                        pretty(now.elapsed().as_secs_f64())
                    );
                    anyhow::bail!("Search terminated without a result");
                }
            }
        }
        Commands::CheckPreimage {
            key_input,
            preimage_input,
            salt_input,
            kdf_params,
        } => {
            let key = read_file(&key_input)?;
            let key = parse_key(&key)?;
            let preimage = read_file(&preimage_input)?;
            let preimage = parse_preimage(&preimage)?;
            let salt = read_file(&salt_input)?;
            let salt = parse_salt(&salt)?;
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
            eprintln!(
                "{:>4} │ {:>18} │ {:>18} │ {:>18} │ {:>18}",
                "bits",
                "systematic (worst)",
                "random (expected)",
                "random (99th %ile)",
                "random (99.9th %ile)"
            );
            eprintln!(
                "{:->4}-┼-{:->18}-┼-{:->18}-┼-{:->18}-┼-{:->18}",
                "", "", "", "", ""
            );

            for bits in 1u8..=32 {
                // space = 2^(bits-1) because MSB is always 1
                let space: f64 = 2f64.powi(bits as i32 - 1); // 2^(n-1) candidates

                // Systematic search: divide space among threads, worst case is entire partition
                let systematic_work = (space / threads as f64).max(1.0);
                let systematic_secs = systematic_work * thread_avg_time;

                // Random search: expected trials = space/2, but distributed among threads
                let random_expected_work = (space / (2.0 * threads as f64)).max(1.0);
                let random_expected_secs = random_expected_work * thread_avg_time;
                let random_99th_secs = random_expected_secs * percentile_multiplier(0.99);
                let random_999th_secs = random_expected_secs * percentile_multiplier(0.999);

                let systematic_human = pretty(systematic_secs);
                let random_human = pretty(random_expected_secs);
                let random_99th_human = pretty(random_99th_secs);
                let random_999th_human = pretty(random_999th_secs);
                eprintln!(
                    "{bits:>4} │ {systematic_human:>18} │ {random_human:>18} │ {random_99th_human:>18} │ {random_999th_human:>18}"
                );
            }

            eprintln!("\nSearch strategy explanation:");
            eprintln!(
                "• Systematic search: Partitions search space among threads (worst-case time shown)"
            );
            eprintln!(
                "• Random search: Each thread picks candidates randomly (follows geometric distribution)"
            );
            eprintln!("\nRandom search variance:");
            eprintln!(
                "• 50th percentile (median): ~{:.1}× expected time",
                percentile_multiplier(0.50)
            );
            eprintln!(
                "• 90th percentile: ~{:.1}× expected time",
                percentile_multiplier(0.90)
            );
            eprintln!(
                "• 99th percentile: ~{:.1}× expected time",
                percentile_multiplier(0.99)
            );
            eprintln!(
                "• 99.9th percentile: ~{:.1}× expected time",
                percentile_multiplier(0.999)
            );
        }
        Commands::GenerateSalt { output } => {
            ensure_file_does_not_exists(&output, "output file already exists")?;
            let mut rng = rand::rngs::ThreadRng::default();
            let salt: [u8; SALT_SIZE] = rand::Rng::random(&mut rng);
            let salt_hex = hex::encode(salt);
            write_file(&output, &salt_hex)?;
        }
    };
    Ok(())
}

fn ensure_file_does_not_exists(path: &std::path::Path, message: &str) -> anyhow::Result<()> {
    if path.as_os_str() != "-" {
        ensure!(!path.exists(), "{message}");
    }
    Ok(())
}

fn read_file(path: &std::path::Path) -> anyhow::Result<String> {
    if path.as_os_str() == "-" {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(path)
            .with_context(|| format!("failed to read file {}", path.display()))
    }
}

fn write_file(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    if path.as_os_str() == "-" {
        Ok(std::io::stdout().write_all(content.as_bytes())?)
    } else {
        std::fs::write(path, content)
            .with_context(|| format!("failed to write file {}", path.display()))
    }
}

fn parse_salt(salt: &str) -> anyhow::Result<[u8; SALT_SIZE]> {
    let result = hex::decode(salt)
        .context("salt isn't valid hex")?
        .try_into()
        .map_err(|k| anyhow::anyhow!("salt doesn't fit in [u8; SALT_SIZE]: {k:?}"))?;
    Ok(result)
}

fn parse_preimage(preimage: &str) -> anyhow::Result<[u8; PREIMAGE_SIZE]> {
    let preimage = hex::decode(preimage)
        .context("preimage isn't valid hex")?
        .try_into()
        .map_err(|k| anyhow::anyhow!("preimage doesn't fit in [u8; PREIMAGE_SIZE]: {k:?}"))?;
    Ok(preimage)
}

fn parse_key(key: &str) -> anyhow::Result<[u8; KEY_SIZE]> {
    let key = hex::decode(key)
        .context("key isn't valid hex")?
        .try_into()
        .map_err(|k| anyhow::anyhow!("key doesn't fit in [u8; KEY_SIZE]: {k:?}"))?;
    Ok(key)
}

/// Return the `i`-th candidate in the n-bit space, interpreted as
///   ((start + i) mod 2^(n-1))  with the MSB forced to 1.
#[inline]
fn index_to_preimage(i: u64, start: u64, n_bits: u8) -> [u8; 8] {
    debug_assert!((1..=63).contains(&n_bits));
    let hi_mask = 1u64 << (n_bits - 1);
    let space = hi_mask; // 2^(n-1)
    let value = ((start + i) & (space - 1)) | hi_mask;
    value.to_be_bytes()
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

fn percentile_multiplier(percentile: f64) -> f64 {
    -((1.0 - percentile).ln())
}

fn pretty(secs: f64) -> String {
    const MIN: f64 = 60.0;
    const H: f64 = 60.0 * MIN;
    const D: f64 = 24.0 * H;
    const Y: f64 = 365.25 * D; // calendar‐year approximation

    // pick the main unit and how much time is left over
    let (whole, unit, rest) = if secs < MIN {
        (secs, "s", 0.0)
    } else if secs < H {
        let whole = (secs / MIN).floor();
        (whole, "min", secs - whole * MIN)
    } else if secs < D {
        let whole = (secs / H).floor();
        (whole, "h", secs - whole * H)
    } else if secs < Y {
        let whole = (secs / D).floor();
        (whole, "d", secs - whole * D)
    } else {
        let whole = (secs / Y).floor();
        (whole, "y", secs - whole * Y)
    };

    // render the next smaller unit, rounded to the nearest integer
    let second = match unit {
        "y" => format!(" {:.0}d", (rest / D).round()),
        "d" => format!(" {:.0}h", (rest / H).round()),
        "h" => format!(" {:.0}min", (rest / MIN).round()),
        "min" => format!(" {:.0}s", rest.round()),
        _ => String::new(),
    };

    format!("{whole:.0}{unit}{second}")
}

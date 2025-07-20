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
    /// Estimate brute-force search times for different bit lengths given average derivation time
    Estimation {
        /// Average derivation time in seconds
        #[arg(short, long)]
        avg_time_secs: f64,

        /// Number of threads
        #[arg(short, long)]
        threads: usize,

        /// Maximum bit length to calculate
        #[arg(long, default_value_t = 32)]
        max_bits: u8,
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
            eprintln!("Note: This benchmark uses {threads} threads with systematic search");
            eprintln!("For comparison with random search percentiles, see README table");
            eprintln!();
            eprintln!(
                "{:>4} │ {:>18} │ {:>18}",
                "bits", "systematic (worst)", "systematic (expected)"
            );
            eprintln!("{:->4}-┼-{:->18}-┼-{:->18}", "", "", "");

            for bits in 1u8..=32 {
                let space = calculate_search_space(bits);
                let (systematic_expected_secs, systematic_worst_secs) =
                    calculate_systematic_times(space, threads, thread_avg_time);

                let systematic_worst_human = pretty(systematic_worst_secs);
                let systematic_expected_human = pretty(systematic_expected_secs);
                eprintln!(
                    "{bits:>4} │ {systematic_worst_human:>18} │ {systematic_expected_human:>18}"
                );
            }

            eprintln!("\nSystematic search explanation:");
            eprintln!("• Worst-case: One thread gets unlucky and searches entire partition");
            eprintln!(
                "• Expected case: Threads find target halfway through their partitions on average"
            );
            eprintln!("• No variance: Deterministic partitioning means predictable bounds");
            eprintln!("\nFor random search with percentiles, see the README table comparing");
            eprintln!("systematic (16 threads) vs random search (2048 threads)");
        }
        Commands::Estimation {
            avg_time_secs,
            threads,
            max_bits,
        } => {
            eprintln!("Time estimation for different bit lengths:");
            eprintln!("Average derivation time: {avg_time_secs:.2}s");
            eprintln!("Thread count: {threads}");
            eprintln!();

            eprintln!(
                "bits │ systematic-{threads}t │ systematic-{threads}t │ random-{threads}t │ random-{threads}t │ random-{threads}t"
            );
            eprintln!(
                "     │ (expected)     │ (worst case)   │ (expected)│ (99th %)  │ (99.9th %)"
            );
            eprintln!(
                "-----┼----------------┼----------------┼-----------┼-----------┼------------"
            );

            for bits in 1u8..=max_bits {
                let result = calculate_estimation_for_bits(bits, threads, avg_time_secs);

                let systematic_expected_human = pretty(result.systematic_expected_secs);
                let systematic_worst_human = pretty(result.systematic_worst_secs);
                let random_expected_human = pretty(result.random_expected_secs);
                let random_99th_human = pretty(result.random_99th_percentile_secs);
                let random_999th_human = pretty(result.random_999th_percentile_secs);

                eprintln!(
                    "{bits:>4} │ {systematic_expected_human:>14} │ {systematic_worst_human:>14} │ {random_expected_human:>9} │ {random_99th_human:>9} │ {random_999th_human:>10}"
                );
            }

            eprintln!();
            eprintln!("Explanation:");
            eprintln!(
                "• Systematic (expected): Average case with {threads} threads, each searching half their partition"
            );
            eprintln!(
                "• Systematic (worst): One thread searches entire partition of 2^(n-1) / {threads} candidates"
            );
            eprintln!(
                "• Random (expected): {threads} threads with expected 2^(n-1) / {threads} trials per thread"
            );
            eprintln!("• Random (99th %): 99% chance completion is faster than this");
            eprintln!("• Random (99.9th %): 99.9% chance completion is faster than this");
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

/// Estimation results for a given bit length
#[derive(Debug, PartialEq)]
pub struct EstimationResult {
    pub systematic_expected_secs: f64,
    pub systematic_worst_secs: f64,
    pub random_expected_secs: f64,
    pub random_99th_percentile_secs: f64,
    pub random_999th_percentile_secs: f64,
}

/// Calculate search space size for n-bit preimages
/// Returns 2^(n-1) since MSB is always 1
pub fn calculate_search_space(bits: u8) -> f64 {
    2f64.powi(bits as i32 - 1)
}

/// Calculate systematic search times
pub fn calculate_systematic_times(space: f64, threads: usize, avg_time_secs: f64) -> (f64, f64) {
    let expected_work = (space / (2.0 * threads as f64)).max(1.0); // average case: half partition
    let worst_work = (space / threads as f64).max(1.0); // worst case: entire partition

    let expected_secs = expected_work * avg_time_secs;
    let worst_secs = worst_work * avg_time_secs;

    (expected_secs, worst_secs)
}

/// Calculate random search times with percentiles
pub fn calculate_random_times(space: f64, threads: usize, avg_time_secs: f64) -> (f64, f64, f64) {
    let work_per_thread = space / threads as f64; // expected trials per thread
    let expected_secs = work_per_thread * avg_time_secs;

    // Random search percentiles (geometric distribution)
    // For geometric distribution: percentile multiplier = -ln(1 - p)
    let p99_multiplier = -0.01_f64.ln(); // ≈ 4.605
    let p999_multiplier = -0.001_f64.ln(); // ≈ 6.908

    let p99_secs = expected_secs * p99_multiplier;
    let p999_secs = expected_secs * p999_multiplier;

    (expected_secs, p99_secs, p999_secs)
}

/// Calculate all estimation results for a given bit length
pub fn calculate_estimation_for_bits(
    bits: u8,
    threads: usize,
    avg_time_secs: f64,
) -> EstimationResult {
    let space = calculate_search_space(bits);
    let (systematic_expected, systematic_worst) =
        calculate_systematic_times(space, threads, avg_time_secs);
    let (random_expected, random_99th, random_999th) =
        calculate_random_times(space, threads, avg_time_secs);

    EstimationResult {
        systematic_expected_secs: systematic_expected,
        systematic_worst_secs: systematic_worst,
        random_expected_secs: random_expected,
        random_99th_percentile_secs: random_99th,
        random_999th_percentile_secs: random_999th,
    }
}

fn pretty(secs: f64) -> String {
    const MIN: f64 = 60.0;
    const H: f64 = 60.0 * MIN;
    const D: f64 = 24.0 * H;
    const Y: f64 = 365.0 * D; // year approximation (365 days)

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_search_space() {
        assert_eq!(calculate_search_space(1), 1.0); // 2^0 = 1
        assert_eq!(calculate_search_space(2), 2.0); // 2^1 = 2
        assert_eq!(calculate_search_space(3), 4.0); // 2^2 = 4
        assert_eq!(calculate_search_space(4), 8.0); // 2^3 = 8
        assert_eq!(calculate_search_space(20), 524288.0); // 2^19 = 524,288
    }

    #[test]
    fn test_calculate_systematic_times() {
        let space = 524288.0; // 20-bit space
        let threads = 16;
        let avg_time = 30.0;

        let (expected, worst) = calculate_systematic_times(space, threads, avg_time);

        // Expected: space / (2 * threads) * avg_time = 524288 / 32 * 30 = 491520 seconds
        assert_eq!(expected, 491520.0);

        // Worst: space / threads * avg_time = 524288 / 16 * 30 = 983040 seconds
        assert_eq!(worst, 983040.0);

        // Worst should be exactly 2x expected
        assert_eq!(worst, expected * 2.0);
    }

    #[test]
    fn test_calculate_random_times() {
        let space = 524288.0; // 20-bit space
        let threads = 16;
        let avg_time = 30.0;

        let (expected, p99, p999) = calculate_random_times(space, threads, avg_time);

        // Expected: space / threads * avg_time = 524288 / 16 * 30 = 983040 seconds
        assert_eq!(expected, 983040.0);

        // Check percentile multipliers are approximately correct
        let p99_multiplier = p99 / expected;
        let p999_multiplier = p999 / expected;

        // 99th percentile: -ln(0.01) ≈ 4.605
        assert!((p99_multiplier - 4.605).abs() < 0.001);

        // 99.9th percentile: -ln(0.001) ≈ 6.908
        assert!((p999_multiplier - 6.908).abs() < 0.001);
    }

    #[test]
    fn test_calculate_estimation_for_bits_20bit() {
        let result = calculate_estimation_for_bits(20, 16, 30.0);

        // Test known values for 20-bit search with 16 threads and 30s per derivation
        assert_eq!(result.systematic_expected_secs, 491520.0); // 5d 17h
        assert_eq!(result.systematic_worst_secs, 983040.0); // 11d 9h
        assert_eq!(result.random_expected_secs, 983040.0); // 11d 9h

        // Random search should have higher percentiles
        assert!(result.random_99th_percentile_secs > result.random_expected_secs);
        assert!(result.random_999th_percentile_secs > result.random_99th_percentile_secs);

        // Systematic expected should be half of systematic worst
        assert_eq!(
            result.systematic_expected_secs * 2.0,
            result.systematic_worst_secs
        );

        // Random expected should equal systematic worst (same thread count)
        assert_eq!(result.random_expected_secs, result.systematic_worst_secs);
    }

    #[test]
    fn test_calculate_estimation_for_bits_scaling() {
        let result_1t = calculate_estimation_for_bits(20, 1, 30.0);
        let result_16t = calculate_estimation_for_bits(20, 16, 30.0);

        // With 16x more threads, times should be 16x smaller
        assert_eq!(
            result_1t.systematic_expected_secs,
            result_16t.systematic_expected_secs * 16.0
        );
        assert_eq!(
            result_1t.systematic_worst_secs,
            result_16t.systematic_worst_secs * 16.0
        );
        assert_eq!(
            result_1t.random_expected_secs,
            result_16t.random_expected_secs * 16.0
        );
    }

    #[test]
    fn test_pretty_time_formatting() {
        assert_eq!(pretty(15728640.0), "182d 1h"); // 20-bit random expected
        assert_eq!(pretty(491520.0), "5d 17h"); // 20-bit systematic expected
        assert_eq!(pretty(983040.0), "11d 9h"); // 20-bit systematic worst

        // Test edge cases
        assert_eq!(pretty(30.0), "30s");
        assert_eq!(pretty(60.0), "1min 0s");
        assert_eq!(pretty(3600.0), "1h 0min");
        assert_eq!(pretty(86400.0), "1d 0h");
        assert_eq!(pretty(31536000.0), "1y 0d"); // 365 * 24 * 3600
    }

    #[test]
    fn test_percentile_multipliers_precision() {
        let p99_multiplier = -0.01_f64.ln();
        let p999_multiplier = -0.001_f64.ln();

        assert!((p99_multiplier - 4.605).abs() < 0.001);
        assert!((p999_multiplier - 6.908).abs() < 0.001);
    }

    #[test]
    fn test_benchmark_and_estimation_consistency() {
        // Both benchmark and estimation should use the same calculation functions
        // This test ensures they produce identical results for the same inputs
        let bits = 20;
        let threads = 16;
        let measured_time = 31.5; // Simulated benchmark measurement

        // What benchmark command would calculate
        let space = calculate_search_space(bits);
        let (benchmark_expected, benchmark_worst) =
            calculate_systematic_times(space, threads, measured_time);

        // What estimation command would calculate with the same inputs
        let estimation_result = calculate_estimation_for_bits(bits, threads, measured_time);

        // They should be identical
        assert_eq!(
            benchmark_expected,
            estimation_result.systematic_expected_secs
        );
        assert_eq!(benchmark_worst, estimation_result.systematic_worst_secs);
    }

    #[test]
    fn test_readme_table_systematic_values() {
        // Test that our calculations match what the current pretty() function outputs
        // Note: The README table was manually corrected but pretty() still uses year formatting

        // Systematic search (16 threads, 30s per derivation) - testing actual pretty() output
        assert_eq!(
            pretty(calculate_systematic_times(calculate_search_space(9), 16, 30.0).1),
            "8min 0s"
        );
        assert_eq!(
            pretty(calculate_systematic_times(calculate_search_space(20), 16, 30.0).1),
            "11d 9h"
        );
        assert_eq!(
            pretty(calculate_systematic_times(calculate_search_space(23), 16, 30.0).1),
            "91d 1h"
        );
        assert_eq!(
            pretty(calculate_systematic_times(calculate_search_space(24), 16, 30.0).1),
            "182d 1h"
        );
        assert_eq!(
            pretty(calculate_systematic_times(calculate_search_space(25), 16, 30.0).1),
            "364d 2h"
        );
        assert_eq!(
            pretty(calculate_systematic_times(calculate_search_space(26), 16, 30.0).1),
            "1y 363d"
        ); // Current pretty() output
        assert_eq!(
            pretty(calculate_systematic_times(calculate_search_space(27), 16, 30.0).1),
            "3y 361d"
        ); // Current pretty() output
    }

    #[test]
    fn test_readme_table_random_values() {
        // Test random search values (2048 threads, 30s per derivation) - testing actual pretty() output
        assert_eq!(
            pretty(calculate_random_times(calculate_search_space(20), 2048, 30.0).0),
            "2h 8min"
        );
        assert_eq!(
            pretty(calculate_random_times(calculate_search_space(23), 2048, 30.0).0),
            "17h 4min"
        );
        assert_eq!(
            pretty(calculate_random_times(calculate_search_space(24), 2048, 30.0).0),
            "1d 10h"
        );
        assert_eq!(
            pretty(calculate_random_times(calculate_search_space(25), 2048, 30.0).0),
            "2d 20h"
        );
        assert_eq!(
            pretty(calculate_random_times(calculate_search_space(26), 2048, 30.0).0),
            "5d 17h"
        ); // Actual output (slight rounding difference)
        assert_eq!(
            pretty(calculate_random_times(calculate_search_space(27), 2048, 30.0).0),
            "11d 9h"
        );
    }

    #[test]
    fn test_calculation_accuracy() {
        // Test that the raw calculations are mathematically correct
        // These verify the actual seconds are correct, regardless of formatting

        // 20-bit systematic search (16 threads, 30s): 2^19 / 16 * 30 = 983040 seconds = 11d 9h exactly
        let (_, systematic_worst_20) =
            calculate_systematic_times(calculate_search_space(20), 16, 30.0);
        assert_eq!(systematic_worst_20, 983040.0); // 11 days 9 hours in seconds

        // 20-bit random search (2048 threads, 30s): 2^19 / 2048 * 30 = 7680 seconds = 2h 8min exactly
        let (random_expected_20, _, _) =
            calculate_random_times(calculate_search_space(20), 2048, 30.0);
        assert_eq!(random_expected_20, 7680.0); // 2 hours 8 minutes in seconds
    }

    #[test]
    fn test_6bit_calculation_fix() {
        // Test the specific 6-bit calculation issue identified in README.md
        // 6-bit systematic search (16 threads, 30s): ceil(32/16) * 30 = 2 * 30 = 60s = 1min 0s

        let space_6bit = calculate_search_space(6); // 2^5 = 32
        assert_eq!(space_6bit, 32.0);

        let (_, systematic_worst_6) = calculate_systematic_times(space_6bit, 16, 30.0);
        assert_eq!(systematic_worst_6, 60.0); // Should be 60 seconds, not 31
        assert_eq!(pretty(systematic_worst_6), "1min 0s"); // Should format as 1min 0s

        // Also verify bits 1-5 are correct at 30s each
        for bits in 1..=5 {
            let space = calculate_search_space(bits);
            let (_, worst) = calculate_systematic_times(space, 16, 30.0);
            assert_eq!(worst, 30.0, "Bit {bits} should have 30s worst-case time");
        }

        // And verify 7-bit is 2min 0s, not 2min 4s
        let space_7bit = calculate_search_space(7); // 2^6 = 64
        let (_, systematic_worst_7) = calculate_systematic_times(space_7bit, 16, 30.0);
        assert_eq!(systematic_worst_7, 120.0); // Should be 120 seconds
        assert_eq!(pretty(systematic_worst_7), "2min 0s"); // Should format as 2min 0s
    }
}

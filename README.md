# Weak, Slow, Key Derivation Function (WSKDF)

> **TL;DR** WSKDF intentionally limits key‑entropy **and** uses a *slow*, memory‑hard KDF (Argon2id) so that:
>
> * 🔑 Deriving the key when you *know* the preimage is fast (one Argon2id call).
> * 🛠️ Recovering the key when the preimage is lost is still feasible by brute‑force, *but* it takes predictable CPU time that scales with the chosen bit‑length.

---

## Why WSKDF?

* **Simple secret** – Small enough to jot on paper or share.
* **Strong key** – run that preimage through one heavy Argon2id pass. One run is quick; billions are costly.
* **Recoverable** – lose the preimage? Brute‑force time is **predictable** and set by *n* bits and Argon2id cost (see table). You decide whether recovery takes days, weeks or months.
* **Coercion‑resistant** – stash the preimage **elsewhere**. If forced to hand over the key, you truthfully can’t; an attacker must steal the stash or spend the compute.

### Example application: Coercion-Resistant Vault
Note: this is just an idea, we don't suggest this scheme as it was **not peer reviewed** and is a very advanced usage
<img width="3333" height="1215" alt="image" src="https://github.com/user-attachments/assets/4b12e31a-60ef-4b8d-a753-4d500da2e4cc" />

This two-layer scheme provides three distinct recovery paths for the final keyfile:

1. **Direct Access (seconds)**: If you have physical access to the second preimage (e.g., stored in a bank vault), you can derive the second key instantly and decrypt the final keyfile.

2. **Computational Recovery (days/weeks)**: If the second preimage is lost, you can brute-force the 24-bit search space. With sufficient computational resources, recovery is expensive but feasible.

3. **Time-Locked Recovery (30 days)**: Using the first preimage, you must wait for the 30-day derivation to complete, decrypt the second preimage, then derive the second key. This path provides guaranteed access but enforces a significant time delay - crucial for coercion resistance.

Under coercion, even if you provide all materials (both preimages, encrypted files, and parameters), an attacker must either:
- Wait 30 days for the time-locked path, giving authorities time to intervene
- Spend significant resources on computational recovery
- Gain physical access to wherever you've stored the second preimage

See `scripts/complex-scheme.sh` for implementation details.

---

## Setup

If you're using rustup, it's recommended to add the musl target for static compilation because it's the default target in cargo:

```bash
$ rustup target add x86_64-unknown-linux-musl
```

Then build with the alkali feature flag for best performance:

```bash
$ cargo build --release -F alkali
```

Alternatively, you can use the provided Nix flake which automatically sets up the environment:

```bash
$ nix develop
$ cargo build --release -F alkali
```

## Development

This project uses Git hooks for maintaining code quality. When you enter the development environment, hooks are automatically configured:

```bash
$ nix develop
📎 Setting up Git hooks for code quality checks...
✅ Git hooks configured automatically!
   • pre-commit: Checks code formatting
   • pre-push: Runs formatting and clippy checks
```

### Git Hooks

The project includes two Git hooks that help maintain code quality:

1. **pre-commit**: Ensures code is properly formatted before committing
2. **pre-push**: Runs both formatting and clippy checks before pushing

These hooks are automatically configured when you enter the nix development shell. To manually configure them:

```bash
git config core.hooksPath .githooks
```

To disable the hooks temporarily:

```bash
git config --unset core.hooksPath
```

### Running Checks Manually

You can run the quality checks manually at any time:

```bash
# Check formatting
nix develop -c cargo fmt --check

# Fix formatting
nix develop -c cargo fmt

# Run clippy
nix develop -c cargo clippy --workspace -- -D warnings

# Run all checks
nix develop -c cargo fmt --check && nix develop -c cargo clippy --workspace -- -D warnings
```

## CLI quick‑start

Note: salt is a hex encoded string of 16 bytes. It's good enough to generate it once and reuse for multiple keys. You can generate with:
```bash
$ cargo run -- generate-salt --output salt
# which is similar to:
$ openssl rand -hex 16
a228c13efadd4f6435a30d62a998d065
```

In these examples we will use `000102030405060708090a0b0c0d0e0f` as salt.

### Generate a 4‑bit preimage + key
Note: for real-world usage we recommend using a larger bit-length (e.g. 20).
```bash
$ cargo run --release -F alkali -- output-random-key -n 4 --preimage-output preimage --key-output key --salt-input salt

$ cat preimage
000000000000000e

$ cat key
6f95db5eec10b1cd3ef6afc7e3163a2a4a935ce602375b787dbc5f0f06df50aa

# Now we can use the key to encrypt a file
$ cat key | gpg --verbose --symmetric --batch --passphrase-fd 0 --cipher-algo AES256 LICENSE 
gpg: enabled compatibility flags:
gpg: using cipher AES256.CFB
gpg: writing to 'LICENSE.gpg'

$ file LICENSE.gpg 
LICENSE.gpg: PGP symmetric key encrypted data - AES with 256-bit key salted & iterated - SHA256 .
```

### Find a key using an external command
Now suppose we lost the preimage and the key. We can recover them using the external command.
In this example we both find the key/preimage and decrypt the file, see `scripts/gpg_decrypt.sh` for the implementation.
```bash
$ INPUT_FILE=LICENSE.gpg OUTPUT_FILE=/tmp/LICENSE cargo run --release -F alkali -- find-key  --command ./scripts/gpg_decrypt.sh -t 4 -n 4 --preimage-output found-preimage --key-output found-key --salt-input salt
Using 4 rayon threads
Starting parallel search
Deriving key for 000000000000000e
Deriving key for 000000000000000a
Deriving key for 000000000000000c
Deriving key for 0000000000000008
Trying to decrypt LICENSE.gpg with key 620522780b9448642f40e1d5f792d8902dd376e302d16c820403d571c95eda7f
Deriving key for 000000000000000b
Trying to decrypt LICENSE.gpg with key 6f95db5eec10b1cd3ef6afc7e3163a2a4a935ce602375b787dbc5f0f06df50aa
Found key!
Trying to decrypt LICENSE.gpg with key 5f1fdf16c1cbd2b559a38d3c113deed004c3ade44227cf03dbbd4dc6ddad0e2c
Trying to decrypt LICENSE.gpg with key f4443f057ebbb2649b0d4a54bb272ce9979326f360bd589584c678f2b9f1df0b
Trying to decrypt LICENSE.gpg with key 268a91ceda464f5fe70f87601a84c821b38ab2a06d796ac183da7fd5ff0ed403


$ wc LICENSE /tmp/LICENSE 
  21  168 1064 LICENSE
  21  168 1064 /tmp/LICENSE
  42  336 2128 total

$ cat found-preimage 
000000000000000e

$ cat found-key 
6f95db5eec10b1cd3ef6afc7e3163a2a4a935ce602375b787dbc5f0f06df50aa
```

### Commands

All commands share the Argon2id cost flags. For release mode we have:

```text
      --ops-limit <OPS_LIMIT>                [default: 7] (iterations)
      --mem-limit-kbytes <MEM_LIMIT_KBYTES>  [default: 4194304] (4 GiB)
```

> ⚠️ The defaults make a single derivation take \~30s on a typical desktop CPU using all cores. See `benchmark` command below for better estimates on your hardware.

---

## Brute‑force search time estimation

### Understanding the Time Estimates

**Search space**: For n-bit preimages, there are 2^(n-1) possible candidates (MSB always 1).

**Expected trials**:
- Systematic search: 2^(n-2) trials (exactly half the space)
- Random search: 2^(n-1) trials (due to replacement/duplicates)

**Wall-clock time** = (Expected trials × Time per trial) / Number of threads

The table below shows realistic scenarios:
- Desktop (16 threads): Limited by available cores
- Cluster (2048 threads): Limited by coordination overhead

**Assumptions**

* Preimages are uniformly from [2<sup>n-1</sup>, 2<sup>n</sup>), i.e. the most‑significant bit is **always 1**. Every candidate truly has *n* bits; the search‑space size is therefore 2<sup>n‑1</sup>
* Each candidate costs **30s** to evaluate (Argon2id with the default cost).

* Two hardware budgets:
  * 🖥️ **16 threads** (e.g. 16-core/64GB RAM desktop machine)
  * 🏭 **2048 threads** (e.g. 64×32-core/128GB RAM machines on some cloud provider)

| Bits | 16 threads 🖥️<br>(**systematic search**) | 2048 threads 🏭<br>(**systematic search**) | 2048 threads 🏭<br>(**random search**) |                       |                       |
|------|------------------------------------------|---------------------------------------------|----------------------------------------|-----------------------|-----------------------|
|      | Worst-case time                          | Worst-case time                             | Expected time                          | 99th percentile       | 99.9th percentile     |
| 1-5  | 30 s                                     | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 6    | 1 min 0 s                                | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 7    | 2 min 0 s                                | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 8    | 4 min 0 s                                | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 9    | 8 min 0 s                                | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 10   | 16 min 0 s                               | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 11   | 32 min 0 s                               | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 12   | 1 h 4 min                                | 30 s                                        | 30 s                                   | 2 min 18 s            | 3 min 27 s            |
| 13   | 2 h 8 min                                | 1 min 0 s                                   | 1 min 0 s                              | 4 min 36 s            | 6 min 54 s            |
| 14   | 4 h 16 min                               | 2 min 0 s                                   | 2 min 0 s                              | 9 min 13 s            | 13 min 49 s           |
| 15   | 8 h 32 min                               | 4 min 0 s                                   | 4 min 0 s                              | 18 min 25 s           | 27 min 38 s           |
| 16   | 17 h 4 min                               | 8 min 0 s                                   | 8 min 0 s                              | 36 min 50 s           | 55 min 16 s           |
| 17   | 1 d 10 h                                 | 16 min 0 s                                  | 16 min 0 s                             | 1 h 14 min            | 1 h 51 min            |
| 18   | 2 d 20 h                                 | 32 min 0 s                                  | 32 min 0 s                             | 2 h 27 min            | 3 h 41 min            |
| 19   | 5 d 17 h                                 | 1 h 4 min                                   | 1 h 4 min                              | 4 h 55 min            | 7 h 22 min            |
| 20   | 11 d 9 h                                 | 2 h 8 min                                   | 2 h 8 min                              | 9 h 49 min            | 14 h 44 min           |
| 21   | 22 d 18 h                                | 4 h 16 min                                  | 4 h 16 min                             | 19 h 39 min           | 1 d 5 h               |
| 22   | 45 d 12 h                                | 8 h 32 min                                  | 8 h 32 min                             | 1 d 15 h              | 2 d 11 h              |
| 23   | 91 d 0 h                                 | 17 h 4 min                                  | 17 h 4 min                             | 3 d 7 h               | 4 d 22 h              |
| 24   | 182 d 0 h                                | 1 d 10 h                                    | 1 d 10 h                               | 6 d 13 h              | 9 d 20 h              |
| 25   | 364 d 0 h                                | 2 d 20 h                                    | 2 d 20 h                               | 13 d 2 h              | 19 d 16 h             |
| 26   | 1 y 363 d                                | 5 d 17 h                                    | 5 d 17 h                               | 26 d 5 h              | 39 d 7 h              |
| 27   | 3 y 361 d                                | 11 d 9 h                                    | 11 d 9 h                               | 52 d 10 h             | 78 d 14 h             |
| 28   | 7 y 358 d                                | 22 d 18 h                                   | 22 d 18 h                              | 104 d 19 h            | 157 d 5 h             |
| 29   | 15 y 351 d                               | 45 d 12 h                                   | 45 d 12 h                              | 209 d 14 h            | 314 d 9 h             |
| 30   | 31 y 338 d                               | 91 d 1 h                                    | 91 d 1 h                               | 1 y 54 d              | 1 y 264 d             |
| 31   | 63 y 311 d                               | 182 d 1 h                                   | 182 d 1 h                              | 2 y 108 d             | 3 y 163 d             |
| 32   | 127 y 257 d                              | 364 d 2 h                                   | 364 d 2 h                              | 4 y 217 d             | 6 y 325 d             |


### Understanding Random Search Variance

Random search follows a geometric distribution with high variance. For planning purposes, consider the 99th percentile times shown in the table above to understand worst-case scenarios.

**Interpretation**

* **Single machine (16 threads)**: Systematic search partitions the space evenly among threads, eliminating duplicate work. Each thread searches 1/16th of the total space. The expected time to find a key is when half the total space has been searched.

* **Cluster (2048 threads)**: Random search where threads independently select candidates. Despite occasional duplicate work, the 128× increase in threads (2048 vs 16) results in much faster wall-clock time.

* **Key insight**: For the same number of threads, systematic search would complete in half the expected time of random search (due to no duplicates). However, the table compares different thread counts to show realistic deployment scenarios.


### Real world example using the `benchmark` command
The following is an **example output**. Run this command on your own hardware to get accurate time estimates for your machine.
```bash
$ cargo run --release -F alkali -- benchmark -i 1 -t 16
Using 16 threads for benchmark
Starting benchmark with 1 iterations across 16 threads...

Benchmark results:
Threads: 16
Total time: 29.63s
Total iterations: 16
Global average time per derivation: 1851.87ms
Global derivations per second: 0.54
Thread average time per derivation: 29.63s
Thread derivations per second: 0.03

Estimated time to brute-force with measured derivation time:
Average derivation time: 29.63s
Thread count: 16

bits │ systematic-16t │ systematic-16t │ random-16t │ random-16t │ random-16t
     │ (expected)     │ (worst case)   │ (expected)│ (99th %)  │ (99.9th %)
-----┼----------------┼----------------┼-----------┼-----------┼------------
   1 │            30s │            30s │       30s │  2min 16s │   3min 25s
   2 │            30s │            30s │       30s │  2min 16s │   3min 25s
   3 │            30s │            30s │       30s │  2min 16s │   3min 25s
   4 │            30s │            30s │       30s │  2min 16s │   3min 25s
   5 │            30s │            30s │       30s │  2min 16s │   3min 25s
   6 │            30s │            59s │       59s │  4min 33s │   6min 49s
   7 │            59s │       1min 59s │  1min 59s │   9min 6s │  13min 39s
   8 │       1min 59s │       3min 57s │  3min 57s │ 18min 12s │  27min 17s
   9 │       3min 57s │       7min 54s │  7min 54s │ 36min 23s │  54min 35s
  10 │       7min 54s │      15min 48s │ 15min 48s │  1h 13min │   1h 49min
  11 │      15min 48s │      31min 36s │ 31min 36s │  2h 26min │   3h 38min
  12 │      31min 36s │        1h 3min │   1h 3min │  4h 51min │   7h 17min
  13 │        1h 3min │        2h 6min │   2h 6min │  9h 42min │  14h 33min
  14 │        2h 6min │       4h 13min │  4h 13min │ 19h 24min │      1d 5h
  15 │       4h 13min │       8h 26min │  8h 26min │    1d 15h │     2d 10h
  16 │       8h 26min │      16h 51min │ 16h 51min │     3d 6h │     4d 20h
  17 │      16h 51min │         1d 10h │    1d 10h │    6d 11h │     9d 17h
  18 │         1d 10h │         2d 19h │    2d 19h │   12d 23h │    19d 10h
  19 │         2d 19h │         5d 15h │    5d 15h │   25d 21h │    38d 20h
  20 │         5d 15h │         11d 6h │    11d 6h │   51d 18h │    77d 15h
  21 │         11d 6h │        22d 11h │   22d 11h │  103d 12h │    155d 6h
  22 │        22d 11h │        44d 23h │   44d 23h │   207d 0h │   310d 12h
  23 │        44d 23h │        89d 22h │   89d 22h │    1y 49d │    1y 256d
  24 │        89d 22h │       179d 19h │  179d 19h │    2y 98d │    3y 147d
  25 │       179d 19h │       359d 14h │  359d 14h │   4y 196d │    6y 294d
  26 │       359d 14h │        1y 354d │   1y 354d │    9y 27d │   13y 223d
  27 │        1y 354d │        3y 343d │   3y 343d │   18y 54d │    27y 81d
  28 │        3y 343d │        7y 322d │   7y 322d │  36y 108d │   54y 162d
  29 │        7y 322d │       15y 279d │  15y 279d │  72y 216d │  108y 324d
  30 │       15y 279d │       31y 192d │  31y 192d │  145y 67d │  217y 283d
  31 │       31y 192d │        63y 19d │   63y 19d │ 290y 135d │  435y 202d
  32 │        63y 19d │       126y 38d │  126y 38d │ 580y 269d │   871y 39d

Explanation:
• Systematic (expected): Average case with 16 threads, each searching half their partition
• Systematic (worst): One thread searches entire partition of 2^(n-1) / 16 candidates
• Random (expected): 16 threads with expected 2^(n-1) / 16 trials per thread
• Random (99th %): 99% chance completion is faster than this
• Random (99.9th %): 99.9% chance completion is faster than this
```
---

## Computational Cost Estimation for Brute-Force Recovery

### Coercion-Resistant Vault Example (26-bit second preimage)

The `scripts/complex-scheme.sh` example uses these parameters for the second layer:
- **Bit length**: 26 bits (67,108,864 possible values, but only 33,554,432 candidates since MSB=1)
- **Argon2id parameters**: 7 iterations, 4GB memory
- **Expected derivation time**: ~30 seconds per attempt (on typical hardware)

### Time Requirements

From the benchmark table above, brute-forcing a 26-bit preimage requires:
- **16 threads** (desktop): 1 year 363 days worst-case, 364 days expected
- **128 threads** (single large instance): ~91 days expected
- **2048 threads** (distributed cluster): 5.7 days expected, 26 days at 99th percentile

### Cloud Computing Cost Analysis

#### AWS EC2 Pricing (as of 2024)
For memory-hard operations requiring 4GB per thread:

**Option 1: High-Memory Instances**
- Instance type: `r6i.32xlarge` (128 vCPUs, 1024 GB RAM)
- Can run 128 parallel threads (1 per vCPU, each using 4GB RAM)
- Cost: ~$8.06/hour
- Time needed: ~91 days (with 128 threads)
- **Total cost: ~$17,600**

**Option 2: Compute-Optimized Cluster**
- Instance type: `c6i.4xlarge` (16 vCPUs, 32 GB RAM) 
- Can run 8 parallel threads (limited by RAM: 32GB/4GB = 8)
- Cost: ~$0.68/hour per instance
- Need 256 instances for 2048 threads
- Time needed: ~5.7 days expected
- **Total cost: ~$23,800** (expected case)
- **Total cost: ~$108,500** (99th percentile, 26 days)

**Option 3: Spot Instances**
- Using spot pricing can reduce costs by 60-90%
- Less reliable, may be interrupted
- **Estimated cost: $4,000-$40,000** depending on availability

#### Other Cloud Providers

**Google Cloud Platform**
- `n2-highmem-128` (128 vCPUs, 864 GB RAM)
- Can run 128 parallel threads (1 per vCPU)
- Cost: ~$6.74/hour
- Time needed: ~91 days
- **Total cost: ~$14,700**

**Local Hardware Investment**
- 64-core AMD Threadripper: ~$4,000
- 256GB RAM: ~$1,000
- Can run 64 threads continuously
- Time: ~182 days (6 months)
- **One-time cost: ~$5,000** (reusable hardware)

### Cost Factors to Consider

1. **CPU vs Memory Constraints**: Argon2id is CPU-intensive; you can only run one thread per CPU core effectively
2. **Memory Requirements**: Each thread needs 4GB RAM, which can limit thread count on lower-memory instances
3. **Spot vs On-Demand**: Spot instances can reduce costs by 60-90% but may be interrupted
4. **Coordination Overhead**: Managing 2048 threads across 256+ machines requires significant orchestration
5. **Electricity Costs**: For local hardware, add ~$800-2,000 for 182 days of operation

### Conclusion

Realistic cost estimates for brute-forcing a 26-bit preimage:
- **Budget approach**: $5,000-$20,000 using local hardware or spot instances
- **Fast approach**: $24,000-$110,000 for on-demand cloud computing
- **Worst case**: Higher costs if extremely unlucky (99.9th percentile)

These costs make brute-force recovery feasible for high-value assets while remaining prohibitively expensive for casual attackers. The actual cost depends heavily on:
- Current cloud pricing
- Luck in finding the preimage
- Available optimization techniques
- Whether time or money is the primary constraint
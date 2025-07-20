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

### Example application
Note: this is just an idea, we don't suggest this scheme as it was **not peer reviewed** and is a very advanced usage
<img width="3333" height="1215" alt="image" src="https://github.com/user-attachments/assets/4b12e31a-60ef-4b8d-a753-4d500da2e4cc" />

See `scripts/complex-scheme.sh` for a related example.

---

## CLI quick‑start

Note: salt is a hex encoded string of 16 bytes. It's good enough to generate it once and reuse for multiple keys. You can generate with:
```bash
$ cargo run -- generate-salt --output salt
# or for instance:
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

**Assumptions**

* Preimages are uniformly from [2<sup>n-1</sup>, 2<sup>n</sup>), i.e. the most‑significant bit is **always 1**. Every candidate truly has *n* bits; the search‑space size is therefore 2<sup>n‑1</sup>
* Each candidate costs **30s** to evaluate (Argon2id with the default cost).


* Two hardware budgets:
  * 🖥️ **16 threads** (e.g. 16-core/64GB RAM desktop machine)
  * 🏭 **2048 threads** (e.g. 64×32-core/128GB RAM machines on some cloud provider)

| Bits | 16 threads 🖥️<br>(**systematic search**) | 2048 threads 🏭<br>(**random search**) |                   |
|------|------------------------------------------|-----------------------------------------|-------------------|
|      | Worst-case time                          | Expected time | 99th percentile     |
| 1‑6  | 31 s                                     | 30 s          | 2 min 19 s          |
| 7    | 2 min 4 s                                | 30 s          | 2 min 19 s          |
| 8    | 4 min 8 s                                | 30 s          | 2 min 19 s          |
| 9    | 8 min 15 s                               | 30 s          | 2 min 19 s          |
| 10   | 16 min 30 s                              | 31 s          | 2 min 23 s          |
| 11   | 33 min 1 s                               | 35 s          | 2 min 41 s          |
| 12   | 1 h 6 min                                | 47 s          | 3 min 36 s          |
| 13   | 2 h 12 min                               | 1 min 17 s    | 5 min 55 s          |
| 14   | 4 h 24 min                               | 2 min 17 s    | 10 min 31 s         |
| 15   | 8 h 48 min                               | 4 min 17 s    | 19 min 44 s         |
| 16   | 17 h 36 min                              | 8 min 20 s    | 38 min 24 s         |
| 17   | 1 d 11 h                                 | 16 min 19 s   | 1 h 15 m            |
| 18   | 2 d 22 h                                 | 32 min 20 s   | 2 h 29 m            |
| 19   | 5 d 21 h                                 | 1 h 4 m       | 4 h 55 m            |
| 20   | 11 d 18 h                                | 2 h 8 m       | 9 h 52 m            |
| 21   | 23 d 11 h                                | 4 h 16 m      | 19 h 44 m           |
| 22   | 46 d 23 h                                | 8 h 32 m      | 1 d 15 h            |
| 23   | 93 d 22 h                                | 17 h 4 m      | 3 d 6 h             |
| 24   | 187 d 19 h                               | 1 d 10 h      | 6 d 13 h            |
| 25   | 1 y 10 d                                 | 2 d 20 h      | 13 d 2 h            |
| 26   | 2 y 21 d                                 | 5 d 16 h      | 26 d 1 h            |
| 27   | 4 y 41 d                                 | 11 d 9 h      | 52 d 4 h            |
| 28   | 8 y 83 d                                 | 22 d 18 h     | 104 d 8 h           |
| 29   | 16 y 165 d                               | 45 d 12 h     | 208 d 16 h          |
| 30   | 32 y 331 d                               | 91 d          | 417 d 8 h           |
| 31   | 65 y 297 d                               | 182 d         | 2 y 105 d           |
| 32   | 131 y 228 d                              | 364 d         | 4 y 212 d           |

## Understanding Random Search Variance

Random search follows a geometric distribution with high variance. While the table shows expected times, actual recovery can vary significantly:

* **50% chance** of finding the key in ~0.7× the expected time
* **90% chance** it will take longer than ~2.3× the expected time  
* **99% chance** it will take longer than ~4.6× the expected time

For planning purposes, consider the 99th percentile times shown in the table above to understand worst-case scenarios.

**Interpretation**

* **Single machine** (16 threads): we partition the space, so duplicates never happen.  Expected time is exactly half the random search.
* **Cluster** (2048 threads): different machines choose ranges independently; occasional overlaps mean the average time is the same as pure random sampling.


### Real world example using the `benchmark` command
```bash
$ cargo run --release -F alkali -- benchmark -i 1 -t 16
Using 16 threads for benchmark
Starting benchmark with 1 iterations across 16 threads...

Benchmark results:
Threads: 16
Total time: 30.95s
Total iterations: 16
Global average time per derivation: 1934.24ms
Global derivations per second: 0.52
Thread average time per derivation: 30.95s
Thread derivations per second: 0.03

Estimated time to brute-force one preimage/key pair:
bits │ systematic (worst) │  random (expected) │ random (99th %ile)
-----┼--------------------┼--------------------┼-------------------
   1 │                31s │                31s │           2min 23s
   2 │                31s │                31s │           2min 23s
   3 │                31s │                31s │           2min 23s
   4 │                31s │                31s │           2min 23s
   5 │                31s │                31s │           2min 23s
   6 │            1min 2s │                31s │           2min 23s
   7 │            2min 4s │            1min 2s │           4min 45s
   8 │            4min 8s │            2min 4s │           9min 30s
   9 │           8min 15s │            4min 8s │           19min 0s
  10 │          16min 30s │           8min 15s │           38min 0s
  11 │           33min 1s │          16min 30s │           1h 16min
  12 │            1h 6min │           33min 1s │           2h 32min
  13 │           2h 12min │            1h 6min │            5h 4min
  14 │           4h 24min │           2h 12min │           10h 8min
  15 │           8h 48min │           4h 24min │          20h 16min
  16 │          17h 36min │           8h 48min │             1d 17h
  17 │             1d 11h │          17h 36min │              3d 9h
  18 │             2d 22h │             1d 11h │             6d 18h
  19 │             5d 21h │             2d 22h │            13d 12h
  20 │            11d 18h │             5d 21h │             27d 1h
  21 │            23d 11h │            11d 18h │             54d 1h
  22 │            46d 23h │            23d 11h │            108d 2h
  23 │            93d 22h │            46d 23h │            216d 5h
  24 │           187d 19h │            93d 22h │             1y 67d
  25 │             1y 10d │           187d 19h │            2y 134d
  26 │             2y 21d │             1y 10d │            4y 269d
  27 │             4y 41d │             2y 21d │            9y 172d
  28 │             8y 83d │             4y 41d │           18y 344d
  29 │           16y 165d │             8y 83d │           37y 323d
  30 │           32y 331d │           16y 165d │           75y 281d
  31 │           65y 297d │           32y 331d │          151y 197d
  32 │          131y 228d │           65y 297d │           303y 28d

Search strategy explanation:
• Systematic search: Partitions search space among threads (worst-case time shown)
• Random search: Each thread picks candidates randomly (follows geometric distribution)

Random search variance:
• 50th percentile (median): ~0.7× expected time
• 90th percentile: ~2.3× expected time
• 99th percentile: ~4.6× expected time
```
---

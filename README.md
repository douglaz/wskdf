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

| Bits | 16 threads 🖥️<br>(**systematic search**) | 2048 threads 🏭<br>(**random search**) |                   |                     |
|------|------------------------------------------|-----------------------------------------|-------------------|---------------------|
|      | Worst-case time                          | Expected time | 99th percentile     | 99.9th percentile   |
| 1‑6  | 31 s                                     | 30 s          | 2 min 19 s          | 3 min 27 s          |
| 7    | 2 min 4 s                                | 30 s          | 2 min 19 s          | 3 min 27 s          |
| 8    | 4 min 8 s                                | 30 s          | 2 min 19 s          | 3 min 27 s          |
| 9    | 8 min 15 s                               | 30 s          | 2 min 19 s          | 3 min 27 s          |
| 10   | 16 min 30 s                              | 31 s          | 2 min 23 s          | 3 min 33 s          |
| 11   | 33 min 1 s                               | 35 s          | 2 min 41 s          | 4 min 1 s           |
| 12   | 1 h 6 min                                | 47 s          | 3 min 36 s          | 5 min 24 s          |
| 13   | 2 h 12 min                               | 1 min 17 s    | 5 min 55 s          | 8 min 53 s          |
| 14   | 4 h 24 min                               | 2 min 17 s    | 10 min 31 s         | 15 min 47 s         |
| 15   | 8 h 48 min                               | 4 min 17 s    | 19 min 44 s         | 29 min 36 s         |
| 16   | 17 h 36 min                              | 8 min 20 s    | 38 min 24 s         | 57 min 36 s         |
| 17   | 1 d 11 h                                 | 16 min 19 s   | 1 h 15 m            | 1 h 52 m            |
| 18   | 2 d 22 h                                 | 32 min 20 s   | 2 h 29 m            | 3 h 43 m            |
| 19   | 5 d 21 h                                 | 1 h 4 m       | 4 h 55 m            | 7 h 22 m            |
| 20   | 11 d 18 h                                | 2 h 8 m       | 9 h 52 m            | 14 h 48 m           |
| 21   | 23 d 11 h                                | 4 h 16 m      | 19 h 44 m           | 1 d 5 h             |
| 22   | 46 d 23 h                                | 8 h 32 m      | 1 d 15 h            | 2 d 11 h            |
| 23   | 93 d 22 h                                | 17 h 4 m      | 3 d 6 h             | 4 d 21 h            |
| 24   | 187 d 19 h                               | 1 d 10 h      | 6 d 13 h            | 9 d 18 h            |
| 25   | 1 y 10 d                                 | 2 d 20 h      | 13 d 2 h            | 19 d 12 h           |
| 26   | 2 y 21 d                                 | 5 d 16 h      | 26 d 1 h            | 39 d 1 h            |
| 27   | 4 y 41 d                                 | 11 d 9 h      | 52 d 4 h            | 78 d 6 h            |
| 28   | 8 y 83 d                                 | 22 d 18 h     | 104 d 8 h           | 156 d 12 h          |
| 29   | 16 y 165 d                               | 45 d 12 h     | 208 d 16 h          | 312 d 24 h          |
| 30   | 32 y 331 d                               | 91 d          | 417 d 8 h           | 1 y 261 d           |
| 31   | 65 y 297 d                               | 182 d         | 2 y 105 d           | 3 y 157 d           |
| 32   | 131 y 228 d                              | 364 d         | 4 y 212 d           | 6 y 318 d           |

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
Benchmark results:
Threads: 16
Total time: 30.79s
Total iterations: 16
Global average time per derivation: 1924.35ms
Global derivations per second: 0.52
Thread average time per derivation: 30.79s
Thread derivations per second: 0.03

Estimated time to brute-force one preimage/key pair:
bits │ systematic (worst) │  random (expected) │ random (99th %ile) │ random (99.9th %ile)
-----┼--------------------┼--------------------┼--------------------┼-------------------
   1 │                31s │                31s │           2min 22s │           3min 33s
   2 │                31s │                31s │           2min 22s │           3min 33s
   3 │                31s │                31s │           2min 22s │           3min 33s
   4 │                31s │                31s │           2min 22s │           3min 33s
   5 │                31s │                31s │           2min 22s │           3min 33s
   6 │            1min 2s │                31s │           2min 22s │           3min 33s
   7 │            2min 3s │            1min 2s │           4min 44s │            7min 5s
   8 │            4min 6s │            2min 3s │           9min 27s │          14min 11s
   9 │           8min 13s │            4min 6s │          18min 54s │          28min 21s
  10 │          16min 25s │           8min 13s │          37min 49s │          56min 43s
  11 │          32min 51s │          16min 25s │           1h 16min │           1h 53min
  12 │            1h 6min │          32min 51s │           2h 31min │           3h 47min
  13 │           2h 11min │            1h 6min │            5h 2min │           7h 34min
  14 │           4h 23min │           2h 11min │           10h 5min │           15h 7min
  15 │           8h 45min │           4h 23min │          20h 10min │              1d 6h
  16 │          17h 31min │           8h 45min │             1d 16h │             2d 12h
  17 │             1d 11h │          17h 31min │              3d 9h │              5d 1h
  18 │             2d 22h │             1d 11h │             6d 17h │             10d 2h
  19 │             5d 20h │             2d 22h │            13d 11h │             20d 4h
  20 │            11d 16h │             5d 20h │            26d 21h │             40d 8h
  21 │             23d 9h │            11d 16h │            53d 19h │            80d 16h
  22 │            46d 17h │             23d 9h │           107d 13h │            161d 8h
  23 │            93d 10h │            46d 17h │            215d 2h │           322d 16h
  24 │           186d 20h │            93d 10h │             1y 65d │            1y 280d
  25 │              1y 8d │           186d 20h │            2y 130d │            3y 195d
  26 │             2y 17d │              1y 8d │            4y 260d │             7y 24d
  27 │             4y 34d │             2y 17d │            9y 154d │            14y 49d
  28 │             8y 67d │             4y 34d │           18y 309d │            28y 98d
  29 │           16y 135d │             8y 67d │           37y 252d │           56y 196d
  30 │           32y 270d │           16y 135d │           75y 139d │           113y 27d
  31 │           65y 174d │           32y 270d │          150y 279d │           226y 53d
  32 │          130y 348d │           65y 174d │          301y 193d │          452y 106d

Search strategy explanation:
• Systematic search: Partitions search space among threads (worst-case time shown)
• Random search: Each thread picks candidates randomly (follows geometric distribution)

Random search variance:
• 50th percentile (median): ~0.7× expected time
• 90th percentile: ~2.3× expected time
• 99th percentile: ~4.6× expected time
• 99.9th percentile: ~6.9× expected time
```
---

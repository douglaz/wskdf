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

| Bits | 16 threads 🖥️<br>(**systematic search**) | 2048 threads 🏭<br>(**random search**) |
| ---- | ----------------------------------------- | -------------------------------------- |
| 1‑6  | 30 s                                      | 30 s                                   |
| 7    | 1 min 0 s                                 | 30 s                                   |
| 8    | 2 min 0 s                                 | 30 s                                   |
| 9    | 4 min 3 s                                 | 30 s                                   |
| 10   | 8 min 29 s                                | 31 s                                   |
| 11   | 16 min 21 s                               | 35 s                                   |
| 12   | 32 min 2 s                                | 47 s                                   |
| 13   | 1 h 4 m                                   | 1 min 17 s                             |
| 14   | 2 h 8 m                                   | 2 min 17 s                             |
| 15   | 4 h 16 m                                  | 4 min 17 s                             |
| 16   | 8 h 32 m                                  | 8 min 20 s                             |
| 17   | 17 h 4 m                                  | 16 min 19 s                            |
| 18   | 1 d 10 h                                  | 32 min 20 s                            |
| 19   | 2 d 20 h                                  | 1 h 4 m                                |
| 20   | 5 d 17 h                                  | 2 h 8 m                                |
| 21   | 11 d 9 h                                  | 4 h 16 m                               |
| 22   | 22 d 18 h                                 | 8 h 32 m                               |
| 23   | 45 d 12 h                                 | 17 h 4 m                               |
| 24   | 91 d 1 h                                  | 1 d 10 h                               |
| 25   | 182 d 17 h                                | 2 d 20 h                               |
| 26   | 364 d 2 h                                 | 5 d 16 h                               |
| 27   | 1 y 363 d                                 | 11 d 9 h                               |
| 28   | 3 y 361 d                                 | 22 d 18 h                              |
| 29   | 7 y 358 d                                 | 45 d 12 h                              |
| 30   | 15 y 355 d                                | 91 d                                   |
| 31   | 31 y 336 d                                | 182 d                                  |
| 32   | 63 y 284 d                                | 364 d                                  |

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
Total time: 29.80s
Total iterations: 16
Global average time per derivation: 1862.77ms
Global derivations per second: 0.54
Thread average time per derivation: 29.80s
Thread derivations per second: 0.03

Estimated time to brute-force one preimage/key pair:
bits │ expected time
-----┼-------------
   1 │          30s
   2 │          30s
   3 │          30s
   4 │          30s
   5 │          30s
   6 │          30s
   7 │          60s
   8 │     1min 59s
   9 │     3min 58s
  10 │     7min 57s
  11 │    15min 54s
  12 │    31min 47s
  13 │      1h 4min
  14 │      2h 7min
  15 │     4h 14min
  16 │     8h 29min
  17 │    16h 57min
  18 │       1d 10h
  19 │       2d 20h
  20 │       5d 16h
  21 │       11d 7h
  22 │      22d 15h
  23 │       45d 5h
  24 │      90d 10h
  25 │     180d 21h
  26 │     361d 17h
  27 │      1y 358d
  28 │      3y 351d
  29 │      7y 337d
  30 │     15y 309d
  31 │     31y 252d
  32 │     63y 139d
```
---

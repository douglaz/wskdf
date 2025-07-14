# Weak, Slow, Key Derivation Function (WSKDF)

> **TL;DR** WSKDF intentionally limits key‑entropy **and** uses a *slow*, memory‑hard KDF (Argon2id) so that:
>
> * 🔑 Deriving the key when you *know* the preimage is fast (one Argon2id call).
> * 🛠️ Recovering the key when the preimage is lost is still feasible by brute‑force, *but* it takes predictable CPU time that scales with the chosen bit‑length.

---

## 1  Motivation

High‑entropy secrets are great—until you lose them. WSKDF lets you trade entropy for *recoverability*:

* The **preimage** is just `n` random bits (1 ≤ *n* ≤ 63 in the CLI).
  – Easy to store on paper/QR and/or with third parties.
* The **derived key** is produced by Argon2id with tunable cost (`--ops-limit`, `--mem-limit-kbytes`).
  – Slow and memory‑hard ⇒ brute‑forcing is expensive but bounded.

If you still have the preimage you can derive the key in seconds.
If you lose it, you (or a recovery service) can brute‑force with parallel hardware within a *predictable* amount of wall‑clock time (see table below).

Typical use‑case: encrypt a Bitcoin seed or small backup with a WSKDF key, stash the preimage on paper in another location, and sleep better knowing you *can* recover it even if the paper is destroyed.

---

## 2 CLI quick‑start

Note: salt is a hex encoded string of 16 bytes. It's good enough to generate it once and reuse for multiple keys. You can generate with something like:
```bash
$ openssl rand -hex 16
a228c13efadd4f6435a30d62a998d065
```

In this examples we will use `000102030405060708090a0b0c0d0e0f` as salt.

### Generate a 4‑bit preimage + key
Note: for real-world usage we recommend using a larger bit-length (e.g. 20).
```bash
$ cargo run --release -F alkali -- output-random-key -n 4 --preimage-output preimage --key-output key --salt 000102030405060708090a0b0c0d0e0f

$ cat preimage
0000000000000009

$ cat key
80a356d902bca7084da0084912183d63478b82a45c37f2df6ea51887d04553e7

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
$ INPUT_FILE=LICENSE.gpg OUTPUT_FILE=/tmp/LICENSE cargo run --release -F alkali -- find-key  --command ./scripts/gpg_decrypt.sh -t 4 -n 4 --preimage-output found-preimage --key-output found-key --salt 000102030405060708090a0b0c0d0e0f
Using 4 rayon threads
Starting parallel search
Deriving key for 0000000000000009
Deriving key for 000000000000000c
Deriving key for 000000000000000c
Deriving key for 000000000000000a
Trying to decrypt LICENSE.gpg with key 80a356d902bca7084da0084912183d63478b82a45c37f2df6ea51887d04553e7
Trying to decrypt LICENSE.gpg with key 5f1fdf16c1cbd2b559a38d3c113deed004c3ade44227cf03dbbd4dc6ddad0e2c
Trying to decrypt LICENSE.gpg with key 5f1fdf16c1cbd2b559a38d3c113deed004c3ade44227cf03dbbd4dc6ddad0e2c
Trying to decrypt LICENSE.gpg with key 620522780b9448642f40e1d5f792d8902dd376e302d16c820403d571c95eda7f

$ wc LICENSE /tmp/LICENSE 
  21  168 1064 LICENSE
  21  168 1064 /tmp/LICENSE
  42  336 2128 total

$ cat found-preimage 
0000000000000009

$ cat found-key 
80a356d902bca7084da0084912183d63478b82a45c37f2df6ea51887d04553e7
```

### Commands

All commands share the Argon2id cost flags. For release mode we have:

```text
      --ops-limit <OPS_LIMIT>                [default: 7] (iterations)
      --mem-limit-kbytes <MEM_LIMIT_KBYTES>  [default: 4194304] (4 GiB)
```

> ⚠️ The defaults make a single derivation take \~30s on a typical desktop CPU using all cores. See `benchmark` command below for better estimates on your hardware.

---

## 3 Brute‑force search time (random search, Argon2id\~30s)

**Assumptions**

* Preimages are uniformly from [2<sup>n-1</sup>, 2<sup>n</sup>), i.e. the most‑significant bit is **always 1**. Every candidate truly has *n* bits; the search‑space size is therefore 2<sup>n‑1</sup>
* Each candidate costs **30s** to evaluate (Argon2id with the default cost).
* Search is **random**; threads may test duplicate candidates.
* Two hardware budgets:

  * 🖥️ **16 threads** (e.g. 16-core/64GB RAM desktop machine)
  * 🏭 **2048 threads** (e.g. 64×32-core/128GB RAM machines on some cloud provider)

| Bits | 16 threads  | 2048 threads |
| ---- | ----------- | ------------ |
| 1    | 30 s        | 30 s         |
| 2    | 30 s        | 30 s         |
| 3    | 30 s        | 30 s         |
| 4    | 34 s        | 30 s         |
| 5    | 47 s        | 30 s         |
| 6    | 1 min 17 s  | 30 s         |
| 7    | 2 min 12 s  | 30 s         |
| 8    | 4 min 10 s  | 30 s         |
| 9    | 8 min 6 s   | 30 s         |
| 10   | 16 min 57 s | 31 s         |
| 11   | 32 min 41 s | 35 s         |
| 12   | 1 h 4 min   | 47 s         |
| 13   | 2 h 8 min   | 1 min 17 s   |
| 14   | 4 h 16 min  | 2 min 17 s   |
| 15   | 8 h 32 min  | 4 min 17 s   |
| 16   | 17 h 4 min  | 8 min 20 s   |
| 17   | 1 d 10 h    | 16 min 19 s  |
| 18   | 2 d 20 h    | 32 min 20 s  |
| 19   | 5 d 16 h    | 1 h 4 min    |
| 20   | 11 d 9 h    | 2 h 8 min    |
| 21   | 22 d 18 h   | 4 h 16 min   |
| 22   | 45 d 12 h   | 8 h 32 min   |
| 23   | 91 d 17 h   | 17 h 4 min   |
| 24   | 182 d 1 h   | 1 d 10 h     |
| 25   | 364 d 2 h   | 2 d 20 h     |
| 26   | 1 y 364 d   | 5 d 16 h     |
| 27   | 3 y 362 d   | 11 d 9 h     |
| 28   | 7 y 358 d   | 22 d 18 h    |
| 29   | 15 y 351 d  | 45 d 12 h    |
| 30   | 31 y 336 d  | 91 d         |
| 31   | 63 y 307 d  | 182 d        |
| 32   | 127 y 249 d | 364 d        |

> 📈 *Rule of thumb:* doubling the thread‑count halves the expected time **until** the pool can scan most of the space in one 30 s block. For *n ≲ 10* bits, adding more threads offers diminishing returns.

### Real world example using the `benchmark` command
```bash
$ cargo run --release -F alkali -- benchmark -i 1 -t 16
Using 16 threads for benchmark
Starting benchmark with 1 iterations across 16 threads...

Benchmark results:
Threads: 16
Total time: 29.27s
Total iterations: 16
Global average time per derivation: 1829.63ms
Global derivations per second: 0.55
Thread average time per derivation: 29.27s
Thread derivations per second: 0.03

Estimated time to brute-force one preimage/key pair:
bits │ expected time
-----┼-------------
   1 │          29s
   2 │          29s
   3 │          30s
   4 │          33s
   5 │          45s
   6 │     1min 13s
   7 │     2min 11s
   8 │      4min 8s
   9 │      8min 2s
  10 │    15min 51s
  11 │    31min 27s
  12 │      1h 3min
  13 │      2h 5min
  14 │     4h 10min
  15 │     8h 20min
  16 │    16h 39min
  17 │        1d 9h
  18 │       2d 19h
  19 │       5d 13h
  20 │       11d 2h
  21 │       22d 5h
  22 │      44d 10h
  23 │      88d 20h
  24 │     177d 15h
  25 │      355d 7h
  26 │      1y 345d
  27 │      3y 325d
  28 │      7y 285d
  29 │     15y 206d
  30 │      31y 46d
  31 │      62y 92d
  32 │    124y 185d
```
---

#!/usr/bin/env bash

set -Eeuo pipefail

# The first preimage requires patience to derive and is impossible to brute-force
FIRST_PREIMAGE_NBITS=63
# if OPS=100 takes 1507s, then for 30 days:
# >>> 30 * 24 * 3600 / 1507 * 100
# 171997
MONTHS_LONG_OPS=172000
# This is 16GB
LARGE_MEM_LIMIT_KBYTES=16777216

# The second preimage is easy to derive but requires money/time to brute-force
SECOND_PREIMAGE_NBITS=24
SECONDS_LONG_OPS=7
# This is 4GB
SMALL_MEM_LIMIT_KBYTES=4194304


echo "Generating first preimage/key"
first_key=$(./target/*/release/wskdf-cli output-random-key -n $FIRST_PREIMAGE_NBITS --preimage-output first-preimage --key-output - --salt-input salt --ops-limit $MONTHS_LONG_OPS --mem-limit-kbytes $LARGE_MEM_LIMIT_KBYTES --params-output first-key-params.json)

echo "Generating second preimage/key"
second_key=$(./target/*/release/wskdf-cli output-random-key -n $SECOND_PREIMAGE_NBITS --preimage-output second-preimage --key-output - --salt-input salt --ops-limit $SECONDS_LONG_OPS --mem-limit-kbytes $SMALL_MEM_LIMIT_KBYTES --params-output second-key-params.json)

echo "Encrypting second preimage"
gpg --symmetric --batch --passphrase-fd 0 --cipher-algo AES256 --output encrypted-second-preimage.gpg "second-preimage" <<< "$first_key"

echo "Generating final keyfile"
./scripts/encrypted-keyfile-generate.sh 10M encrypted-final-keyfile.gpg <<< "$second_key"

echo "Deriving again second key (sanity check)"
second_key=$(./target/*/release/wskdf-cli derive-key --preimage-input second-preimage --key-output - --salt-input salt --ops-limit $SECONDS_LONG_OPS --mem-limit-kbytes $SMALL_MEM_LIMIT_KBYTES)

echo "Decrypting final keyfile"
gpg --decrypt --batch --passphrase-fd 0 --cipher-algo AES256 "encrypted-final-keyfile.gpg" <<< "$second_key" > final-keyfile

echo "Done"

# To decrypt the second-preimage, use:

# first_key=$(./target/*/release/wskdf-cli derive-key --preimage-input first-preimage --key-output - --salt-input salt --ops-limit $MONTHS_LONG_OPS --mem-limit-kbytes $LARGE_MEM_LIMIT_KBYTES)
# gpg --decrypt --batch --passphrase-fd 0 --cipher-algo AES256 "encrypted-second-preimage.gpg" <<< "$first_key" > second-preimage-recovered


# To brute force the second preimage, use:

# INPUT_FILE="encrypted-final-keyfile.gpg" OUTPUT_FILE="final-keyfile-recovered" ./target/*/release/wskdf-cli find-key --command ./scripts/gpg_decrypt.sh --preimage-output second-preimage-recovered --n-bits $SECOND_PREIMAGE_NBITS --threads 16 --salt-input salt --ops-limit $SECONDS_LONG_OPS --mem-limit-kbytes $SMALL_MEM_LIMIT_KBYTES
#!/usr/bin/env bash

set -Eeuo pipefail

OUTPUT_DIR="${1:-complex-example-output}"
SALT="${2:-./salt}"
SECOND_PREIMAGE_NBITS="${3:-26}"
DAYS="${4:-30}"

# 32 bytes ought to be enough for anyone
KEYFILE_SIZE="32"

# The first preimage requires patience to derive and is impossible to brute-force.
# There is no good reason to use a low value here
FIRST_PREIMAGE_NBITS=63
# Calculate ops based on input days parameter
# if OPS=10 takes 30s, then for N days:
# N * 24 * 3600 / 30 * 10 = N * 28800
MONTHS_LONG_OPS=$((DAYS * 28800))
# This is 8GiB
LARGE_MEM_LIMIT_KBYTES=8388608

# The second preimage is easy to derive but requires money/time to brute-force
SECONDS_LONG_OPS=7
# This is 4GiB
SMALL_MEM_LIMIT_KBYTES=4194304

if [[ ! -f "$SALT" ]]; then
    echo "Salt file not found"
    exit 1
fi

if [[ $SECOND_PREIMAGE_NBITS -lt 20 ]]; then
    echo "Warning: low second preimage nbits ($SECOND_PREIMAGE_NBITS) may make it easy to brute-force"
fi

mkdir -p "$OUTPUT_DIR"
cp "$SALT" "$OUTPUT_DIR/salt"

echo "Generating first preimage/key (time-locked for $DAYS days), output: $OUTPUT_DIR/first-preimage-nbits-$FIRST_PREIMAGE_NBITS"
first_key=$(./target/*/release/wskdf-cli output-random-key -n $FIRST_PREIMAGE_NBITS --preimage-output $OUTPUT_DIR/first-preimage-nbits-$FIRST_PREIMAGE_NBITS --key-output - --salt-input $OUTPUT_DIR/salt --ops-limit $MONTHS_LONG_OPS --mem-limit-kbytes $LARGE_MEM_LIMIT_KBYTES --params-output $OUTPUT_DIR/first-key-params-nbits-$FIRST_PREIMAGE_NBITS.json)

echo "Generating second preimage/key, output: $OUTPUT_DIR/second-preimage-nbits-$SECOND_PREIMAGE_NBITS"
second_key=$(./target/*/release/wskdf-cli output-random-key -n $SECOND_PREIMAGE_NBITS --preimage-output $OUTPUT_DIR/second-preimage-nbits-$SECOND_PREIMAGE_NBITS --key-output - --salt-input $OUTPUT_DIR/salt --ops-limit $SECONDS_LONG_OPS --mem-limit-kbytes $SMALL_MEM_LIMIT_KBYTES --params-output $OUTPUT_DIR/second-key-params-nbits-$SECOND_PREIMAGE_NBITS.json)

echo "Encrypting second preimage, output: $OUTPUT_DIR/encrypted-second-preimage-nbits-$SECOND_PREIMAGE_NBITS.gpg"
gpg --symmetric --batch --passphrase-fd 0 --cipher-algo AES256 --output $OUTPUT_DIR/encrypted-second-preimage-nbits-$SECOND_PREIMAGE_NBITS.gpg "$OUTPUT_DIR/second-preimage-nbits-$SECOND_PREIMAGE_NBITS" <<< "$first_key"

echo "Generating final keyfile, output: $OUTPUT_DIR/encrypted-final-keyfile.gpg"
./scripts/encrypted-keyfile-generate.sh "$KEYFILE_SIZE" $OUTPUT_DIR/encrypted-final-keyfile.gpg <<< "$second_key"

echo "Deriving again second key (sanity check)"
second_key=$(./target/*/release/wskdf-cli derive-key --preimage-input $OUTPUT_DIR/second-preimage-nbits-$SECOND_PREIMAGE_NBITS --key-output - --salt-input $OUTPUT_DIR/salt --ops-limit $SECONDS_LONG_OPS --mem-limit-kbytes $SMALL_MEM_LIMIT_KBYTES)

echo "Decrypting final keyfile"
gpg --decrypt --batch --passphrase-fd 0 --cipher-algo AES256 "$OUTPUT_DIR/encrypted-final-keyfile.gpg" <<< "$second_key" > "$OUTPUT_DIR/final-keyfile"

echo "Done. Generated keyfile: $OUTPUT_DIR/final-keyfile"

# To decrypt the second-preimage, use:

# first_key=$(./target/*/release/wskdf-cli derive-key --preimage-input $OUTPUT_DIR/first-preimage-nbits-$FIRST_PREIMAGE_NBITS --key-output - --salt-input $OUTPUT_DIR/salt --ops-limit $MONTHS_LONG_OPS --mem-limit-kbytes $LARGE_MEM_LIMIT_KBYTES)
# gpg --decrypt --batch --passphrase-fd 0 --cipher-algo AES256 "$OUTPUT_DIR/encrypted-second-preimage-nbits-$SECOND_PREIMAGE_NBITS.gpg" <<< "$first_key" > second-preimage-nbits-$SECOND_PREIMAGE_NBITS-recovered


# To brute force the second preimage, use:

# INPUT_FILE="$OUTPUT_DIR/encrypted-final-keyfile.gpg" OUTPUT_FILE="$OUTPUT_DIR/final-keyfile-recovered" ./target/*/release/wskdf-cli find-key --command ./scripts/gpg_decrypt.sh --preimage-output $OUTPUT_DIR/second-preimage-nbits-$SECOND_PREIMAGE_NBITS-recovered --n-bits $SECOND_PREIMAGE_NBITS --threads 16 --salt-input $OUTPUT_DIR/salt --ops-limit $SECONDS_LONG_OPS --mem-limit-kbytes $SMALL_MEM_LIMIT_KBYTES
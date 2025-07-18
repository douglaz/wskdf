#!/usr/bin/env bash

block_size=${BLOCK_SIZE:?"BLOCK_SIZE is not set"}
encrypted_output_file=${ENCRYPTED_OUTPUT_FILE:?"ENCRYPTED_OUTPUT_FILE is not set"}

exec gpg --symmetric --batch --passphrase-fd 0 --cipher-algo AES256 --output "$encrypted_output_file" <(dd if=/dev/random  bs=$block_size count=1)
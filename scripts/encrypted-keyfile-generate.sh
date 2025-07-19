#!/usr/bin/env bash

block_size=${1:?"Missing block size"}
encrypted_output_file=${2:?"Missing encrypted output file"}

exec gpg --symmetric --batch --passphrase-fd 0 --cipher-algo AES256 --output "$encrypted_output_file" <(dd if=/dev/random  bs=$block_size count=1)
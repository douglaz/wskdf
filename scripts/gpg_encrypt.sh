#!/usr/bin/env bash
input_file=${1:?"input file required"}
exec gpg --symmetric --batch --passphrase-fd 0 --cipher-algo AES256 "$input_file"
#!/usr/bin/env bash
input_file=${INPUT_FILE:?"INPUT_FILE is not set"}
if [[ ! -f "$input_file" ]]; then
    echo "Input file $input_file does not exist"
    exit 1
fi
exec gpg --symmetric --batch --passphrase-fd 0 --cipher-algo AES256 "$input_file"
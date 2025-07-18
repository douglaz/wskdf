#!/usr/bin/env bash
# This will come from the environment variables
input_file=${INPUT_FILE:?"INPUT_FILE is not set"}
output_file=${OUTPUT_FILE:?"OUTPUT_FILE is not set"}
if [[ ! -f "$input_file" ]]; then
    echo "Input file $input_file does not exist"
    exit 1
fi
# Note: we could call gpg directly with the key on stdin, but we want to log the key for debugging purposes
read -r key
echo "Trying to decrypt $input_file with key $key"
temp_file=$(mktemp)
trap "rm -f $temp_file" EXIT
if gpg --decrypt --batch --passphrase-fd 0 --cipher-algo AES256 "$input_file" <<< "$key" > "$temp_file" 2>/dev/null; then
    echo "Found key!"
    mv "$temp_file" "$output_file"
    exit 0
fi
exit 1
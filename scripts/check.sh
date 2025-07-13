#!/usr/bin/env bash
# Note: for this example the correct preimage is "0000000000000000000000000000000f"
read -r key
echo "Checking $key"
if [[ "$key" == "b691f54cf57f87ad6a0661abac0669e07972862fe52556151a3f6206781b47d5" ]]; then
  exit 0
else
  exit 1
fi
